//! `settle` and `close_pool`, PROGRAM §4.5, under Mollusk with the Token,
//! Token-2022 and Associated Token programs loaded. Every spec number cites
//! its line. Run after `anchor build --arch v3`.
//!
//! The standard drawn pool (`sol_params`: PRICE 0.05 SOL, Standard, 2 % add-on, no
//! integrator; config 500 / 500): P 1,250,000,000; fees 62,500,000 / 87,500,000 / 0;
//! prize_pool 1,100,000,000; quarters 220,000,000 × 3, 440,000,000 (ARCHITECTURE › Fees
//! worked example). Identity axes with the Step 3 scores: Q1 box 17 (buyer_2), Q2 box 4
//! (creator), Q3 box 12 (buyer), Q4 box 4 (creator).

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::Check;
use mybarpool::constants::NO_WINNING_BOX;
use mybarpool::{
    CreatePoolParams, MybarpoolError as E, PayoutPreset, Pool, PoolClosed, PoolStatus,
    QuarterSettled,
};
use solana_pubkey::Pubkey;

fn custom(code: u32) -> solana_program_error::ProgramError {
    solana_program_error::ProgramError::Custom(code)
}

fn anchor(code: ErrorCode) -> u32 {
    code as u32
}

const PRIZE_POOL: u64 = 1_100_000_000; // ARCHITECTURE › Fees: 1.1 SOL
const Q: [u64; 4] = [220_000_000, 220_000_000, 220_000_000, 440_000_000]; // 0.22 ×3, 0.44
const PLATFORM_FEE: u64 = 62_500_000; // 0.0625
const CREATOR_FEE: u64 = 87_500_000; // 0.0875 = 0.0625 base + 0.025 add-on
const SPONSORED: u64 = 1_000_000_000; // the 1 SOL sponsorship variant
/// Identity axes: 7–3 → 17, 14–10 → 4, 17–17 → 12, 24–20 → 4 (this brief).
const BOXES_WON: [u8; 4] = [17, 4, 12, 4];

fn winner_of(f: &Fixture, q: usize) -> Pubkey {
    match BOXES_WON[q] {
        0..=4 => f.creator,
        5..=14 => f.buyer,
        _ => f.buyer_2,
    }
}

fn pool_key(f: &Fixture, pool: &Pool) -> Pubkey {
    pool_pda(&standard_game(), &f.creator, pool.nonce).0
}

fn lamports_of(accounts: &[(Pubkey, solana_account::Account)], key: &Pubkey) -> u64 {
    accounts
        .iter()
        .find(|(k, _)| k == key)
        .map_or(0, |(_, a)| a.lamports)
}

fn standard(f: &Fixture) -> Pool {
    drawn_pool_for_settlement(f, &f.expected_config(), &sol_params(0), 0)
}

fn standard_settled(f: &Fixture, n: u8, fees_paid: bool) -> Pool {
    settled_pool(f, &f.expected_config(), &sol_params(0), 0, n, fees_paid)
}

fn integrator_params() -> CreatePoolParams {
    CreatePoolParams {
        integrator_bps: 100,
        ..sol_params(0)
    }
}

fn preset_params(preset: PayoutPreset) -> CreatePoolParams {
    CreatePoolParams {
        preset,
        ..sol_params(0)
    }
}

/// `settle(quarter)` by the keeper on a SOL pool against `record_with_quarters(quarters)`.
fn settle_sol(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    pool: &Pool,
    quarters: u8,
    quarter: u8,
    winner: &Pubkey,
    integrator: Option<Pubkey>,
) -> (
    solana_instruction::Instruction,
    Vec<(Pubkey, solana_account::Account)>,
) {
    let ix = settle_ix(
        f,
        &f.keeper,
        pool,
        &standard_game(),
        quarter,
        winner,
        SettleAccounts {
            integrator,
            spl: None,
        },
    );
    let accounts = settlement_accounts(f, m, &f.expected_config(), pool, quarters, None);
    (ix, accounts)
}

// ---------------------------------------------------------------------------
// The fixture's own numbers (so `settled_pool` cannot drift with the code)
// ---------------------------------------------------------------------------

#[test]
fn the_fixtures_numbers_are_the_worked_examples() {
    // ARCHITECTURE › Fees worked examples; §5.1 / §5.2.
    let f = Fixture::new();
    let p = standard_settled(&f, 1, true);
    assert_eq!(
        (p.platform_fee, p.creator_fee, p.integrator_fee),
        (PLATFORM_FEE, CREATOR_FEE, 0)
    );
    assert_eq!(p.prize_pool, PRIZE_POOL);
    assert_eq!(p.quarter_prize, Q);
    assert_eq!(p.unpaid_prize_pool, 880_000_000);
    assert_eq!(p.winning_box, [17, 255, 255, 255]);
    let s = settled_pool(&f, &f.expected_config(), &sol_params(0), SPONSORED, 1, true);
    assert_eq!(s.prize_pool, 2_100_000_000);
    assert_eq!(
        s.quarter_prize,
        [420_000_000, 420_000_000, 420_000_000, 840_000_000]
    );
    assert_eq!(s.unpaid_prize_pool, 1_680_000_000);
    assert_eq!((s.platform_fee, s.creator_fee), (PLATFORM_FEE, CREATOR_FEE));
    let i = settled_pool(&f, &f.expected_config(), &integrator_params(), 0, 1, true);
    assert_eq!(
        (i.platform_fee, i.creator_fee, i.integrator_fee),
        (PLATFORM_FEE, CREATOR_FEE, 12_500_000)
    );
    assert_eq!(i.prize_pool, 1_087_500_000);
    assert_eq!(
        i.quarter_prize,
        [217_500_000, 217_500_000, 217_500_000, 435_000_000]
    );
    let fo = settled_pool(
        &f,
        &f.expected_config(),
        &preset_params(PayoutPreset::FinalOnly),
        0,
        1,
        false,
    );
    assert_eq!(fo.quarter_prize, [0, 0, 0, PRIZE_POOL]);
    let ore = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 1, true);
    assert_eq!(
        (ore.platform_fee, ore.creator_fee),
        (6_250_000_000, 8_750_000_000)
    );
    assert_eq!(ore.prize_pool, 110_000_000_000);
    assert_eq!(
        ore.quarter_prize,
        [
            22_000_000_000,
            22_000_000_000,
            22_000_000_000,
            44_000_000_000
        ]
    );
    // Dust cases, §5.2 "≤ 3 base units".
    let d = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params(0),
        1_000_000_003,
        4,
        true,
    );
    assert_eq!(
        d.quarter_prize,
        [420_000_000, 420_000_000, 420_000_000, 840_000_001]
    );
    assert_eq!(d.unpaid_prize_pool, 2);
    let e = settled_pool(
        &f,
        &f.expected_config(),
        &preset_params(PayoutPreset::Even),
        1_000_000_002,
        4,
        true,
    );
    assert_eq!(e.quarter_prize, [525_000_000; 4]);
    assert_eq!(e.unpaid_prize_pool, 2);
    // Smallest possible quarter prize: 15 % total fee, 20 % share (§5.3).
    let mut c = f.expected_config();
    c.platform_bps = 500;
    c.creator_bps = 500;
    let small = settled_pool(
        &f,
        &c,
        &CreatePoolParams {
            creator_addon_bps: 500,
            ..sol_params(0)
        },
        0,
        1,
        true,
    );
    assert_eq!(small.quarter_prize[0], 212_500_000);
}

