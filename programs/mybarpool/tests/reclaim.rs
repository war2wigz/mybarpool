//! `reclaim` and `reclaim_sponsorship`, PROGRAM §4.6, under Mollusk. Every
//! spec number cites its line. Run after `anchor build --arch v3`.
//!
//! The reclaim clock runs from `game.scheduled_kickoff` (`SCHEDULED`) plus `RECLAIM_DELAY`
//! (2,592,000 s, PROGRAM §1), never from the recorded kickoff. While the fees are untaken a
//! reclaim pays the price; once they are paid it pays the split share, fixed on the first
//! reclaim exactly as `split` would (880M / 25 = 35,200,000 on the standard pool after Q1).

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::{Check, InstructionResult};
use mollusk_svm::Mollusk;
use mybarpool::{
    BoxesReclaimed, CreatePoolParams, GameRecord, MybarpoolError as E, PayoutPreset, Pool,
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

const CREATOR_RETURN: u64 = 250_000_000;
const BUYER_RETURN: u64 = 500_000_000;
const SPLIT_AMOUNT: u64 = 35_200_000;
const SMALL_SPONSORSHIP: u64 = 50_000_000;

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

fn rc(f: &Fixture, signer: &Pubkey, pool: &Pool, counter: Option<Pubkey>) -> Instruction {
    let _ = f;
    reclaim_ix(signer, pool, &standard_game(), counter, None)
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

// ---------------------------------------------------------------------------
// reclaim
// ---------------------------------------------------------------------------

#[test]
fn reclaim_at_the_boundary() {
    // At 30 days minus one second fails; at 30 days succeeds: the buyer takes 10 boxes at the
    // price, the pool is abandoned and Returned, the counter drops.
    let f = Fixture::new();
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let key = pool_key(&pool);
    let early = mollusk_for_settlement(RECLAIM_AT - 1);
    fails_with(
        &early,
        &rc(&f, &f.buyer, &pool, Some(counter(&f))),
        &setup(&f, &early, &pool, &standard_record(), 1),
        err(E::ReclaimTooEarly),
    );
    let m = mollusk_for_settlement(RECLAIM_AT);
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let result = ok(&m, &rc(&f, &f.buyer, &pool, Some(counter(&f))), &accounts);
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(BUYER_RETURN)
    );
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.status, PoolStatus::Returned);
    assert!(after.abandoned);
    assert_eq!(after.returned, 0x7FE0);
    assert_eq!(after.split_amount, 0);
    assert_eq!(
        decode_counter(account_of(&result, &counter(&f))).open_count,
        0
    );
    assert_eq!(event_names(&result), ["BoxesReclaimed"]);
    let ev: BoxesReclaimed = emitted_event(&result).expect("BoxesReclaimed");
    assert_eq!(
        (ev.time, ev.pool, ev.owner, ev.boxes, ev.amount),
        (
            RECLAIM_AT,
            to_a(&key),
            to_a(&f.buyer),
            (5..=14).collect::<Vec<u8>>(),
            BUYER_RETURN
        )
    );
}

#[test]
fn reclaim_is_measured_from_the_scheduled_kickoff() {
    // Measured from scheduled kickoff even after update_kickoff; PROGRAM §10 Timing.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &moved_record(SCHEDULED + 3 * 3_600), 1);
    ok(&m, &rc(&f, &f.buyer, &pool, Some(counter(&f))), &accounts);
}

#[test]
fn reclaim_only_your_own_boxes_once() {
    // Only reclaim their own boxes; a second reclaim fails: the buyer again, a stranger,
    // buyer_2 (owns nothing on this pool), then the creator finishes it.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let key = pool_key(&pool);
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let first = ok(&m, &rc(&f, &f.buyer, &pool, Some(counter(&f))), &accounts);
    let again = m.process_instruction(&rc(&f, &f.buyer, &pool, None), &first.resulting_accounts);
    assert_eq!(custom_error(&again), Some(err(E::NothingToReturn)));
    for who in [f.stranger, f.buyer_2] {
        let r = m.process_instruction(&rc(&f, &who, &pool, None), &first.resulting_accounts);
        assert_eq!(custom_error(&r), Some(err(E::NotOwner)));
    }
    let c0 = account_of(&first, &f.creator).lamports;
    let creator = ok(
        &m,
        &rc(&f, &f.creator, &pool, None),
        &first.resulting_accounts,
    );
    assert_eq!(
        account_of(&creator, &f.creator).lamports - c0,
        CREATOR_RETURN
    );
    let after = decode_pool(account_of(&creator, &key));
    assert_eq!(after.status, PoolStatus::Returned);
    assert!(after.abandoned);
    assert_eq!(after.returned, 0x7FFF);
}

