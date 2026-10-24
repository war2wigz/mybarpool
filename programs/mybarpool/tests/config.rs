//! Admin instructions, PROGRAM §4.1, under Mollusk. Every spec number cites
//! its line. Run after `anchor build --arch v3`.

mod common;

use anchor_spl::token_2022::spl_token_2022::extension::ExtensionType;
use anchor_spl::token_2022::spl_token_2022::state::AccountState;
use common::*;
use mollusk_svm::result::Check;
use mybarpool::constants::TOKEN_COUNT;
use mybarpool::{ConfigUpdated, MybarpoolError as E, OverrideClosed, OverrideSet, TokenRule};
use solana_account::Account;
use solana_pubkey::Pubkey;

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

#[test]
fn initialize_by_the_upgrade_authority_writes_every_field_and_emits_config_updated() {
    let f = Fixture::new();
    let m = mollusk();
    let ix = initialize_ix(&f.admin, f.initialize_params(), Some(&f.ore_mint));
    let result =
        m.process_and_validate_instruction(&ix, &base_accounts(&f, None), &[Check::success()]);

    let config = decode_config(account_of(&result, &config_pda().0));
    assert_eq!(config, f.expected_config());
    assert_eq!(config.admin, to_a(&f.admin)); // PROGRAM §4.1: the signer becomes admin
    assert_eq!(config.bump, config_pda().1);
    assert!(config.reserved.iter().all(|&b| b == 0));
    assert_eq!(account_of(&result, &config_pda().0).data.len(), 992); // PROGRAM §3.1 size

    let event: ConfigUpdated = emitted_event(&result).expect("ConfigUpdated self-CPI");
    assert_eq!(event.admin, to_a(&f.admin));
    assert_eq!(event.platform_bps, 500);
    assert_eq!(event.tokens, config.tokens);
    assert_eq!(emitted_event_count(&result), 1);
}

#[test]
fn initialize_by_a_signer_who_is_not_the_upgrade_authority_is_unauthorized() {
    let f = Fixture::new();
    let m = mollusk();
    let ix = initialize_ix(&f.keeper, f.initialize_params(), Some(&f.ore_mint));
    m.process_and_validate_instruction(
        &ix,
        &base_accounts(&f, None),
        &[Check::err(custom(err(E::Unauthorized)))],
    );
}

#[test]
fn initialize_with_program_data_at_the_wrong_address_fails() {
    let f = Fixture::new();
    let m = mollusk();
    let wrong = Pubkey::new_unique();
    let mut ix = initialize_ix(&f.admin, f.initialize_params(), Some(&f.ore_mint));
    ix.accounts[2].pubkey = wrong;
    let mut accounts = base_accounts(&f, None);
    accounts.push((wrong, program_data_account(Some(&f.admin))));
    let result = m.process_instruction(&ix, &accounts);
    assert!(
        !result.program_result.is_ok(),
        "seeds constraint must reject a foreign ProgramData"
    );
}

#[test]
fn initialize_twice_fails() {
    let f = Fixture::new();
    let m = mollusk();
    let ix = initialize_ix(&f.admin, f.initialize_params(), Some(&f.ore_mint));
    let first = m.process_instruction(&ix, &base_accounts(&f, None));
    assert!(first.program_result.is_ok());
    let second = m.process_instruction(&ix, &first.resulting_accounts);
    assert!(
        !second.program_result.is_ok(),
        "PROGRAM §3.1: one per deployment"
    );
}

// ---------------------------------------------------------------------------
// §3.1 invariants, each a negative test on initialize and on update_config
// ---------------------------------------------------------------------------

fn expect_initialize_error(mutate: impl Fn(&mut mybarpool::InitializeParams), code: u32) {
    let f = Fixture::new();
    let m = mollusk();
    let mut params = f.initialize_params();
    mutate(&mut params);
    let ix = initialize_ix(&f.admin, params, Some(&f.ore_mint));
    m.process_and_validate_instruction(&ix, &base_accounts(&f, None), &[Check::err(custom(code))]);
}

fn expect_update_error(mutate: impl Fn(&mut mybarpool::UpdateConfigParams), code: u32) {
    let f = Fixture::new();
    let m = mollusk();
    let mut params = no_update();
    mutate(&mut params);
    let ix = update_config_ix(&f.admin, params, None, Some(&f.ore_mint));
    m.process_and_validate_instruction(
        &ix,
        &base_accounts(&f, Some(&f.expected_config())),
        &[Check::err(custom(code))],
    );
}

