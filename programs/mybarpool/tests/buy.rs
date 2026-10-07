//! `buy`, PROGRAM §4.3 and §6.1, under Mollusk with an explicit clock and
//! slot hash. Every spec number cites its line. Run after
//! `anchor build --arch v3`.

mod common;

use std::collections::BTreeSet;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::Check;
use mybarpool::{
    assignment::assign_boxes, BoxesBought, MybarpoolError as E, Pool, PoolLocked, PoolStatus,
};
use solana_pubkey::Pubkey;

fn custom(code: u32) -> solana_program_error::ProgramError {
    solana_program_error::ProgramError::Custom(code)
}

fn anchor(code: ErrorCode) -> u32 {
    code as u32
}

fn pool_key(f: &Fixture) -> Pubkey {
    pool_pda(&standard_game(), &f.creator, NONCE).0
}

/// A fresh open SOL pool with its vault at the rent floor and the counter at 1.
fn open_pool(f: &Fixture) -> (Pool, u64) {
    let pool = fresh_pool(f, &f.expected_config(), &sol_params(0));
    (pool, rent_for(0))
}

#[allow(clippy::too_many_arguments)]
fn expect_buy(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    pool: &Pool,
    vault_lamports: u64,
    open_count: u8,
    buyer: &Pubkey,
    count: u8,
    expect: Result<(), u32>,
) -> mollusk_svm::result::InstructionResult {
    let accounts = pool_accounts(f, m, &f.expected_config(), pool, vault_lamports, open_count);
    let check = match expect {
        Ok(()) => Check::success(),
        Err(code) => Check::err(custom(code)),
    };
    m.process_and_validate_instruction(
        &buy_ix(buyer, pool, count, TokenPath::default()),
        &accounts,
        &[check],
    )
}

// ---------------------------------------------------------------------------
// The happy path and determinism
// ---------------------------------------------------------------------------

#[test]
fn buy_3_assigns_three_distinct_boxes_and_moves_the_money_in() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let (pool, vault_lamports) = open_pool(&f);
    let accounts = pool_accounts(&f, &m, &f.expected_config(), &pool, vault_lamports, 1);
    let buyer_before = accounts
        .iter()
        .find(|(k, _)| *k == f.buyer)
        .unwrap()
        .1
        .lamports;
    let result = m.process_and_validate_instruction(
        &buy_ix(&f.buyer, &pool, 3, TokenPath::default()),
        &accounts,
        &[Check::success()],
    );

    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.sold, 3);
    assert_eq!(after.creator_boxes, 0);
    assert_eq!(after.status, PoolStatus::Open);
    let owned: Vec<usize> = after
        .owners
        .iter()
        .enumerate()
        .filter(|(_, o)| **o == to_a(&f.buyer))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(owned.len(), 3);
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&f)).0).lamports,
        vault_lamports + 3 * PRICE
    );
    assert_eq!(
        account_of(&result, &f.buyer).lamports,
        buyer_before - 3 * PRICE
    );

    assert_eq!(event_names(&result), ["BoxesBought"]);
    let event: BoxesBought = emitted_event(&result).expect("BoxesBought");
    assert_eq!(event.buyer, to_a(&f.buyer));
    assert_eq!((event.count, event.sold_after), (3, 3));
    assert_eq!(event.boxes.len(), 3);
    assert_eq!(
        event
            .boxes
            .iter()
            .map(|b| usize::from(*b))
            .collect::<BTreeSet<_>>(),
        owned.into_iter().collect::<BTreeSet<_>>()
    );

    // The boxes are exactly the pure function's answer for the fixture's slot hash (§6.1).
    let mut owners = pool.owners;
    let expected = assign_boxes(&SLOT_HASH, &to_a(&f.buyer), 0, 3, &mut owners).unwrap();
    assert_eq!(event.boxes, expected);
    assert_eq!(after.owners, owners);
}

