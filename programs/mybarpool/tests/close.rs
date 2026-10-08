//! `close_sponsorship`, PROGRAM §4.6, and `close_pool` (§4.5) reached through
//! the real return, split and reclaim paths; `close_counter` once a return took
//! the last `Open` pool to zero; the event, layout and IDL counts. Every spec
//! number cites its line. Run after `anchor build --arch v3`.

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::{Check, InstructionResult};
use mollusk_svm::Mollusk;
use mybarpool::{
    GameRecord, GameStatus, MybarpoolError as E, Pool, PoolClosed, PoolStatus, Sponsorship,
    SponsorshipClosed,
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

const SPONSORED: u64 = 1_000_000_000;
const STEP6_DUST_SPONSORED: u64 = 1_000_000_003; // the Step 6 dust pool: 2 lamports left
const SPLIT_DUST_SPONSORED: u64 = 1_000_000_013; // the Step 7 dust pool: 11 lamports left

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

fn with_sponsorship(
    mut accounts: Vec<(Pubkey, Account)>,
    pool: &Pool,
    f: &Fixture,
) -> Vec<(Pubkey, Account)> {
    let key = pool_key(pool);
    set_account(
        &mut accounts,
        sponsorship_pda(&key, &f.sponsor).0,
        sponsorship_account(&key, &f.sponsor, pool.sponsored_total),
    );
    accounts
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

fn rb(f: &Fixture, pool: &Pool, counter: Option<Pubkey>, owners: &[Pubkey]) -> Instruction {
    let batch: Vec<_> = owners.iter().map(|o| (*o, None)).collect();
    return_boxes_ix(&f.keeper, pool, &standard_game(), counter, &batch, None)
}

/// PROGRAM §3.4: the SOL vault holds exactly what the pool's state says.
fn assert_invariant(result: &InstructionResult, pool: &Pool) {
    let key = pool_key(pool);
    let account = account_of(result, &key);
    if account.data.is_empty() {
        assert_eq!(account_of(result, &vault_pda(&key).0).lamports, 0, "closed");
        return;
    }
    let after = decode_pool(account);
    assert_eq!(
        account_of(result, &vault_pda(&key).0).lamports,
        vault_balance_for(&after),
        "§3.4 after {:?}",
        after.status
    );
}

/// Run `chain` one instruction at a time from `accounts`, asserting the §3.4 invariant after
/// each step; returns the last result.
fn run_chain(
    m: &Mollusk,
    chain: &[Instruction],
    accounts: &[(Pubkey, Account)],
    pool: &Pool,
) -> InstructionResult {
    let mut state = accounts.to_vec();
    let mut last = None;
    for ix in chain {
        let r = ok(m, ix, &state);
        assert_invariant(&r, pool);
        state = r.resulting_accounts.clone();
        last = Some(r);
    }
    last.expect("non-empty chain")
}

fn total_out(
    result: &InstructionResult,
    accounts: &[(Pubkey, Account)],
    recipients: &[Pubkey],
) -> u64 {
    recipients
        .iter()
        .map(|k| account_of(result, k).lamports - lamports_of(accounts, k))
        .sum()
}

// ---------------------------------------------------------------------------
// close_sponsorship
// ---------------------------------------------------------------------------

#[test]
fn close_sponsorship_on_settled_then_close_pool_sweeps_the_dust() {
    // PROGRAM §4.6; a Sponsorship account left open after its pool settled: the account goes,
    // the sponsor gets its rent and nothing else, then close_pool sweeps the 2 lamports.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 4 * 3_600);
    let pool = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params(0),
        STEP6_DUST_SPONSORED,
        4,
        true,
    );
    assert_eq!(
        (pool.status, pool.unpaid_prize_pool, pool.sponsorships_open),
        (PoolStatus::Settled, 2, 1)
    );
    let key = pool_key(&pool);
    let accounts = with_sponsorship(setup(&f, &m, &pool, &record_with_quarters(4), 0), &pool, &f);
    let s0 = lamports_of(&accounts, &f.sponsor);
    let result = ok(
        &m,
        &close_sponsorship_ix(&pool, &f.sponsor, None),
        &accounts,
    );
    let gone = account_of(&result, &sponsorship_pda(&key, &f.sponsor).0);
    assert_eq!((gone.lamports, gone.data.len()), (0, 0));
    assert_eq!(
        account_of(&result, &f.sponsor).lamports - s0,
        rent_for(Sponsorship::SIZE)
    );
    assert_eq!(decode_pool(account_of(&result, &key)).sponsorships_open, 0);
    assert_eq!(event_names(&result), ["SponsorshipClosed"]);
    let ev: SponsorshipClosed = emitted_event(&result).expect("event");
    assert_eq!(
        (ev.time, ev.pool, ev.sponsor, ev.amount),
        (
            SCHEDULED + 4 * 3_600,
            to_a(&key),
            to_a(&f.sponsor),
            STEP6_DUST_SPONSORED
        )
    );
    let fw0 = account_of(&result, &f.fee_wallet).lamports;
    let closed = ok(
        &m,
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &result.resulting_accounts,
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - fw0,
        rent_for(0) + 2 + rent_for(Pool::SIZE)
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.dust, 2);
}

#[test]
fn close_sponsorship_on_split_then_close_pool_sweeps_the_dust() {
    // PROGRAM §4.6 on a Split pool planted at the end of its batches (split 67,200,000 ×
    // 25, 11 left).
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 4 * 3_600);
    let bits: Vec<u8> = (0..25).collect();
    let pool = split_partial(
        settled_pool(
            &f,
            &f.expected_config(),
            &sol_params(0),
            SPLIT_DUST_SPONSORED,
            1,
            true,
        ),
        &bits,
        67_200_000,
        11,
    );
    let key = pool_key(&pool);
    let accounts = with_sponsorship(
        setup(&f, &m, &pool, &suspended_record_with_quarters(1), 0),
        &pool,
        &f,
    );
    assert_eq!(lamports_of(&accounts, &vault_pda(&key).0), rent_for(0) + 11);
    let result = ok(
        &m,
        &close_sponsorship_ix(&pool, &f.sponsor, None),
        &accounts,
    );
    let fw0 = account_of(&result, &f.fee_wallet).lamports;
    let closed = ok(
        &m,
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &result.resulting_accounts,
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - fw0,
        rent_for(0) + 11 + rent_for(Pool::SIZE)
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.dust, 11);
}