#[test]
fn invariant_sum_above_1500_is_invalid_config() {
    // PROGRAM §3.1: platform_bps + creator_bps + addon_budget_bps ≤ TOTAL_BPS_MAX (1500).
    // Each cap is 500, so the only way over the sum is over a cap; 501 + 500 + 500 = 1501.
    expect_initialize_error(|p| p.platform_bps = 501, err(E::InvalidConfig));
    expect_update_error(|p| p.platform_bps = Some(501), err(E::InvalidConfig));
}

#[test]
fn invariant_each_bps_above_its_constant_is_invalid_config() {
    // PROGRAM §1: PLATFORM_BPS_MAX, CREATOR_BPS_MAX, ADDON_BUDGET_BPS_MAX = 500.
    expect_initialize_error(
        |p| {
            p.platform_bps = 501;
            p.creator_bps = 0;
        },
        err(E::InvalidConfig),
    );
    expect_initialize_error(
        |p| {
            p.creator_bps = 501;
            p.platform_bps = 0;
        },
        err(E::InvalidConfig),
    );
    expect_initialize_error(
        |p| {
            p.addon_budget_bps = 501;
            p.platform_bps = 0;
        },
        err(E::InvalidConfig),
    );
    expect_update_error(
        |p| {
            p.platform_bps = Some(501);
            p.creator_bps = Some(0);
        },
        err(E::InvalidConfig),
    );
    expect_update_error(
        |p| {
            p.creator_bps = Some(501);
            p.platform_bps = Some(0);
        },
        err(E::InvalidConfig),
    );
    expect_update_error(
        |p| {
            p.addon_budget_bps = Some(501);
            p.platform_bps = Some(0);
        },
        err(E::InvalidConfig),
    );
}

#[test]
fn invariant_ladder_violations_are_invalid_config() {
    // PROGRAM §3.1: min_price ≥ 1; step ≥ 1; min_price ≤ max_price; (max − min) % step == 0.
    let sol =
        |f: fn(&mut TokenRule)| move |p: &mut mybarpool::InitializeParams| f(&mut p.tokens[0]);
    expect_initialize_error(sol(|r| r.min_price = 0), err(E::InvalidConfig));
    expect_initialize_error(sol(|r| r.step = 0), err(E::InvalidConfig));
    expect_initialize_error(
        sol(|r| {
            r.min_price = 2_000_000_000;
        }),
        err(E::InvalidConfig),
    ); // > max 1 SOL
    expect_initialize_error(sol(|r| r.step = 30_000_000), err(E::InvalidConfig)); // 0.95 SOL span not a multiple

    let upd = |f: fn(&mut TokenRule)| {
        move |p: &mut mybarpool::UpdateConfigParams| {
            let mut r = Fixture::sol_rule();
            f(&mut r);
            p.tokens[0] = Some(r);
        }
    };
    expect_update_error(upd(|r| r.min_price = 0), err(E::InvalidConfig));
    expect_update_error(upd(|r| r.step = 0), err(E::InvalidConfig));
    expect_update_error(upd(|r| r.min_price = 2_000_000_000), err(E::InvalidConfig));
    expect_update_error(upd(|r| r.step = 30_000_000), err(E::InvalidConfig));
}

#[test]
fn invariant_creator_limits_are_invalid_config() {
    // PROGRAM §3.1: max_open_pools ≥ 1; 1 ≤ max_own_boxes ≤ MAX_OWN_BOXES_ABSOLUTE (25).
    expect_initialize_error(|p| p.max_open_pools = 0, err(E::InvalidConfig));
    expect_initialize_error(|p| p.max_own_boxes = 0, err(E::InvalidConfig));
    expect_initialize_error(|p| p.max_own_boxes = 26, err(E::InvalidConfig));
    expect_update_error(|p| p.max_open_pools = Some(0), err(E::InvalidConfig));
    expect_update_error(|p| p.max_own_boxes = Some(0), err(E::InvalidConfig));
    expect_update_error(|p| p.max_own_boxes = Some(26), err(E::InvalidConfig));
}

#[test]
fn default_preset_outside_the_three_is_invalid_preset() {
    // PROGRAM §1: presets 0, 1, 2; anything else rejected.
    expect_initialize_error(|p| p.default_preset = 3, err(E::InvalidPreset));
    expect_initialize_error(|p| p.default_preset = 255, err(E::InvalidPreset));
    expect_update_error(|p| p.default_preset = Some(3), err(E::InvalidPreset));
    for preset in 0..=2u8 {
        let f = Fixture::new();
        let m = mollusk();
        let mut params = f.initialize_params();
        params.default_preset = preset;
        let ix = initialize_ix(&f.admin, params, Some(&f.ore_mint));
        m.process_and_validate_instruction(&ix, &base_accounts(&f, None), &[Check::success()]);
    }
}