// ---------------------------------------------------------------------------
// settle: the happy paths
// ---------------------------------------------------------------------------

#[test]
fn settle_q1_sol_is_the_worked_example() {
    // ARCHITECTURE › Fees worked example; PROGRAM §4.5; BUILD-PLAN acceptance line 1: the Q1
    // transaction moves 0.22 + 0.0625 + 0.0875 and the vault goes 1.25 → 0.88.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    let winner = winner_of(&f, 0);
    assert_eq!(winner, f.buyer_2);
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &winner, None);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&pool_key(&f, &pool)).0),
        rent_for(0) + 1_250_000_000
    );
    let before = |k: &Pubkey| lamports_of(&accounts, k);
    let (w0, fw0, c0, i0, k0) = (
        before(&f.buyer_2),
        before(&f.fee_wallet),
        before(&f.creator),
        before(&f.integrator),
        before(&f.keeper),
    );

    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert_eq!(
        after.winning_box,
        [17, NO_WINNING_BOX, NO_WINNING_BOX, NO_WINNING_BOX]
    );
    assert_eq!(after.prize_pool, PRIZE_POOL);
    assert_eq!(after.quarter_prize, Q);
    assert_eq!(after.unpaid_prize_pool, 880_000_000);
    assert_eq!(after.quarters_settled, 1);
    assert!(after.fees_paid);
    assert_eq!(after.status, PoolStatus::Drawn);
    assert_eq!(
        after,
        standard_settled(&f, 1, true),
        "whole-struct: nothing else changed"
    );
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&f, &pool)).0).lamports,
        rent_for(0) + 880_000_000
    );
    assert_eq!(account_of(&result, &f.buyer_2).lamports - w0, Q[0]);
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        PLATFORM_FEE
    );
    assert_eq!(account_of(&result, &f.creator).lamports - c0, CREATOR_FEE);
    assert_eq!(account_of(&result, &f.integrator).lamports, i0);
    assert_eq!(account_of(&result, &f.keeper).lamports, k0, "no ATA on SOL");
    assert_eq!(event_names(&result), ["QuarterSettled"]);
    let ev: QuarterSettled = emitted_event(&result).expect("QuarterSettled");
    assert_eq!(ev.time, T0);
    assert_eq!(ev.pool, to_a(&pool_key(&f, &pool)));
    assert_eq!((ev.quarter, ev.home, ev.away, ev.box_index), (1, 7, 3, 17));
    assert_eq!(
        (ev.winner, ev.amount, ev.fees_paid_now),
        (to_a(&f.buyer_2), Q[0], true)
    );
    assert_eq!(
        (ev.platform_fee, ev.creator_fee, ev.integrator_fee),
        (PLATFORM_FEE, CREATOR_FEE, 0)
    );
}

#[test]
fn settle_q1_sponsored_variant() {
    // BUILD-PLAN acceptance "Sponsored variant": 1 SOL sponsored → Q1 moves 0.42 + 0.0625 +
    // 0.0875, vault 2.25 → 1.68; fees unchanged (§5.1: never computed on a sponsorship).
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &sol_params(0), SPONSORED);
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    let vault = vault_pda(&pool_key(&f, &pool)).0;
    assert_eq!(lamports_of(&accounts, &vault), rent_for(0) + 2_250_000_000);
    let w0 = lamports_of(&accounts, &f.buyer_2);
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let c0 = lamports_of(&accounts, &f.creator);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert_eq!(after.prize_pool, 2_100_000_000);
    assert_eq!(account_of(&result, &f.buyer_2).lamports - w0, 420_000_000);
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        PLATFORM_FEE
    );
    assert_eq!(account_of(&result, &f.creator).lamports - c0, CREATOR_FEE);
    assert_eq!(
        account_of(&result, &vault).lamports,
        rent_for(0) + 1_680_000_000
    );
    assert_eq!(
        after,
        settled_pool(&f, &f.expected_config(), &sol_params(0), SPONSORED, 1, true)
    );
}

#[test]
fn settle_q2_pays_the_prize_only_fees_once() {
    // PROGRAM §4.5 (3) "If additionally !fees_paid"; §10 Fees "paid exactly once".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard_settled(&f, 1, true);
    let winner = winner_of(&f, 1);
    assert_eq!(winner, f.creator);
    let (ix, accounts) = settle_sol(&f, &m, &pool, 2, 2, &winner, None);
    let c0 = lamports_of(&accounts, &f.creator);
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert_eq!(after.winning_box[1], 4);
    assert_eq!(
        account_of(&result, &f.creator).lamports - c0,
        Q[1],
        "prize only"
    );
    assert_eq!(account_of(&result, &f.fee_wallet).lamports, fw0);
    assert_eq!(after.unpaid_prize_pool, 660_000_000);
    assert!(after.fees_paid);
    assert_eq!(after, standard_settled(&f, 2, true));
    let ev: QuarterSettled = emitted_event(&result).expect("QuarterSettled");
    assert!(!ev.fees_paid_now);
    assert_eq!(
        (ev.platform_fee, ev.creator_fee, ev.integrator_fee),
        (0, 0, 0)
    );
    assert_eq!(
        (ev.quarter, ev.home, ev.away, ev.box_index, ev.amount),
        (2, 14, 10, 4, Q[1])
    );
}

#[test]
fn settle_q3_and_q4_in_turn_settle_the_pool_and_money_in_equals_money_out() {
    // PROGRAM §4.5 (4); §10 Money: total out over the four = 25 × price.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let p2 = standard_settled(&f, 2, true);
    let (ix, accounts) = settle_sol(&f, &m, &p2, 3, 3, &f.buyer, None);
    let b0 = lamports_of(&accounts, &f.buyer);
    let r3 = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(account_of(&r3, &f.buyer).lamports - b0, Q[2]);
    let p3 = decode_pool(account_of(&r3, &pool_key(&f, &p2)));
    assert_eq!(p3, standard_settled(&f, 3, true));

    let (ix, accounts) = settle_sol(&f, &m, &p3, 4, 4, &f.creator, None);
    let c0 = lamports_of(&accounts, &f.creator);
    let r4 = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let p4 = decode_pool(account_of(&r4, &pool_key(&f, &p3)));
    assert_eq!(account_of(&r4, &f.creator).lamports - c0, Q[3]);
    assert_eq!(p4.winning_box, BOXES_WON);
    assert_eq!(p4.status, PoolStatus::Settled);
    assert_eq!(p4.quarters_settled, 4);
    assert_eq!(p4.unpaid_prize_pool, 0);
    assert_eq!(
        account_of(&r4, &vault_pda(&pool_key(&f, &p3)).0).lamports,
        rent_for(0)
    );
    assert_eq!(p4, standard_settled(&f, 4, true));
    assert_eq!(
        Q.iter().sum::<u64>() + PLATFORM_FEE + CREATOR_FEE,
        1_250_000_000
    );
}

