//! `split`, PROGRAM §4.6, under Mollusk. Every spec number cites its line.
//! Run after `anchor build --arch v3`.
//!
//! The standard pool after Q1 (`settled_pool(Standard, 1, true)`): prize_pool 1.1 SOL,
//! Q1 0.22 paid, `unpaid_prize_pool` 880,000,000 → `split_amount` 35,200,000 per box
//! (PROGRAM §5.3 Split row), so 176M / 352M / 352M for 5 / 10 / 10 boxes. The sponsored dust
//! pool (1,000,000,013 sponsored): unpaid 1,680,000,011 → 67,200,000 per box, 11 left.

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::{Check, InstructionResult};
use mollusk_svm::Mollusk;
use mybarpool::{
    BoxesSplit, CreatePoolParams, GameRecord, GameStatus, MybarpoolError as E, PayoutPreset, Pool,
    PoolClosed, PoolStatus, SponsorshipClosed,
};
use solana_account::Account;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

fn custom(code: u32) -> solana_program_error::ProgramError {
    solana_program_error::ProgramError::Custom(code)
}

const UNPAID_AFTER_Q1: u64 = 880_000_000; // 1.1 SOL − 0.22
const SPLIT_AMOUNT: u64 = 35_200_000; // 880M / 25
const DUST_SPONSORED: u64 = 1_000_000_013;
const DUST_UNPAID: u64 = 1_680_000_011; // 2,100,000,013 − 420,000,002 (Q1 of the dust pool)
const DUST_SPLIT: u64 = 67_200_000; // 1,680,000,011 / 25
const DUST: u64 = 11;

fn pool_key(pool: &Pool) -> Pubkey {
    pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0
}

fn lamports_of(accounts: &[(Pubkey, Account)], key: &Pubkey) -> u64 {
    accounts
        .iter()
        .find(|(k, _)| k == key)
        .map_or(0, |(_, a)| a.lamports)
}

fn delta(result: &InstructionResult, accounts: &[(Pubkey, Account)], key: &Pubkey) -> i128 {
    i128::from(account_of(result, key).lamports) - i128::from(lamports_of(accounts, key))
}

fn after_q1(f: &Fixture) -> Pool {
    settled_pool(f, &f.expected_config(), &sol_params(0), 0, 1, true)
}

fn setup(f: &Fixture, m: &Mollusk, pool: &Pool, record: &GameRecord) -> Vec<(Pubkey, Account)> {
    returns_accounts(f, m, &f.expected_config(), pool, record, 0, None)
}

fn sp(f: &Fixture, pool: &Pool, owners: &[Pubkey]) -> Instruction {
    let batch: Vec<_> = owners.iter().map(|o| (*o, None)).collect();
    split_ix(&f.keeper, pool, &standard_game(), &batch, None)
}

fn ok(m: &Mollusk, ix: &Instruction, accounts: &[(Pubkey, Account)]) -> InstructionResult {
    m.process_and_validate_instruction(ix, accounts, &[Check::success()])
}

fn fails_with(
    m: &Mollusk,
    ix: &Instruction,
    accounts: &[(Pubkey, Account)],
    code: u32,
) -> InstructionResult {
    m.process_and_validate_instruction(ix, accounts, &[Check::err(custom(code))])
}

fn three(f: &Fixture) -> [Pubkey; 3] {
    [f.creator, f.buyer, f.buyer_2]
}