#[test]
fn reclaim_needs_the_counter_only_from_open() {
    // PROGRAM §4.6 "The counter": Open without it fails; Locked without it succeeds at the
    // price and abandons the pool.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let open = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    fails_with(
        &m,
        &rc(&f, &f.buyer, &open, None),
        &setup(&f, &m, &open, &standard_record(), 1),
        anchor(ErrorCode::ConstraintAccountIsNone),
    );
    let locked = sol_pool(&f, PoolStatus::Locked, &full_plan(&f));
    let accounts = setup(&f, &m, &locked, &standard_record(), 0);
    let result = ok(&m, &rc(&f, &f.buyer, &locked, None), &accounts);
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(BUYER_RETURN)
    );
    let after = decode_pool(account_of(&result, &pool_key(&locked)));
    assert_eq!(
        (after.status, after.abandoned),
        (PoolStatus::Returned, true)
    );
}

#[test]
fn reclaim_on_a_drawn_pool_never_paid_returns_the_price() {
    // PROGRAM §5.3 "Abandoned, never paid".
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sol_pool(&f, PoolStatus::Drawn, &full_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 0);
    let result = ok(&m, &rc(&f, &f.creator, &pool, None), &accounts);
    assert_eq!(
        delta(&result, &accounts, &f.creator),
        i128::from(CREATOR_RETURN)
    );
    let after = decode_pool(account_of(&result, &pool_key(&pool)));
    assert_eq!(
        (after.status, after.abandoned),
        (PoolStatus::Returned, true)
    );
}

#[test]
fn reclaim_on_a_drawn_pool_after_q1_is_a_split() {
    // PROGRAM §5.3 "Abandoned, partly paid"; ARCHITECTURE › Returns "the same arithmetic as a
    // split": the first reclaim fixes split_amount and moves the pool to Split; the rest
    // follow at that share; the fee wallet keeps its fee.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 1, true);
    let key = pool_key(&pool);
    let accounts = setup(&f, &m, &pool, &record_with_quarters(1), 0);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + 880_000_000
    );
    let first = ok(&m, &rc(&f, &f.buyer, &pool, None), &accounts);
    let after = decode_pool(account_of(&first, &key));
    assert_eq!(after.status, PoolStatus::Split);
    assert!(after.abandoned);
    assert_eq!(after.split_amount, SPLIT_AMOUNT);
    assert_eq!(after.unpaid_prize_pool, 528_000_000);
    assert_eq!(
        delta(&first, &accounts, &f.buyer),
        i128::from(10 * SPLIT_AMOUNT)
    );
    assert_eq!(
        account_of(&first, &vault_pda(&key).0).lamports,
        rent_for(0) + 528_000_000
    );
    let second = ok(
        &m,
        &rc(&f, &f.creator, &pool, None),
        &first.resulting_accounts,
    );
    let third = ok(
        &m,
        &rc(&f, &f.buyer_2, &pool, None),
        &second.resulting_accounts,
    );
    let end = decode_pool(account_of(&third, &key));
    assert_eq!(end.unpaid_prize_pool, 0);
    assert_eq!(end.returned, 0x1FF_FFFF);
    assert_eq!(account_of(&third, &vault_pda(&key).0).lamports, rent_for(0));
    assert_eq!(delta(&third, &accounts, &f.fee_wallet), 0);
}

#[test]
fn reclaim_on_final_only_after_three_empty_settlements_is_a_return() {
    // PROGRAM §4.6 "if !fees_paid, status = Returned": the 1.1 SOL prize_pool on the account
    // is irrelevant, the price is paid.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = settled_pool(
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
    let accounts = setup(&f, &m, &pool, &record_with_quarters(3), 0);
    let result = ok(&m, &rc(&f, &f.buyer, &pool, None), &accounts);
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(BUYER_RETURN)
    );
    assert_eq!(
        decode_pool(account_of(&result, &pool_key(&pool))).status,
        PoolStatus::Returned
    );
}