#[test]
fn the_four_settlements_as_one_chain_equal_the_fixtures() {
    // Fixture-vs-program equality: `settled_pool` is proven right by the live state.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    let accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, 4, None);
    let chain: Vec<_> = (1..=4u8)
        .map(|q| {
            settle_ix(
                &f,
                &f.keeper,
                &pool,
                &standard_game(),
                q,
                &winner_of(&f, usize::from(q - 1)),
                SettleAccounts::default(),
            )
        })
        .collect();
    let result = m.process_instruction_chain(&chain, &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.program_result);
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert_eq!(after, standard_settled(&f, 4, true));
    // §3.4: vault = rent + unpaid_prize_pool at the end (0).
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&f, &pool)).0).lamports,
        rent_for(0)
    );
    let paid = [f.buyer_2, f.creator, f.buyer]
        .iter()
        .map(|k| account_of(&result, k).lamports - lamports_of(&accounts, k))
        .sum::<u64>()
        + (account_of(&result, &f.fee_wallet).lamports - lamports_of(&accounts, &f.fee_wallet));
    assert_eq!(paid, 1_250_000_000);
}

#[test]
fn settle_while_paused_succeeds() {
    // ARCHITECTURE › Trust model: the pause flag never blocks a settlement.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    let (ix, mut accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    let mut config = f.expected_config();
    config.paused = true;
    set_account(&mut accounts, config_pda().0, f.config_account(&config));
    m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
}

#[test]
fn settle_moves_the_stored_fees_not_a_recomputation_from_the_config() {
    // Audit focus: a config change after creation must not change a payout.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    let (ix, mut accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    let mut config = f.expected_config();
    config.platform_bps = 100;
    set_account(&mut accounts, config_pda().0, f.config_account(&config));
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        PLATFORM_FEE
    );
}

// ---------------------------------------------------------------------------
// Presets and the integrator
// ---------------------------------------------------------------------------

#[test]
fn final_only_q1_to_q3_record_the_box_and_move_nothing() {
    // BUILD-PLAN acceptance "Q4 100%"; PROGRAM §4.5 "A zero-share quarter".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let params = preset_params(PayoutPreset::FinalOnly);
    let mut pool = drawn_pool_for_settlement(&f, &f.expected_config(), &params, 0);
    for q in 0..3usize {
        let ix = settle_ix(
            &f,
            &f.keeper,
            &pool,
            &standard_game(),
            q as u8 + 1,
            &winner_of(&f, q),
            SettleAccounts::default(),
        );
        let accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, q as u8 + 1, None);
        let snapshot: Vec<u64> = [
            winner_of(&f, q),
            f.fee_wallet,
            f.creator,
            vault_pda(&pool_key(&f, &pool)).0,
        ]
        .iter()
        .map(|k| lamports_of(&accounts, k))
        .collect();
        let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
        let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
        assert_eq!(after.winning_box[q], BOXES_WON[q]);
        assert!(!after.fees_paid);
        assert_eq!(after.prize_pool, PRIZE_POOL);
        assert_eq!(after.quarter_prize, [0, 0, 0, PRIZE_POOL]);
        assert_eq!(after.unpaid_prize_pool, PRIZE_POOL);
        let now: Vec<u64> = [
            winner_of(&f, q),
            f.fee_wallet,
            f.creator,
            vault_pda(&pool_key(&f, &pool)).0,
        ]
        .iter()
        .map(|k| account_of(&result, k).lamports)
        .collect();
        assert_eq!(now, snapshot, "no transfer on a zero share");
        assert_eq!(result.inner_instructions.len(), 1, "only the event CPI");
        let ev: QuarterSettled = emitted_event(&result).expect("QuarterSettled");
        assert_eq!((ev.amount, ev.fees_paid_now), (0, false));
        assert_eq!(
            (ev.platform_fee, ev.creator_fee, ev.integrator_fee),
            (0, 0, 0)
        );
        assert_eq!(
            after,
            settled_pool(&f, &f.expected_config(), &params, 0, q as u8 + 1, false)
        );
        pool = after;
    }
}

#[test]
fn final_only_q4_pays_everything_and_the_fees_now() {
    // §10 Fees "FinalOnly pays them with the final".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let params = preset_params(PayoutPreset::FinalOnly);
    let pool = settled_pool(&f, &f.expected_config(), &params, 0, 3, false);
    let (ix, accounts) = settle_sol(&f, &m, &pool, 4, 4, &f.creator, None);
    let c0 = lamports_of(&accounts, &f.creator);
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    // The creator wins Q4 (box 4) and receives creator_fee in the same call.
    assert_eq!(
        account_of(&result, &f.creator).lamports - c0,
        PRIZE_POOL + CREATOR_FEE
    );
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        PLATFORM_FEE
    );
    assert!(after.fees_paid);
    assert_eq!(after.status, PoolStatus::Settled);
    let ev: QuarterSettled = emitted_event(&result).expect("QuarterSettled");
    assert!(ev.fees_paid_now);
    assert_eq!(
        (ev.amount, ev.platform_fee, ev.creator_fee),
        (PRIZE_POOL, PLATFORM_FEE, CREATOR_FEE)
    );
}

#[test]
fn even_q1_pays_a_quarter() {
    // PROGRAM §1 preset table: Even 25 %.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = drawn_pool_for_settlement(
        &f,
        &f.expected_config(),
        &preset_params(PayoutPreset::Even),
        0,
    );
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    let w0 = lamports_of(&accounts, &f.buyer_2);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(account_of(&result, &f.buyer_2).lamports - w0, 275_000_000);
}

#[test]
fn integrator_variant_pays_the_integrator_with_the_first_prize() {
    // PROGRAM §5.1; §4.5 (3).
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let mut params = integrator_params();
    params.integrator = to_a(&f.integrator);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &params, 0);
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, Some(f.integrator));
    let i0 = lamports_of(&accounts, &f.integrator);
    let w0 = lamports_of(&accounts, &f.buyer_2);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(account_of(&result, &f.integrator).lamports - i0, 12_500_000);
    assert_eq!(account_of(&result, &f.buyer_2).lamports - w0, 217_500_000);
    let ev: QuarterSettled = emitted_event(&result).expect("QuarterSettled");
    assert_eq!(ev.integrator_fee, 12_500_000);
    assert_eq!(
        decode_pool(account_of(&result, &pool_key(&f, &pool))),
        settled_pool(&f, &f.expected_config(), &params, 0, 1, true)
    );
}

#[test]
fn integrator_variant_without_or_with_the_wrong_integrator_is_fee_account_mismatch() {
    // PROGRAM §4.5 "the fee, creator and integrator accounts match".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let mut params = integrator_params();
    params.integrator = to_a(&f.integrator);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &params, 0);
    for slot in [None, Some(f.stranger)] {
        let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, slot);
        m.process_and_validate_instruction(
            &ix,
            &accounts,
            &[Check::err(custom(err(E::FeeAccountMismatch)))],
        );
    }
}