#[test]
fn split_is_the_worked_case() {
    // PROGRAM §5.3 Split row: 880M / 25 = 35,200,000 per box; the split touches only
    // `unpaid_prize_pool`, the fee wallet keeps its fee.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = after_q1(&f);
    assert_eq!(pool.unpaid_prize_pool, UNPAID_AFTER_Q1);
    let accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(1));
    let key = pool_key(&pool);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + UNPAID_AFTER_Q1
    );
    let result = ok(&m, &sp(&f, &pool, &three(&f)), &accounts);
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.split_amount, SPLIT_AMOUNT);
    assert_eq!(after.status, PoolStatus::Split);
    assert_eq!(after.unpaid_prize_pool, 0);
    assert_eq!(after.returned, 0x1FF_FFFF);
    assert!(after.fees_paid);
    let mut expected = pool.clone();
    expected.split_amount = SPLIT_AMOUNT;
    expected.status = PoolStatus::Split;
    expected.unpaid_prize_pool = 0;
    expected.returned = 0x1FF_FFFF;
    assert_eq!(after, expected, "whole-struct");
    assert_eq!(
        delta(&result, &accounts, &f.creator),
        i128::from(5 * SPLIT_AMOUNT)
    );
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(10 * SPLIT_AMOUNT)
    );
    assert_eq!(
        delta(&result, &accounts, &f.buyer_2),
        i128::from(10 * SPLIT_AMOUNT)
    );
    assert_eq!(delta(&result, &accounts, &f.fee_wallet), 0);
    assert_eq!(
        account_of(&result, &vault_pda(&key).0).lamports,
        rent_for(0)
    );
    assert_eq!(event_names(&result), ["BoxesSplit"; 3]);
    let ev: BoxesSplit = emitted_event(&result).expect("BoxesSplit");
    assert_eq!(
        (ev.time, ev.pool, ev.owner, ev.boxes, ev.amount),
        (
            SCHEDULED + 7_200,
            to_a(&key),
            to_a(&f.creator),
            vec![0, 1, 2, 3, 4],
            5 * SPLIT_AMOUNT
        )
    );
}

#[test]
fn split_leaves_the_dust_then_close_sponsorship_and_close_pool_sweep_it() {
    // PROGRAM §3.4 Split invariant; §4.6 "What the division leaves": 11 lamports stay in the
    // vault, the Sponsorship closes for its rent only, close_pool sweeps the dust.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params(0),
        DUST_SPONSORED,
        1,
        true,
    );
    assert_eq!(pool.unpaid_prize_pool, DUST_UNPAID);
    assert_eq!(pool.sponsorships_open, 1);
    let key = pool_key(&pool);
    let mut accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(1));
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + DUST_UNPAID
    );
    set_account(
        &mut accounts,
        sponsorship_pda(&key, &f.sponsor).0,
        sponsorship_account(&key, &f.sponsor, DUST_SPONSORED),
    );
    let split = ok(&m, &sp(&f, &pool, &three(&f)), &accounts);
    let after = decode_pool(account_of(&split, &key));
    assert_eq!(after.split_amount, DUST_SPLIT);
    assert_eq!(after.unpaid_prize_pool, DUST);
    assert_eq!(
        account_of(&split, &vault_pda(&key).0).lamports,
        rent_for(0) + DUST
    );

    // close_pool first: the sponsorship is still open.
    let early = m.process_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &split.resulting_accounts,
    );
    assert_eq!(custom_error(&early), Some(err(E::SponsorshipsStillOpen)));

    let s0 = account_of(&split, &f.sponsor).lamports;
    let closed_s = ok(
        &m,
        &close_sponsorship_ix(&pool, &f.sponsor, None),
        &split.resulting_accounts,
    );
    assert_eq!(
        account_of(&closed_s, &f.sponsor).lamports - s0,
        rent_for(mybarpool::Sponsorship::SIZE),
        "rent only, the sponsorship was spent"
    );
    assert_eq!(
        decode_pool(account_of(&closed_s, &key)).sponsorships_open,
        0
    );
    let ev: SponsorshipClosed = emitted_event(&closed_s).expect("SponsorshipClosed");
    assert_eq!((ev.sponsor, ev.amount), (to_a(&f.sponsor), DUST_SPONSORED));

    let fw0 = account_of(&closed_s, &f.fee_wallet).lamports;
    let closed = ok(
        &m,
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &closed_s.resulting_accounts,
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - fw0,
        rent_for(0) + DUST + rent_for(Pool::SIZE)
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.dust, DUST);
    assert_eq!(ev.destination, to_a(&f.fee_wallet));
}

