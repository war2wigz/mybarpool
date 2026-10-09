//! Mollusk glue (feature `runner`): fixtures on disk ↔ the plain model, and the replay.

use std::path::{Path, PathBuf};

use mollusk_svm::program::loader_keys::LOADER_V3;
use mollusk_svm::result::InstructionResult;
use mollusk_svm::Mollusk;
use mollusk_svm_fuzz_fixture::Fixture;
use mybarpool_fuzz::model::{Account, Context, Meta, Outcome, Result as Res};
use solana_account::Account as SolanaAccount;
use solana_instruction::AccountMeta;
use solana_pubkey::Pubkey;

/// One seed: the fixture, its plain context, and the file it came from (named on failure).
pub struct Seed {
    pub path: PathBuf,
    pub fixture: Fixture,
    pub context: Context,
}

/// Every `*.fix` file under `dir`, in path order.
pub fn load(dir: &Path) -> std::io::Result<Vec<Seed>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "fix"))
        .collect();
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|path| {
            let fixture = Fixture::load_from_blob_file(path.to_str().expect("utf-8 path"));
            let context = context_of(&fixture);
            Seed {
                path,
                fixture,
                context,
            }
        })
        .collect())
}

fn account_of(k: &Pubkey, a: &SolanaAccount) -> Account {
    Account {
        key: k.to_bytes(),
        lamports: a.lamports,
        data: a.data.clone(),
        owner: a.owner.to_bytes(),
        executable: a.executable,
    }
}

/// The fixture's input as the plain model.
pub fn context_of(f: &Fixture) -> Context {
    let input = &f.input;
    Context {
        program_id: input.program_id.to_bytes(),
        metas: input
            .instruction_accounts
            .iter()
            .map(|m| Meta {
                key: m.pubkey.to_bytes(),
                is_signer: m.is_signer,
                is_writable: m.is_writable,
            })
            .collect(),
        data: input.instruction_data.clone(),
        accounts: input
            .accounts
            .iter()
            .map(|(k, a)| account_of(k, a))
            .collect(),
        unix_timestamp: input.sysvars.clock.unix_timestamp,
        compute_unit_limit: input.compute_budget.compute_unit_limit,
    }
}

/// A copy of `f` whose input is `ctx` (the mutated context); the fixture's effects are kept
/// only so the type is whole, the harness never reads them for a mutated run.
pub fn fixture_with(f: &Fixture, ctx: &Context) -> Fixture {
    let mut out = f.clone();
    out.input.program_id = Pubkey::new_from_array(ctx.program_id);
    out.input.instruction_accounts = ctx
        .metas
        .iter()
        .map(|m| AccountMeta {
            pubkey: Pubkey::new_from_array(m.key),
            is_signer: m.is_signer,
            is_writable: m.is_writable,
        })
        .collect();
    out.input.instruction_data = ctx.data.clone();
    out.input.accounts = ctx
        .accounts
        .iter()
        .map(|a| {
            (
                Pubkey::new_from_array(a.key),
                SolanaAccount {
                    lamports: a.lamports,
                    data: a.data.clone(),
                    owner: Pubkey::new_from_array(a.owner),
                    executable: a.executable,
                    rent_epoch: 0,
                },
            )
        })
        .collect();
    out.input.sysvars.clock.unix_timestamp = ctx.unix_timestamp;
    out
}

/// Mollusk's result as the plain model. Classified from `program_result`, not `raw_result`:
/// Mollusk's post-execution rent check (on by default) overrides only the former, and a run the
/// runtime would reject must not pass for a success. That check also leaves the post-execution
/// accounts in the result, where the runtime would leave the inputs untouched, so it is named
/// (`mybarpool_fuzz::oracle::RENT_CHECK`) and the oracle skips its atomicity line for it.
pub fn outcome_of(r: &InstructionResult) -> Outcome {
    use mollusk_svm::result::ProgramResult as P;
    use solana_instruction::error::InstructionError as E;
    use solana_program_error::ProgramError as PE;
    let result = match &r.program_result {
        P::Success => Res::Success,
        P::Failure(PE::Custom(code)) => Res::Custom(*code),
        P::Failure(PE::AccountNotRentExempt) if r.raw_result.is_ok() => {
            Res::Runtime(mybarpool_fuzz::oracle::RENT_CHECK.to_string())
        }
        P::Failure(other) => Res::Runtime(format!("{other:?}")),
        P::UnknownError(E::ProgramFailedToComplete)
        | P::UnknownError(E::ComputationalBudgetExceeded)
        | P::UnknownError(E::CallDepth)
        | P::UnknownError(E::MaxInstructionTraceLengthExceeded)
        | P::UnknownError(E::ProgramEnvironmentSetupFailure) => Res::FailedToComplete,
        P::UnknownError(other) => Res::Runtime(format!("{other:?}")),
    };
    Outcome {
        result,
        accounts: r
            .resulting_accounts
            .iter()
            .map(|(k, a)| account_of(k, a))
            .collect(),
        compute_units: r.compute_units_consumed,
    }
}