#[test]
fn a_pool_without_an_integrator_never_reads_the_slot() {
    // this brief: a passed account is ignored.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, Some(f.stranger));
    let s0 = lamports_of(&accounts, &f.stranger);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(account_of(&result, &f.stranger).lamports, s0);
}

// ---------------------------------------------------------------------------
// settle: the negatives
// ---------------------------------------------------------------------------

#[test]
fn settle_needs_a_drawn_pool() {
    // BUILD-PLAN "Settle before draw fails"; PROGRAM §9: Open, Locked (sampled, revealed,
    // undrawn), Settled, Returned, Split are PoolNotDrawn.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let mut cases = vec![
        pool_with(&f, PoolStatus::Open, 3, &f.buyer_2),
        sampled_pool(&f, &f.var_key(), END_AT, END_AT + 3, END_HASH),
        standard_settled(&f, 4, true),
    ];
    let mut returned = standard(&f);
    returned.status = PoolStatus::Returned;
    returned.drawn = false;
    let mut split = standard_settled(&f, 1, true);
    split.status = PoolStatus::Split;
    cases.push(returned);
    cases.push(split);
    for pool in cases {
        let quarter = pool.quarters_settled + 1;
        let (ix, accounts) = settle_sol(&f, &m, &pool, 4, quarter.min(4), &f.buyer_2, None);
        let r = m.process_instruction(&ix, &accounts);
        assert_eq!(
            custom_error(&r),
            Some(err(E::PoolNotDrawn)),
            "{:?}",
            pool.status
        );
    }
}

#[test]
fn settle_out_of_order_is_quarter_out_of_order() {
    // BUILD-PLAN "out of order", "twice"; PROGRAM §4.5 Idempotency.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let fresh = standard(&f);
    for quarter in [0u8, 2, 3, 4, 5, 255] {
        let (ix, accounts) = settle_sol(&f, &m, &fresh, 4, quarter, &f.buyer_2, None);
        let r = m.process_instruction(&ix, &accounts);
        assert_eq!(
            custom_error(&r),
            Some(err(E::QuarterOutOfOrder)),
            "quarter {quarter}"
        );
    }
    let after_q1 = standard_settled(&f, 1, true);
    for quarter in [1u8, 3] {
        let (ix, accounts) = settle_sol(&f, &m, &after_q1, 4, quarter, &f.buyer_2, None);
        let r = m.process_instruction(&ix, &accounts);
        assert_eq!(
            custom_error(&r),
            Some(err(E::QuarterOutOfOrder)),
            "quarter {quarter} after Q1"
        );
    }
    // A fifth call: Settled → 6028 first; on a planted Drawn pool with quarters_settled = 4 it
    // is the ordering check (quarter 5 == 4 + 1 would pass; `quarter 1` does not).
    let mut planted = standard_settled(&f, 4, true);
    planted.status = PoolStatus::Drawn;
    let (ix, accounts) = settle_sol(&f, &m, &planted, 4, 1, &f.buyer_2, None);
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(err(E::QuarterOutOfOrder))
    );
}

#[test]
fn settle_a_quarter_whose_score_is_not_posted_is_scores_not_posted() {
    // BUILD-PLAN "score isn't posted".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let (ix, accounts) = settle_sol(&f, &m, &standard(&f), 0, 1, &f.buyer_2, None);
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(err(E::ScoresNotPosted))
    );
    let (ix, accounts) = settle_sol(
        &f,
        &m,
        &standard_settled(&f, 1, true),
        1,
        2,
        &f.creator,
        None,
    );
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(err(E::ScoresNotPosted))
    );
}

#[test]
fn settle_with_the_wrong_winner_is_winner_mismatch() {
    // PROGRAM §4.5; audit focus "never passed in": box 17 is buyer_2's.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    for wrong in [f.buyer, f.stranger, f.creator] {
        let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &wrong, None);
        assert_eq!(
            custom_error(&m.process_instruction(&ix, &accounts)),
            Some(err(E::WinnerMismatch))
        );
    }
}

#[test]
fn the_winner_is_computed_from_the_stored_axes() {
    // PROGRAM §6.3: home_axis [7,0,1,2,3,4,5,6,8,9] puts 7 in lane 0 → col 0; away_axis
    // [3,0,1,2,4,5,6,7,8,9] puts 3 in lane 0 → row 0 → box 0, the creator's.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let mut pool = standard(&f);
    pool.home_axis = [7, 0, 1, 2, 3, 4, 5, 6, 8, 9];
    pool.away_axis = [3, 0, 1, 2, 4, 5, 6, 7, 8, 9];
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(err(E::WinnerMismatch))
    );
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.creator, None);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert_eq!(after.winning_box[0], 0);
    let ev: QuarterSettled = emitted_event(&result).expect("QuarterSettled");
    assert_eq!((ev.box_index, ev.winner), (0, to_a(&f.creator)));
}

#[test]
fn settle_by_the_admin_or_a_stranger_is_unauthorized() {
    // PROGRAM §10 Authority.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    for signer in [f.admin, f.stranger] {
        let ix = settle_ix(
            &f,
            &signer,
            &pool,
            &standard_game(),
            1,
            &f.buyer_2,
            SettleAccounts::default(),
        );
        let accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, 1, None);
        assert_eq!(
            custom_error(&m.process_instruction(&ix, &accounts)),
            Some(err(E::Unauthorized))
        );
    }
}

#[test]
fn settle_with_wrong_fee_wallet_creator_game_or_config() {
    // this brief: the `@` constraints and Anchor's own.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    let (base_ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    // Account order in Settle: score_authority 0, config 1, game 2, pool 3, vault 4, winner 5,
    // fee_wallet 6, creator 7.
    let mut ix = base_ix.clone();
    ix.accounts[6].pubkey = f.stranger;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(err(E::FeeAccountMismatch))
    );
    let mut ix = base_ix.clone();
    ix.accounts[7].pubkey = f.stranger;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(err(E::FeeAccountMismatch))
    );
    // Another game record.
    let other = {
        let mut r = standard_record();
        r.key.week += 1;
        r
    };
    let other_key = game_pda(&other.key, other.scheduled_kickoff).0;
    let mut accounts2 = accounts.clone();
    accounts2.push((other_key, game_record_account(&other)));
    let mut ix = base_ix.clone();
    ix.accounts[2].pubkey = other_key;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts2)),
        Some(anchor(ErrorCode::ConstraintHasOne))
    );
    // A config at the wrong address.
    let mut ix = base_ix;
    let bogus = Pubkey::new_unique();
    let mut accounts3 = accounts.clone();
    accounts3.push((bogus, f.config_account(&f.expected_config())));
    ix.accounts[1].pubkey = bogus;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts3)),
        Some(anchor(ErrorCode::ConstraintSeeds))
    );
}

