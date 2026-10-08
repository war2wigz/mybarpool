//! Entropy as deployed (PROGRAM §4.4; the Step 5 first task) and as forked
//! (Step 5b): Regolith's dumped ELF against the `verify.osec.io` report, the
//! fork's ELF against the hash the fork's CI printed, the `Var` decoder
//! against the live ORE `Var` fetched from mainnet, the two keccak formulas
//! against vectors the TypeScript side shares, `Open` on both bytecodes, and
//! the whole flow with nothing planted under Mollusk.

mod common;

use anchor_lang::prelude::Pubkey as APubkey;
use anchor_lang::solana_program::account_info::AccountInfo;
use common::*;
use mybarpool::entropy::{
    expected_value, fallback_hash, sample_instruction, Var, SAMPLE_DISCRIMINATOR,
    VAR_DISCRIMINATOR, VAR_LEN,
};
use mybarpool::MybarpoolError as E;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn code_of(error: anchor_lang::error::Error) -> u32 {
    match error {
        anchor_lang::error::Error::AnchorError(e) => e.error_code_number,
        other => panic!("not an AnchorError: {other:?}"),
    }
}

/// Decode `data` under `owner` as the program would see the account.
fn decode(owner: &APubkey, data: &mut [u8]) -> anchor_lang::Result<Var> {
    let key = APubkey::new_unique();
    let mut lamports = 1u64;
    let info = AccountInfo::new(&key, false, false, &mut lamports, data, owner, false);
    Var::try_from_account(&info)
}

/// `e_shoff + e_shentsize × e_shnum`: where the section-header table ends.
fn section_header_end(elf: &[u8]) -> u64 {
    let shoff = u64::from_le_bytes(elf[0x28..0x30].try_into().unwrap());
    let shentsize = u16::from_le_bytes(elf[0x3a..0x3c].try_into().unwrap());
    let shnum = u16::from_le_bytes(elf[0x3c..0x3e].try_into().unwrap());
    shoff + u64::from(shentsize) * u64::from(shnum)
}

#[test]
fn the_elf_fixture_is_the_verified_deployed_bytecode() {
    // verify.osec.io/status/3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X, fetched 2026-10-07:
    // repo regolith-labs/entropy, commit f26ae03c…, on_chain_hash == executable_hash below.
    assert_eq!(REGOLITH_ENTROPY_ELF.len(), 98_929);
    // The section-header table runs to byte 98,944; the fixture's last 15 bytes of it are zero
    // and were stripped with the padding, which is why `with_regolith_entropy` pads first.
    assert_eq!(
        section_header_end(REGOLITH_ENTROPY_ELF),
        REGOLITH_ENTROPY_ELF_LOADABLE_LEN as u64
    );
    assert_eq!(
        hex(&solana_sha256_hasher::hashv(&[REGOLITH_ENTROPY_ELF]).to_bytes()),
        "b64ffdfef7bb05839fbe0d24196697cc20de6bc72081031bc0523699181b18b1"
    );
}

#[test]
fn the_fork_elf_fixture_is_the_forks_verifiable_build() {
    // war2wigz/entropy at ENTROPY_FORK_COMMIT, `solana-verify build --arch v3` on the Agave
    // 4.3.0 image: 106,648 bytes; file sha256 below (NOTES › First task). The executable hash
    // (trailing zeros stripped, 106,633 bytes) is dae04201…bf53f1. CI rebuilds the fork and
    // compares the bytes with this fixture.
    assert_eq!(ENTROPY_FORK_ELF.len(), 106_648);
    // The verifiable build's section-header table ends exactly at the file's end: no padding.
    assert_eq!(
        section_header_end(ENTROPY_FORK_ELF),
        ENTROPY_FORK_ELF.len() as u64
    );
    assert_eq!(
        hex(&solana_sha256_hasher::hashv(&[ENTROPY_FORK_ELF]).to_bytes()),
        "2ec504ac1a71ff0f0f515c70a541522a5c58ef126aebb893dbf6ad7245d65fe8"
    );
    let stripped_len = ENTROPY_FORK_ELF
        .iter()
        .rposition(|&b| b != 0)
        .map_or(0, |i| i + 1);
    assert_eq!(stripped_len, 106_633);
    assert_eq!(
        hex(&solana_sha256_hasher::hashv(&[&ENTROPY_FORK_ELF[..stripped_len]]).to_bytes()),
        "dae042011c5d881edd8287a61694d60cf19471fb68c7fccd45f8965734bf53f1"
    );
    // SBPFv3: e_flags at 0x30 is 0x3.
    assert_eq!(
        u32::from_le_bytes(ENTROPY_FORK_ELF[0x30..0x34].try_into().unwrap()),
        3
    );
}