#[test]
fn admin_may_not_be_the_default_pubkey() {
    expect_update_error(
        |p| p.admin = Some(anchor_lang::prelude::Pubkey::default()),
        err(E::InvalidConfig),
    );
}

#[test]
fn boundary_values_are_accepted() {
    let f = Fixture::new();
    let m = mollusk();
    let mut params = f.initialize_params();
    // PROGRAM §3.1: 500 / 500 / 500 sums to exactly TOTAL_BPS_MAX; min == max with any step; limits 1 and 25.
    params.platform_bps = 500;
    params.creator_bps = 500;
    params.addon_budget_bps = 500;
    params.tokens[0].min_price = 1_000_000_000;
    params.tokens[0].max_price = 1_000_000_000;
    params.tokens[0].step = 7;
    params.max_open_pools = 1;
    params.max_own_boxes = 25;
    let ix = initialize_ix(&f.admin, params, Some(&f.ore_mint));
    m.process_and_validate_instruction(&ix, &base_accounts(&f, None), &[Check::success()]);

    let mut params = f.initialize_params();
    params.max_own_boxes = 1;
    let ix = initialize_ix(&f.admin, params, Some(&f.ore_mint));
    m.process_and_validate_instruction(&ix, &base_accounts(&f, None), &[Check::success()]);
}

// ---------------------------------------------------------------------------
// update_config
// ---------------------------------------------------------------------------

#[test]
fn update_config_lowers_then_raises_platform_bps_within_the_constant() {
    // PROGRAM §4.1: bps may go down or up but never above the constants.
    let f = Fixture::new();
    let m = mollusk();
    let accounts = base_accounts(&f, Some(&f.expected_config()));
    let lower = update_config_ix(
        &f.admin,
        mybarpool::UpdateConfigParams {
            platform_bps: Some(400),
            ..no_update()
        },
        None,
        None,
    );
    let r1 = m.process_and_validate_instruction(&lower, &accounts, &[Check::success()]);
    assert_eq!(
        decode_config(account_of(&r1, &config_pda().0)).platform_bps,
        400
    );
    let raise = update_config_ix(
        &f.admin,
        mybarpool::UpdateConfigParams {
            platform_bps: Some(500),
            ..no_update()
        },
        None,
        None,
    );
    let r2 =
        m.process_and_validate_instruction(&raise, &r1.resulting_accounts, &[Check::success()]);
    assert_eq!(
        decode_config(account_of(&r2, &config_pda().0)).platform_bps,
        500
    );
}

#[test]
fn update_config_by_the_keeper_or_a_stranger_is_unauthorized_and_changes_nothing() {
    // ARCHITECTURE › Trust model: only the admin updates config.
    let f = Fixture::new();
    let m = mollusk();
    let config = f.expected_config();
    let mut accounts = base_accounts(&f, Some(&config));
    let stranger = Pubkey::new_unique();
    accounts.push((stranger, system_account(LAMPORTS_PER_SOL)));
    for signer in [f.keeper, stranger] {
        let ix = update_config_ix(
            &signer,
            mybarpool::UpdateConfigParams {
                paused: Some(true),
                ..no_update()
            },
            None,
            None,
        );
        let r = m.process_and_validate_instruction(
            &ix,
            &accounts,
            &[Check::err(custom(err(E::Unauthorized)))],
        );
        assert_eq!(decode_config(account_of(&r, &config_pda().0)), config);
    }
}

#[test]
fn update_config_hands_admin_to_a_new_key() {
    let f = Fixture::new();
    let m = mollusk();
    let new_admin = Pubkey::new_unique();
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((new_admin, system_account(LAMPORTS_PER_SOL)));
    let hand_over = update_config_ix(
        &f.admin,
        mybarpool::UpdateConfigParams {
            admin: Some(to_a(&new_admin)),
            ..no_update()
        },
        None,
        None,
    );
    let r1 = m.process_and_validate_instruction(&hand_over, &accounts, &[Check::success()]);
    let old_again = update_config_ix(
        &f.admin,
        mybarpool::UpdateConfigParams {
            paused: Some(true),
            ..no_update()
        },
        None,
        None,
    );
    m.process_and_validate_instruction(
        &old_again,
        &r1.resulting_accounts,
        &[Check::err(custom(err(E::Unauthorized)))],
    );
    let new_ok = update_config_ix(
        &new_admin,
        mybarpool::UpdateConfigParams {
            paused: Some(true),
            ..no_update()
        },
        None,
        None,
    );
    let r2 =
        m.process_and_validate_instruction(&new_ok, &r1.resulting_accounts, &[Check::success()]);
    assert!(decode_config(account_of(&r2, &config_pda().0)).paused);
}