#[test]
fn split_in_batches_and_the_success_rule() {
    // PROGRAM §4.6 "already splitting" and the success rule: two batches finish the pool, a
    // third pays nobody (NothingToReturn); an empty first call fixes the share and succeeds,
    // an empty second call does not.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = after_q1(&f);
    let key = pool_key(&pool);
    let accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(1));
    let first = ok(&m, &sp(&f, &pool, &[f.creator]), &accounts);
    assert_eq!(
        decode_pool(account_of(&first, &key)).unpaid_prize_pool,
        704_000_000
    );
    let second = ok(
        &m,
        &sp(&f, &pool, &[f.buyer, f.buyer_2]),
        &first.resulting_accounts,
    );
    assert_eq!(decode_pool(account_of(&second, &key)).unpaid_prize_pool, 0);
    let third = m.process_instruction(
        &sp(&f, &pool, &[f.creator, f.buyer]),
        &second.resulting_accounts,
    );
    assert_eq!(custom_error(&third), Some(err(E::NothingToReturn)));

    let empty_first = ok(&m, &sp(&f, &pool, &[]), &accounts);
    let after = decode_pool(account_of(&empty_first, &key));
    assert_eq!(
        (after.status, after.split_amount),
        (PoolStatus::Split, SPLIT_AMOUNT)
    );
    assert!(event_names(&empty_first).is_empty());
    let empty_second = m.process_instruction(&sp(&f, &pool, &[]), &empty_first.resulting_accounts);
    assert_eq!(custom_error(&empty_second), Some(err(E::NothingToReturn)));
}

#[test]
fn split_needs_a_suspended_game() {
    // split without the suspended mark fails: Scheduled, Postponed, Final.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = after_q1(&f);
    for record in [
        record_with_quarters(1),
        marked_record(GameStatus::Postponed),
        record_with_quarters(4),
    ] {
        let accounts = setup(&f, &m, &pool, &record);
        fails_with(
            &m,
            &sp(&f, &pool, &three(&f)),
            &accounts,
            err(E::NotSuspended),
        );
    }
}

#[test]
fn split_before_fees_were_paid_is_not_splittable() {
    // PROGRAM §4.6 "fees_paid (NotSplittable)": a Drawn pool with nothing settled, a FinalOnly
    // pool after three empty settlements, an Open and a Locked pool.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let final_only = settled_pool(
        &f,
        &f.expected_config(),
        &CreatePoolParams {
            preset: PayoutPreset::FinalOnly,
            ..sol_params(0)
        },
        0,
        3,
        false,
    );
    let pools = [
        pool_with_owners(
            &f,
            &f.expected_config(),
            &sol_params(0),
            PoolStatus::Drawn,
            &full_plan(&f),
        ),
        final_only,
        pool_with_owners(
            &f,
            &f.expected_config(),
            &sol_params(0),
            PoolStatus::Open,
            &unfilled_15_plan(&f),
        ),
        pool_with_owners(
            &f,
            &f.expected_config(),
            &sol_params(0),
            PoolStatus::Locked,
            &full_plan(&f),
        ),
    ];
    for pool in pools {
        let accounts = setup(&f, &m, &pool, &marked_record(GameStatus::Suspended));
        fails_with(
            &m,
            &sp(&f, &pool, &three(&f)),
            &accounts,
            err(E::NotSplittable),
        );
    }
}

#[test]
fn split_outside_drawn_or_split_is_not_splittable() {
    // PROGRAM §4.6: Settled; a planted Returned with fees_paid (impossible on-chain) pins the
    // status check.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let mut settled = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 4, true);
    settled.status = PoolStatus::Settled;
    let mut returned = after_q1(&f);
    returned.status = PoolStatus::Returned;
    for pool in [settled, returned] {
        let accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(1));
        fails_with(
            &m,
            &sp(&f, &pool, &three(&f)),
            &accounts,
            err(E::NotSplittable),
        );
    }
}

#[test]
fn split_by_the_admin_is_unauthorized() {
    // PROGRAM §10 Authority.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = after_q1(&f);
    let accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(1));
    let batch: Vec<_> = three(&f).iter().map(|o| (*o, None)).collect();
    fails_with(
        &m,
        &split_ix(&f.admin, &pool, &standard_game(), &batch, None),
        &accounts,
        err(E::Unauthorized),
    );
}