#[test]
fn the_live_ore_var_decodes_against_the_declared_layout() {
    // BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E at slot 454,331,258 (NOTES, first task).
    assert_eq!(LIVE_VAR.len(), VAR_LEN);
    let mut data = LIVE_VAR.to_vec();
    let v = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut data).expect("decodes");
    assert_eq!(
        v.authority.to_string(),
        "BrcSxdp1nXFzou1YyDnQJcPNBNHgoypZmTsyKBSLLXzi" // the ORE v3 Board account
    );
    assert_eq!(v.id, 0);
    assert_eq!(
        v.provider.to_string(),
        "AKBXJ7jQ2DiqLQKzgPn791r1ZVNvLchTFH6kpesPAAWF" // config.entropy_provider
    );
    assert_eq!(v.is_auto, 0);
    assert!(v.samples > 0);
    assert!(v.end_at > v.start_at);
    // As fetched: rolled by Next, committed, waiting for its window.
    assert_ne!(v.commit, [0u8; 32]);
    assert_eq!(v.seed, [0u8; 32]);
    assert_eq!(v.slot_hash, [0u8; 32]);
    assert_eq!(v.value, [0u8; 32]);
}

#[test]
fn the_decoder_refuses_a_wrong_owner_the_legacy_length_and_a_nonzero_discriminator() {
    let mut data = LIVE_VAR.to_vec();
    let err = decode(&anchor_lang::system_program::ID, &mut data).unwrap_err();
    assert_eq!(code_of(err), err_code(E::VarNotEntropy));

    // The two legacy mainnet accounts (21eRdeK7…, 2hhpsEkR…) are 232 bytes: no discriminator.
    let mut legacy = LIVE_VAR[8..].to_vec();
    assert_eq!(legacy.len(), 232);
    let err = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut legacy).unwrap_err();
    assert_eq!(code_of(err), err_code(E::VarNotEntropy));

    let mut bad = LIVE_VAR.to_vec();
    bad[0] = 1;
    let err = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut bad).unwrap_err();
    assert_eq!(code_of(err), err_code(E::VarNotEntropy));
    assert_eq!(VAR_DISCRIMINATOR, [0u8; 8]);
}

#[test]
fn the_planter_and_the_decoder_agree_on_every_offset() {
    let f = Fixture::new();
    let fields = f.revealed_var(END_AT, END_HASH, &SEED);
    let mut data = var_bytes(&fields).to_vec();
    let v = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut data).expect("decodes");
    assert_eq!(v.authority, to_a(&fields.authority));
    assert_eq!(v.id, fields.id);
    assert_eq!(v.provider, to_a(&fields.provider));
    assert_eq!(v.commit, fields.commit);
    assert_eq!(v.seed, fields.seed);
    assert_eq!(v.slot_hash, fields.slot_hash);
    assert_eq!(v.value, fields.value);
    assert_eq!(
        (v.samples, v.is_auto, v.start_at, v.end_at),
        (1, 0, fields.start_at, END_AT)
    );
}