#[test]
fn update_config_with_all_none_changes_nothing_and_still_emits() {
    let f = Fixture::new();
    let m = mollusk();
    let config = f.expected_config();
    let ix = update_config_ix(&f.admin, no_update(), None, None);
    let r = m.process_and_validate_instruction(
        &ix,
        &base_accounts(&f, Some(&config)),
        &[Check::success()],
    );
    assert_eq!(decode_config(account_of(&r, &config_pda().0)), config);
    let event: ConfigUpdated = emitted_event(&r).expect("ConfigUpdated");
    assert_eq!(event.paused, false);
    assert_eq!(event.tokens, config.tokens);
}

#[test]
fn update_config_changes_each_scalar_field_alone() {
    let f = Fixture::new();
    let m = mollusk();
    let base = f.expected_config();
    let accounts = base_accounts(&f, Some(&base));
    let k = to_a(&Pubkey::new_unique());

    let cases: Vec<(
        mybarpool::UpdateConfigParams,
        Box<dyn Fn(&mybarpool::PlatformConfig) -> bool>,
    )> = vec![
        (
            mybarpool::UpdateConfigParams {
                paused: Some(true),
                ..no_update()
            },
            Box::new(|c| c.paused),
        ),
        (
            mybarpool::UpdateConfigParams {
                preseason_enabled: Some(true),
                ..no_update()
            },
            Box::new(|c| c.preseason_enabled),
        ),
        (
            mybarpool::UpdateConfigParams {
                entropy_provider: Some(k),
                ..no_update()
            },
            Box::new(move |c| c.entropy_provider == k),
        ),
        (
            mybarpool::UpdateConfigParams {
                score_authority: Some(k),
                ..no_update()
            },
            Box::new(move |c| c.score_authority == k),
        ),
        (
            mybarpool::UpdateConfigParams {
                fee_wallet: Some(k),
                ..no_update()
            },
            Box::new(move |c| c.fee_wallet == k),
        ),
        (
            mybarpool::UpdateConfigParams {
                default_preset: Some(2),
                ..no_update()
            },
            Box::new(|c| c.default_preset == 2),
        ),
        (
            mybarpool::UpdateConfigParams {
                max_open_pools: Some(10),
                ..no_update()
            },
            Box::new(|c| c.max_open_pools == 10),
        ),
        (
            mybarpool::UpdateConfigParams {
                max_own_boxes: Some(25),
                ..no_update()
            },
            Box::new(|c| c.max_own_boxes == 25),
        ),
    ];
    for (params, changed) in cases {
        let ix = update_config_ix(&f.admin, params, None, None);
        let r = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
        let after = decode_config(account_of(&r, &config_pda().0));
        assert!(changed(&after));
        // Everything else is untouched: reset the changed field class and compare.
        let mut probe = after.clone();
        probe.paused = base.paused;
        probe.preseason_enabled = base.preseason_enabled;
        probe.entropy_provider = base.entropy_provider;
        probe.score_authority = base.score_authority;
        probe.fee_wallet = base.fee_wallet;
        probe.default_preset = base.default_preset;
        probe.max_open_pools = base.max_open_pools;
        probe.max_own_boxes = base.max_own_boxes;
        assert_eq!(probe, base);
    }
}

// ---------------------------------------------------------------------------
// Token rules and mints
// ---------------------------------------------------------------------------

#[test]
fn token_rule_with_a_mint_but_no_mint_account_is_invalid_config() {
    let f = Fixture::new();
    let m = mollusk();
    let ix = initialize_ix(&f.admin, f.initialize_params(), None);
    m.process_and_validate_instruction(
        &ix,
        &base_accounts(&f, None),
        &[Check::err(custom(err(E::InvalidConfig)))],
    );
}

#[test]
fn token_rule_mint_mismatches_are_invalid_config() {
    let f = Fixture::new();
    let m = mollusk();
    // Mint key differs from the rule.
    let other = Pubkey::new_unique();
    let mut accounts = base_accounts(&f, None);
    accounts.push((other, legacy_mint_account(11)));
    let ix = initialize_ix(&f.admin, f.initialize_params(), Some(&other));
    m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[Check::err(custom(err(E::InvalidConfig)))],
    );

    // Owner differs from rule.token_program (rule says Token-2022, account is classic Token).
    let mut params = f.initialize_params();
    params.tokens[2].token_program = anchor_spl::token_2022::ID;
    let ix = initialize_ix(&f.admin, params, Some(&f.ore_mint));
    m.process_and_validate_instruction(
        &ix,
        &base_accounts(&f, None),
        &[Check::err(custom(err(E::InvalidConfig)))],
    );

    // Decimals differ.
    let mut params = f.initialize_params();
    params.tokens[2].decimals = 9;
    let ix = initialize_ix(&f.admin, params, Some(&f.ore_mint));
    m.process_and_validate_instruction(
        &ix,
        &base_accounts(&f, None),
        &[Check::err(custom(err(E::InvalidConfig)))],
    );
}

