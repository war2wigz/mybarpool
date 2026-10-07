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
    let m = mollusk_for_pools(T0, SLOT_HASH);
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

    let close_counter = close_counter_ix(&f.creator, &standard_game(), &f.fee_wallet);
    let mut close_counter_accounts = base_accounts(&f, Some(&config));
    close_counter_accounts.push((
        counter_pda(&f.creator, &standard_game()).0,
        counter_account(&f.creator, &standard_game(), 0),
    ));
    close_counter_accounts.push((f.fee_wallet, system_account(LAMPORTS_PER_SOL)));

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
        .bench(("close_counter", &close_counter, &close_counter_accounts))
        .must_pass(true)
        .out_dir(env!("CARGO_MANIFEST_DIR"))
        .execute();
}