#[test]
fn keccak_formulas_match_the_vectors_the_typescript_side_shares() {
    // packages/shared/test/entropy.test.ts carries the same three vectors (pycryptodome).
    assert_eq!(
        hex(&solana_keccak_hasher::hashv(&[&[]]).to_bytes()),
        "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    );
    assert_eq!(
        hex(&fallback_hash(1000)),
        "21c9650f3e9e5e2b94468fe9f1a4351613d2f305993af006c012c30ea53117e9"
    );
    assert_eq!(
        hex(&expected_value(&[0x11; 32], &[0x22; 32], 1)),
        "afb7ecc0aa543bf6cd7d2deac8d34b0476b669b383df70b55622236745e78744"
    );
    assert_ne!(
        expected_value(&[0x11; 32], &[0x22; 32], 2),
        expected_value(&[0x11; 32], &[0x22; 32], 1)
    );
    // And against the hasher directly, on the standard fixture values.
    let mut concat = Vec::new();
    concat.extend_from_slice(&END_HASH);
    concat.extend_from_slice(&SEED);
    concat.extend_from_slice(&1u64.to_le_bytes());
    assert_eq!(
        standard_value(),
        solana_keccak_hasher::hashv(&[&concat]).to_bytes()
    );
}

#[test]
fn the_sample_instruction_is_entropys_with_a_read_only_signer() {
    // api/src/sdk.rs at f26ae03: [signer (s), var (w), SlotHashes]; data [5].
    let signer = APubkey::new_unique();
    let var = APubkey::new_unique();
    let ix = sample_instruction(&signer, &var);
    assert_eq!(ix.program_id, mybarpool::constants::ENTROPY_PROGRAM);
    assert_eq!(ix.data, vec![SAMPLE_DISCRIMINATOR]);
    assert_eq!(ix.data, vec![5]);
    assert_eq!(ix.accounts.len(), 3);
    assert_eq!(
        (
            ix.accounts[0].pubkey,
            ix.accounts[0].is_signer,
            ix.accounts[0].is_writable
        ),
        (signer, true, false)
    );
    assert_eq!(
        (
            ix.accounts[1].pubkey,
            ix.accounts[1].is_signer,
            ix.accounts[1].is_writable
        ),
        (var, false, true)
    );
    assert_eq!(
        (
            ix.accounts[2].pubkey,
            ix.accounts[2].is_signer,
            ix.accounts[2].is_writable
        ),
        (solana_sdk_ids::sysvar::slot_hashes::ID, false, false)
    );
}

#[test]
fn mollusk_loads_the_fork_elf_and_runs_sample_against_a_planted_var() {
    // The fork's bytecode (`Sample` is Regolith's, unmodified) writes the SlotHashes entry for
    // end_at into a planted, fresh Var.
    let f = Fixture::new();
    let mut m = mollusk_for_draw(END_AT + 3, &standard_window());
    with_entropy(&mut m);
    let var = f.var_key();
    let accounts = vec![
        (f.stranger, system_account(LAMPORTS_PER_SOL)),
        (var, var_account(&f.fresh_var(END_AT))),
        m.sysvars.keyed_account_for_slot_hashes_sysvar(),
    ];
    let ix = {
        let a = sample_instruction(&to_a(&f.stranger), &to_a(&var));
        solana_instruction::Instruction::new_with_bytes(
            to_m(&a.program_id),
            &a.data,
            a.accounts
                .iter()
                .map(|m| solana_instruction::AccountMeta {
                    pubkey: to_m(&m.pubkey),
                    is_signer: m.is_signer,
                    is_writable: m.is_writable,
                })
                .collect(),
        )
    };
    let result = m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[mollusk_svm::result::Check::success()],
    );
    let after = account_of(&result, &var);
    assert_eq!(&after.data[144..176], &END_HASH);
    println!(
        "Entropy Sample under Mollusk: {} CU",
        result.compute_units_consumed
    );
}

/// `Open` on `program` for the standard Var from `authority`, provider `provider`,
/// `commit_of(&SEED)`, one manual sample, `END_AT`; the accounts an `Open` needs (the PDA slot
/// empty).
fn open_setup(
    f: &Fixture,
    program: &solana_pubkey::Pubkey,
    authority: &solana_pubkey::Pubkey,
    provider: &solana_pubkey::Pubkey,
) -> (
    solana_instruction::Instruction,
    Vec<(solana_pubkey::Pubkey, solana_account::Account)>,
    solana_pubkey::Pubkey,
) {
    let ix = open_ix_at(
        program,
        authority,
        authority,
        provider,
        VAR_ID,
        commit_of(&SEED),
        false,
        1,
        END_AT,
    );
    let var = ix.accounts[3].pubkey;
    let mut accounts = vec![
        (*authority, system_account(LAMPORTS_PER_SOL)),
        (var, system_account(0)),
        mollusk_svm::program::keyed_account_for_system_program(),
    ];
    if provider != authority {
        accounts.push((*provider, system_account(0)));
    }
    let _ = f;
    (ix, accounts, var)
}