#[test]
fn buy_is_deterministic_in_the_slot_hash() {
    // PROGRAM §6.1 "Deterministic given the inputs".
    let f = Fixture::new();
    let (pool, vault_lamports) = open_pool(&f);
    let boxes = |hash: [u8; 32]| {
        let m = mollusk_for_pools(T0, hash);
        let result = expect_buy(&f, &m, &pool, vault_lamports, 1, &f.buyer, 3, Ok(()));
        emitted_event::<BoxesBought>(&result).unwrap().boxes
    };
    assert_eq!(boxes(SLOT_HASH), boxes(SLOT_HASH));
    assert_ne!(boxes(SLOT_HASH), boxes([0xA5; 32]));
}

#[test]
fn count_bounds_nothing_to_buy_and_too_many_boxes() {
    // PROGRAM §4.3: 1 ≤ count ≤ 25 − sold.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let (fresh, floor) = open_pool(&f);
    expect_buy(
        &f,
        &m,
        &fresh,
        floor,
        1,
        &f.buyer,
        0,
        Err(err(E::NothingToBuy)),
    );
    let three_sold = pool_with(&f, PoolStatus::Open, 3, &f.buyer_2);
    expect_buy(
        &f,
        &m,
        &three_sold,
        floor + 3 * PRICE,
        1,
        &f.buyer,
        23,
        Err(err(E::TooManyBoxes)),
    );
    let result = expect_buy(
        &f,
        &m,
        &three_sold,
        floor + 3 * PRICE,
        1,
        &f.buyer,
        22,
        Ok(()),
    );
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.sold, 25);
    assert_eq!(after.status, PoolStatus::Locked);
}

// ---------------------------------------------------------------------------
// The creator's cap
// ---------------------------------------------------------------------------

#[test]
fn the_creator_is_capped_at_max_own_boxes_with_override_precedence() {
    // ARCHITECTURE › Limits: 5 own boxes across create and later buys; §3.6 override wins.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let two_own = pool_with(&f, PoolStatus::Open, 2, &f.creator);
    let floor = rent_for(0) + 2 * PRICE;
    let result = expect_buy(&f, &m, &two_own, floor, 1, &f.creator, 3, Ok(()));
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.creator_boxes, 5);
    assert_eq!(after.sold, 5);
    expect_buy(
        &f,
        &m,
        &two_own,
        floor,
        1,
        &f.creator,
        4,
        Err(err(E::OwnBoxLimit)),
    );

    let mut accounts = pool_accounts(&f, &m, &f.expected_config(), &two_own, floor, 1);
    set_account(
        &mut accounts,
        override_pda(&f.creator).0,
        override_account(&f.creator, 3, 3),
    );
    m.process_and_validate_instruction(
        &buy_ix(&f.creator, &two_own, 3, TokenPath::default()),
        &accounts,
        &[Check::err(custom(err(E::OwnBoxLimit)))],
    );
    m.process_and_validate_instruction(
        &buy_ix(&f.creator, &two_own, 1, TokenPath::default()),
        &accounts,
        &[Check::success()],
    );
}

#[test]
fn the_creator_pays_full_price_like_any_buyer() {
    // ARCHITECTURE › Pool creation: the creator's boxes go into the pot at full price.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let (pool, floor) = open_pool(&f);
    let accounts = pool_accounts(&f, &m, &f.expected_config(), &pool, floor, 1);
    let before = accounts
        .iter()
        .find(|(k, _)| *k == f.creator)
        .unwrap()
        .1
        .lamports;
    let result = m.process_and_validate_instruction(
        &buy_ix(&f.creator, &pool, 2, TokenPath::default()),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        account_of(&result, &vault_pda(&pool_key(&f)).0).lamports,
        floor + 2 * PRICE
    );
    assert_eq!(account_of(&result, &f.creator).lamports, before - 2 * PRICE);
    assert_eq!(
        decode_pool(account_of(&result, &pool_key(&f))).creator_boxes,
        2
    );
}

// ---------------------------------------------------------------------------
// The lock
// ---------------------------------------------------------------------------

