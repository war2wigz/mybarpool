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
    let wallet = solana_pubkey::Pubkey::new_unique();
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

    MolluskComputeUnitBencher::new(mollusk())
        .bench(("initialize", &initialize, &initialize_accounts))
        .bench(("update_config_full", &update_config_full, &update_accounts))
        .bench(("set_wallet_override_create", &set_create, &create_accounts))
        .bench((
            "set_wallet_override_update",
            &set_update,
            &existing_accounts,
        ))
        .bench(("close_wallet_override", &close, &existing_accounts))
        .must_pass(true)
        .out_dir(env!("CARGO_MANIFEST_DIR"))
        .execute();
}