#[test]
fn a_split_pool_can_no_longer_be_settled() {
    // PROGRAM §9 "a Split pool can no longer be settled": settle(2) → PoolNotDrawn.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = after_q1(&f);
    let accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(1));
    let split = ok(&m, &sp(&f, &pool, &three(&f)), &accounts);
    let mut after = split.resulting_accounts.clone();
    set_account(
        &mut after,
        standard_game(),
        game_record_account(&record_with_quarters(2)),
    );
    let settle = settle_ix(
        &f,
        &f.keeper,
        &pool,
        &standard_game(),
        2,
        &f.creator,
        SettleAccounts::default(),
    );
    let r = m.process_instruction(&settle, &after);
    assert_eq!(custom_error(&r), Some(err(E::PoolNotDrawn)));
}

#[test]
fn split_ore_after_q1_creates_the_atas() {
    // PROGRAM §5.4: ORE after Q1, vault 88e9, split 3.52e9 per box, three ATAs created at the
    // keeper's expense.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 1, true);
    assert_eq!(pool.unpaid_prize_pool, 88_000_000_000);
    let key = pool_key(&pool);
    let spl = SplBatch::of(&pool);
    let mut accounts = returns_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &suspended_record_with_quarters(1),
        0,
        Some(88_000_000_000),
    );
    let batch: Vec<_> = three(&f)
        .iter()
        .map(|w| {
            set_account(&mut accounts, spl.ata(w), system_account(0));
            (*w, Some(spl.ata(w)))
        })
        .collect();
    let k0 = lamports_of(&accounts, &f.keeper);
    let result = ok(
        &m,
        &split_ix(&f.keeper, &pool, &standard_game(), &batch, Some(spl)),
        &accounts,
    );
    assert_eq!(
        k0 - account_of(&result, &f.keeper).lamports,
        3 * rent_for(TOKEN_ACCOUNT_LEN)
    );
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.split_amount, 3_520_000_000);
    assert_eq!(
        decode_token_amount(account_of(&result, &spl.ata(&f.creator))),
        17_600_000_000
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &spl.ata(&f.buyer))),
        35_200_000_000
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &spl.ata(&f.buyer_2))),
        35_200_000_000
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &vault_pda(&key).0)),
        0
    );
}

#[test]
fn return_sponsorship_on_a_split_pool_is_fees_already_paid() {
    // suspended after Q1 … return_sponsorship fails.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params(0),
        DUST_SPONSORED,
        1,
        true,
    );
    let key = pool_key(&pool);
    let mut accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(1));
    set_account(
        &mut accounts,
        sponsorship_pda(&key, &f.sponsor).0,
        sponsorship_account(&key, &f.sponsor, DUST_SPONSORED),
    );
    let split = ok(&m, &sp(&f, &pool, &three(&f)), &accounts);
    let r = m.process_instruction(
        &return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, None, None),
        &split.resulting_accounts,
    );
    assert_eq!(custom_error(&r), Some(err(E::FeesAlreadyPaid)));
}

#[test]
fn split_batch_shape_errors() {
    // PROGRAM §4.6 preamble on the split path: an odd SPL list, a wrong ATA.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = settled_pool(&f, &f.expected_config(), &ore_params(0), 0, 1, true);
    let spl = SplBatch::of(&pool);
    let mut accounts = returns_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &suspended_record_with_quarters(1),
        0,
        Some(88_000_000_000),
    );
    for w in three(&f) {
        set_account(&mut accounts, spl.ata(&w), system_account(0));
    }
    let mut odd = split_ix(&f.keeper, &pool, &standard_game(), &[], Some(spl));
    odd.accounts
        .push(solana_instruction::AccountMeta::new(f.creator, false));
    fails_with(&m, &odd, &accounts, ErrorCode::AccountNotEnoughKeys as u32);
    let wrong = [(f.buyer, Some(spl.ata(&f.buyer_2)))];
    let r = fails_with(
        &m,
        &split_ix(&f.keeper, &pool, &standard_game(), &wrong, Some(spl)),
        &accounts,
        ErrorCode::RequireKeysEqViolated as u32,
    );
    assert_eq!(decode_pool(account_of(&r, &pool_key(&pool))), pool);
}