/// The six extensions that can fail, redirect or freeze a transfer (PROGRAM §5.4).
const REFUSED: [ExtensionType; 6] = [
    ExtensionType::TransferFeeConfig,
    ExtensionType::TransferHook,
    ExtensionType::PermanentDelegate,
    ExtensionType::NonTransferable,
    ExtensionType::DefaultAccountState,
    ExtensionType::Pausable,
];

/// A Token-2022 rule in the ORE slot pointing at `mint` (the `initialize` path carries
/// `mint_2`), and the same rule as an `update_config` of the SKR slot.
fn rule_2022(f: &Fixture, mint: &Pubkey) -> TokenRule {
    let mut rule = f.skr_rule_2022();
    rule.mint = to_a(mint);
    rule
}

fn expect_mint(
    m: &mollusk_svm::Mollusk,
    f: &Fixture,
    mint_account: Account,
    expected: Option<u32>,
) {
    let mint = Pubkey::new_unique();
    let check = || match expected {
        Some(code) => Check::err(custom(code)),
        None => Check::success(),
    };
    // initialize: the rule in slot 2 with `mint_2`.
    let mut params = f.initialize_params();
    params.tokens[2] = rule_2022(f, &mint);
    let mut accounts = base_accounts(f, None);
    accounts.push((mint, mint_account.clone()));
    m.process_and_validate_instruction(
        &initialize_ix(&f.admin, params, Some(&mint)),
        &accounts,
        &[check()],
    );
    // update_config: the rule in slot 1 with `mint_1`.
    let mut params = no_update();
    params.tokens[1] = Some(rule_2022(f, &mint));
    let mut accounts = base_accounts(f, Some(&f.expected_config()));
    accounts.push((mint, mint_account));
    m.process_and_validate_instruction(
        &update_config_ix(&f.admin, params, Some(&mint), None),
        &accounts,
        &[check()],
    );
}

#[test]
fn token_2022_extensions_that_break_transfers_are_unsupported_the_rest_are_fine() {
    // PROGRAM §5.4: transfer fees, transfer hooks, a permanent delegate, non-transferable,
    // a default account state and pausable are refused — each alone, at `initialize` and at
    // `update_config`. A metadata pointer and a bare mint are fine.
    let f = Fixture::new();
    let m = mollusk();
    for ext in REFUSED {
        expect_mint(
            &m,
            &f,
            token_2022_mint_account(6, &[ext]),
            Some(err(E::UnsupportedMintExtension)),
        );
    }
    expect_mint(
        &m,
        &f,
        token_2022_mint_account(6, &[ExtensionType::MetadataPointer]),
        None,
    );
    expect_mint(&m, &f, token_2022_mint_account(6, &[]), None);
    // Refused even when it rides along with an accepted extension, in any position.
    expect_mint(
        &m,
        &f,
        token_2022_mint_account(
            6,
            &[ExtensionType::MetadataPointer, ExtensionType::Pausable],
        ),
        Some(err(E::UnsupportedMintExtension)),
    );
}

#[test]
fn token_2022_refused_extension_is_refused_whatever_its_value() {
    // PROGRAM §5.4: the check is on the extension's presence, not its current value — a
    // zero fee, an unset delegate, `Initialized` as the default state and an unpaused mint
    // are all one authority action away from breaking a payout.
    use anchor_spl::token_2022::spl_token_2022::extension::{
        default_account_state::DefaultAccountState, pausable::PausableConfig,
        permanent_delegate::PermanentDelegate, transfer_fee::TransferFeeConfig,
        BaseStateWithExtensionsMut, StateWithExtensionsMut,
    };
    use anchor_spl::token_2022::spl_token_2022::state::Mint as Mint2022;
    let f = Fixture::new();
    let m = mollusk();

    let mut fee = token_2022_mint_account(6, &[ExtensionType::TransferFeeConfig]);
    {
        let mut state = StateWithExtensionsMut::<Mint2022>::unpack(&mut fee.data).unwrap();
        let ext = state.get_extension_mut::<TransferFeeConfig>().unwrap();
        ext.newer_transfer_fee.transfer_fee_basis_points = 0.into();
        ext.newer_transfer_fee.maximum_fee = 0.into();
        ext.older_transfer_fee.transfer_fee_basis_points = 0.into();
        ext.older_transfer_fee.maximum_fee = 0.into();
    }
    let mut delegate = token_2022_mint_account(6, &[ExtensionType::PermanentDelegate]);
    {
        let mut state = StateWithExtensionsMut::<Mint2022>::unpack(&mut delegate.data).unwrap();
        let ext = state.get_extension_mut::<PermanentDelegate>().unwrap();
        ext.delegate = None.try_into().unwrap();
    }
    let mut initialized = token_2022_mint_account(6, &[ExtensionType::DefaultAccountState]);
    {
        let mut state = StateWithExtensionsMut::<Mint2022>::unpack(&mut initialized.data).unwrap();
        let ext = state.get_extension_mut::<DefaultAccountState>().unwrap();
        ext.state = AccountState::Initialized.into();
    }
    let mut unpaused = token_2022_mint_account(6, &[ExtensionType::Pausable]);
    {
        let mut state = StateWithExtensionsMut::<Mint2022>::unpack(&mut unpaused.data).unwrap();
        let ext = state.get_extension_mut::<PausableConfig>().unwrap();
        ext.paused = false.into();
        ext.authority = Some(f.admin.to_bytes().into()).try_into().unwrap();
    }
    for account in [fee, delegate, initialized, unpaused] {
        expect_mint(&m, &f, account, Some(err(E::UnsupportedMintExtension)));
    }
}

