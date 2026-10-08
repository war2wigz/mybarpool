//! `return_boxes`, `return_sponsorship` and `cancel_pool`, PROGRAM §4.6,
//! under Mollusk with the Token, Token-2022 and Associated Token programs
//! loaded. Every spec number cites its line. Run after `anchor build --arch v3`.
//!
//! The standard pool (`sol_params`: PRICE 0.05 SOL): a return is always the full purchase
//! price, so the `UNFILLED_15` pool returns 250,000,000 to the creator (5 boxes) and
//! 500,000,000 to `buyer` (10 boxes), and the `FULL` pool 250M / 500M / 500M (ARCHITECTURE ›
//! Returns "a return is always the full purchase price").

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::{Check, InstructionResult};
use mollusk_svm::Mollusk;
use mybarpool::{
    BoxesReturned, GameRecord, GameStatus, MybarpoolError as E, PayoutPreset, Pool, PoolCancelled,
    PoolClosed, PoolStatus, SponsorshipReturned,
};
use solana_account::Account;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

fn custom(code: u32) -> solana_program_error::ProgramError {
    solana_program_error::ProgramError::Custom(code)
}

fn anchor(code: ErrorCode) -> u32 {
    code as u32
}

const FULL_RETURN: u64 = 1_250_000_000; // 25 × 0.05 SOL
const CREATOR_RETURN: u64 = 250_000_000; // 5 boxes
const BUYER_RETURN: u64 = 500_000_000; // 10 boxes
const SPONSORED: u64 = 1_000_000_000; // the 1 SOL sponsorship

fn pool_key(pool: &Pool) -> Pubkey {
    pool_pda(&to_m(&pool.game), &to_m(&pool.creator), pool.nonce).0
}

fn lamports_of(accounts: &[(Pubkey, Account)], key: &Pubkey) -> u64 {
    accounts
        .iter()
        .find(|(k, _)| k == key)
        .map_or(0, |(_, a)| a.lamports)
}

fn counter(f: &Fixture) -> Pubkey {
    counter_pda(&f.creator, &standard_game()).0
}

fn sol_pool(f: &Fixture, status: PoolStatus, plan: &OwnerPlan) -> Pool {
    pool_with_owners(f, &f.expected_config(), &sol_params(0), status, plan)
}

fn setup(
    f: &Fixture,
    m: &Mollusk,
    pool: &Pool,
    record: &GameRecord,
    open_count: u8,
) -> Vec<(Pubkey, Account)> {
    returns_accounts(f, m, &f.expected_config(), pool, record, open_count, None)
}

/// A SOL `return_boxes` by the keeper with a plain owner batch.
fn rb(f: &Fixture, pool: &Pool, counter: Option<Pubkey>, owners: &[Pubkey]) -> Instruction {
    let batch: Vec<_> = owners.iter().map(|o| (*o, None)).collect();
    return_boxes_ix(&f.keeper, pool, &standard_game(), counter, &batch, None)
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

fn delta(result: &InstructionResult, accounts: &[(Pubkey, Account)], key: &Pubkey) -> i128 {
    i128::from(account_of(result, key).lamports) - i128::from(lamports_of(accounts, key))
}

fn returned_event(result: &InstructionResult, owner: &Pubkey) -> BoxesReturned {
    use anchor_lang::{AnchorDeserialize, Discriminator};
    event_payloads(result)
        .into_iter()
        .filter(|p| &p[..8] == BoxesReturned::DISCRIMINATOR)
        .map(|p| BoxesReturned::deserialize(&mut &p[8..]).expect("decode"))
        .find(|e| e.owner == to_a(owner))
        .expect("BoxesReturned for owner")
}

// ---------------------------------------------------------------------------
// return_boxes: unfilled at kickoff
// ---------------------------------------------------------------------------

#[test]
fn return_boxes_unfilled_is_the_worked_case() {
    // PROGRAM §4.6 unfilled; ARCHITECTURE › Returns "Unfilled at kickoff": at the recorded
    // kickoff exactly, the Open pool goes Returned, the counter drops, every owner gets the
    // purchase price, the vault ends at its rent floor.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    assert_eq!((pool.sold, pool.creator_boxes), (15, 5));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let key = pool_key(&pool);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + 750_000_000
    );
    let ix = rb(&f, &pool, Some(counter(&f)), &[f.creator, f.buyer]);
    let result = ok(&m, &ix, &accounts);

    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.status, PoolStatus::Returned);
    assert_eq!(after.returned, 0x7FFF);
    assert_eq!(after.returned, returned_bits(&(0..15).collect::<Vec<_>>()));
    assert!(!after.cancelled_by_admin && !after.abandoned);
    let mut expected = pool.clone();
    expected.status = PoolStatus::Returned;
    expected.returned = 0x7FFF;
    assert_eq!(after, expected, "whole-struct: nothing else changed");
    assert_eq!(
        decode_counter(account_of(&result, &counter(&f))).open_count,
        0
    );
    assert_eq!(
        delta(&result, &accounts, &f.creator),
        i128::from(CREATOR_RETURN)
    );
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(BUYER_RETURN)
    );
    assert_eq!(
        account_of(&result, &vault_pda(&key).0).lamports,
        rent_for(0)
    );
    assert_eq!(delta(&result, &accounts, &f.keeper), 0, "no ATA on SOL");
    assert_eq!(event_names(&result), ["BoxesReturned", "BoxesReturned"]);
    let c = returned_event(&result, &f.creator);
    assert_eq!(
        (c.time, c.pool, c.boxes, c.amount),
        (SCHEDULED, to_a(&key), vec![0, 1, 2, 3, 4], CREATOR_RETURN)
    );
    let b = returned_event(&result, &f.buyer);
    assert_eq!(b.boxes, (5..=14).collect::<Vec<u8>>());
    assert_eq!(b.amount, BUYER_RETURN);
}