#[test]
fn open_succeeds_on_the_fork_and_leaves_a_fresh_var() {
    // PROGRAM §4.4 `Open`, enabled in the platform's deployment: a 240-byte Var owned by
    // ENTROPY_PROGRAM with the commit, samples 1, is_auto 0, end_at as given, zero
    // seed/hash/value, provider as passed.
    let f = Fixture::new();
    let mut m = mollusk_for_draw(900, &[]);
    with_entropy(&mut m);
    let (ix, accounts, var) = open_setup(&f, &entropy_id(), &f.keeper, &f.entropy_provider);
    let result = m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[mollusk_svm::result::Check::success()],
    );
    let after = account_of(&result, &var);
    assert_eq!(after.owner, entropy_id());
    assert_eq!(after.data.len(), VAR_LEN);
    assert_eq!(after.lamports, rent_for(VAR_LEN));
    let mut data = after.data.clone();
    let v = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut data).expect("decodes");
    assert_eq!(v.authority, to_a(&f.keeper));
    assert_eq!(v.id, VAR_ID);
    assert_eq!(v.provider, to_a(&f.entropy_provider));
    assert_eq!(v.commit, commit_of(&SEED));
    assert_eq!(
        (v.seed, v.slot_hash, v.value),
        ([0u8; 32], [0u8; 32], [0u8; 32])
    );
    assert_eq!((v.samples, v.is_auto, v.end_at), (1, 0, END_AT));
    // The planter's bytes for the same state agree with what the real program wrote
    // (`start_at` is the slot of the Open; the fixture's default differs, so compare around it).
    let planted = var_bytes(&VarFields {
        start_at: v.start_at,
        ..f.fresh_var(END_AT)
    });
    assert_eq!(after.data, planted.to_vec());
    println!(
        "Entropy Open under Mollusk: {} CU",
        result.compute_units_consumed
    );
}

#[test]
fn open_with_an_unsigned_stranger_as_provider_also_succeeds_on_the_fork() {
    // The fork keeps Regolith's commented `provider_info.is_signer()?`: the provider is a label
    // the opener sets, which is why set_var's provider check is a label too (PROGRAM §4.4).
    let f = Fixture::new();
    let mut m = mollusk_for_draw(900, &[]);
    with_entropy(&mut m);
    let (ix, accounts, var) = open_setup(&f, &entropy_id(), &f.keeper, &f.stranger);
    assert!(!ix.accounts[2].is_signer);
    let result = m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[mollusk_svm::result::Check::success()],
    );
    let mut data = account_of(&result, &var).data.clone();
    assert_eq!(
        decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut data)
            .unwrap()
            .provider,
        to_a(&f.stranger)
    );
}

#[test]
fn open_canary_regoliths_deployed_bytecode_still_refuses_open() {
    // The Step 5 finding, kept against Regolith's bytecode, loaded at Regolith's own id
    // (`with_regolith_entropy` / `regolith_entropy_id()`: steel's entrypoint checks the program
    // id before dispatching, so it cannot stand in at ENTROPY_PROGRAM): the same Open fails with
    // InvalidInstructionData from the dispatcher's `_` arm.
    // If this ever passes, Regolith re-enabled `Open`; tell the auditor before anything else.
    let f = Fixture::new();
    let mut m = mollusk_for_draw(900, &[]);
    with_regolith_entropy(&mut m);
    let (ix, accounts, _) = open_setup(&f, &regolith_entropy_id(), &f.keeper, &f.entropy_provider);
    assert_eq!(ix.data.len(), 65);
    m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[mollusk_svm::result::Check::err(
            solana_program_error::ProgramError::InvalidInstructionData,
        )],
    );
}