#[test]
fn token_shape_rules() {
    // PROGRAM §2 "Token index": 0 is native SOL; §3.1 "Pubkey::default() for SOL".
    expect_initialize_error(
        |p| p.tokens[0].mint = anchor_spl::token::ID,
        err(E::InvalidConfig),
    );
    expect_initialize_error(|p| p.tokens[0].decimals = 6, err(E::InvalidConfig));
    expect_initialize_error(
        |p| p.tokens[0].token_program = anchor_spl::token::ID,
        err(E::InvalidConfig),
    );
    // A default-mint rule at index 1 must be disabled (placeholder) and have no program.
    expect_initialize_error(|p| p.tokens[1].enabled = true, err(E::InvalidConfig));
    expect_initialize_error(
        |p| p.tokens[1].token_program = anchor_spl::token::ID,
        err(E::InvalidConfig),
    );
    // The spare slots (3–5) follow the same placeholder rule.
    for i in 3..TOKEN_COUNT {
        expect_initialize_error(move |p| p.tokens[i].enabled = true, err(E::InvalidConfig));
        expect_initialize_error(
            move |p| p.tokens[i].token_program = anchor_spl::token_2022::ID,
            err(E::InvalidConfig),
        );
        expect_update_error(
            move |p| {
                let mut rule = TokenRule::PLACEHOLDER;
                rule.enabled = true;
                p.tokens[i] = Some(rule);
            },
            err(E::InvalidConfig),
        );
    }
}

#[test]
fn a_spare_slot_is_filled_by_update_config_with_its_mint() {
    // PROGRAM §2 "Token index": slots 3–5 ship as disabled placeholders so a token is added
    // by `update_config` (admin) rather than by an upgrade. The rule's mint rides in the
    // matching `mint_N` account and is checked like SKR's and ORE's; without it the
    // instruction is refused, and the other slots are untouched.
    let f = Fixture::new();
    let m = mollusk();
    let mint = Pubkey::new_unique();
    let mint_account = token_2022_mint_account(6, &[ExtensionType::MetadataPointer]);
    for i in 3..TOKEN_COUNT {
        let mut params = no_update();
        params.tokens[i] = Some(rule_2022(&f, &mint));

        // Without the mint account.
        m.process_and_validate_instruction(
            &update_config_ix(&f.admin, params.clone(), None, None),
            &base_accounts(&f, Some(&f.expected_config())),
            &[Check::err(custom(err(E::InvalidConfig)))],
        );

        // With it, in the slot's own position.
        let mut mints: Mints = [None; TOKEN_COUNT];
        mints[i] = Some(&mint);
        let mut accounts = base_accounts(&f, Some(&f.expected_config()));
        accounts.push((mint, mint_account.clone()));
        let result = m.process_and_validate_instruction(
            &update_config_ix_with_mints(&f.admin, params, mints),
            &accounts,
            &[Check::success()],
        );
        let config = decode_config(account_of(&result, &config_pda().0));
        let mut expected = f.expected_config();
        expected.tokens[i] = rule_2022(&f, &mint);
        assert_eq!(config, expected);
        let event: ConfigUpdated = emitted_event(&result).expect("ConfigUpdated self-CPI");
        assert_eq!(event.tokens, expected.tokens);

        // The mint in a different slot's position does not count.
        let mut elsewhere: Mints = [None; TOKEN_COUNT];
        elsewhere[if i == 3 { 4 } else { 3 }] = Some(&mint);
        let mut params = no_update();
        params.tokens[i] = Some(rule_2022(&f, &mint));
        m.process_and_validate_instruction(
            &update_config_ix_with_mints(&f.admin, params, elsewhere),
            &accounts,
            &[Check::err(custom(err(E::InvalidConfig)))],
        );
    }

    // `initialize` accepts a filled spare slot the same way.
    let mut params = f.initialize_params();
    params.tokens[5] = rule_2022(&f, &mint);
    let mut accounts = base_accounts(&f, None);
    accounts.push((mint, mint_account));
    let result = m.process_and_validate_instruction(
        &initialize_ix_with_mints(
            &f.admin,
            params,
            [None, None, Some(&f.ore_mint), None, None, Some(&mint)],
        ),
        &accounts,
        &[Check::success()],
    );
    let config = decode_config(account_of(&result, &config_pda().0));
    assert_eq!(config.tokens[5], rule_2022(&f, &mint));
    assert_eq!(config.tokens[2], f.ore_rule());
}