#[test]
fn return_boxes_unfilled_before_kickoff_is_not_returnable() {
    // PROGRAM §10 Timing "no return_boxes (unfilled) before it".
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED - 1);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let ix = rb(&f, &pool, Some(counter(&f)), &[f.creator, f.buyer]);
    let result = fails_with(&m, &ix, &accounts, err(E::NotReturnable));
    assert_eq!(delta(&result, &accounts, &f.creator), 0);
    assert_eq!(decode_pool(account_of(&result, &pool_key(&pool))), pool);
}

#[test]
fn return_boxes_unfilled_reads_the_recorded_kickoff() {
    // PROGRAM §4.6 "now ≥ game.recorded_kickoff": after an update_kickoff to +1h, the
    // half-hour mark is too early and the new kickoff is the first accepted second.
    let f = Fixture::new();
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let record = moved_record(SCHEDULED + 3_600);
    let ix = rb(&f, &pool, Some(counter(&f)), &[f.creator, f.buyer]);
    let early = mollusk_for_settlement(SCHEDULED + 1_800);
    fails_with(
        &early,
        &ix,
        &setup(&f, &early, &pool, &record, 1),
        err(E::NotReturnable),
    );
    let at = mollusk_for_settlement(SCHEDULED + 3_600);
    ok(&at, &ix, &setup(&f, &at, &pool, &record, 1));
}

#[test]
fn return_boxes_unfilled_needs_the_creators_counter() {
    // PROGRAM §4.6 "The counter": absent → ConstraintAccountIsNone; a wrong address fails the
    // seeds; a planted zero is a checked underflow.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    fails_with(
        &m,
        &rb(&f, &pool, None, &[f.creator]),
        &accounts,
        anchor(ErrorCode::ConstraintAccountIsNone),
    );
    let wrong = counter_pda(&f.buyer, &standard_game()).0;
    let mut with_wrong = accounts.clone();
    set_account(
        &mut with_wrong,
        wrong,
        counter_account(&f.buyer, &standard_game(), 1),
    );
    fails_with(
        &m,
        &rb(&f, &pool, Some(wrong), &[f.creator]),
        &with_wrong,
        anchor(ErrorCode::ConstraintSeeds),
    );
    let zero = setup(&f, &m, &pool, &standard_record(), 0);
    fails_with(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.creator]),
        &zero,
        err(E::MathOverflow),
    );
}

#[test]
fn return_boxes_on_an_empty_pool_then_close_pool() {
    // PROGRAM §4.6 "An unfilled pool with no box sold": the status change alone is a success,
    // no event; close_pool then returns both rents to the fee wallet.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &empty_plan());
    assert_eq!(pool.sold, 0);
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let key = pool_key(&pool);
    assert_eq!(lamports_of(&accounts, &vault_pda(&key).0), rent_for(0));
    let result = ok(&m, &rb(&f, &pool, Some(counter(&f)), &[]), &accounts);
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.status, PoolStatus::Returned);
    assert_eq!(
        decode_counter(account_of(&result, &counter(&f))).open_count,
        0
    );
    assert!(event_names(&result).is_empty());
    let fw0 = account_of(&result, &f.fee_wallet).lamports;
    let closed = ok(
        &m,
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &result.resulting_accounts,
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - fw0,
        rent_for(0) + rent_for(Pool::SIZE)
    );
    assert_eq!(account_of(&closed, &key).data.len(), 0);
}

#[test]
fn return_boxes_continues_across_calls_and_refuses_a_repeat() {
    // PROGRAM §4.6 "already returning"; a repeated return_boxes for the same owners moves
    // nothing (NothingToReturn).
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let key = pool_key(&pool);
    let first = ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.creator]),
        &accounts,
    );
    assert_eq!(decode_pool(account_of(&first, &key)).returned, 0x1F);
    assert_eq!(
        delta(&first, &accounts, &f.creator),
        i128::from(CREATOR_RETURN)
    );
    // The second call is a continuation: status already Returned, the counter slot is not
    // read (passed anyway, harmless).
    let second = ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.buyer]),
        &first.resulting_accounts,
    );
    assert_eq!(decode_pool(account_of(&second, &key)).returned, 0x7FFF);
    assert_eq!(
        account_of(&second, &vault_pda(&key).0).lamports,
        rent_for(0)
    );
    let third = m.process_instruction(
        &rb(&f, &pool, None, &[f.creator, f.buyer]),
        &second.resulting_accounts,
    );
    assert_eq!(custom_error(&third), Some(err(E::NothingToReturn)));
}