#[test]
fn the_real_flow_under_mollusk_open_set_var_sample_var_reveal_draw_close() {
    // PROGRAM §4.4 flow with nothing planted: Open (fork) → set_var → clock to END_AT + 3 with
    // the standard window → sample_var by a stranger (CPI) → Reveal(SEED) by a stranger (the
    // real bytecode) → draw → Close by the keeper (three accounts).
    use mollusk_svm::result::Check;
    use mybarpool::axes::draw_axes;
    use mybarpool::{Pool, PoolStatus};

    let f = Fixture::new();
    let mut m = mollusk_for_draw(900, &[]);
    with_entropy(&mut m);
    let var = f.var_key();
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;

    // Open: the keeper is authority, payer and provider.
    let mut accounts = draw_accounts(&f, &m, &pool, None);
    set_account(&mut accounts, var, system_account(0));
    set_account(&mut accounts, f.entropy_provider, system_account(0));
    accounts.push(mollusk_svm::program::keyed_account_for_system_program());
    let open = open_ix(
        &f.keeper,
        &f.keeper,
        &f.entropy_provider,
        VAR_ID,
        commit_of(&SEED),
        false,
        1,
        END_AT,
    );
    let r = m.process_and_validate_instruction(&open, &accounts, &[Check::success()]);
    let keeper_after_open = account_of(&r, &f.keeper).lamports;

    // set_var: the pool records var, end_at and the commit.
    let r = m.process_and_validate_instruction(
        &set_var_ix(&f.keeper, &pool, &var),
        &r.resulting_accounts,
        &[Check::success()],
    );
    let p: Pool = decode_pool(account_of(&r, &pool_key));
    assert_eq!(
        (p.var, p.var_end_at, p.var_commit),
        (to_a(&var), END_AT, commit_of(&SEED))
    );

    // The window: clock to END_AT + 3, the standard SlotHashes.
    let mut m2 = mollusk_for_draw(END_AT + 3, &standard_window());
    with_entropy(&mut m2);
    let mut accounts = r.resulting_accounts.clone();
    set_account(
        &mut accounts,
        to_m(&solana_sdk_ids::sysvar::slot_hashes::ID),
        m2.sysvars.keyed_account_for_slot_hashes_sysvar().1,
    );
    set_account(
        &mut accounts,
        to_m(&solana_sdk_ids::sysvar::clock::ID),
        m2.sysvars.keyed_account_for_clock_sysvar().1,
    );
    let r = m2.process_and_validate_instruction(
        &sample_var_ix(&f.stranger, &p, &var),
        &accounts,
        &[Check::success()],
    );
    let p = decode_pool(account_of(&r, &pool_key));
    assert_eq!((p.sampled_slot, p.sampled_hash), (END_AT + 3, END_HASH));

    // Reveal(SEED) by the stranger: the real bytecode computes the value.
    let r = m2.process_and_validate_instruction(
        &reveal_ix(&f.stranger, &var, &SEED),
        &r.resulting_accounts,
        &[Check::success()],
    );
    let mut data = account_of(&r, &var).data.clone();
    let v = decode(&mybarpool::constants::ENTROPY_PROGRAM, &mut data).unwrap();
    assert_eq!(v.seed, SEED);
    assert_eq!(v.value, expected_value(&END_HASH, &SEED, 1));

    // draw.
    let r = m2.process_and_validate_instruction(
        &draw_ix(&f.keeper, &p, &var),
        &r.resulting_accounts,
        &[Check::success()],
    );
    let p = decode_pool(account_of(&r, &pool_key));
    assert_eq!(p.status, PoolStatus::Drawn);
    let (home, away) = draw_axes(&expected_value(&END_HASH, &SEED, 1));
    assert_eq!((p.home_axis, p.away_axis), (home, away));

    // Close by the keeper: the account is gone and the rent came back.
    let keeper_before_close = account_of(&r, &f.keeper).lamports;
    let r = m2.process_and_validate_instruction(
        &close_ix(&f.keeper, &var),
        &r.resulting_accounts,
        &[Check::success()],
    );
    let closed = account_of(&r, &var);
    assert_eq!(
        (closed.lamports, closed.data.len(), closed.owner),
        (0, 0, solana_pubkey::Pubkey::default())
    );
    assert_eq!(
        account_of(&r, &f.keeper).lamports - keeper_before_close,
        rent_for(VAR_LEN)
    );
    assert!(keeper_after_open < keeper_before_close + rent_for(VAR_LEN));
}

/// Anchor's custom code offset + the variant index.
fn err_code(e: E) -> u32 {
    err(e)
}