#[test]
fn reclaim_on_a_keeper_partial_returned_pool_is_not_abandoned() {
    // PROGRAM §4.6 "not marked abandoned"; §3.3 abandoned: the keeper paid the creator and
    // vanished; the buyers reclaim at the price, the pool stays un-abandoned, and close_pool
    // goes to the fee wallet.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = returned_partial(
        sol_pool(&f, PoolStatus::Drawn, &full_plan(&f)),
        &[0, 1, 2, 3, 4],
    );
    let key = pool_key(&pool);
    let accounts = setup(&f, &m, &pool, &standard_record(), 0);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + 1_000_000_000,
        "§3.4 Returned: 20 outstanding boxes"
    );
    let first = ok(&m, &rc(&f, &f.buyer, &pool, None), &accounts);
    assert_eq!(delta(&first, &accounts, &f.buyer), i128::from(BUYER_RETURN));
    let after = decode_pool(account_of(&first, &key));
    assert_eq!(
        (after.status, after.abandoned),
        (PoolStatus::Returned, false)
    );
    let second = ok(
        &m,
        &rc(&f, &f.buyer_2, &pool, None),
        &first.resulting_accounts,
    );
    let closed = ok(
        &m,
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &second.resulting_accounts,
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.destination, to_a(&f.fee_wallet));
}

#[test]
fn reclaim_on_a_keeper_partial_split_pool_uses_the_fixed_share() {
    // PROGRAM §4.6 "at the amount the pool already fixed": not recomputed from unpaid.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = split_partial(
        settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 1, true),
        &[0, 1, 2, 3, 4],
        SPLIT_AMOUNT,
        704_000_000,
    );
    let key = pool_key(&pool);
    let accounts = setup(&f, &m, &pool, &record_with_quarters(1), 0);
    let result = ok(&m, &rc(&f, &f.buyer, &pool, None), &accounts);
    assert_eq!(
        delta(&result, &accounts, &f.buyer),
        i128::from(10 * SPLIT_AMOUNT)
    );
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.unpaid_prize_pool, 352_000_000);
    assert!(!after.abandoned);
    assert_eq!(after.split_amount, SPLIT_AMOUNT);
}

#[test]
fn reclaim_on_a_settled_pool_is_not_returnable() {
    // PROGRAM §4.6: however late.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 10 * RECLAIM_DELAY);
    let pool = settled_pool(&f, &f.expected_config(), &sol_params(0), 0, 4, true);
    let accounts = setup(&f, &m, &pool, &record_with_quarters(4), 0);
    fails_with(
        &m,
        &rc(&f, &f.buyer, &pool, None),
        &accounts,
        err(E::NotReturnable),
    );
}

#[test]
fn reclaim_with_a_planted_max_kickoff_is_math_overflow() {
    // Standing rule: a checked add, not a panic.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sol_pool(&f, PoolStatus::Locked, &full_plan(&f));
    let mut record = standard_record();
    record.scheduled_kickoff = i64::MAX;
    let accounts = setup(&f, &m, &pool, &record, 0);
    fails_with(
        &m,
        &rc(&f, &f.buyer, &pool, None),
        &accounts,
        err(E::MathOverflow),
    );
}

#[test]
fn reclaim_ore_locked_the_owner_pays_their_own_ata() {
    // PROGRAM §4.6 "payer for their own missing ATA".
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = pool_with_owners(
        &f,
        &f.expected_config(),
        &ore_params(0),
        PoolStatus::Locked,
        &full_plan(&f),
    );
    let key = pool_key(&pool);
    let spl = SplOne::derived(&pool, &f.buyer);
    let mut accounts = returns_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &standard_record(),
        0,
        Some(25 * PRICE_ORE),
    );
    set_account(&mut accounts, spl.ata, system_account(0));
    let b0 = lamports_of(&accounts, &f.buyer);
    let result = ok(
        &m,
        &reclaim_ix(&f.buyer, &pool, &standard_game(), None, Some(spl)),
        &accounts,
    );
    assert_eq!(
        b0 - account_of(&result, &f.buyer).lamports,
        rent_for(TOKEN_ACCOUNT_LEN),
        "the buyer paid the ATA"
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &spl.ata)),
        50_000_000_000
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &vault_pda(&key).0)),
        75_000_000_000
    );

    let wrong = SplOne {
        ata: ata(&f.buyer_2, &spl.mint, &spl.token_program),
        ..spl
    };
    set_account(&mut accounts, wrong.ata, system_account(0));
    fails_with(
        &m,
        &reclaim_ix(&f.buyer, &pool, &standard_game(), None, Some(wrong)),
        &accounts,
        anchor(ErrorCode::RequireKeysEqViolated),
    );
    let mut no_mint = reclaim_ix(&f.buyer, &pool, &standard_game(), None, Some(spl));
    // Reclaim accounts: box_owner 0, game 1, pool 2, vault 3, counter 4, mint 5.
    assert_eq!(no_mint.accounts[5].pubkey, f.ore_mint);
    no_mint.accounts[5].pubkey = program_id();
    fails_with(
        &m,
        &no_mint,
        &accounts,
        anchor(ErrorCode::ConstraintAccountIsNone),
    );
}