#[test]
fn the_25th_box_locks_the_pool_decrements_the_counter_and_emits_pool_locked() {
    // PROGRAM §4.3 effects, §9 Open --buy (25th)--> Locked.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let almost = pool_with(&f, PoolStatus::Open, 24, &f.buyer_2);
    let floor = rent_for(0) + 24 * PRICE;
    let result = expect_buy(&f, &m, &almost, floor, 2, &f.buyer, 1, Ok(()));
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.status, PoolStatus::Locked);
    assert_eq!(after.locked_at, T0);
    assert_eq!(after.sold, 25);
    assert!(after
        .owners
        .iter()
        .all(|o| *o != anchor_lang::prelude::Pubkey::default()));
    let counter = decode_counter(account_of(
        &result,
        &counter_pda(&f.creator, &standard_game()).0,
    ));
    assert_eq!(counter.open_count, 1); // 2 − 1
    assert_eq!(event_names(&result), ["BoxesBought", "PoolLocked"]);
    let locked: PoolLocked = emitted_event(&result).expect("PoolLocked");
    assert_eq!((locked.time, locked.locked_at), (T0, T0));
    assert_eq!(locked.pool, to_a(&pool_key(&f)));
}

#[test]
fn a_lock_with_a_zero_counter_is_math_overflow() {
    // The decrement cannot go negative (this brief).
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let almost = pool_with(&f, PoolStatus::Open, 24, &f.buyer_2);
    expect_buy(
        &f,
        &m,
        &almost,
        rent_for(0) + 24 * PRICE,
        0,
        &f.buyer,
        1,
        Err(err(E::MathOverflow)),
    );
}

// ---------------------------------------------------------------------------
// Status, time, pause, game
// ---------------------------------------------------------------------------

#[test]
fn buy_on_a_pool_that_is_not_open_is_pool_not_open() {
    // PROGRAM §9: only Open sells.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    for status in [
        PoolStatus::Locked,
        PoolStatus::Drawn,
        PoolStatus::Returned,
        PoolStatus::Settled,
        PoolStatus::Split,
    ] {
        let planted = pool_with(&f, status, 25, &f.buyer_2);
        expect_buy(
            &f,
            &m,
            &planted,
            rent_for(0) + 25 * PRICE,
            0,
            &f.buyer,
            1,
            Err(err(E::PoolNotOpen)),
        );
    }
}

#[test]
fn buy_after_kickoff_on_a_marked_game_or_while_paused_is_refused() {
    // PROGRAM §4.3: now < recorded_kickoff; game Scheduled; !paused.
    let f = Fixture::new();
    let (pool, floor) = open_pool(&f);
    let m = mollusk_for_pools(SCHEDULED, SLOT_HASH);
    expect_buy(
        &f,
        &m,
        &pool,
        floor,
        1,
        &f.buyer,
        1,
        Err(err(E::SalesClosed)),
    );

    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut record = standard_record();
    record.status = mybarpool::GameStatus::Postponed;
    record.marked_at = T0 - 60;
    let mut accounts = pool_accounts(&f, &m, &f.expected_config(), &pool, floor, 1);
    set_account(&mut accounts, standard_game(), game_record_account(&record));
    m.process_and_validate_instruction(
        &buy_ix(&f.buyer, &pool, 1, TokenPath::default()),
        &accounts,
        &[Check::err(custom(err(E::GameNotScheduled)))],
    );

    let mut paused = f.expected_config();
    paused.paused = true;
    let accounts = pool_accounts(&f, &m, &paused, &pool, floor, 1);
    m.process_and_validate_instruction(
        &buy_ix(&f.buyer, &pool, 1, TokenPath::default()),
        &accounts,
        &[Check::err(custom(err(E::Paused)))],
    );
}

// ---------------------------------------------------------------------------
// Plumbing: wrong accounts fail their constraints (PROGRAM §10)
// ---------------------------------------------------------------------------