#[test]
fn the_fee_transfers_are_atomic_with_the_prize() {
    // Audit focus "fee atomicity": the vault one lamport short of what the call moves (prize +
    // two fees = 370,000,000; the brief's "rent + 1.25 SOL − 1" still covers that and merely
    // leaves the §3.4 balance a lamport low, NOTES) → the prize and the platform fee go through,
    // the creator fee fails inside the System program (not a §8 code), and the whole instruction
    // reverts: nothing changed.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard(&f);
    let (ix, mut accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    let vault = vault_pda(&pool_key(&f, &pool)).0;
    set_account(
        &mut accounts,
        vault,
        sol_vault_account(Q[0] + PLATFORM_FEE + CREATOR_FEE - 1),
    );
    let result = m.process_instruction(&ix, &accounts);
    assert!(result.program_result.is_err());
    // The System program's own `InsufficientFunds` (custom 1), below the §8 range.
    assert_eq!(
        custom_error(&result),
        Some(1),
        "{:?}",
        result.program_result
    );
    for k in [f.buyer_2, f.fee_wallet, f.creator, vault] {
        assert_eq!(
            account_of(&result, &k).lamports,
            lamports_of(&accounts, &k),
            "{k}"
        );
    }
    assert_eq!(decode_pool(account_of(&result, &pool_key(&f, &pool))), pool);
}

#[test]
fn settle_overflow_the_largest_pool_computes_and_a_sponsored_overflow_is_math_overflow() {
    // Audit focus "overflow on the largest SKR pool"; §5.2 u128 inside.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let price = u64::MAX / 25;
    let mut pool = standard(&f);
    pool.price = price;
    let fees = mybarpool::money::fee_amounts(price, 500, 500, 200, 0).unwrap();
    pool.platform_fee = fees.platform_fee;
    pool.creator_fee = fees.creator_fee;
    // The vault cannot hold 25 × price lamports (more than exist), so plant it with the prize
    // and fees the first call moves; what is under test is the arithmetic.
    let expected_pool = mybarpool::money::prize_pool(price, &fees, 0).unwrap();
    let q1 = mybarpool::money::quarter_prizes(expected_pool, PayoutPreset::Standard).unwrap();
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts::default(),
    );
    let mut accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, 1, None);
    let vault = vault_pda(&pool_key(&f, &pool)).0;
    set_account(
        &mut accounts,
        vault,
        sol_vault_account(rent_for(0) + q1[0] + fees.platform_fee + fees.creator_fee),
    );
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert_eq!(after.prize_pool, expected_pool);
    assert_eq!(after.quarter_prize, q1);

    pool.sponsored_total = u64::MAX;
    set_account(&mut accounts, pool_key(&f, &pool), pool_account(&pool));
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts::default(),
    );
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(err(E::MathOverflow))
    );
}

#[test]
fn dust_pools_leave_the_dust_in_unpaid_prize_pool() {
    // PROGRAM §5.2 "≤ 3 base units"; §3.4: a Settled pool's unpaid_prize_pool is the dust.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &sol_params(0), 1_000_000_003);
    let accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, 4, None);
    let chain: Vec<_> = (1..=4u8)
        .map(|q| {
            settle_ix(
                &f,
                &f.keeper,
                &pool,
                &standard_game(),
                q,
                &winner_of(&f, usize::from(q - 1)),
                SettleAccounts::default(),
            )
        })
        .collect();
    let result = m.process_instruction_chain(&chain, &accounts);
    assert!(result.program_result.is_ok());
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert_eq!(
        after.quarter_prize,
        [420_000_000, 420_000_000, 420_000_000, 840_000_001]
    );
    assert_eq!(after.unpaid_prize_pool, 2);
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&f, &pool)).0).lamports,
        rent_for(0) + 2
    );
    let even = drawn_pool_for_settlement(
        &f,
        &f.expected_config(),
        &preset_params(PayoutPreset::Even),
        1_000_000_002,
    );
    let accounts = settlement_accounts(&f, &m, &f.expected_config(), &even, 4, None);
    let chain: Vec<_> = (1..=4u8)
        .map(|q| {
            settle_ix(
                &f,
                &f.keeper,
                &even,
                &standard_game(),
                q,
                &winner_of(&f, usize::from(q - 1)),
                SettleAccounts::default(),
            )
        })
        .collect();
    let result = m.process_instruction_chain(&chain, &accounts);
    assert!(result.program_result.is_ok());
    assert_eq!(
        decode_pool(account_of(&result, &pool_key(&f, &even))).unpaid_prize_pool,
        2
    );
}

#[test]
fn settle_is_unaffected_by_neighbouring_instructions() {
    // PROGRAM §10 Composability.
    let f = Fixture::new();
    let mut m = mollusk_for_settlement(T0);
    m.compute_budget.compute_unit_limit = 400_000;
    let pool = standard(&f);
    let (ix, accounts) = settle_sol(&f, &m, &pool, 1, 1, &f.buyer_2, None);
    let noop = solana_instruction::Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], // SystemInstruction::Transfer { lamports: 1 }
        vec![
            solana_instruction::AccountMeta::new(f.keeper, true),
            solana_instruction::AccountMeta::new(f.admin, false),
        ],
    );
    let alone = m.process_instruction_chain(&[ix.clone()], &accounts);
    let sandwiched = m.process_instruction_chain(&[noop.clone(), ix, noop], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_pool(account_of(&alone, &pool_key(&f, &pool))),
        decode_pool(account_of(&sandwiched, &pool_key(&f, &pool)))
    );
    assert_eq!(
        account_of(&alone, &f.buyer_2).lamports,
        account_of(&sandwiched, &f.buyer_2).lamports
    );
}

// ---------------------------------------------------------------------------
// SPL settlement (PROGRAM §5.4): the recipient's ATA, created idempotently when funds move
// ---------------------------------------------------------------------------

const ORE_Q: [u64; 4] = [
    22_000_000_000,
    22_000_000_000,
    22_000_000_000,
    44_000_000_000,
];
const ORE_PLATFORM_FEE: u64 = 6_250_000_000;
const ORE_CREATOR_FEE: u64 = 8_750_000_000;
/// A legacy Token account is 165 bytes; its ATA rent is what the keeper pays per create.
const TOKEN_ACCOUNT_LEN: usize = 165;

/// An ORE drawn pool with the vault holding the pot, every recipient's ATA an empty system
/// account (`missing`) or a zero-balance token account (`present`), the game at `quarters`.
fn ore_setup(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    pool: &Pool,
    quarters: u8,
    vault_amount: u64,
    winner: &Pubkey,
    atas_present: bool,
) -> (SplSettle, Vec<(Pubkey, solana_account::Account)>) {
    let spl = SplSettle::derived(f, pool, winner);
    let mut accounts = settlement_accounts(
        f,
        m,
        &f.expected_config(),
        pool,
        quarters,
        Some(vault_amount),
    );
    for (ata_key, wallet) in [
        (spl.winner_ata, *winner),
        (spl.fee_ata, f.fee_wallet),
        (spl.creator_ata, f.creator),
    ] {
        let account = if atas_present {
            token_account(&f.ore_mint, &wallet, 0)
        } else {
            system_account(0)
        };
        set_account(&mut accounts, ata_key, account);
    }
    (spl, accounts)
}