#[test]
fn reclaim_while_paused_and_without_a_config_slot() {
    // ARCHITECTURE › Trust model: reclaim takes no config — a paused config cannot block it.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sol_pool(&f, PoolStatus::Locked, &full_plan(&f));
    let mut paused = f.expected_config();
    paused.paused = true;
    let accounts = returns_accounts(&f, &m, &paused, &pool, &standard_record(), 0, None);
    let ix = rc(&f, &f.buyer, &pool, None);
    assert!(
        !ix.accounts.iter().any(|a| a.pubkey == config_pda().0),
        "no config slot"
    );
    ok(&m, &ix, &accounts);
}

#[test]
fn reclaim_composes_with_neighbours() {
    // PROGRAM §10.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sol_pool(&f, PoolStatus::Locked, &full_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 0);
    let ix = rc(&f, &f.buyer, &pool, None);
    let noop = Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
        vec![
            solana_instruction::AccountMeta::new(f.buyer, true),
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
        account_of(&sandwiched, &f.buyer).lamports + 2
    );
}

// ---------------------------------------------------------------------------
// reclaim_sponsorship
// ---------------------------------------------------------------------------

fn sponsored_open(f: &Fixture) -> Pool {
    sponsored(
        sol_pool(f, PoolStatus::Open, &unfilled_15_plan(f)),
        SMALL_SPONSORSHIP,
    )
}

fn with_sponsorship(
    f: &Fixture,
    m: &Mollusk,
    pool: &Pool,
    record: &GameRecord,
    open_count: u8,
) -> Vec<(Pubkey, Account)> {
    let mut accounts = setup(f, m, pool, record, open_count);
    let key = pool_key(pool);
    set_account(
        &mut accounts,
        sponsorship_pda(&key, &f.sponsor).0,
        sponsorship_account(&key, &f.sponsor, pool.sponsored_total),
    );
    accounts
}

#[test]
fn reclaim_sponsorship_at_thirty_days_on_a_never_paid_pool() {
    // reclaim_sponsorship succeeds at 30 days on a never-paid pool.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sponsored_open(&f);
    let key = pool_key(&pool);
    let accounts = with_sponsorship(&f, &m, &pool, &standard_record(), 1);
    let result = ok(
        &m,
        &reclaim_sponsorship_ix(&f.sponsor, &pool, &standard_game(), Some(counter(&f)), None),
        &accounts,
    );
    assert_eq!(
        delta(&result, &accounts, &f.sponsor),
        i128::from(SMALL_SPONSORSHIP + rent_for(mybarpool::Sponsorship::SIZE))
    );
    let gone = account_of(&result, &sponsorship_pda(&key, &f.sponsor).0);
    assert_eq!((gone.lamports, gone.data.len()), (0, 0));
    let after = decode_pool(account_of(&result, &key));
    assert_eq!(
        (after.status, after.abandoned),
        (PoolStatus::Returned, true)
    );
    assert_eq!(after.sponsorships_open, 0);
    assert_eq!(
        decode_counter(account_of(&result, &counter(&f))).open_count,
        0
    );
    assert_eq!(event_names(&result), ["SponsorshipReturned"]);
    let ev: SponsorshipReturned = emitted_event(&result).expect("event");
    assert_eq!(
        (ev.sponsor, ev.amount),
        (to_a(&f.sponsor), SMALL_SPONSORSHIP)
    );
}