#[test]
fn wrong_game_counter_vault_or_pool_address_fails_its_constraint() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let (pool, floor) = open_pool(&f);
    let base = pool_accounts(&f, &m, &f.expected_config(), &pool, floor, 1);
    // Account order in Buy: buyer 0, config 1, game 2, pool 3, vault 4, counter 5, override 6, ...

    // Another record as `game` → has_one.
    let other_game = game_pda(&standard_key(), SCHEDULED + 3_600).0;
    let mut accounts = base.clone();
    accounts.push((
        other_game,
        game_record_account(&fresh_record(standard_key(), SCHEDULED + 3_600)),
    ));
    let mut ix = buy_ix(&f.buyer, &pool, 1, TokenPath::default());
    ix.accounts[2].pubkey = other_game;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(anchor(ErrorCode::ConstraintHasOne))
    );

    // Another creator's counter → seeds.
    let other_counter = counter_pda(&f.stranger, &standard_game()).0;
    let mut accounts = base.clone();
    accounts.push((
        other_counter,
        counter_account(&f.stranger, &standard_game(), 1),
    ));
    let mut ix = buy_ix(&f.buyer, &pool, 1, TokenPath::default());
    ix.accounts[5].pubkey = other_counter;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(anchor(ErrorCode::ConstraintSeeds))
    );

    // A vault at the wrong address → seeds.
    let other_vault = Pubkey::new_unique();
    let mut accounts = base.clone();
    accounts.push((other_vault, sol_vault_account(floor)));
    let mut ix = buy_ix(&f.buyer, &pool, 1, TokenPath::default());
    ix.accounts[4].pubkey = other_vault;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(anchor(ErrorCode::ConstraintSeeds))
    );

    // The pool planted at another address → seeds.
    let planted = Pubkey::new_unique();
    let mut accounts = base.clone();
    accounts.push((planted, pool_account(&pool)));
    let mut ix = buy_ix(&f.buyer, &pool, 1, TokenPath::default());
    ix.accounts[3].pubkey = planted;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(anchor(ErrorCode::ConstraintSeeds))
    );
}

// ---------------------------------------------------------------------------
// SPL (PROGRAM §5.4)
// ---------------------------------------------------------------------------

/// An open ORE pool whose vault is a token account owned by the pool holding `amount`.
fn ore_pool_accounts(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    pool: &Pool,
    vault_amount: u64,
    buyer_ata: Pubkey,
    buyer_amount: u64,
) -> Vec<(Pubkey, solana_account::Account)> {
    let mut accounts = pool_accounts(f, m, &f.expected_config(), pool, 0, 1);
    let key = pool_key(f);
    set_account(
        &mut accounts,
        vault_pda(&key).0,
        token_account(&f.ore_mint, &key, vault_amount),
    );
    accounts.push((
        buyer_ata,
        token_account(&f.ore_mint, &f.buyer, buyer_amount),
    ));
    accounts
}

#[test]
fn ore_buy_moves_tokens_by_transfer_checked_into_the_vault() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = fresh_pool(&f, &f.expected_config(), &ore_params(0));
    let buyer_ata = Pubkey::new_unique();
    let accounts = ore_pool_accounts(&f, &m, &pool, 0, buyer_ata, 10 * PRICE_ORE);
    let path = TokenPath {
        mint: Some(f.ore_mint),
        token_account: Some(buyer_ata),
        token_program: Some(token_program_id()),
    };
    let result = m.process_and_validate_instruction(
        &buy_ix(&f.buyer, &pool, 2, path),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &vault_pda(&pool_key(&f)).0)),
        2 * PRICE_ORE
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &buyer_ata)),
        8 * PRICE_ORE
    );
    assert_eq!(decode_pool(account_of(&result, &pool_key(&f))).sold, 2);
    // The transfer_checked CPI is beside the event and not counted as one (Step 2 audit L2).
    assert_eq!(event_names(&result), ["BoxesBought"]);
    assert!(result.inner_instructions.len() >= 2);
}