fn token_amount_of(result: &mollusk_svm::result::InstructionResult, key: &Pubkey) -> u64 {
    decode_token_amount(account_of(result, key))
}

#[test]
fn ore_settle_q1_with_every_ata_missing_creates_three_and_pays() {
    // PROGRAM §5.4; BUILD-PLAN "closed ATA still gets paid": three new token accounts owned by
    // the Token program, the keeper down by three rents, the prize and both fees landed.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &ore_params(0), 0);
    let (spl, accounts) = ore_setup(&f, &m, &pool, 1, 25 * PRICE_ORE, &f.buyer_2, false);
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts {
            integrator: None,
            spl: Some(spl),
        },
    );
    let k0 = lamports_of(&accounts, &f.keeper);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    for (ata_key, wallet, amount) in [
        (spl.winner_ata, f.buyer_2, ORE_Q[0]),
        (spl.fee_ata, f.fee_wallet, ORE_PLATFORM_FEE),
        (spl.creator_ata, f.creator, ORE_CREATOR_FEE),
    ] {
        let a = account_of(&result, &ata_key);
        assert_eq!(a.owner, token_program_id());
        assert_eq!(decode_token_owner(a), wallet);
        assert_eq!(decode_token_amount(a), amount);
    }
    assert_eq!(
        token_amount_of(&result, &vault_pda(&pool_key(&f, &pool)).0),
        88_000_000_000
    );
    assert_eq!(
        k0 - account_of(&result, &f.keeper).lamports,
        3 * rent_for(TOKEN_ACCOUNT_LEN)
    );
    let after = decode_pool(account_of(&result, &pool_key(&f, &pool)));
    assert!(after.fees_paid);
    assert_eq!(
        after,
        settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 1, true)
    );
    assert_eq!(event_names(&result), ["QuarterSettled"]);
}

#[test]
fn ore_settle_q1_with_every_ata_present_creates_nothing() {
    // ARCHITECTURE › Payouts "a no-op when the account exists": same amounts, the keeper's
    // lamports unchanged, no InitializeAccount among the inner instructions.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &ore_params(0), 0);
    let (spl, accounts) = ore_setup(&f, &m, &pool, 1, 25 * PRICE_ORE, &f.buyer_2, true);
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts {
            integrator: None,
            spl: Some(spl),
        },
    );
    let k0 = lamports_of(&accounts, &f.keeper);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(token_amount_of(&result, &spl.winner_ata), ORE_Q[0]);
    assert_eq!(token_amount_of(&result, &spl.fee_ata), ORE_PLATFORM_FEE);
    assert_eq!(token_amount_of(&result, &spl.creator_ata), ORE_CREATOR_FEE);
    assert_eq!(account_of(&result, &f.keeper).lamports, k0);
    // Inner instructions: three create_idempotent CPIs (each a no-op that calls nothing),
    // three transfer_checked, one event CPI; no System `CreateAccount` (0) and no Token
    // `InitializeAccount3` (18).
    assert_eq!(inner_count(&result, &Pubkey::default(), 0), 0);
    assert_eq!(inner_count(&result, &token_program_id(), 18), 0);
    assert_eq!(
        inner_count(&result, &token_program_id(), 12),
        3,
        "three transfer_checked"
    );
}

#[test]
fn ore_settle_q2_with_the_fee_atas_missing_creates_only_the_winners() {
    // this brief: "no account is created for a transfer that does not happen".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 1, true);
    let (spl, accounts) = ore_setup(&f, &m, &pool, 2, 88_000_000_000, &f.creator, false);
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        2,
        &f.creator,
        SettleAccounts {
            integrator: None,
            spl: Some(spl),
        },
    );
    let k0 = lamports_of(&accounts, &f.keeper);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(token_amount_of(&result, &spl.winner_ata), ORE_Q[1]);
    assert_eq!(
        account_of(&result, &spl.fee_ata).owner,
        Pubkey::default(),
        "fee ATA stays an empty system account"
    );
    assert_eq!(account_of(&result, &spl.fee_ata).data.len(), 0);
    assert_eq!(
        k0 - account_of(&result, &f.keeper).lamports,
        rent_for(TOKEN_ACCOUNT_LEN)
    );
}

#[test]
fn ore_final_only_q1_with_every_ata_missing_creates_nothing() {
    // PROGRAM §4.5 "moves no funds".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let params = CreatePoolParams {
        preset: PayoutPreset::FinalOnly,
        ..ore_params(0)
    };
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &params, 0);
    let (spl, accounts) = ore_setup(&f, &m, &pool, 1, 25 * PRICE_ORE, &f.buyer_2, false);
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts {
            integrator: None,
            spl: Some(spl),
        },
    );
    let k0 = lamports_of(&accounts, &f.keeper);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    assert_eq!(account_of(&result, &f.keeper).lamports, k0);
    for k in [spl.winner_ata, spl.fee_ata, spl.creator_ata] {
        assert_eq!(account_of(&result, &k).data.len(), 0);
    }
    assert_eq!(result.inner_instructions.len(), 1, "only the event CPI");
}

#[test]
fn ore_settle_refuses_a_wrong_ata_program_or_missing_mint() {
    // PROGRAM §5.4 "the recipient's associated token account".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &ore_params(0), 0);
    let (spl, mut accounts) = ore_setup(&f, &m, &pool, 1, 25 * PRICE_ORE, &f.buyer_2, false);
    let settle_with = |s: SplSettle| {
        settle_ix(
            &f,
            &f.keeper,
            &pool,
            &standard_game(),
            1,
            &f.buyer_2,
            SettleAccounts {
                integrator: None,
                spl: Some(s),
            },
        )
    };
    // The creator's ATA passed as the winner's.
    let r = m.process_instruction(
        &settle_with(SplSettle {
            winner_ata: spl.creator_ata,
            ..spl
        }),
        &accounts,
    );
    assert_eq!(custom_error(&r), Some(err(E::WinnerMismatch)));
    // A token account for ORE owned by fee_wallet at a non-ATA address.
    let stray = Pubkey::new_unique();
    accounts.push((stray, token_account(&f.ore_mint, &f.fee_wallet, 0)));
    let r = m.process_instruction(
        &settle_with(SplSettle {
            fee_ata: stray,
            ..spl
        }),
        &accounts,
    );
    assert_eq!(custom_error(&r), Some(err(E::FeeAccountMismatch)));
    // Token-2022 passed as the program.
    let r = m.process_instruction(
        &settle_with(SplSettle {
            token_program: token_2022_program_id(),
            ..spl
        }),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::RequireKeysEqViolated))
    );
    // The mint missing: the Option is None (account index 9 = the program id).
    let mut ix = settle_with(spl);
    ix.accounts[9].pubkey = program_id();
    let r = m.process_instruction(&ix, &accounts);
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::ConstraintAccountIsNone))
    );
}