#[test]
fn return_boxes_duplicates_and_strangers_are_harmless() {
    // PROGRAM §4.6 "a duplicate or a stranger in the list is harmless": paid once, unpaid
    // strangers untouched; a batch that pays nobody on a pool already Returned is
    // NothingToReturn.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let dup = ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.buyer, f.buyer]),
        &accounts,
    );
    assert_eq!(delta(&dup, &accounts, &f.buyer), i128::from(BUYER_RETURN));
    assert_eq!(event_names(&dup), ["BoxesReturned"]);

    let mixed = ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.stranger, f.buyer]),
        &accounts,
    );
    assert_eq!(delta(&mixed, &accounts, &f.stranger), 0);
    assert_eq!(delta(&mixed, &accounts, &f.buyer), i128::from(BUYER_RETURN));
    assert_eq!(event_names(&mixed), ["BoxesReturned"]);

    // Already Returned with the creator's boxes outstanding.
    let stranger_only = m.process_instruction(
        &rb(&f, &pool, None, &[f.stranger]),
        &mixed.resulting_accounts,
    );
    assert_eq!(custom_error(&stranger_only), Some(err(E::NothingToReturn)));
    let empty = m.process_instruction(&rb(&f, &pool, None, &[]), &mixed.resulting_accounts);
    assert_eq!(custom_error(&empty), Some(err(E::NothingToReturn)));
}

#[test]
fn return_boxes_locked_or_drawn_on_a_scheduled_game_waits() {
    // ARCHITECTURE › Returns "Ambiguous game status … stays locked": however late, a Locked
    // or Drawn pool on a Scheduled game is not returnable.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 86_400);
    for status in [PoolStatus::Locked, PoolStatus::Drawn] {
        let pool = sol_pool(&f, status, &full_plan(&f));
        let accounts = setup(&f, &m, &pool, &standard_record(), 0);
        fails_with(
            &m,
            &rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]),
            &accounts,
            err(E::NotReturnable),
        );
    }
}

// ---------------------------------------------------------------------------
// return_boxes: marked games
// ---------------------------------------------------------------------------

#[test]
fn return_boxes_drawn_on_a_postponed_game_returns_every_box() {
    // PROGRAM §4.6 marked: every pre-settlement state including Drawn returns the full price;
    // the counter slot is never read on a Drawn pool.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let pool = sol_pool(&f, PoolStatus::Drawn, &full_plan(&f));
    let accounts = setup(&f, &m, &pool, &marked_record(GameStatus::Postponed), 0);
    let key = pool_key(&pool);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + FULL_RETURN
    );
    let result = ok(
        &m,
        &rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]),
        &accounts,
    );
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.status, PoolStatus::Returned);
    assert_eq!(after.returned, 0x1FF_FFFF);
    assert_eq!(
        delta(&result, &accounts, &f.creator),
        i128::from(CREATOR_RETURN)
    );
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(BUYER_RETURN)
    );
    assert_eq!(
        delta(&result, &accounts, &f.buyer_2),
        i128::from(BUYER_RETURN)
    );
    assert_eq!(
        account_of(&result, &vault_pda(&key).0).lamports,
        rent_for(0)
    );
    assert_eq!(event_names(&result).len(), 3);
    assert_eq!(
        decode_counter(account_of(&result, &counter(&f))).open_count,
        0,
        "the planted counter is untouched"
    );
}

#[test]
fn return_boxes_on_cancelled_suspended_and_locked_pools() {
    // PROGRAM §4.6 marked, suspended: Cancelled and Suspended (nothing posted) behave as
    // Postponed; a Locked pool returns the same amounts.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    for (status, record) in [
        (PoolStatus::Drawn, marked_record(GameStatus::Cancelled)),
        (PoolStatus::Drawn, marked_record(GameStatus::Suspended)),
        (PoolStatus::Locked, marked_record(GameStatus::Postponed)),
    ] {
        let pool = sol_pool(&f, status, &full_plan(&f));
        let accounts = setup(&f, &m, &pool, &record, 0);
        let result = ok(
            &m,
            &rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]),
            &accounts,
        );
        assert_eq!(
            delta(&result, &accounts, &f.creator),
            i128::from(CREATOR_RETURN)
        );
        assert_eq!(
            delta(&result, &accounts, &f.buyer_2),
            i128::from(BUYER_RETURN)
        );
        assert_eq!(
            account_of(&result, &vault_pda(&pool_key(&pool)).0).lamports,
            rent_for(0)
        );
    }
}

#[test]
fn return_boxes_final_only_after_three_empty_settlements_returns_the_price() {
    // PROGRAM §4.6 "a FinalOnly pool whose first three settlements moved nothing"; §10 Fees
    // "never paid on a returned pool": fees still in the vault, every owner gets the price.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3 * 2_700 + 600);
    let pool = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params_with_preset(PayoutPreset::FinalOnly),
        0,
        3,
        false,
    );
    assert!(!pool.fees_paid);
    let accounts = setup(&f, &m, &pool, &suspended_record_with_quarters(3), 0);
    let key = pool_key(&pool);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + FULL_RETURN
    );
    let result = ok(
        &m,
        &rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]),
        &accounts,
    );
    assert_eq!(
        delta(&result, &accounts, &f.creator),
        i128::from(CREATOR_RETURN),
        "the creator gets boxes only, no fee"
    );
    assert_eq!(delta(&result, &accounts, &f.fee_wallet), 0);
    assert_eq!(
        account_of(&result, &vault_pda(&key).0).lamports,
        rent_for(0)
    );
    assert!(!decode_pool(account_of(&result, &key)).fees_paid);
}