/// A Mollusk with the program under test at `mybarpool::ID`, the token programs, the
/// associated-token program and the platform's Entropy fork loaded; the fixtures bring their
/// own compute budget, feature set and sysvars.
pub fn mollusk(program_elf: &[u8], entropy_elf: &[u8]) -> Mollusk {
    let mut m = Mollusk::default();
    m.add_program_with_loader_and_elf(
        &Pubkey::new_from_array(mybarpool::ID.to_bytes()),
        &LOADER_V3,
        program_elf,
    );
    m.add_program_with_loader_and_elf(
        &Pubkey::new_from_array(mybarpool::constants::ENTROPY_PROGRAM.to_bytes()),
        &LOADER_V3,
        entropy_elf,
    );
    mollusk_svm_programs_token::token::add_program(&mut m);
    mollusk_svm_programs_token::token2022::add_program(&mut m);
    mollusk_svm_programs_token::associated_token::add_program(&mut m);
    // A result below rent is a failure the oracle classifies, not a panic (Step 7).
    m.config.panic = false;
    m
}

/// Replay `seed` unmutated and compare with the recorded effects: `program_result` and every
/// resulting account (lamports, data, owner), not compute units.
pub fn replay(m: &mut Mollusk, seed: &Seed) -> Result<(), String> {
    let recorded = InstructionResult::from(&seed.fixture.output);
    let got = m.process_fixture(&seed.fixture);
    if got.program_result != recorded.program_result {
        return Err(format!(
            "{}: program_result {:?}, recorded {:?}",
            seed.path.display(),
            got.program_result,
            recorded.program_result
        ));
    }
    for (k, a) in &recorded.resulting_accounts {
        match got.resulting_accounts.iter().find(|(x, _)| x == k) {
            None => {
                return Err(format!(
                    "{}: {k} missing from the replay",
                    seed.path.display()
                ))
            }
            Some((_, b)) => {
                if a.lamports != b.lamports || a.data != b.data || a.owner != b.owner {
                    return Err(format!(
                        "{}: {k} differs on replay (lamports {} / {}, data {} / {} bytes)",
                        seed.path.display(),
                        a.lamports,
                        b.lamports,
                        a.data.len(),
                        b.data.len()
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Run a mutated context through Mollusk.
pub fn run(m: &mut Mollusk, seed: &Seed, ctx: &Context) -> Outcome {
    let f = fixture_with(&seed.fixture, ctx);
    outcome_of(&m.process_fixture(&f))
}

/// Print one fixture: the instruction, its metas, each account (and the pool's and vault's
/// state when present), the recorded result — before and after.
pub fn inspect(path: &Path) {
    use anchor_lang::AccountDeserialize;
    use mybarpool_fuzz::oracle::{expected_vault, token_account, vault_rent_floor};
    use mybarpool_fuzz::seeds::instruction_name;
    let fixture = Fixture::load_from_blob_file(path.to_str().expect("utf-8 path"));
    let ctx = context_of(&fixture);
    let recorded = InstructionResult::from(&fixture.output);
    let after = outcome_of(&recorded);
    println!("{}", path.display());
    println!(
        "instruction: {} ({} bytes of data), clock {}, {} CU limit",
        instruction_name(&ctx).unwrap_or("(other program / unknown)"),
        ctx.data.len(),
        ctx.unix_timestamp,
        ctx.compute_unit_limit
    );
    for (i, m) in ctx.metas.iter().enumerate() {
        println!(
            "  meta {i}: {}{}{}",
            Pubkey::new_from_array(m.key),
            if m.is_signer { " signer" } else { "" },
            if m.is_writable { " writable" } else { "" }
        );
    }
    let describe = |label: &str, accounts: &[Account]| {
        println!("{label}:");
        for a in accounts {
            let mut note = String::new();
            if let Ok(p) = mybarpool::Pool::try_deserialize(&mut &a.data[..]) {
                let key = a.key;
                note = format!(
                    " Pool {{ status {:?}, sold {}, price {}, sponsored_total {}, quarters_settled {}, fees_paid {}, unpaid {}, returned {:#027b}, sponsorships_open {}, §3.4 expects {} }}",
                    p.status, p.sold, p.price, p.sponsored_total, p.quarters_settled, p.fees_paid,
                    p.unpaid_prize_pool, p.returned, p.sponsorships_open,
                    expected_vault(&p, &key, accounts)
                );
            } else if let Some((mint, owner, amount)) = token_account(a) {
                note = format!(
                    " token {{ mint {}, owner {}, amount {amount} }}",
                    Pubkey::new_from_array(mint),
                    Pubkey::new_from_array(owner)
                );
            } else if a.owner == [0u8; 32] && a.data.is_empty() && a.lamports >= vault_rent_floor()
            {
                note = format!(
                    " system (above floor by {})",
                    a.lamports - vault_rent_floor()
                );
            }
            println!(
                "  {}: {} lamports, {} bytes, owner {}{}",
                Pubkey::new_from_array(a.key),
                a.lamports,
                a.data.len(),
                Pubkey::new_from_array(a.owner),
                note
            );
        }
    };
    describe("before", &ctx.accounts);
    println!(
        "recorded result: {:?} ({} CU)",
        after.result, after.compute_units
    );
    describe("after", &after.accounts);
}
