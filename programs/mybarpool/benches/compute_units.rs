//! Compute-unit table (ARCHITECTURE › Environments, build plan Step 0).
//!
//! `cargo bench -p mybarpool` after `anchor build --arch v3` rewrites
//! `programs/mybarpool/compute_units.md`, which is committed. CI regenerates
//! it and shows the diff as a review item; a changed row is not a failure.
//! Each step adds one row per instruction; the DESIGN §10.4 headroom rule
//! (+20% and +30k units) is checked against this table.

#[path = "../tests/common/mod.rs"]
mod common;

use common::*;
use mollusk_svm_bencher::MolluskComputeUnitBencher;
use mybarpool::{CreatePoolParams, GameStatus, PayoutPreset, PoolStatus};

fn main() {
    let f = Fixture::new();
    // Every key that is not a Fixture field is a literal, so a row never moves because the
    // fixture grew (Step 4 audit L1: `Pubkey::new_unique()` is a deterministic counter and
    // the keys it yields shift with every key the fixture adds).
    let wallet = solana_pubkey::Pubkey::new_from_array([7; 32]);
    let config = f.expected_config();

    // initialize: fresh config PDA, ProgramData check, one mint (ORE) attached.
    let initialize = initialize_ix(&f.admin, f.initialize_params(), Some(&f.ore_mint));
    let initialize_accounts = base_accounts(&f, None);

    // update_config_full: every field Some, the ORE mint re-checked.
    let p = f.initialize_params();
    let full = mybarpool::UpdateConfigParams {
        admin: Some(to_a(&f.admin)),
        score_authority: Some(p.score_authority),
        entropy_provider: Some(p.entropy_provider),
        fee_wallet: Some(p.fee_wallet),
        platform_bps: Some(p.platform_bps),
        creator_bps: Some(p.creator_bps),
        addon_budget_bps: Some(p.addon_budget_bps),
        default_preset: Some(p.default_preset),
        max_open_pools: Some(p.max_open_pools),
        max_own_boxes: Some(p.max_own_boxes),
        preseason_enabled: Some(p.preseason_enabled),
        paused: Some(p.paused),
        tokens: [Some(p.tokens[0]), Some(p.tokens[1]), Some(p.tokens[2])],
    };
    let update_config_full = update_config_ix(&f.admin, full, None, Some(&f.ore_mint));
    let update_accounts = base_accounts(&f, Some(&config));

    // set_wallet_override_create: PDA does not exist yet.
    let set_create = set_override_ix(&f.admin, &wallet, 10, 5);
    let mut create_accounts = base_accounts(&f, Some(&config));
    create_accounts.push((override_pda(&wallet).0, system_account(0)));

    // set_wallet_override_update / close_wallet_override: PDA already exists.
    let existing = mybarpool::WalletOverride {
        wallet: to_a(&wallet),
        max_open_pools: 10,
        max_own_boxes: 5,
        bump: override_pda(&wallet).1,
    };
    let set_update = set_override_ix(&f.admin, &wallet, 2, 25);
    let close = close_override_ix(&f.admin, &wallet);
    let mut existing_accounts = base_accounts(&f, Some(&config));
    existing_accounts.push((
        override_pda(&wallet).0,
        account_for(&existing, &program_id(), mybarpool::WalletOverride::SIZE),
    ));

    // Step 3: the game instructions. The bencher takes one Mollusk, so one clock (T0) must
    // satisfy every row: create_game, update_kickoff and mark_game run on the standard record
    // (kickoff a day ahead); the two post_scores rows use a record whose kickoff was a day ago,
    // with Q1–Q3 posted 45 minutes apart, so the 15-minute floors are met.
    let game = game_pda(&standard_key(), SCHEDULED).0;
    let create_game = create_game_ix(&f.keeper, standard_key(), SCHEDULED);
    let create_game_accounts = game_accounts(&f, None);
    let update_kickoff = update_kickoff_ix(&f.keeper, &game, SCHEDULED + 3_600);
    let mark_game = mark_game_ix(&f.admin, &game, mybarpool::GameStatus::Postponed);
    let fresh_accounts = game_accounts(&f, Some(&standard_record()));

    let played_kickoff = T0 - 86_400;
    let played = fresh_record(standard_key(), played_kickoff);
    let played_game = game_pda(&standard_key(), played_kickoff).0;
    let post_q1 = post_scores_ix(&f.keeper, &played_game, 1, 7, 3, false, false);
    let post_q1_accounts = game_accounts(&f, Some(&played));
    let mut played_after_q3 = played;
    for (q, (home, away)) in [(7u16, 3u16), (14, 10), (17, 17)].iter().enumerate() {
        played_after_q3.home_score[q] = *home;
        played_after_q3.away_score[q] = *away;
        played_after_q3.posted_at[q] = played_kickoff + 2_700 * (q as i64 + 1);
    }
    played_after_q3.quarters_posted = 3;
    let post_final = post_scores_ix(&f.keeper, &played_game, 4, 24, 20, true, true);
    let post_final_accounts = game_accounts(&f, Some(&played_after_q3));

    // Step 4: pools. One Mollusk with the token programs and a slot hash; the standard game at
    // T0 is open. The eleven rows the brief names.
    // The ATA program is loaded too (Step 6's SPL settlement rows); the earlier rows ignore it.
    let mut m = mollusk_for_pools(T0, SLOT_HASH);
    mollusk_svm_programs_token::associated_token::add_program(&mut m);
    let m = m;
    let config = f.expected_config();
    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;

    let create_pool_sol = create_pool_ix(
        &f.creator,
        &standard_game(),
        sol_params(0),
        TokenPath::default(),
    );
    let create_pool_sol_5 = create_pool_ix(
        &f.creator,
        &standard_game(),
        sol_params(5),
        TokenPath::default(),
    );
    let create_sol_accounts = pool_base(&f, &m, &config);

    let creator_ore = solana_pubkey::Pubkey::new_from_array([8; 32]);
    let ore_path = TokenPath {
        mint: Some(f.ore_mint),
        token_account: Some(creator_ore),
        token_program: Some(token_program_id()),
    };
    let create_pool_spl = create_pool_ix(&f.creator, &standard_game(), ore_params(1), ore_path);
    let mut create_spl_accounts = pool_base(&f, &m, &config);
    create_spl_accounts.push((
        creator_ore,
        token_account(&f.ore_mint, &f.creator, 10 * PRICE_ORE),
    ));

    let open = fresh_pool(&f, &config, &sol_params(0));
    let buy_1 = buy_ix(&f.buyer, &open, 1, TokenPath::default());
    let buy_3 = buy_ix(&f.buyer, &open, 3, TokenPath::default());
    let open_accounts = pool_accounts(&f, &m, &config, &open, rent_for(0), 1);
    let almost = pool_with(&f, mybarpool::PoolStatus::Open, 24, &f.buyer_2);
    let buy_25th = buy_ix(&f.buyer, &almost, 1, TokenPath::default());
    let almost_accounts = pool_accounts(&f, &m, &config, &almost, rent_for(0) + 24 * PRICE, 1);

    let ore_pool = fresh_pool(&f, &config, &ore_params(0));
    let buyer_ore = solana_pubkey::Pubkey::new_from_array([9; 32]);
    let buy_spl_1 = buy_ix(
        &f.buyer,
        &ore_pool,
        1,
        TokenPath {
            mint: Some(f.ore_mint),
            token_account: Some(buyer_ore),
            token_program: Some(token_program_id()),
        },
    );
    let mut ore_accounts = pool_accounts(&f, &m, &config, &ore_pool, 0, 1);
    set_account(
        &mut ore_accounts,
        vault_pda(&pool_key).0,
        token_account(&f.ore_mint, &pool_key, 0),
    );
    ore_accounts.push((
        buyer_ore,
        token_account(&f.ore_mint, &f.buyer, 10 * PRICE_ORE),
    ));

    let sponsor_new = sponsor_ix(&f.sponsor, &open, PRICE, TokenPath::default());
    let mut sponsored = fresh_pool(&f, &config, &sol_params(0));
    sponsored.sponsored_total = PRICE;
    sponsored.sponsor_count = 1;
    sponsored.sponsorships_open = 1;
    let sponsor_top_up = sponsor_ix(&f.sponsor, &sponsored, PRICE, TokenPath::default());
    let mut top_up_accounts = pool_accounts(&f, &m, &config, &sponsored, rent_for(0) + PRICE, 1);
    set_account(
        &mut top_up_accounts,
        sponsorship_pda(&pool_key, &f.sponsor).0,
        account_for(
            &mybarpool::Sponsorship {
                pool: to_a(&pool_key),
                wallet: to_a(&f.sponsor),
                amount: PRICE,
                bump: sponsorship_pda(&pool_key, &f.sponsor).1,
            },
            &program_id(),
            mybarpool::Sponsorship::SIZE,
        ),
    );

    let mut link = fresh_pool(&f, &config, &sol_params(0));
    link.access_type = mybarpool::AccessType::Link;
    link.gate_key = to_a(&solana_pubkey::Pubkey::new_from_array([10; 32]));
    let rotate = rotate_gate_key_ix(
        &f.creator,
        &link,
        &solana_pubkey::Pubkey::new_from_array([11; 32]),
    );
    let link_accounts = pool_accounts(&f, &m, &config, &link, rent_for(0), 1);

    // Step 8: gating in `buy` (PROGRAM §4.3, §6.4). The gate key signs read-only; the
    // allowlist rows plant a root at three depths. Depth 32 folds 32 planted siblings (a 2^32
    // tree is not built); the proof is 1,024 bytes of instruction data.
    let gate_key = solana_pubkey::Pubkey::new_from_array([10; 32]);
    let buy_link = buy_ix_gated(
        &f.buyer,
        &link,
        1,
        TokenPath::default(),
        Some((gate_key, true)),
        vec![],
    );
    let mut buy_link_accounts = link_accounts.clone();
    buy_link_accounts.push((gate_key, system_account(0)));

    let two = allowlist_wallets(2);
    let allow_1 = allowlist_pool(&f, &two);
    let buy_allow_1 = buy_ix_gated(
        &two[0],
        &allow_1,
        1,
        TokenPath::default(),
        None,
        tree::proof(&two, &two[0]),
    );
    let allow_1_accounts = pool_accounts_with_wallets(&f, &m, &allow_1, rent_for(0), 1, &two);

    let many = allowlist_wallets(1_024);
    let allow_10 = allowlist_pool(&f, &many);
    let proof_10 = tree::proof(&many, &many[0]);
    assert_eq!(proof_10.len(), 10);
    let buy_allow_10 = buy_ix_gated(&many[0], &allow_10, 1, TokenPath::default(), None, proof_10);
    let allow_10_accounts =
        pool_accounts_with_wallets(&f, &m, &allow_10, rent_for(0), 1, &many[..1]);

    let proof_32: Vec<[u8; 32]> = (0..32u8)
        .map(|i| {
            let mut s = [0x50u8; 32];
            s[31] = i;
            s
        })
        .collect();
    let root_32 = proof_32
        .iter()
        .fold(mybarpool::allowlist::leaf(&to_a(&f.buyer)), |acc, s| {
            mybarpool::allowlist::node(&acc, s)
        });
    let mut allow_32 = fresh_pool(&f, &config, &sol_params(0));
    allow_32.access_type = mybarpool::AccessType::Allowlist;
    allow_32.allowlist_root = root_32;
    let buy_allow_32 = buy_ix_gated(&f.buyer, &allow_32, 1, TokenPath::default(), None, proof_32);
    let allow_32_accounts = pool_accounts(&f, &m, &config, &allow_32, rent_for(0), 1);

    let close_counter = close_counter_ix(&f.creator, &standard_game(), &f.fee_wallet);
    let mut close_counter_accounts = base_accounts(&f, Some(&config));
    close_counter_accounts.push((
        counter_pda(&f.creator, &standard_game()).0,
        counter_account(&f.creator, &standard_game(), 0),
    ));
    close_counter_accounts.push((f.fee_wallet, system_account(LAMPORTS_PER_SOL)));

    // Step 5: the draw. The same Mollusk: its clock sits at `T0 × 5 / 2`; the SlotHashes keeps
    // `(SLOT, SLOT_HASH)` first (the `buy` rows read the newest entry) and gains a window
    // ending at `end_b = clock.slot − 3` for the `Var` the sample rows bind. The real Entropy
    // bytecode is loaded so the `sample_var` row measures the CPI.
    let mut m = m;
    let end_b = m.sysvars.clock.slot - 3;
    m.sysvars.slot_hashes = anchor_lang::prelude::SlotHashes::new(&[
        (SLOT, solana_hash::Hash::new_from_array(SLOT_HASH)),
        (end_b + 2, solana_hash::Hash::new_from_array([0x5D; 32])),
        (end_b + 1, solana_hash::Hash::new_from_array([0x5C; 32])),
        (end_b, solana_hash::Hash::new_from_array(END_HASH)),
    ]);
    with_entropy(&mut m);
    let var = f.var_key();
    let var_b = var_pda(&f.keeper, VAR_ID + 1).0;

    let set_var_pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let set_var = set_var_ix(&f.keeper, &set_var_pool, &var);
    let set_var_accounts = draw_accounts(
        &f,
        &m,
        &set_var_pool,
        Some((&var, &f.fresh_var(end_b + 1_000))),
    );

    let bound = locked_pool_with_var(&f, &var, end_b);
    let sample_var = sample_var_ix(&f.keeper, &bound, &var);
    let sample_var_accounts = draw_accounts(&f, &m, &bound, Some((&var, &f.fresh_var(end_b))));
    let already_sampled_accounts = draw_accounts(
        &f,
        &m,
        &bound,
        Some((&var, &f.sampled_var(end_b, END_HASH))),
    );

    let sampled = sampled_pool(&f, &var, end_b, m.sysvars.clock.slot, END_HASH);
    let draw = draw_ix(&f.keeper, &sampled, &var);
    let draw_accounts_ = draw_accounts(
        &f,
        &m,
        &sampled,
        Some((&var, &f.revealed_var(end_b, END_HASH, &SEED))),
    );

    let replace_var = replace_var_ix(&f.admin, &bound, &var_b);
    let mut replace_var_accounts = draw_accounts(&f, &m, &bound, Some((&var, &f.fresh_var(end_b))));
    replace_var_accounts.push((
        var_b,
        var_account(&VarFields {
            id: VAR_ID + 1,
            ..f.fresh_var(end_b + 1_000)
        }),
    ));

    // Step 6: settlement. The standard drawn pool (identity axes, the Step 3 scores): Q1 pays
    // buyer_2 + two fees; Q2 the creator, prize only; Q4 the creator and settles; FinalOnly Q1
    // moves nothing; the integrator variant adds a third fee; the ORE row creates three ATAs and
    // moves three token transfers (the worst case the CU table is read against).
    let config = f.expected_config();
    let settle_pool = drawn_pool_for_settlement(&f, &config, &sol_params(0), 0);
    let settle_q1 = settle_ix(
        &f,
        &f.keeper,
        &settle_pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts::default(),
    );
    let settle_q1_accounts = settlement_accounts(&f, &m, &config, &settle_pool, 4, None);
    let after_q1 = settled_pool(&f, &config, &sol_params(0), 0, 1, true);
    let settle_q2 = settle_ix(
        &f,
        &f.keeper,
        &after_q1,
        &standard_game(),
        2,
        &f.creator,
        SettleAccounts::default(),
    );
    let settle_q2_accounts = settlement_accounts(&f, &m, &config, &after_q1, 4, None);
    let after_q3 = settled_pool(&f, &config, &sol_params(0), 0, 3, true);
    let settle_q4 = settle_ix(
        &f,
        &f.keeper,
        &after_q3,
        &standard_game(),
        4,
        &f.creator,
        SettleAccounts::default(),
    );
    let settle_q4_accounts = settlement_accounts(&f, &m, &config, &after_q3, 4, None);
    let final_only = drawn_pool_for_settlement(
        &f,
        &config,
        &CreatePoolParams {
            preset: PayoutPreset::FinalOnly,
            ..sol_params(0)
        },
        0,
    );
    let settle_final_only = settle_ix(
        &f,
        &f.keeper,
        &final_only,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts::default(),
    );
    let settle_final_only_accounts = settlement_accounts(&f, &m, &config, &final_only, 4, None);
    let with_integrator = drawn_pool_for_settlement(
        &f,
        &config,
        &CreatePoolParams {
            integrator: to_a(&f.integrator),
            integrator_bps: 100,
            ..sol_params(0)
        },
        0,
    );
    let settle_integrator = settle_ix(
        &f,
        &f.keeper,
        &with_integrator,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts {
            integrator: Some(f.integrator),
            spl: None,
        },
    );
    let settle_integrator_accounts =
        settlement_accounts(&f, &m, &config, &with_integrator, 4, None);
    let ore_pool = drawn_pool_for_settlement(&f, &config, &ore_params(0), 0);
    let ore_spl = SplSettle::derived(&f, &ore_pool, &f.buyer_2);
    let settle_ore = settle_ix(
        &f,
        &f.keeper,
        &ore_pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts {
            integrator: None,
            spl: Some(ore_spl),
        },
    );
    let mut settle_ore_accounts =
        settlement_accounts(&f, &m, &config, &ore_pool, 4, Some(25 * PRICE_ORE));
    for k in [ore_spl.winner_ata, ore_spl.fee_ata, ore_spl.creator_ata] {
        set_account(&mut settle_ore_accounts, k, system_account(0));
    }
    // The same call with the three ATAs present at balance 0: three transfer_checked and three
    // no-op idempotent creates (Step 6 audit L1, the row the table was missing).
    let mut settle_ore_atas_exist_accounts = settle_ore_accounts.clone();
    for (k, wallet) in [
        (ore_spl.winner_ata, f.buyer_2),
        (ore_spl.fee_ata, f.fee_wallet),
        (ore_spl.creator_ata, f.creator),
    ] {
        set_account(
            &mut settle_ore_atas_exist_accounts,
            k,
            token_account(&f.ore_mint, &wallet, 0),
        );
    }
    let settled = settled_pool(&f, &config, &sol_params(0), 0, 4, true);
    let close_sol = close_pool_ix(&f, &f.stranger, &settled, None);
    let close_sol_accounts = settlement_accounts(&f, &m, &config, &settled, 4, None);
    let settled_ore = settled_pool(&f, &config, &ore_params(0), 0, 4, true);
    let dest_ata = ata(&f.fee_wallet, &f.ore_mint, &token_program_id());
    let close_ore = close_pool_ix(
        &f,
        &f.stranger,
        &settled_ore,
        Some(SplClose {
            mint: f.ore_mint,
            token_program: token_program_id(),
            destination_ata: dest_ata,
        }),
    );
    let mut close_ore_accounts = settlement_accounts(&f, &m, &config, &settled_ore, 4, Some(2));
    set_account(&mut close_ore_accounts, dest_ata, system_account(0));

    // Step 7 — the §4.6 returns. The bench clock is T0 (before the standard kickoff), so the
    // time-gated rows plant the game record they need: `moved_record(now)` makes the pool
    // unfilled-at-kickoff, and a record whose scheduled kickoff is RECLAIM_DELAY in the past
    // opens the reclaim window; a marked record needs no clock at all.
    let now = m.sysvars.clock.unix_timestamp;
    let game = standard_game();
    let counter_key = counter_pda(&f.creator, &game).0;
    let plan_unfilled = unfilled_15_plan(&f);
    let plan_full = full_plan(&f);
    let owners_of = |n: u8, base: u8| -> Vec<solana_pubkey::Pubkey> {
        (0..n)
            .map(|i| solana_pubkey::Pubkey::new_from_array([base + i; 32]))
            .collect()
    };
    let sol_batch = |owners: &[solana_pubkey::Pubkey]| -> Vec<(solana_pubkey::Pubkey, Option<solana_pubkey::Pubkey>)> {
        owners.iter().map(|o| (*o, None)).collect()
    };

    // return_boxes_first_sol_2_owners: Open UNFILLED_15 at its recorded kickoff, the counter
    // decremented, two transfers.
    let unfilled = pool_with_owners(
        &f,
        &config,
        &sol_params(0),
        PoolStatus::Open,
        &plan_unfilled,
    );
    let return_first = return_boxes_ix(
        &f.keeper,
        &unfilled,
        &game,
        Some(counter_key),
        &sol_batch(&[f.creator, f.buyer]),
        None,
    );
    let return_first_accounts =
        returns_accounts(&f, &m, &config, &unfilled, &moved_record(now), 1, None);

    // return_boxes_25_owners_sol: a Drawn pool on a postponed game, 25 distinct owners.
    let mut twenty_five =
        pool_with_owners(&f, &config, &sol_params(0), PoolStatus::Drawn, &plan_full);
    let owners_25 = owners_of(25, 100);
    for (i, o) in owners_25.iter().enumerate() {
        twenty_five.owners[i] = to_a(o);
    }
    twenty_five.creator_boxes = 0;
    let return_25 = return_boxes_ix(
        &f.keeper,
        &twenty_five,
        &game,
        None,
        &sol_batch(&owners_25),
        None,
    );
    let mut return_25_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &twenty_five,
        &marked_record(GameStatus::Postponed),
        0,
        None,
    );
    for o in &owners_25 {
        return_25_accounts.push((*o, system_account(LAMPORTS_PER_SOL)));
    }

    // return_boxes_ore_3_owners_atas_missing / _11_owners_atas_missing: the SPL batch with
    // every ATA created on the way.
    let ore_drawn = pool_with_owners(&f, &config, &ore_params(0), PoolStatus::Drawn, &plan_full);
    let ore_batch = SplBatch::of(&ore_drawn);
    let ore_3: Vec<_> = [f.creator, f.buyer, f.buyer_2]
        .iter()
        .map(|w| (*w, Some(ore_batch.ata(w))))
        .collect();
    let return_ore_3 = return_boxes_ix(&f.keeper, &ore_drawn, &game, None, &ore_3, Some(ore_batch));
    let mut return_ore_3_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &ore_drawn,
        &marked_record(GameStatus::Postponed),
        0,
        Some(25 * PRICE_ORE),
    );
    for (_, a) in &ore_3 {
        set_account(&mut return_ore_3_accounts, a.unwrap(), system_account(0));
    }
    // The SPL page. With every ATA missing each owner costs seven inner instructions (the
    // ATA program's create and its nested System and Token calls, the transfer_checked, the
    // event), and the runtime's instruction trace is capped at 64, so nine owners is the
    // largest batch that can create its ATAs (ten fails with
    // MaxInstructionTraceLengthExceeded, measured). Eleven owners — the transaction-size
    // page the brief estimated — fits only when the ATAs already exist (three inner
    // instructions per owner). Both rows are benched; NOTES records the limit.
    let owners_9 = owners_of(9, 10);
    let mut ore_nine = ore_drawn.clone();
    for (i, o) in owners_9.iter().enumerate() {
        ore_nine.owners[i] = to_a(o);
    }
    ore_nine.creator_boxes = 0;
    let ore_9: Vec<_> = owners_9
        .iter()
        .map(|w| (*w, Some(ore_batch.ata(w))))
        .collect();
    let return_ore_9 = return_boxes_ix(&f.keeper, &ore_nine, &game, None, &ore_9, Some(ore_batch));
    let mut return_ore_9_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &ore_nine,
        &marked_record(GameStatus::Postponed),
        0,
        Some(25 * PRICE_ORE),
    );
    for (o, a) in &ore_9 {
        return_ore_9_accounts.push((*o, system_account(LAMPORTS_PER_SOL)));
        set_account(&mut return_ore_9_accounts, a.unwrap(), system_account(0));
    }
    let owners_11 = owners_of(11, 30);
    let mut ore_eleven = ore_drawn.clone();
    for (i, o) in owners_11.iter().enumerate() {
        ore_eleven.owners[i] = to_a(o);
    }
    ore_eleven.creator_boxes = 0;
    let ore_11: Vec<_> = owners_11
        .iter()
        .map(|w| (*w, Some(ore_batch.ata(w))))
        .collect();
    let return_ore_11 = return_boxes_ix(
        &f.keeper,
        &ore_eleven,
        &game,
        None,
        &ore_11,
        Some(ore_batch),
    );
    let mut return_ore_11_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &ore_eleven,
        &marked_record(GameStatus::Postponed),
        0,
        Some(25 * PRICE_ORE),
    );
    for (o, a) in &ore_11 {
        return_ore_11_accounts.push((*o, system_account(LAMPORTS_PER_SOL)));
        set_account(
            &mut return_ore_11_accounts,
            a.unwrap(),
            token_account(&f.ore_mint, o, 0),
        );
    }

    // return_sponsorship_sol / _ore_ata_missing: a Returned pool, every box paid, one open
    // sponsorship.
    let all_15: Vec<u8> = (0..15).collect();
    let returned_sol = common::sponsored(
        returned_partial(
            pool_with_owners(
                &f,
                &config,
                &sol_params(0),
                PoolStatus::Open,
                &plan_unfilled,
            ),
            &all_15,
        ),
        LAMPORTS_PER_SOL,
    );
    let return_sponsorship_sol =
        return_sponsorship_ix(&f.keeper, &returned_sol, &f.sponsor, None, None);
    let mut return_sponsorship_sol_accounts =
        returns_accounts(&f, &m, &config, &returned_sol, &moved_record(now), 0, None);
    let returned_sol_key = pool_pda(&game, &f.creator, returned_sol.nonce).0;
    set_account(
        &mut return_sponsorship_sol_accounts,
        sponsorship_pda(&returned_sol_key, &f.sponsor).0,
        sponsorship_account(&returned_sol_key, &f.sponsor, LAMPORTS_PER_SOL),
    );
    let all_25: Vec<u8> = (0..25).collect();
    let returned_ore = common::sponsored(
        returned_partial(
            pool_with_owners(&f, &config, &ore_params(0), PoolStatus::Drawn, &plan_full),
            &all_25,
        ),
        10 * PRICE_ORE,
    );
    let sponsor_one = SplOne::derived(&returned_ore, &f.sponsor);
    let return_sponsorship_ore = return_sponsorship_ix(
        &f.keeper,
        &returned_ore,
        &f.sponsor,
        None,
        Some(sponsor_one),
    );
    let mut return_sponsorship_ore_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &returned_ore,
        &marked_record(GameStatus::Postponed),
        0,
        Some(10 * PRICE_ORE),
    );
    let returned_ore_key = pool_pda(&game, &f.creator, returned_ore.nonce).0;
    set_account(
        &mut return_sponsorship_ore_accounts,
        sponsorship_pda(&returned_ore_key, &f.sponsor).0,
        sponsorship_account(&returned_ore_key, &f.sponsor, 10 * PRICE_ORE),
    );
    set_account(
        &mut return_sponsorship_ore_accounts,
        sponsor_one.ata,
        system_account(0),
    );

    // cancel_pool_open: the counter decremented.
    let cancel_open = cancel_pool_ix(&f.admin, &unfilled, Some(counter_key));
    let cancel_open_accounts =
        returns_accounts(&f, &m, &config, &unfilled, &standard_record(), 1, None);

    // split_first_sol_3_owners / split_ore_3_owners_atas_missing: after Q1 on a suspended game.
    let split_sol_pool = settled_pool(&f, &config, &sol_params(0), 0, 1, true);
    let split_first = split_ix(
        &f.keeper,
        &split_sol_pool,
        &game,
        &sol_batch(&[f.creator, f.buyer, f.buyer_2]),
        None,
    );
    let split_first_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &split_sol_pool,
        &suspended_record_with_quarters(1),
        0,
        None,
    );
    let split_ore_pool = settled_pool(&f, &config, &ore_params(0), 0, 1, true);
    let split_ore_batch = SplBatch::of(&split_ore_pool);
    let split_ore_3: Vec<_> = [f.creator, f.buyer, f.buyer_2]
        .iter()
        .map(|w| (*w, Some(split_ore_batch.ata(w))))
        .collect();
    let split_ore = split_ix(
        &f.keeper,
        &split_ore_pool,
        &game,
        &split_ore_3,
        Some(split_ore_batch),
    );
    let mut split_ore_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &split_ore_pool,
        &suspended_record_with_quarters(1),
        0,
        Some(split_ore_pool.unpaid_prize_pool),
    );
    for (_, a) in &split_ore_3 {
        set_account(&mut split_ore_accounts, a.unwrap(), system_account(0));
    }

    // reclaim_sol_open / reclaim_ore_locked_ata_missing / reclaim_sponsorship_sol: a record
    // whose scheduled kickoff is RECLAIM_DELAY in the past.
    let mut reclaimable = standard_record();
    reclaimable.scheduled_kickoff = now - RECLAIM_DELAY;
    reclaimable.recorded_kickoff = reclaimable.scheduled_kickoff;
    let reclaim_sol = reclaim_ix(&f.buyer, &unfilled, &game, Some(counter_key), None);
    let reclaim_sol_accounts = returns_accounts(&f, &m, &config, &unfilled, &reclaimable, 1, None);
    let ore_locked = pool_with_owners(&f, &config, &ore_params(0), PoolStatus::Locked, &plan_full);
    let buyer_one = SplOne::derived(&ore_locked, &f.buyer);
    let reclaim_ore = reclaim_ix(&f.buyer, &ore_locked, &game, None, Some(buyer_one));
    let mut reclaim_ore_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &ore_locked,
        &reclaimable,
        0,
        Some(25 * PRICE_ORE),
    );
    set_account(&mut reclaim_ore_accounts, buyer_one.ata, system_account(0));
    let sponsored_open = common::sponsored(unfilled.clone(), 50_000_000);
    let reclaim_sponsorship_sol =
        reclaim_sponsorship_ix(&f.sponsor, &sponsored_open, &game, Some(counter_key), None);
    let mut reclaim_sponsorship_sol_accounts =
        returns_accounts(&f, &m, &config, &sponsored_open, &reclaimable, 1, None);
    let sponsored_open_key = pool_pda(&game, &f.creator, sponsored_open.nonce).0;
    set_account(
        &mut reclaim_sponsorship_sol_accounts,
        sponsorship_pda(&sponsored_open_key, &f.sponsor).0,
        sponsorship_account(&sponsored_open_key, &f.sponsor, 50_000_000),
    );

    // close_sponsorship: on the Step 6 dust pool (Settled, one sponsorship open).
    let settled_sponsored = settled_pool(&f, &config, &sol_params(0), 1_000_000_003, 4, true);
    let close_sponsorship = close_sponsorship_ix(&settled_sponsored, &f.sponsor, None);
    let mut close_sponsorship_accounts = returns_accounts(
        &f,
        &m,
        &config,
        &settled_sponsored,
        &record_with_quarters(4),
        0,
        None,
    );
    let settled_sponsored_key = pool_pda(&game, &f.creator, settled_sponsored.nonce).0;
    set_account(
        &mut close_sponsorship_accounts,
        sponsorship_pda(&settled_sponsored_key, &f.sponsor).0,
        sponsorship_account(&settled_sponsored_key, &f.sponsor, 1_000_000_003),
    );

    MolluskComputeUnitBencher::new(m)
        .bench(("initialize", &initialize, &initialize_accounts))
        .bench(("update_config_full", &update_config_full, &update_accounts))
        .bench(("set_wallet_override_create", &set_create, &create_accounts))
        .bench((
            "set_wallet_override_update",
            &set_update,
            &existing_accounts,
        ))
        .bench(("close_wallet_override", &close, &existing_accounts))
        .bench(("create_game", &create_game, &create_game_accounts))
        .bench(("update_kickoff", &update_kickoff, &fresh_accounts))
        .bench(("post_scores_q1", &post_q1, &post_q1_accounts))
        .bench(("post_scores_final", &post_final, &post_final_accounts))
        .bench(("mark_game", &mark_game, &fresh_accounts))
        .bench(("create_pool_sol", &create_pool_sol, &create_sol_accounts))
        .bench((
            "create_pool_sol_5_boxes",
            &create_pool_sol_5,
            &create_sol_accounts,
        ))
        .bench(("create_pool_spl", &create_pool_spl, &create_spl_accounts))
        .bench(("buy_1", &buy_1, &open_accounts))
        .bench(("buy_3", &buy_3, &open_accounts))
        .bench(("buy_25th_locks", &buy_25th, &almost_accounts))
        .bench(("buy_spl_1", &buy_spl_1, &ore_accounts))
        .bench(("sponsor_new", &sponsor_new, &open_accounts))
        .bench(("sponsor_top_up", &sponsor_top_up, &top_up_accounts))
        .bench(("rotate_gate_key", &rotate, &link_accounts))
        .bench(("buy_link_sol", &buy_link, &buy_link_accounts))
        .bench(("buy_allowlist_sol_depth_1", &buy_allow_1, &allow_1_accounts))
        .bench((
            "buy_allowlist_sol_depth_10",
            &buy_allow_10,
            &allow_10_accounts,
        ))
        .bench((
            "buy_allowlist_sol_depth_32",
            &buy_allow_32,
            &allow_32_accounts,
        ))
        .bench(("close_counter", &close_counter, &close_counter_accounts))
        .bench(("set_var", &set_var, &set_var_accounts))
        .bench(("sample_var", &sample_var, &sample_var_accounts))
        .bench((
            "sample_var_already_sampled",
            &sample_var,
            &already_sampled_accounts,
        ))
        .bench(("draw", &draw, &draw_accounts_))
        .bench(("replace_var", &replace_var, &replace_var_accounts))
        .bench(("settle_q1_sol", &settle_q1, &settle_q1_accounts))
        .bench(("settle_q2_sol", &settle_q2, &settle_q2_accounts))
        .bench(("settle_q4_sol", &settle_q4, &settle_q4_accounts))
        .bench((
            "settle_q1_final_only",
            &settle_final_only,
            &settle_final_only_accounts,
        ))
        .bench((
            "settle_q1_sol_integrator",
            &settle_integrator,
            &settle_integrator_accounts,
        ))
        .bench(("settle_q1_ore", &settle_ore, &settle_ore_accounts))
        .bench((
            "settle_q1_ore_atas_exist",
            &settle_ore,
            &settle_ore_atas_exist_accounts,
        ))
        .bench(("close_pool_sol", &close_sol, &close_sol_accounts))
        .bench(("close_pool_ore", &close_ore, &close_ore_accounts))
        .bench((
            "return_boxes_first_sol_2_owners",
            &return_first,
            &return_first_accounts,
        ))
        .bench((
            "return_boxes_25_owners_sol",
            &return_25,
            &return_25_accounts,
        ))
        .bench((
            "return_boxes_ore_3_owners_atas_missing",
            &return_ore_3,
            &return_ore_3_accounts,
        ))
        .bench((
            "return_boxes_ore_9_owners_atas_missing",
            &return_ore_9,
            &return_ore_9_accounts,
        ))
        .bench((
            "return_boxes_ore_11_owners_atas_exist",
            &return_ore_11,
            &return_ore_11_accounts,
        ))
        .bench((
            "return_sponsorship_sol",
            &return_sponsorship_sol,
            &return_sponsorship_sol_accounts,
        ))
        .bench((
            "return_sponsorship_ore_ata_missing",
            &return_sponsorship_ore,
            &return_sponsorship_ore_accounts,
        ))
        .bench(("cancel_pool_open", &cancel_open, &cancel_open_accounts))
        .bench((
            "split_first_sol_3_owners",
            &split_first,
            &split_first_accounts,
        ))
        .bench((
            "split_ore_3_owners_atas_missing",
            &split_ore,
            &split_ore_accounts,
        ))
        .bench(("reclaim_sol_open", &reclaim_sol, &reclaim_sol_accounts))
        .bench((
            "reclaim_ore_locked_ata_missing",
            &reclaim_ore,
            &reclaim_ore_accounts,
        ))
        .bench((
            "reclaim_sponsorship_sol",
            &reclaim_sponsorship_sol,
            &reclaim_sponsorship_sol_accounts,
        ))
        .bench((
            "close_sponsorship",
            &close_sponsorship,
            &close_sponsorship_accounts,
        ))
        .must_pass(true)
        .out_dir(env!("CARGO_MANIFEST_DIR"))
        .execute();
}