fn sol_params_with_preset(preset: PayoutPreset) -> mybarpool::CreatePoolParams {
    mybarpool::CreatePoolParams {
        preset,
        ..sol_params(0)
    }
}

#[test]
fn return_boxes_after_fees_were_paid_is_not_returnable() {
    // PROGRAM §4.6 "and !fees_paid": a suspended or postponed game after Q1 is a split, never
    // a return.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let pool = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 1, true);
    for record in [
        suspended_record_with_quarters(1),
        marked_record(GameStatus::Postponed),
    ] {
        let accounts = setup(&f, &m, &pool, &record, 0);
        fails_with(
            &m,
            &rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]),
            &accounts,
            err(E::NotReturnable),
        );
    }
}

#[test]
fn return_boxes_on_an_admin_cancelled_pool_before_kickoff() {
    // PROGRAM §4.6 cancelled: a Returned pool with cancelled_by_admin on a Scheduled game,
    // before kickoff, is returnable through the cancelled branch alone.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let mut pool = sol_pool(&f, PoolStatus::Drawn, &full_plan(&f));
    pool.status = PoolStatus::Returned;
    pool.cancelled_by_admin = true;
    let accounts = setup(&f, &m, &pool, &standard_record(), 0);
    let result = ok(&m, &rb(&f, &pool, None, &[f.buyer]), &accounts);
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(BUYER_RETURN)
    );
}

#[test]
fn return_boxes_on_settled_or_split_is_not_returnable() {
    // PROGRAM §4.6 "a Settled or Split pool satisfies none".
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 86_400);
    let settled = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 4, true);
    let mut split = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 1, true);
    split.status = PoolStatus::Split;
    split.split_amount = 35_200_000;
    for (pool, record) in [
        (settled, record_with_quarters(4)),
        (split, suspended_record_with_quarters(1)),
    ] {
        let accounts = setup(&f, &m, &pool, &record, 0);
        fails_with(
            &m,
            &rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]),
            &accounts,
            err(E::NotReturnable),
        );
    }
}

#[test]
fn return_boxes_by_anyone_but_the_keeper_is_unauthorized() {
    // PROGRAM §10 Authority: the admin included.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    for signer in [f.admin, f.stranger] {
        let ix = return_boxes_ix(
            &signer,
            &pool,
            &standard_game(),
            Some(counter(&f)),
            &[(f.creator, None)],
            None,
        );
        fails_with(&m, &ix, &accounts, err(E::Unauthorized));
    }
}

#[test]
fn return_boxes_while_paused_succeeds() {
    // ARCHITECTURE › Trust model: the pause flag never blocks a return.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let mut paused = f.expected_config();
    paused.paused = true;
    let accounts = returns_accounts(&f, &m, &paused, &pool, &standard_record(), 1, None);
    ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.creator, f.buyer]),
        &accounts,
    );
}

// ---------------------------------------------------------------------------
// return_boxes: SPL
// ---------------------------------------------------------------------------

fn token_amount_of(result: &InstructionResult, key: &Pubkey) -> u64 {
    decode_token_amount(account_of(result, key))
}

/// An ORE Drawn FULL pool on a postponed game, the vault holding the pot.
fn ore_postponed(f: &Fixture, m: &Mollusk) -> (Pool, SplBatch, Vec<(Pubkey, Account)>) {
    let pool = pool_with_owners(
        f,
        &f.expected_config(),
        &ore_params(0),
        PoolStatus::Drawn,
        &full_plan(f),
    );
    let spl = SplBatch::of(&pool);
    let accounts = returns_accounts(
        f,
        m,
        &f.expected_config(),
        &pool,
        &marked_record(GameStatus::Postponed),
        0,
        Some(25 * PRICE_ORE),
    );
    (pool, spl, accounts)
}

#[test]
fn return_boxes_ore_creates_the_missing_atas_and_pays() {
    // PROGRAM §5.4; ARCHITECTURE › Payouts "idempotent create": two of three ATAs missing →
    // two created at the keeper's expense, every owner paid the price.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let (pool, spl, mut accounts) = ore_postponed(&f, &m);
    let key = pool_key(&pool);
    assert_eq!(
        decode_token_amount(
            &accounts
                .iter()
                .find(|(k, _)| *k == vault_pda(&key).0)
                .unwrap()
                .1
        ),
        125_000_000_000
    );
    set_account(&mut accounts, spl.ata(&f.creator), system_account(0));
    set_account(
        &mut accounts,
        spl.ata(&f.buyer),
        token_account(&f.ore_mint, &f.buyer, 0),
    );
    set_account(&mut accounts, spl.ata(&f.buyer_2), system_account(0));
    let batch = [
        (f.creator, Some(spl.ata(&f.creator))),
        (f.buyer, Some(spl.ata(&f.buyer))),
        (f.buyer_2, Some(spl.ata(&f.buyer_2))),
    ];
    let ix = return_boxes_ix(&f.keeper, &pool, &standard_game(), None, &batch, Some(spl));
    let k0 = lamports_of(&accounts, &f.keeper);
    let result = ok(&m, &ix, &accounts);
    assert_eq!(
        k0 - account_of(&result, &f.keeper).lamports,
        2 * rent_for(TOKEN_ACCOUNT_LEN)
    );
    assert_eq!(
        token_amount_of(&result, &spl.ata(&f.creator)),
        25_000_000_000
    );
    assert_eq!(token_amount_of(&result, &spl.ata(&f.buyer)), 50_000_000_000);
    assert_eq!(
        token_amount_of(&result, &spl.ata(&f.buyer_2)),
        50_000_000_000
    );
    assert_eq!(token_amount_of(&result, &vault_pda(&key).0), 0);
    assert_eq!(
        account_of(&result, &spl.ata(&f.creator)).owner,
        token_program_id()
    );
    assert_eq!(decode_pool(account_of(&result, &key)).returned, 0x1FF_FFFF);
    assert_eq!(event_names(&result).len(), 3);
}