#[test]
fn close_sponsorship_needs_a_settled_or_split_pool() {
    // PROGRAM §8 "PoolNotTerminal (6055) is also close_sponsorship": Returned, Drawn, Open,
    // Locked.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let pools = [
        sponsored(
            returned_partial(sol_pool(&f, PoolStatus::Drawn, &full_plan(&f)), &[]),
            SPONSORED,
        ),
        sponsored(sol_pool(&f, PoolStatus::Drawn, &full_plan(&f)), SPONSORED),
        sponsored(
            sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f)),
            SPONSORED,
        ),
        sponsored(sol_pool(&f, PoolStatus::Locked, &full_plan(&f)), SPONSORED),
    ];
    for pool in pools {
        let accounts = with_sponsorship(setup(&f, &m, &pool, &standard_record(), 1), &pool, &f);
        fails_with(
            &m,
            &close_sponsorship_ix(&pool, &f.sponsor, None),
            &accounts,
            err(E::PoolNotTerminal),
        );
    }
}

#[test]
fn close_sponsorship_with_a_substituted_sponsor_fails() {
    // PROGRAM §4.6: `address = sponsorship.wallet`.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 4 * 3_600);
    let pool = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params(0),
        STEP6_DUST_SPONSORED,
        4,
        true,
    );
    let accounts = with_sponsorship(setup(&f, &m, &pool, &record_with_quarters(4), 0), &pool, &f);
    fails_with(
        &m,
        &close_sponsorship_ix(&pool, &f.sponsor, Some(f.stranger)),
        &accounts,
        anchor(ErrorCode::ConstraintAddress),
    );
}

// ---------------------------------------------------------------------------
// close_pool through the real paths
// ---------------------------------------------------------------------------