// ---------------------------------------------------------------------------
// Wallet overrides
// ---------------------------------------------------------------------------

#[test]
fn set_wallet_override_creates_then_updates_in_place_and_rejects_the_keeper() {
    let f = Fixture::new();
    let m = mollusk();
    let wallet = Pubkey::new_unique();
    let (pda, bump) = override_pda(&wallet);
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((pda, system_account(0)));

    let create = set_override_ix(&f.admin, &wallet, 10, 5);
    let r1 = m.process_and_validate_instruction(&create, &accounts, &[Check::success()]);
    let created = account_of(&r1, &pda);
    assert_eq!(created.data.len(), 43); // PROGRAM §3.6 size
    let value = decode_override(created);
    assert_eq!(value.wallet, to_a(&wallet));
    assert_eq!(
        (value.max_open_pools, value.max_own_boxes, value.bump),
        (10, 5, bump)
    );
    let event: OverrideSet = emitted_event(&r1).expect("OverrideSet");
    assert_eq!(
        (event.wallet, event.max_open_pools, event.max_own_boxes),
        (to_a(&wallet), 10, 5)
    );
    let rent_paid = created.lamports;

    let update = set_override_ix(&f.admin, &wallet, 2, 25);
    let r2 =
        m.process_and_validate_instruction(&update, &r1.resulting_accounts, &[Check::success()]);
    let updated = account_of(&r2, &pda);
    let value = decode_override(updated);
    assert_eq!(
        (value.max_open_pools, value.max_own_boxes, value.bump),
        (2, 25, bump)
    );
    assert_eq!(updated.lamports, rent_paid, "no second rent on update");

    let by_keeper = set_override_ix(&f.keeper, &wallet, 1, 1);
    m.process_and_validate_instruction(
        &by_keeper,
        &r2.resulting_accounts,
        &[Check::err(custom(err(E::Unauthorized)))],
    );
}

#[test]
fn set_wallet_override_rejects_zero_and_above_absolute_limits() {
    // PROGRAM §3.6: max_own_boxes ≤ MAX_OWN_BOXES_ABSOLUTE; both limits ≥ 1 (NOTES).
    let f = Fixture::new();
    let m = mollusk();
    let wallet = Pubkey::new_unique();
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((override_pda(&wallet).0, system_account(0)));
    for (open, own) in [(0u8, 5u8), (3, 0), (3, 26)] {
        let ix = set_override_ix(&f.admin, &wallet, open, own);
        m.process_and_validate_instruction(
            &ix,
            &accounts,
            &[Check::err(custom(err(E::InvalidConfig)))],
        );
    }
}

#[test]
fn set_wallet_override_for_the_zero_key_is_invalid_config() {
    // Step 2 audit L1: `wallet == Pubkey::default()` is the "just created" sentinel in the
    // handler, so an override for the zero key is refused before the account is touched.
    let f = Fixture::new();
    let m = mollusk();
    let zero = Pubkey::default();
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((override_pda(&zero).0, system_account(0)));
    let ix = set_override_ix(&f.admin, &zero, 3, 5);
    let result = m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[Check::err(custom(err(E::InvalidConfig)))],
    );
    assert_eq!(emitted_event_count(&result), 0);
}