#[test]
fn return_boxes_ore_rejects_a_malformed_batch_before_moving_anything() {
    // PROGRAM §4.6 preamble: odd list, a token account that is not the owner's ATA, the
    // wrong token program, a missing mint.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let (pool, spl, mut accounts) = ore_postponed(&f, &m);
    for wallet in [f.creator, f.buyer, f.buyer_2] {
        set_account(&mut accounts, spl.ata(&wallet), system_account(0));
    }
    // Odd: the owner without its ATA — a raw meta list of length 1.
    let mut odd = return_boxes_ix(&f.keeper, &pool, &standard_game(), None, &[], Some(spl));
    odd.accounts
        .push(solana_instruction::AccountMeta::new(f.creator, false));
    fails_with(&m, &odd, &accounts, anchor(ErrorCode::AccountNotEnoughKeys));

    // buyer with buyer_2's ATA: refused before the CPI, nothing moved (creator first in the
    // list would have been paid otherwise — so put the bad entry first).
    let swapped = [
        (f.buyer, Some(spl.ata(&f.buyer_2))),
        (f.creator, Some(spl.ata(&f.creator))),
    ];
    let ix = return_boxes_ix(
        &f.keeper,
        &pool,
        &standard_game(),
        None,
        &swapped,
        Some(spl),
    );
    let r = fails_with(&m, &ix, &accounts, anchor(ErrorCode::RequireKeysEqViolated));
    assert_eq!(
        account_of(&r, &spl.ata(&f.creator)).data.len(),
        0,
        "nothing moved"
    );
    assert_eq!(decode_pool(account_of(&r, &pool_key(&pool))), pool);

    // Token-2022 as the token program.
    let wrong_program = SplBatch {
        mint: spl.mint,
        token_program: token_2022_program_id(),
    };
    let ix = return_boxes_ix(
        &f.keeper,
        &pool,
        &standard_game(),
        None,
        &[(f.creator, Some(spl.ata(&f.creator)))],
        Some(wrong_program),
    );
    fails_with(&m, &ix, &accounts, anchor(ErrorCode::RequireKeysEqViolated));

    // mint = None (the program id in its slot, index 6).
    let mut no_mint = return_boxes_ix(
        &f.keeper,
        &pool,
        &standard_game(),
        None,
        &[(f.creator, Some(spl.ata(&f.creator)))],
        Some(spl),
    );
    assert_eq!(no_mint.accounts[6].pubkey, f.ore_mint);
    no_mint.accounts[6].pubkey = program_id();
    fails_with(
        &m,
        &no_mint,
        &accounts,
        anchor(ErrorCode::ConstraintAccountIsNone),
    );
}

#[test]
fn return_boxes_skr_through_token_2022() {
    // PROGRAM §5.4: the Token-2022 path, new accounts owned by token_2022::ID.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let config = f.config_with_skr();
    let pool = pool_with_owners(
        &f,
        &config,
        &skr_params(0),
        PoolStatus::Drawn,
        &full_plan(&f),
    );
    let spl = SplBatch::of(&pool);
    assert_eq!(spl.token_program, token_2022_program_id());
    let mut accounts = returns_accounts(
        &f,
        &m,
        &config,
        &pool,
        &marked_record(GameStatus::Postponed),
        0,
        Some(25 * PRICE_SKR),
    );
    let batch: Vec<_> = [f.creator, f.buyer, f.buyer_2]
        .iter()
        .map(|w| {
            set_account(&mut accounts, spl.ata(w), system_account(0));
            (*w, Some(spl.ata(w)))
        })
        .collect();
    let ix = return_boxes_ix(&f.keeper, &pool, &standard_game(), None, &batch, Some(spl));
    let result = ok(&m, &ix, &accounts);
    assert_eq!(
        account_of(&result, &spl.ata(&f.buyer)).owner,
        token_2022_program_id()
    );
    assert_eq!(token_amount_of(&result, &spl.ata(&f.buyer)), 10 * PRICE_SKR);
    assert_eq!(token_amount_of(&result, &vault_pda(&pool_key(&pool)).0), 0);
}