#[test]
fn reclaim_sponsorship_too_early_or_after_fees() {
    // Fails on one that paid: ReclaimTooEarly at −1; FeesAlreadyPaid after Q1 and on Settled.
    let f = Fixture::new();
    let early = mollusk_for_settlement(RECLAIM_AT - 1);
    let pool = sponsored_open(&f);
    fails_with(
        &early,
        &reclaim_sponsorship_ix(&f.sponsor, &pool, &standard_game(), Some(counter(&f)), None),
        &with_sponsorship(&f, &early, &pool, &standard_record(), 1),
        err(E::ReclaimTooEarly),
    );
    let m = mollusk_for_settlement(RECLAIM_AT);
    for (n, record) in [(1u8, record_with_quarters(1)), (4, record_with_quarters(4))] {
        let paid = settled_pool(
            &f,
            &f.expected_config(),
            &sol_params(0),
            SMALL_SPONSORSHIP,
            n,
            true,
        );
        fails_with(
            &m,
            &reclaim_sponsorship_ix(&f.sponsor, &paid, &standard_game(), None, None),
            &with_sponsorship(&f, &m, &paid, &record, 0),
            err(E::FeesAlreadyPaid),
        );
    }
}

#[test]
fn reclaim_sponsorship_by_a_stranger_has_no_account() {
    // PROGRAM §4.6 "has no account to pass": AccountNotInitialized at the stranger's seeds.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sponsored_open(&f);
    let mut accounts = with_sponsorship(&f, &m, &pool, &standard_record(), 1);
    accounts.push((
        sponsorship_pda(&pool_key(&pool), &f.stranger).0,
        system_account(0),
    ));
    fails_with(
        &m,
        &reclaim_sponsorship_ix(
            &f.stranger,
            &pool,
            &standard_game(),
            Some(counter(&f)),
            None,
        ),
        &accounts,
        anchor(ErrorCode::AccountNotInitialized),
    );
}

#[test]
fn reclaim_sponsorship_on_a_keeper_partial_pool_stays_unabandoned() {
    // PROGRAM §4.6: the status is already Returned, nothing else changes but the count.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sponsored(
        returned_partial(
            sol_pool(&f, PoolStatus::Drawn, &full_plan(&f)),
            &[0, 1, 2, 3, 4],
        ),
        SMALL_SPONSORSHIP,
    );
    let accounts = with_sponsorship(&f, &m, &pool, &standard_record(), 0);
    let result = ok(
        &m,
        &reclaim_sponsorship_ix(&f.sponsor, &pool, &standard_game(), None, None),
        &accounts,
    );
    let after = decode_pool(account_of(&result, &pool_key(&pool)));
    assert_eq!(
        (after.status, after.abandoned),
        (PoolStatus::Returned, false)
    );
    assert_eq!(after.sponsorships_open, 0);
}

#[test]
fn reclaim_sponsorship_ore_the_sponsor_pays_the_ata() {
    // PROGRAM §4.6: the sponsor pays their own ATA.
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let amount = 10 * PRICE_ORE;
    let pool = sponsored(
        pool_with_owners(
            &f,
            &f.expected_config(),
            &ore_params(0),
            PoolStatus::Locked,
            &full_plan(&f),
        ),
        amount,
    );
    let key = pool_key(&pool);
    let spl = SplOne::derived(&pool, &f.sponsor);
    let mut accounts = returns_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &standard_record(),
        0,
        Some(25 * PRICE_ORE + amount),
    );
    set_account(
        &mut accounts,
        sponsorship_pda(&key, &f.sponsor).0,
        sponsorship_account(&key, &f.sponsor, amount),
    );
    set_account(&mut accounts, spl.ata, system_account(0));
    let s0 = lamports_of(&accounts, &f.sponsor);
    let result = ok(
        &m,
        &reclaim_sponsorship_ix(&f.sponsor, &pool, &standard_game(), None, Some(spl)),
        &accounts,
    );
    // The sponsor paid the ATA rent and got the Sponsorship rent back.
    assert_eq!(
        i128::from(account_of(&result, &f.sponsor).lamports) - i128::from(s0),
        i128::from(rent_for(mybarpool::Sponsorship::SIZE))
            - i128::from(rent_for(TOKEN_ACCOUNT_LEN))
    );
    assert_eq!(decode_token_amount(account_of(&result, &spl.ata)), amount);
    assert_eq!(
        decode_token_amount(account_of(&result, &vault_pda(&key).0)),
        25 * PRICE_ORE
    );
}