#[test]
fn close_wallet_override_returns_rent_to_admin_and_emits_the_closed_values() {
    let f = Fixture::new();
    let m = mollusk();
    let wallet = Pubkey::new_unique();
    let (pda, _) = override_pda(&wallet);
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((pda, system_account(0)));
    let r1 = m.process_and_validate_instruction(
        &set_override_ix(&f.admin, &wallet, 10, 5),
        &accounts,
        &[Check::success()],
    );
    let rent = account_of(&r1, &pda).lamports;
    let admin_before = account_of(&r1, &f.admin).lamports;

    let r2 = m.process_and_validate_instruction(
        &close_override_ix(&f.admin, &wallet),
        &r1.resulting_accounts,
        &[Check::success()],
    );
    let closed = account_of(&r2, &pda);
    assert_eq!(closed.lamports, 0);
    assert!(closed.data.is_empty() || closed.data.iter().all(|&b| b == 0));
    assert_eq!(account_of(&r2, &f.admin).lamports, admin_before + rent); // PROGRAM §4.1: rent to admin
    let event: OverrideClosed = emitted_event(&r2).expect("OverrideClosed");
    assert_eq!(
        (event.wallet, event.max_open_pools, event.max_own_boxes),
        (to_a(&wallet), 10, 5)
    );

    // By the keeper: Unauthorized. With a mismatched wallet param: fails.
    m.process_and_validate_instruction(
        &close_override_ix(&f.keeper, &wallet),
        &r1.resulting_accounts,
        &[Check::err(custom(err(E::Unauthorized)))],
    );
    let other = Pubkey::new_unique();
    let mut ix = close_override_ix(&f.admin, &other);
    ix.accounts[2].pubkey = pda; // point at the real account with the wrong wallet param
    let r = m.process_instruction(&ix, &r1.resulting_accounts);
    assert!(!r.program_result.is_ok());
}

// ---------------------------------------------------------------------------
// Composability (PROGRAM §10)
// ---------------------------------------------------------------------------

#[test]
fn update_config_and_set_override_are_unaffected_by_neighbouring_instructions() {
    let f = Fixture::new();
    let m = mollusk();
    let wallet = Pubkey::new_unique();
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((override_pda(&wallet).0, system_account(0)));

    // A harmless system transfer stands in for a wallet-appended instruction (memo, Lighthouse).
    let noop = solana_instruction::Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], // SystemInstruction::Transfer { lamports: 1 }
        vec![
            solana_instruction::AccountMeta::new(f.admin, true),
            solana_instruction::AccountMeta::new(f.keeper, false),
        ],
    );
    let update = update_config_ix(
        &f.admin,
        mybarpool::UpdateConfigParams {
            max_open_pools: Some(7),
            ..no_update()
        },
        None,
        None,
    );
    let set = set_override_ix(&f.admin, &wallet, 9, 9);

    let alone = m.process_instruction_chain(&[update.clone(), set.clone()], &accounts);
    let sandwiched =
        m.process_instruction_chain(&[noop.clone(), update, noop.clone(), set, noop], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_config(account_of(&alone, &config_pda().0)),
        decode_config(account_of(&sandwiched, &config_pda().0))
    );
    assert_eq!(
        decode_override(account_of(&alone, &override_pda(&wallet).0)),
        decode_override(account_of(&sandwiched, &override_pda(&wallet).0))
    );
}

#[test]
fn event_helpers_ignore_inner_instructions_that_are_not_our_event_cpi() {
    // Step 2 audit L2: `emitted_event` / `emitted_event_count` match the program id and the
    // event-CPI tag, so system and (from Step 4) token CPIs are never counted as events.
    let f = Fixture::new();
    let m = mollusk();
    let accounts = base_accounts(&f, Some(&f.expected_config()));
    let transfer = solana_instruction::Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], // SystemInstruction::Transfer { lamports: 1 }
        vec![
            solana_instruction::AccountMeta::new(f.admin, true),
            solana_instruction::AccountMeta::new(f.keeper, false),
        ],
    );
    let result = m.process_instruction_chain(&[transfer.clone(), transfer], &accounts);
    assert!(result.program_result.is_ok());
    assert_eq!(emitted_event_count(&result), 0);
    assert!(emitted_event::<ConfigUpdated>(&result).is_none());
    assert!(emitted_event::<OverrideSet>(&result).is_none());

    // And the helpers still find every Step 2 event: `set_wallet_override` creates the PDA
    // (a system-program CPI) and emits exactly one event.
    let wallet = Pubkey::new_unique();
    let mut accounts = accounts;
    accounts.push((override_pda(&wallet).0, system_account(0)));
    let result = m.process_and_validate_instruction(
        &set_override_ix(&f.admin, &wallet, 3, 5),
        &accounts,
        &[Check::success()],
    );
    assert!(
        result.inner_instructions.len() >= 2,
        "system CPI + event CPI"
    );
    assert_eq!(emitted_event_count(&result), 1);
    assert!(emitted_event::<OverrideSet>(&result).is_some());
}

fn custom(code: u32) -> solana_program_error::ProgramError {
    solana_program_error::ProgramError::Custom(code)
}