#[test]
fn return_boxes_pays_twenty_five_distinct_owners_in_one_batch() {
    // Multi-transfer returns: 25 owners, 25 transfers of the price, 25 events.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let mut pool = sol_pool(&f, PoolStatus::Drawn, &full_plan(&f));
    let owners: Vec<Pubkey> = (0..25).map(|_| Pubkey::new_unique()).collect();
    for (i, o) in owners.iter().enumerate() {
        pool.owners[i] = to_a(o);
    }
    pool.creator_boxes = 0;
    let mut accounts = setup(&f, &m, &pool, &marked_record(GameStatus::Postponed), 0);
    for o in &owners {
        accounts.push((*o, system_account(LAMPORTS_PER_SOL)));
    }
    let result = ok(&m, &rb(&f, &pool, None, &owners), &accounts);
    for o in &owners {
        assert_eq!(delta(&result, &accounts, o), i128::from(PRICE));
    }
    assert_eq!(event_names(&result).len(), 25);
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&pool)).0).lamports,
        rent_for(0)
    );
    assert_eq!(
        decode_pool(account_of(&result, &pool_key(&pool))).returned,
        0x1FF_FFFF
    );
}

#[test]
fn return_boxes_is_atomic_when_the_vault_is_short() {
    // Audit focus "no state where funds are stuck". Two shortfalls, both fail the whole
    // instruction with no §8 code and leave every account as it was. (a) One lamport short
    // of `rent + 1.25 SOL`: every transfer succeeds but the vault ends below its rent floor,
    // which the runtime rejects at the transaction level (`InsufficientFundsForRent`) —
    // Mollusk reports that as `AccountNotRentExempt` with `config.panic` off. (b) One lamport
    // short of the transfers themselves: the last System transfer fails inside the
    // instruction (`SystemError::ResultWithNegativeLamports`, Custom(1)) and nothing moved.
    let f = Fixture::new();
    let mut m = mollusk_for_settlement(SCHEDULED + 3_600);
    m.config.panic = false;
    let pool = sol_pool(&f, PoolStatus::Drawn, &full_plan(&f));
    let key = pool_key(&pool);
    let ix = rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]);

    let mut below_rent = setup(&f, &m, &pool, &marked_record(GameStatus::Postponed), 0);
    set_account(
        &mut below_rent,
        vault_pda(&key).0,
        sol_vault_account(rent_for(0) + FULL_RETURN - 1),
    );
    let a = m.process_instruction(&ix, &below_rent);
    assert_eq!(
        a.program_result,
        mollusk_svm::result::ProgramResult::Failure(
            solana_program_error::ProgramError::AccountNotRentExempt
        ),
        "the runtime refuses the transaction: nothing is committed"
    );

    let mut short = setup(&f, &m, &pool, &marked_record(GameStatus::Postponed), 0);
    set_account(
        &mut short,
        vault_pda(&key).0,
        sol_vault_account(FULL_RETURN - 1),
    );
    let b = m.process_instruction(&ix, &short);
    assert_eq!(custom_error(&b), Some(1), "SystemError, no §8 code");
    assert_eq!(delta(&b, &short, &f.creator), 0);
    assert_eq!(delta(&b, &short, &f.buyer), 0);
    assert_eq!(decode_pool(account_of(&b, &key)), pool);
    assert_eq!(account_of(&b, &vault_pda(&key).0).lamports, FULL_RETURN - 1);
}

#[test]
fn return_boxes_composes_with_neighbours() {
    // PROGRAM §10: identical results with a compute-budget and a transfer around it.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let ix = rb(&f, &pool, Some(counter(&f)), &[f.creator, f.buyer]);
    let noop = Instruction::new_with_bytes(
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
        decode_pool(account_of(&alone, &pool_key(&pool))),
        decode_pool(account_of(&sandwiched, &pool_key(&pool)))
    );
    assert_eq!(
        account_of(&alone, &f.buyer).lamports,
        account_of(&sandwiched, &f.buyer).lamports
    );
}

// ---------------------------------------------------------------------------
// return_sponsorship
// ---------------------------------------------------------------------------

/// A Returned UNFILLED_15 pool with every box paid and one open 1 SOL sponsorship.
fn returned_with_sponsorship(f: &Fixture) -> Pool {
    let bits: Vec<u8> = (0..15).collect();
    sponsored(
        returned_partial(sol_pool(f, PoolStatus::Open, &unfilled_15_plan(f)), &bits),
        SPONSORED,
    )
}

fn sponsorship_setup(
    f: &Fixture,
    m: &Mollusk,
    pool: &Pool,
    record: &GameRecord,
    amount: u64,
) -> Vec<(Pubkey, Account)> {
    let mut accounts = setup(f, m, pool, record, 0);
    let key = pool_key(pool);
    set_account(
        &mut accounts,
        sponsorship_pda(&key, &f.sponsor).0,
        sponsorship_account(&key, &f.sponsor, amount),
    );
    accounts
}