#[test]
fn chain_unfilled_return_boxes_return_sponsorship_close_pool() {
    // PROGRAM §4.5, §5.3, §10 Money: Σ out = 15 × price + the sponsorship; the vault ends at
    // zero; a keeper-run return closes to the fee wallet; the two guards on the way.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sponsored(
        sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f)),
        SPONSORED,
    );
    let key = pool_key(&pool);
    let accounts = with_sponsorship(setup(&f, &m, &pool, &standard_record(), 1), &pool, &f);
    assert_eq!(
        lamports_of(&accounts, &vault_pda(&key).0),
        rent_for(0) + 15 * PRICE + SPONSORED
    );
    let first = ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.creator]),
        &accounts,
    );
    assert_invariant(&first, &pool);
    // Sponsorship still open, and a box still outstanding: the sponsorship guard comes first.
    let early = m.process_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &first.resulting_accounts,
    );
    assert_eq!(custom_error(&early), Some(err(E::SponsorshipsStillOpen)));
    let returned_s = ok(
        &m,
        &return_sponsorship_ix(&f.keeper, &pool, &f.sponsor, None, None),
        &first.resulting_accounts,
    );
    assert_invariant(&returned_s, &pool);
    let outstanding = m.process_instruction(
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &returned_s.resulting_accounts,
    );
    assert_eq!(
        custom_error(&outstanding),
        Some(err(E::BoxesStillOutstanding))
    );
    let second = ok(
        &m,
        &rb(&f, &pool, None, &[f.buyer]),
        &returned_s.resulting_accounts,
    );
    assert_invariant(&second, &pool);
    let closed = ok(
        &m,
        &close_pool_ix(&f, &f.stranger, &pool, None),
        &second.resulting_accounts,
    );
    assert_eq!(account_of(&closed, &vault_pda(&key).0).lamports, 0);
    assert_eq!(account_of(&closed, &key).data.len(), 0);
    let out = total_out(&closed, &accounts, &[f.creator, f.buyer, f.sponsor]);
    assert_eq!(
        out,
        15 * PRICE + SPONSORED + rent_for(Sponsorship::SIZE),
        "every purchase, the sponsorship, the Sponsorship rent"
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - lamports_of(&accounts, &f.fee_wallet),
        rent_for(0) + rent_for(Pool::SIZE)
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.destination, to_a(&f.fee_wallet));
}

#[test]
fn chain_drawn_cancel_pool_return_boxes_twice_close_pool() {
    // The admin cancels, the keeper returns in two batches, anyone closes.
    let f = Fixture::new();
    let m = mollusk_for_settlement(T0 + 600);
    let pool = sol_pool(&f, PoolStatus::Drawn, &full_plan(&f));
    let key = pool_key(&pool);
    let accounts = setup(&f, &m, &pool, &standard_record(), 0);
    let chain = [
        cancel_pool_ix(&f.admin, &pool, None),
        rb(&f, &pool, None, &[f.creator, f.buyer]),
        rb(&f, &pool, None, &[f.buyer_2]),
        close_pool_ix(&f, &f.stranger, &pool, None),
    ];
    let closed = run_chain(&m, &chain, &accounts, &pool);
    assert_eq!(account_of(&closed, &vault_pda(&key).0).lamports, 0);
    assert_eq!(
        total_out(&closed, &accounts, &[f.creator, f.buyer, f.buyer_2]),
        25 * PRICE
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.destination, to_a(&f.fee_wallet));
}

#[test]
fn chain_drawn_q1_split_close_sponsorship_close_pool() {
    // The suspended-after-Q1 chain: fees stay paid, the sponsorship stays in the prize pool,
    // the dust goes to the fee wallet.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 7_200);
    let pool = settled_pool(
        &f,
        &f.expected_config(),
        &sol_params(0),
        SPLIT_DUST_SPONSORED,
        1,
        true,
    );
    let key = pool_key(&pool);
    let accounts = with_sponsorship(
        setup(&f, &m, &pool, &suspended_record_with_quarters(1), 0),
        &pool,
        &f,
    );
    let batch: Vec<_> = [f.creator, f.buyer, f.buyer_2]
        .iter()
        .map(|o| (*o, None))
        .collect();
    let chain = [
        split_ix(&f.keeper, &pool, &standard_game(), &batch, None),
        close_sponsorship_ix(&pool, &f.sponsor, None),
        close_pool_ix(&f, &f.stranger, &pool, None),
    ];
    let closed = run_chain(&m, &chain, &accounts, &pool);
    assert_eq!(account_of(&closed, &vault_pda(&key).0).lamports, 0);
    assert_eq!(
        total_out(&closed, &accounts, &[f.creator, f.buyer, f.buyer_2]),
        25 * 67_200_000
    );
    assert_eq!(
        account_of(&closed, &f.sponsor).lamports - lamports_of(&accounts, &f.sponsor),
        rent_for(Sponsorship::SIZE),
        "the sponsor gets rent only"
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - lamports_of(&accounts, &f.fee_wallet),
        rent_for(0) + 11 + rent_for(Pool::SIZE)
    );
}