#[test]
fn skr_token_2022_settle_q1_with_atas_missing() {
    // PROGRAM §5.4 "the program never assumes the legacy Token program": the new accounts are
    // owned by Token-2022 and transfer_checked runs with 6 decimals.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let config = f.config_with_skr();
    let pool = drawn_pool_for_settlement(&f, &config, &skr_params(0), 0);
    let spl = SplSettle::derived(&f, &pool, &f.buyer_2);
    assert_eq!(spl.token_program, token_2022_program_id());
    let mut accounts = settlement_accounts(&f, &m, &config, &pool, 1, Some(25 * PRICE_SKR));
    for k in [spl.winner_ata, spl.fee_ata, spl.creator_ata] {
        set_account(&mut accounts, k, system_account(0));
    }
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts {
            integrator: None,
            spl: Some(spl),
        },
    );
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let expected = settled_pool(&f, &config, &skr_params(0), 0, 1, true);
    for (k, amount) in [
        (spl.winner_ata, expected.quarter_prize[0]),
        (spl.fee_ata, expected.platform_fee),
        (spl.creator_ata, expected.creator_fee),
    ] {
        let a = account_of(&result, &k);
        assert_eq!(a.owner, token_2022_program_id());
        assert_eq!(decode_token_amount(a), amount);
    }
    assert_eq!(
        decode_pool(account_of(&result, &pool_key(&f, &pool))),
        expected
    );
}

// ---------------------------------------------------------------------------
// close_pool
// ---------------------------------------------------------------------------

/// Accounts for a `close_pool` on a SOL pool whose vault holds `vault_lamports`.
fn close_setup(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    pool: &Pool,
    vault_lamports: u64,
) -> Vec<(Pubkey, solana_account::Account)> {
    let mut accounts = settlement_accounts(f, m, &f.expected_config(), pool, 4, None);
    set_account(
        &mut accounts,
        vault_pda(&pool_key(f, pool)).0,
        sol_vault_account(vault_lamports),
    );
    accounts
}

#[test]
fn close_pool_sol_sends_both_rents_to_the_fee_wallet() {
    // PROGRAM §4.5; ARCHITECTURE › Pool creation "pool and vault rent go to the platform".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard_settled(&f, 4, true);
    assert_eq!(pool.sponsorships_open, 0);
    let accounts = close_setup(&f, &m, &pool, rent_for(0));
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let s0 = lamports_of(&accounts, &f.stranger);
    let result = m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &accounts,
        &[Check::success()],
    );
    let key = pool_key(&f, &pool);
    let closed = account_of(&result, &key);
    assert_eq!((closed.lamports, closed.data.len()), (0, 0));
    assert_eq!(account_of(&result, &vault_pda(&key).0).lamports, 0);
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        rent_for(0) + rent_for(Pool::SIZE)
    );
    assert_eq!(
        account_of(&result, &f.stranger).lamports,
        s0,
        "Mollusk charges no fee"
    );
    assert_eq!(event_names(&result), ["PoolClosed"]);
    let ev: PoolClosed = emitted_event(&result).expect("PoolClosed");
    assert_eq!(
        (ev.pool, ev.destination, ev.dust),
        (to_a(&key), to_a(&f.fee_wallet), 0)
    );
    assert_eq!(ev.time, T0);
}

#[test]
fn close_pool_sol_sweeps_the_dust() {
    // PROGRAM §5.3 Settled row; BUILD-PLAN "Dust … goes to the platform at close".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params(0),
        1_000_000_003,
        4,
        true,
    );
    let mut pool = pool;
    pool.sponsorships_open = 0; // Step 7 closes the sponsorship; planted closed here
    assert_eq!(pool.unpaid_prize_pool, 2);
    let accounts = close_setup(&f, &m, &pool, rent_for(0) + 2);
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let result = m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        rent_for(0) + 2 + rent_for(Pool::SIZE)
    );
    let ev: PoolClosed = emitted_event(&result).expect("PoolClosed");
    assert_eq!(ev.dust, 2);
}

#[test]
fn close_pool_abandoned_goes_to_the_creator() {
    // PROGRAM §5.3 Abandoned rows; ARCHITECTURE › Pool creation "The exception".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let mut pool = standard(&f);
    pool.status = PoolStatus::Returned;
    pool.returned = (1u32 << 25) - 1;
    pool.abandoned = true;
    let accounts = close_setup(&f, &m, &pool, rent_for(0) + 3);
    let c0 = lamports_of(&accounts, &f.creator);
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let result = m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        account_of(&result, &f.creator).lamports - c0,
        rent_for(0) + 3 + rent_for(Pool::SIZE)
    );
    assert_eq!(account_of(&result, &f.fee_wallet).lamports, fw0);
    let ev: PoolClosed = emitted_event(&result).expect("PoolClosed");
    assert_eq!((ev.destination, ev.dust), (to_a(&f.creator), 3));
}

#[test]
fn close_pool_needs_a_terminal_pool_without_open_sponsorships_or_outstanding_boxes() {
    // PROGRAM §4.5 checks in order: PoolNotTerminal; SponsorshipsStillOpen; BoxesStillOutstanding.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    for pool in [
        pool_with(&f, PoolStatus::Open, 3, &f.buyer),
        pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2),
        standard(&f),
    ] {
        let accounts = close_setup(&f, &m, &pool, vault_balance_for(&pool));
        assert_eq!(
            custom_error(
                &m.process_instruction(&close_pool_ix(&f, &f.stranger, &pool, None), &accounts)
            ),
            Some(err(E::PoolNotTerminal)),
            "{:?}",
            pool.status
        );
    }
    let mut sponsored = standard_settled(&f, 4, true);
    sponsored.sponsorships_open = 1;
    let accounts = close_setup(&f, &m, &sponsored, rent_for(0));
    let r = m.process_instruction(&close_pool_ix(&f, &f.stranger, &sponsored, None), &accounts);
    assert_eq!(custom_error(&r), Some(err(E::SponsorshipsStillOpen)));
    assert_eq!(
        account_of(&r, &vault_pda(&pool_key(&f, &sponsored)).0).lamports,
        rent_for(0),
        "nothing moved"
    );

    for status in [PoolStatus::Returned, PoolStatus::Split] {
        let mut pool = standard(&f);
        pool.status = status;
        pool.returned = ((1u32 << 25) - 1) & !(1 << 7); // box 7 (buyer's) outstanding
        let accounts = close_setup(&f, &m, &pool, rent_for(0));
        assert_eq!(
            custom_error(
                &m.process_instruction(&close_pool_ix(&f, &f.stranger, &pool, None), &accounts)
            ),
            Some(err(E::BoxesStillOutstanding)),
            "{status:?}"
        );
    }
    let mut all_back = standard(&f);
    all_back.status = PoolStatus::Returned;
    all_back.returned = (1u32 << 25) - 1;
    let accounts = close_setup(&f, &m, &all_back, rent_for(0));
    m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &all_back, None),
        &accounts,
        &[Check::success()],
    );
}