#[test]
fn return_sponsorship_pays_the_wallet_and_closes_the_account() {
    // PROGRAM §4.6; return the full sponsorship to the sponsor and close its account.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 600);
    let pool = returned_with_sponsorship(&f);
    let accounts = sponsorship_setup(&f, &m, &pool, &standard_record(), SPONSORED);
    let key = pool_key(&pool);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + SPONSORED,
        "§3.4 Returned: no box outstanding, one sponsorship open"
    );
    let ix = return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, None, None);
    let result = ok(&m, &ix, &accounts);
    assert_eq!(
        delta(&result, &accounts, &f.sponsor),
        i128::from(SPONSORED + rent_for(mybarpool::Sponsorship::SIZE))
    );
    let gone = account_of(&result, &sponsorship_pda(&key, &f.sponsor).0);
    assert_eq!((gone.lamports, gone.data.len()), (0, 0));
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.sponsorships_open, 0);
    assert_eq!((after.sponsored_total, after.sponsor_count), (SPONSORED, 1));
    assert_eq!(
        account_of(&result, &vault_pda(&key).0).lamports,
        rent_for(0)
    );
    assert_eq!(event_names(&result), ["SponsorshipReturned"]);
    let ev: SponsorshipReturned = emitted_event(&result).expect("event");
    assert_eq!(
        (ev.time, ev.pool, ev.sponsor, ev.amount),
        (SCHEDULED + 600, to_a(&key), to_a(&f.sponsor), SPONSORED)
    );
}

#[test]
fn return_sponsorship_with_a_substituted_destination_fails() {
    // A substituted destination account fails: `address = sponsorship.wallet`.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 600);
    let pool = returned_with_sponsorship(&f);
    let accounts = sponsorship_setup(&f, &m, &pool, &standard_record(), SPONSORED);
    for dest in [f.stranger, f.buyer] {
        let ix = return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, Some(dest), None);
        let r = fails_with(&m, &ix, &accounts, anchor(ErrorCode::ConstraintAddress));
        assert_eq!(delta(&r, &accounts, &dest), 0);
        assert_eq!(delta(&r, &accounts, &f.sponsor), 0);
    }
}

#[test]
fn return_sponsorship_checks_fees_then_status() {
    // PROGRAM §4.6 checks in order: a Drawn pool with fees untaken is NotReturnable; any pool
    // with fees paid is FeesAlreadyPaid whatever its status.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 600);
    let drawn = sponsored(sol_pool(&f, PoolStatus::Drawn, &full_plan(&f)), SPONSORED);
    let accounts = sponsorship_setup(&f, &m, &drawn, &standard_record(), SPONSORED);
    fails_with(
        &m,
        &return_sponsorship_ix(&f.keeper, &drawn, &f.sponsor, None, None),
        &accounts,
        err(E::NotReturnable),
    );
    let after_q1 = settled_pool(&f, &f.expected_config(), &sol_params(0), SPONSORED, 1, true);
    let mut split = after_q1.clone();
    split.status = PoolStatus::Split;
    split.split_amount = split.unpaid_prize_pool / 25;
    let settled = settled_pool(&f, &f.expected_config(), &sol_params(0), SPONSORED, 4, true);
    for pool in [after_q1, split, settled] {
        let accounts = sponsorship_setup(&f, &m, &pool, &record_with_quarters(1), SPONSORED);
        fails_with(
            &m,
            &return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, None, None),
            &accounts,
            err(E::FeesAlreadyPaid),
        );
    }
}

#[test]
fn return_sponsorship_by_the_admin_is_unauthorized() {
    // PROGRAM §10 Authority.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 600);
    let pool = returned_with_sponsorship(&f);
    let accounts = sponsorship_setup(&f, &m, &pool, &standard_record(), SPONSORED);
    fails_with(
        &m,
        &return_sponsorship_ix(&f.admin, &pool, &f.sponsor, None, None),
        &accounts,
        err(E::Unauthorized),
    );
}

#[test]
fn return_sponsorship_ore_creates_the_ata_and_refuses_a_wrong_one() {
    // PROGRAM §5.4: the sponsor's ATA created at the keeper's expense; a wrong ATA is refused.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 600);
    let bits: Vec<u8> = (0..25).collect();
    let pool = sponsored(
        returned_partial(
            pool_with_owners(
                &f,
                &f.expected_config(),
                &ore_params(0),
                PoolStatus::Drawn,
                &full_plan(&f),
            ),
            &bits,
        ),
        100 * PRICE_ORE,
    );
    let key = pool_key(&pool);
    let spl = SplOne::derived(&pool, &f.sponsor);
    let mut accounts = returns_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &marked_record(GameStatus::Postponed),
        0,
        Some(100 * PRICE_ORE),
    );
    set_account(
        &mut accounts,
        sponsorship_pda(&key, &f.sponsor).0,
        sponsorship_account(&key, &f.sponsor, 100 * PRICE_ORE),
    );
    set_account(&mut accounts, spl.ata, system_account(0));
    let k0 = lamports_of(&accounts, &f.keeper);
    let result = ok(
        &m,
        &return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, None, Some(spl)),
        &accounts,
    );
    assert_eq!(
        k0 - account_of(&result, &f.keeper).lamports,
        rent_for(TOKEN_ACCOUNT_LEN)
    );
    assert_eq!(token_amount_of(&result, &spl.ata), 100 * PRICE_ORE);
    assert_eq!(token_amount_of(&result, &vault_pda(&key).0), 0);

    let wrong = SplOne {
        ata: ata(&f.stranger, &spl.mint, &spl.token_program),
        ..spl
    };
    set_account(&mut accounts, wrong.ata, system_account(0));
    fails_with(
        &m,
        &return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, None, Some(wrong)),
        &accounts,
        anchor(ErrorCode::RequireKeysEqViolated),
    );
}