#[test]
fn chain_open_reclaim_twice_reclaim_sponsorship_close_pool_to_the_creator() {
    // The abandoned chain: nobody resolved the pool, the owners and the sponsor reclaim at
    // 30 days, and close_pool goes to the creator (ARCHITECTURE › Pool creation).
    let f = Fixture::new();
    let m = mollusk_for_settlement(RECLAIM_AT);
    let pool = sponsored(
        sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f)),
        SPONSORED,
    );
    let key = pool_key(&pool);
    let accounts = with_sponsorship(setup(&f, &m, &pool, &standard_record(), 1), &pool, &f);
    let chain = [
        reclaim_ix(&f.buyer, &pool, &standard_game(), Some(counter(&f)), None),
        reclaim_ix(&f.creator, &pool, &standard_game(), None, None),
        reclaim_sponsorship_ix(&f.sponsor, &pool, &standard_game(), None, None),
        close_pool_ix(&f, &f.stranger, &pool, None),
    ];
    let closed = run_chain(&m, &chain, &accounts, &pool);
    assert_eq!(account_of(&closed, &vault_pda(&key).0).lamports, 0);
    assert_eq!(
        total_out(&closed, &accounts, &[f.buyer, f.sponsor]),
        10 * PRICE + SPONSORED + rent_for(Sponsorship::SIZE)
    );
    assert_eq!(
        account_of(&closed, &f.creator).lamports - lamports_of(&accounts, &f.creator),
        5 * PRICE + rent_for(0) + rent_for(Pool::SIZE),
        "the creator's boxes plus both rents"
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports,
        lamports_of(&accounts, &f.fee_wallet)
    );
    let ev: PoolClosed = emitted_event(&closed).expect("PoolClosed");
    assert_eq!(ev.destination, to_a(&f.creator));
}

#[test]
fn chain_ore_locked_reclaim_then_the_keeper_finishes_close_pool_to_the_creators_ata() {
    // ORE: the buyer reclaims (abandoning the pool), the keeper returns the rest, and
    // close_pool sweeps nothing but closes the vault to the creator's ATA.
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
    let spl = SplBatch::of(&pool);
    let mut accounts = returns_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &standard_record(),
        0,
        Some(25 * PRICE_ORE),
    );
    for w in [f.creator, f.buyer, f.buyer_2] {
        set_account(&mut accounts, spl.ata(&w), system_account(0));
    }
    let reclaim = reclaim_ix(
        &f.buyer,
        &pool,
        &standard_game(),
        None,
        Some(SplOne::derived(&pool, &f.buyer)),
    );
    let batch = [
        (f.creator, Some(spl.ata(&f.creator))),
        (f.buyer_2, Some(spl.ata(&f.buyer_2))),
    ];
    let finish = return_boxes_ix(&f.keeper, &pool, &standard_game(), None, &batch, Some(spl));
    let close = close_pool_ix(
        &f,
        &f.stranger,
        &pool,
        Some(SplClose {
            mint: spl.mint,
            token_program: spl.token_program,
            destination_ata: spl.ata(&f.creator),
        }),
    );
    let r1 = ok(&m, &reclaim, &accounts);
    let after1 = decode_pool(account_of(&r1, &key));
    assert_eq!(
        (after1.status, after1.abandoned),
        (PoolStatus::Returned, true)
    );
    let r2 = ok(&m, &finish, &r1.resulting_accounts);
    assert_eq!(decode_pool(account_of(&r2, &key)).returned, 0x1FF_FFFF);
    assert_eq!(decode_token_amount(account_of(&r2, &vault_pda(&key).0)), 0);
    let r3 = ok(&m, &close, &r2.resulting_accounts);
    let vault = account_of(&r3, &vault_pda(&key).0);
    assert_eq!((vault.lamports, vault.data.len()), (0, 0));
    assert_eq!(
        decode_token_amount(account_of(&r3, &spl.ata(&f.creator))),
        5 * PRICE_ORE
    );
    assert_eq!(
        decode_token_amount(account_of(&r3, &spl.ata(&f.buyer))),
        10 * PRICE_ORE
    );
    assert_eq!(
        decode_token_amount(account_of(&r3, &spl.ata(&f.buyer_2))),
        10 * PRICE_ORE
    );
    let ev: PoolClosed = emitted_event(&r3).expect("PoolClosed");
    assert_eq!((ev.destination, ev.dust), (to_a(&f.creator), 0));
    // The vault's own rent and the pool's rent went to the creator.
    assert_eq!(
        account_of(&r3, &f.creator).lamports - lamports_of(&accounts, &f.creator),
        rent_for(TOKEN_ACCOUNT_LEN) + rent_for(Pool::SIZE)
    );
}