#[test]
fn close_pool_with_the_wrong_fee_wallet_or_creator_is_fee_account_mismatch() {
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard_settled(&f, 4, true);
    let accounts = close_setup(&f, &m, &pool, rent_for(0));
    // ClosePool account order: payer 0, config 1, pool 2, vault 3, fee_wallet 4, creator 5.
    for index in [4usize, 5] {
        let mut ix = close_pool_ix(&f, &f.stranger, &pool, None);
        ix.accounts[index].pubkey = f.stranger;
        assert_eq!(
            custom_error(&m.process_instruction(&ix, &accounts)),
            Some(err(E::FeeAccountMismatch))
        );
    }
}

#[test]
fn close_pool_ore_sweeps_the_dust_creates_the_ata_and_closes_the_vault() {
    // PROGRAM §4.5 "SPL: close_account after the token balance is swept".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 4, true);
    let key = pool_key(&f, &pool);
    let dest_ata = ata(&f.fee_wallet, &f.ore_mint, &token_program_id());
    let spl = SplClose {
        mint: f.ore_mint,
        token_program: token_program_id(),
        destination_ata: dest_ata,
    };
    let mut accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, 4, Some(2));
    set_account(&mut accounts, dest_ata, system_account(0));
    let s0 = lamports_of(&accounts, &f.stranger);
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let result = m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, Some(spl)),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        s0 - account_of(&result, &f.stranger).lamports,
        rent_for(TOKEN_ACCOUNT_LEN),
        "the payer paid the ATA"
    );
    assert_eq!(token_amount_of(&result, &dest_ata), 2);
    let vault = account_of(&result, &vault_pda(&key).0);
    assert_eq!((vault.lamports, vault.data.len()), (0, 0));
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        rent_for(TOKEN_ACCOUNT_LEN) + rent_for(Pool::SIZE)
    );
    assert_eq!(account_of(&result, &key).data.len(), 0);
    let ev: PoolClosed = emitted_event(&result).expect("PoolClosed");
    assert_eq!(ev.dust, 2);
}

#[test]
fn close_pool_ore_with_nothing_to_sweep_creates_no_ata() {
    // this brief: no account is created for a transfer that does not happen.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 4, true);
    let dest_ata = ata(&f.fee_wallet, &f.ore_mint, &token_program_id());
    let spl = SplClose {
        mint: f.ore_mint,
        token_program: token_program_id(),
        destination_ata: dest_ata,
    };
    let mut accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, 4, Some(0));
    set_account(&mut accounts, dest_ata, system_account(0));
    let s0 = lamports_of(&accounts, &f.stranger);
    let fw0 = lamports_of(&accounts, &f.fee_wallet);
    let result = m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, Some(spl)),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(account_of(&result, &f.stranger).lamports, s0);
    assert_eq!(account_of(&result, &dest_ata).data.len(), 0);
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&f, &pool)).0).lamports,
        0
    );
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports - fw0,
        rent_for(TOKEN_ACCOUNT_LEN) + rent_for(Pool::SIZE)
    );
    let ev: PoolClosed = emitted_event(&result).expect("PoolClosed");
    assert_eq!(ev.dust, 0);
}

#[test]
fn close_pool_ore_with_the_creators_ata_as_destination_is_fee_account_mismatch() {
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 4, true);
    let wrong = ata(&f.creator, &f.ore_mint, &token_program_id());
    let spl = SplClose {
        mint: f.ore_mint,
        token_program: token_program_id(),
        destination_ata: wrong,
    };
    let mut accounts = settlement_accounts(&f, &m, &f.expected_config(), &pool, 4, Some(2));
    set_account(&mut accounts, wrong, system_account(0));
    assert_eq!(
        custom_error(
            &m.process_instruction(&close_pool_ix(&f, &f.stranger, &pool, Some(spl)), &accounts)
        ),
        Some(err(E::FeeAccountMismatch))
    );
}

#[test]
fn close_pool_skr_token_2022_closes_the_vault() {
    // PROGRAM §5.4: the Token-2022 close_account path.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let config = f.config_with_skr();
    let pool = settled_pool(&f, &config, &skr_params(0), 0, 4, true);
    let dest_ata = ata(&f.fee_wallet, &f.skr_mint, &token_2022_program_id());
    let spl = SplClose {
        mint: f.skr_mint,
        token_program: token_2022_program_id(),
        destination_ata: dest_ata,
    };
    let mut accounts = settlement_accounts(&f, &m, &config, &pool, 4, Some(0));
    set_account(&mut accounts, dest_ata, system_account(0));
    let result = m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, Some(spl)),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&f, &pool)).0).lamports,
        0
    );
    assert_eq!(account_of(&result, &pool_key(&f, &pool)).data.len(), 0);
}

#[test]
fn settle_on_a_closed_pool_fails_without_a_panic() {
    // PROGRAM §9 terminal: the account is gone.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = standard_settled(&f, 4, true);
    let accounts = close_setup(&f, &m, &pool, rent_for(0));
    let closed = m.process_and_validate_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &accounts,
        &[Check::success()],
    );
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts::default(),
    );
    let r = m.process_instruction(&ix, &closed.resulting_accounts);
    assert!(r.program_result.is_err());
    let code = custom_error(&r);
    assert!(
        code == Some(anchor(ErrorCode::AccountNotInitialized))
            || code == Some(anchor(ErrorCode::AccountOwnedByWrongProgram)),
        "{code:?}"
    );
}

#[test]
fn events_one_per_instruction_never_the_token_cpis() {
    // PROGRAM §7.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = drawn_pool_for_settlement(&f, &f.expected_config(), &ore_params(0), 0);
    let (spl, accounts) = ore_setup(&f, &m, &pool, 1, 25 * PRICE_ORE, &f.buyer_2, false);
    let ix = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        1,
        &f.buyer_2,
        SettleAccounts {
            integrator: None,
            spl: Some(spl),
        },
    );
    let r = m.process_instruction(&ix, &accounts);
    assert!(
        r.inner_instructions.len() > 4,
        "ATA creates and transfers are inner instructions"
    );
    assert_eq!(event_names(&r), ["QuarterSettled"]);
    assert_eq!(emitted_event_count(&r), 1);
    let closed_pool = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 4, true);
    let dest_ata = ata(&f.fee_wallet, &f.ore_mint, &token_program_id());
    let mut accounts = settlement_accounts(&f, &m, &f.expected_config(), &closed_pool, 4, Some(2));
    set_account(&mut accounts, dest_ata, system_account(0));
    let r = m.process_instruction(
        &close_pool_ix(
            &f,
            &f.stranger,
            &closed_pool,
            Some(SplClose {
                mint: f.ore_mint,
                token_program: token_program_id(),
                destination_ata: dest_ata,
            }),
        ),
        &accounts,
    );
    assert_eq!(event_names(&r), ["PoolClosed"]);
    assert_eq!(emitted_event_count(&r), 1);
}

#[test]
fn layout_and_errors_unchanged() {
    // item 2 of "Read this first".
    assert_eq!(Pool::SIZE, 1442);
    assert_eq!(err(E::VarCommitMismatch), 6064);
    assert_eq!(err(E::PoolNotDrawn), 6028);
    assert_eq!(err(E::WinnerMismatch), 6044);
    assert_eq!(err(E::FeeAccountMismatch), 6045);
    assert_eq!(err(E::PoolNotTerminal), 6055);
}