#[test]
fn return_sponsorship_twice_is_account_not_initialized() {
    // PROGRAM §3.7: the account is gone after the first return.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 600);
    let pool = returned_with_sponsorship(&f);
    let accounts = sponsorship_setup(&f, &m, &pool, &standard_record(), SPONSORED);
    let ix = return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, None, None);
    let first = ok(&m, &ix, &accounts);
    let second = m.process_instruction(&ix, &first.resulting_accounts);
    assert_eq!(
        custom_error(&second),
        Some(anchor(ErrorCode::AccountNotInitialized))
    );
}

// ---------------------------------------------------------------------------
// cancel_pool
// ---------------------------------------------------------------------------

#[test]
fn cancel_pool_on_open_locked_and_drawn() {
    // PROGRAM §4.6: Returned with cancelled_by_admin; the counter decremented only from Open.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0 + 600);
    for (status, plan, open_count, with_counter) in [
        (PoolStatus::Open, unfilled_15_plan(&f), 1u8, true),
        (PoolStatus::Locked, full_plan(&f), 0, false),
        (PoolStatus::Drawn, full_plan(&f), 0, false),
    ] {
        let pool = sol_pool(&f, status, &plan);
        let accounts = setup(&f, &m, &pool, &standard_record(), open_count);
        let ix = cancel_pool_ix(&f.admin, &pool, with_counter.then(|| counter(&f)));
        let result = ok(&m, &ix, &accounts);
        let after = decode_pool(account_of(&result, &pool_key(&pool)));
        let mut expected = pool.clone();
        expected.status = PoolStatus::Returned;
        expected.cancelled_by_admin = true;
        assert_eq!(after, expected, "whole-struct");
        assert_eq!(
            decode_counter(account_of(&result, &counter(&f))).open_count,
            0
        );
        assert_eq!(event_names(&result), ["PoolCancelled"]);
        let ev: PoolCancelled = emitted_event(&result).expect("event");
        assert_eq!((ev.time, ev.pool), (T0 + 600, to_a(&pool_key(&pool))));
    }
}

#[test]
fn cancel_pool_open_without_the_counter_fails() {
    // PROGRAM §4.6 "The counter".
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0 + 600);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    fails_with(
        &m,
        &cancel_pool_ix(&f.admin, &pool, None),
        &accounts,
        anchor(ErrorCode::ConstraintAccountIsNone),
    );
}

#[test]
fn cancel_pool_after_the_first_settlement_fails() {
    // cancel_pool after first settlement fails: FeesAlreadyPaid.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let pool = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 1, true);
    let accounts = setup(&f, &m, &pool, &record_with_quarters(1), 0);
    fails_with(
        &m,
        &cancel_pool_ix(&f.admin, &pool, None),
        &accounts,
        err(E::FeesAlreadyPaid),
    );
}

#[test]
fn cancel_pool_on_a_terminal_pool_is_not_returnable() {
    // PROGRAM §4.6: Settled, Returned, Split.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let settled = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 4, true);
    let returned = returned_partial(sol_pool(&f, PoolStatus::Drawn, &full_plan(&f)), &[]);
    let split = split_partial(
        settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 1, true),
        &[],
        35_200_000,
        880_000_000,
    );
    for pool in [settled, returned, split] {
        let accounts = setup(&f, &m, &pool, &record_with_quarters(1), 0);
        fails_with(
            &m,
            &cancel_pool_ix(&f.admin, &pool, None),
            &accounts,
            err(E::NotReturnable),
        );
    }
}

#[test]
fn cancel_pool_by_anyone_but_the_admin_is_unauthorized() {
    // PROGRAM §10 Authority "cancel_pool are admin-only": the keeper included.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0 + 600);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    for signer in [f.keeper, f.stranger] {
        fails_with(
            &m,
            &cancel_pool_ix(&signer, &pool, Some(counter(&f))),
            &accounts,
            err(E::Unauthorized),
        );
    }
}

#[test]
fn cancel_pool_then_return_boxes_before_kickoff() {
    // PROGRAM §4.6 "The keeper then runs return_boxes": on a Scheduled game before kickoff,
    // the cancelled branch is the only one that holds — and it does.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0);
    let pool = sol_pool(&f, PoolStatus::Drawn, &full_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 0);
    let key = pool_key(&pool);
    let chain = [
        cancel_pool_ix(&f.admin, &pool, None),
        rb(&f, &pool, None, &[f.creator, f.buyer, f.buyer_2]),
    ];
    let result = m.process_instruction_chain(&chain, &accounts);
    assert!(result.program_result.is_ok(), "{:?}", result.program_result);
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.status, PoolStatus::Returned);
    assert!(after.cancelled_by_admin);
    assert_eq!(after.returned, 0x1FF_FFFF);
    assert_eq!(
        account_of(&result, &vault_pda(&key).0).lamports,
        rent_for(0)
    );
    let paid: u64 = [f.creator, f.buyer, f.buyer_2]
        .iter()
        .map(|k| account_of(&result, k).lamports - lamports_of(&accounts, k))
        .sum();
    assert_eq!(paid, FULL_RETURN);
    // close_pool after: everything returned, no sponsorship → both rents to the fee wallet.
    let fw0 = account_of(&result, &f.fee_wallet).lamports;
    let closed = ok(
        &m,
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &result.resulting_accounts,
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - fw0,
        rent_for(0) + rent_for(Pool::SIZE)
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.destination, to_a(&f.fee_wallet));
}