// ---------------------------------------------------------------------------
// close_counter after a return
// ---------------------------------------------------------------------------

#[test]
fn close_counter_once_a_return_took_the_last_open_pool_to_zero() {
    // PROGRAM §3.5: the counter reaches zero through return_boxes; with one pool still Open
    // it is CounterNotEmpty.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED);
    let pool = sol_pool(&f, PoolStatus::Open, &unfilled_15_plan(&f));
    let accounts = setup(&f, &m, &pool, &standard_record(), 1);
    let returned = ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.creator, f.buyer]),
        &accounts,
    );
    let fw0 = account_of(&returned, &f.fee_wallet).lamports;
    let counter_rent = account_of(&returned, &counter(&f)).lamports;
    let closed = ok(
        &m,
        &close_counter_ix(&f.creator, &standard_game(), &f.fee_wallet),
        &returned.resulting_accounts,
    );
    assert_eq!(
        account_of(&closed, &f.fee_wallet).lamports - fw0,
        counter_rent,
        "the counter's lamports (the fixture funds it above rent)"
    );
    assert_eq!(account_of(&closed, &counter(&f)).data.len(), 0);

    // Two pools open, one returned: the counter is at 1.
    let two = setup(&f, &m, &pool, &standard_record(), 2);
    let one_left = ok(
        &m,
        &rb(&f, &pool, Some(counter(&f)), &[f.creator, f.buyer]),
        &two,
    );
    assert_eq!(
        decode_counter(account_of(&one_left, &counter(&f))).open_count,
        1
    );
    let r = m.process_instruction(
        &close_counter_ix(&f.creator, &standard_game(), &f.fee_wallet),
        &one_left.resulting_accounts,
    );
    assert_eq!(custom_error(&r), Some(err(E::CounterNotEmpty)));
}

// ---------------------------------------------------------------------------
// Events, layout, IDL
// ---------------------------------------------------------------------------

#[test]
fn events_count_one_per_owner_paid_and_never_a_token_cpi() {
    // PROGRAM §7: three owners paid through three ATA creates and three transfers → exactly
    // three BoxesReturned payloads; the Token / ATA CPIs are inner instructions, not events.
    let f = Fixture::new();
    let m = mollusk_for_settlement(SCHEDULED + 3_600);
    let pool = pool_with_owners(
        &f,
        &f.expected_config(),
        &ore_params(0),
        PoolStatus::Drawn,
        &full_plan(&f),
    );
    let spl = SplBatch::of(&pool);
    let mut accounts = returns_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &marked_record(GameStatus::Postponed),
        0,
        Some(25 * PRICE_ORE),
    );
    let batch: Vec<_> = [f.creator, f.buyer, f.buyer_2]
        .iter()
        .map(|w| {
            set_account(&mut accounts, spl.ata(w), system_account(0));
            (*w, Some(spl.ata(w)))
        })
        .collect();
    let result = ok(
        &m,
        &return_boxes_ix(&f.keeper, &pool, &standard_game(), None, &batch, Some(spl)),
        &accounts,
    );
    assert_eq!(event_names(&result), ["BoxesReturned"; 3]);
    assert_eq!(emitted_event_count(&result), 3);
    let token_cpis = inner_program_ids(&result)
        .into_iter()
        .filter(|p| *p == token_program_id() || *p == associated_token_program_id())
        .count();
    assert!(
        token_cpis >= 6,
        "three creates and three transfers: {token_cpis}"
    );
}

#[test]
fn layout_and_idl_counts_are_unchanged_but_for_the_new_items() {
    // "Read this first" item 2: Pool::SIZE 1442, 65 errors, IDL 26 / 6 / 24 / 65.
    assert_eq!(Pool::SIZE, 1442);
    let idl: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../idl/mybarpool.json"
        ))
        .expect("idl/mybarpool.json"),
    )
    .expect("json");
    let count = |k: &str| idl[k].as_array().map_or(0, Vec::len);
    assert_eq!(
        (
            count("instructions"),
            count("accounts"),
            count("events"),
            count("errors")
        ),
        (26, 6, 24, 65)
    );
    let names: Vec<&str> = idl["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    for n in [
        "return_boxes",
        "return_sponsorship",
        "cancel_pool",
        "split",
        "reclaim",
        "reclaim_sponsorship",
        "close_sponsorship",
    ] {
        assert!(names.contains(&n), "{n} in the IDL");
    }
}