#[test]
fn ore_buy_plumbing_errors_are_anchors() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = fresh_pool(&f, &f.expected_config(), &ore_params(0));
    let buyer_ata = Pubkey::new_unique();
    let mut accounts = ore_pool_accounts(&f, &m, &pool, 0, buyer_ata, 10 * PRICE_ORE);
    let good = TokenPath {
        mint: Some(f.ore_mint),
        token_account: Some(buyer_ata),
        token_program: Some(token_program_id()),
    };

    // A token account of another mint → ConstraintTokenMint.
    let wrong_mint_ata = Pubkey::new_unique();
    accounts.push((
        wrong_mint_ata,
        token_2022_account(&f.skr_mint, &f.buyer, 10 * PRICE_ORE),
    ));
    let r = m.process_instruction(
        &buy_ix(
            &f.buyer,
            &pool,
            1,
            TokenPath {
                token_account: Some(wrong_mint_ata),
                ..good
            },
        ),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::ConstraintTokenMint))
    );

    // Token-2022 for an ORE pool → RequireKeysEqViolated.
    let r = m.process_instruction(
        &buy_ix(
            &f.buyer,
            &pool,
            1,
            TokenPath {
                token_program: Some(token_2022_program_id()),
                ..good
            },
        ),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::RequireKeysEqViolated))
    );

    // No mint → ConstraintAccountIsNone.
    let r = m.process_instruction(
        &buy_ix(&f.buyer, &pool, 1, TokenPath { mint: None, ..good }),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::ConstraintAccountIsNone))
    );
}

// ---------------------------------------------------------------------------
// Scatter and composability
// ---------------------------------------------------------------------------

#[test]
fn a_3_box_buy_on_a_fresh_grid_is_never_1_2_3_across_100_seeded_runs() {
    // ARCHITECTURE › Buying "never 1-2-3"; the Step 4 acceptance list. Pure-function check over
    // 100 slot hashes sha256(i), plus one Mollusk run showing the instruction uses that function.
    use solana_sha256_hasher::hashv;
    let f = Fixture::new();
    let buyer = to_a(&f.buyer);
    let first_three: BTreeSet<u8> = [0u8, 1, 2].into_iter().collect();
    let mut hashes = Vec::new();
    for i in 0u32..100 {
        let h = hashv(&[&i.to_le_bytes()]).to_bytes();
        hashes.push(h);
        let mut owners = [anchor_lang::prelude::Pubkey::default(); 25];
        let boxes = assign_boxes(&h, &buyer, 0, 3, &mut owners).unwrap();
        assert_ne!(
            boxes.iter().copied().collect::<BTreeSet<u8>>(),
            first_three,
            "run {i}"
        );
    }
    let (pool, floor) = open_pool(&f);
    let m = mollusk_for_pools(T0, hashes[17]);
    let result = expect_buy(&f, &m, &pool, floor, 1, &f.buyer, 3, Ok(()));
    let mut owners = pool.owners;
    let expected = assign_boxes(&hashes[17], &buyer, 0, 3, &mut owners).unwrap();
    assert_eq!(
        emitted_event::<BoxesBought>(&result).unwrap().boxes,
        expected
    );
}

#[test]
fn buy_and_sponsor_are_unaffected_by_neighbouring_instructions() {
    // PROGRAM §10 Composability.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let (pool, floor) = open_pool(&f);
    let accounts = pool_accounts(&f, &m, &f.expected_config(), &pool, floor, 1);
    let noop = solana_instruction::Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], // SystemInstruction::Transfer { lamports: 1 }
        vec![
            solana_instruction::AccountMeta::new(f.buyer, true),
            solana_instruction::AccountMeta::new(f.admin, false),
        ],
    );
    let buy = buy_ix(&f.buyer, &pool, 1, TokenPath::default());
    let sponsor = sponsor_ix(&f.sponsor, &pool, PRICE, TokenPath::default());
    let alone = m.process_instruction_chain(&[buy.clone(), sponsor.clone()], &accounts);
    let sandwiched =
        m.process_instruction_chain(&[noop.clone(), buy, noop.clone(), sponsor, noop], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_pool(account_of(&alone, &pool_key(&f))),
        decode_pool(account_of(&sandwiched, &pool_key(&f)))
    );
    assert_eq!(
        account_of(&alone, &vault_pda(&pool_key(&f)).0).lamports,
        account_of(&sandwiched, &vault_pda(&pool_key(&f)).0).lamports
    );
}
