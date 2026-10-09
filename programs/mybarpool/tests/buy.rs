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

// ---------------------------------------------------------------------------
// Gating (PROGRAM §4.3, §6.4; Step 8)
// ---------------------------------------------------------------------------

use mybarpool::{allowlist, AccessType};

/// `buy` on `pool` with a gate slot and proof, every wallet in `extra` funded.
#[allow(clippy::too_many_arguments)]
fn gated_buy(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    pool: &Pool,
    buyer: &Pubkey,
    count: u8,
    gate: Option<(Pubkey, bool)>,
    proof: Vec<[u8; 32]>,
    extra: &[Pubkey],
) -> mollusk_svm::result::InstructionResult {
    let vault = rent_for(0) + u64::from(pool.sold) * PRICE;
    let mut accounts = pool_accounts_with_wallets(f, m, pool, vault, 1, extra);
    if let Some((key, _)) = gate {
        if !accounts.iter().any(|(k, _)| *k == key) {
            accounts.push((key, system_account(0)));
        }
    }
    m.process_instruction(
        &buy_ix_gated(buyer, pool, count, TokenPath::default(), gate, proof),
        &accounts,
    )
}

fn assert_nothing_moved(result: &mollusk_svm::result::InstructionResult, f: &Fixture, pool: &Pool) {
    let key = pool_key(f);
    assert_eq!(decode_pool(account_of(result, &key)), *pool);
    assert_eq!(
        account_of(result, &vault_pda(&key).0).lamports,
        rent_for(0) + u64::from(pool.sold) * PRICE
    );
}

fn boxes_of(result: &mollusk_svm::result::InstructionResult, f: &Fixture, owner: &Pubkey) -> usize {
    decode_pool(account_of(result, &pool_key(f)))
        .owners
        .iter()
        .filter(|o| **o == to_a(owner))
        .count()
}

#[test]
fn buy_link_with_the_gate_key_signing_succeeds() {
    // PROGRAM §4.3: on a Link pool the gate_key account is present, equals pool.gate_key and signed.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = link_pool(&f);
    let r = gated_buy(
        &f,
        &m,
        &pool,
        &f.buyer,
        2,
        Some((gate_key_g(), true)),
        vec![],
        &[],
    );
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
    assert_eq!(boxes_of(&r, &f, &f.buyer), 2);
    assert_eq!(decode_pool(account_of(&r, &pool_key(&f))).sold, 7);
    assert_eq!(event_names(&r), ["BoxesBought"]);
    let e: BoxesBought = emitted_event(&r).unwrap();
    assert_eq!((e.buyer, e.count, e.sold_after), (to_a(&f.buyer), 2, 7));
    // The gate key is a read-only signer: untouched.
    assert_eq!(account_of(&r, &gate_key_g()).lamports, 0);
}

#[test]
fn buy_link_without_the_slot_is_gate_key_not_signer() {
    // PROGRAM §8: GateKeyNotSigner 6017 when the gate_key account is absent.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = link_pool(&f);
    let r = gated_buy(&f, &m, &pool, &f.buyer, 1, None, vec![], &[]);
    assert_eq!(custom_error(&r), Some(err(E::GateKeyNotSigner)));
    assert_eq!(err(E::GateKeyNotSigner), 6017);
    assert_nothing_moved(&r, &f, &pool);
}

#[test]
fn buy_link_with_another_key_signing_is_gate_key_not_signer() {
    // PROGRAM §4.3: its key equals pool.gate_key.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = link_pool(&f);
    let r = gated_buy(
        &f,
        &m,
        &pool,
        &f.buyer,
        1,
        Some((f.buyer_2, true)),
        vec![],
        &[],
    );
    assert_eq!(custom_error(&r), Some(err(E::GateKeyNotSigner)));
    assert_nothing_moved(&r, &f, &pool);
}

#[test]
fn buy_link_with_the_gate_key_not_signing_is_gate_key_not_signer() {
    // PROGRAM §4.3: "and it signed". The right key without is_signer is still 6017.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = link_pool(&f);
    let r = gated_buy(
        &f,
        &m,
        &pool,
        &f.buyer,
        1,
        Some((gate_key_g(), false)),
        vec![],
        &[],
    );
    assert_eq!(custom_error(&r), Some(err(E::GateKeyNotSigner)));
    assert_nothing_moved(&r, &f, &pool);
}

#[test]
fn buy_link_after_rotate_gate_key_takes_only_the_new_key() {
    // PROGRAM §4.3 rotate_gate_key: only signatures from the new key matter.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = link_pool(&f);
    let g2 = Pubkey::new_from_array([0x62; 32]);
    let accounts = pool_accounts_with_wallets(&f, &m, &pool, rent_for(0) + 5 * PRICE, 1, &[g2]);
    let rotate = rotate_gate_key_ix(&f.creator, &pool, &g2);
    let with_old = buy_ix_gated(
        &f.buyer,
        &pool,
        1,
        TokenPath::default(),
        Some((gate_key_g(), true)),
        vec![],
    );
    let with_new = buy_ix_gated(
        &f.buyer,
        &pool,
        1,
        TokenPath::default(),
        Some((g2, true)),
        vec![],
    );

    let old_fails = m.process_instruction_chain(&[rotate.clone(), with_old], &accounts);
    assert_eq!(custom_error(&old_fails), Some(err(E::GateKeyNotSigner)));

    let new_works = m.process_instruction_chain(&[rotate, with_new], &accounts);
    assert!(
        new_works.program_result.is_ok(),
        "{:?}",
        new_works.program_result
    );
    let after = decode_pool(account_of(&new_works, &pool_key(&f)));
    assert_eq!(after.gate_key, to_a(&g2));
    assert_eq!(after.sold, 6);
}

#[test]
fn buy_link_by_the_creator_is_gated_too() {
    // PROGRAM §4.3: "The creator buying in their own gated pool through buy is gated like
    // anyone else." ARCHITECTURE › Limits: 5 own boxes, so the creator at 5 needs a raise.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut pool = link_pool(&f);
    pool.creator_boxes = 2;
    let r = gated_buy(&f, &m, &pool, &f.creator, 1, None, vec![], &[]);
    assert_eq!(custom_error(&r), Some(err(E::GateKeyNotSigner)));
    let r = gated_buy(
        &f,
        &m,
        &pool,
        &f.creator,
        1,
        Some((gate_key_g(), true)),
        vec![],
        &[],
    );
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
    assert_eq!(decode_pool(account_of(&r, &pool_key(&f))).creator_boxes, 3);
}

#[test]
fn buy_allowlist_with_a_valid_proof_succeeds() {
    // PROGRAM §6.4: verify(root, w, proof) for every member; the 8-list and the 25-list.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    for (n, members) in [(8usize, 8usize), (25, 3)] {
        let wallets = allowlist_wallets(n);
        let pool = allowlist_pool(&f, &wallets);
        for w in wallets.iter().take(members) {
            let proof = tree::proof(&wallets, w);
            assert!(allowlist::verify(&pool.allowlist_root, &to_a(w), &proof));
            let r = gated_buy(&f, &m, &pool, w, 1, None, proof, &wallets);
            assert!(r.program_result.is_ok(), "list {n}: {:?}", r.program_result);
            assert_eq!(boxes_of(&r, &f, w), 1);
            assert_eq!(event_names(&r), ["BoxesBought"]);
        }
    }
}

#[test]
fn buy_allowlist_with_a_wrong_proof_is_allowlist_proof_invalid() {
    // PROGRAM §8: AllowlistProofInvalid 6018 on a false verify.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let wallets = allowlist_wallets(8);
    let pool = allowlist_pool(&f, &wallets);
    assert_eq!(err(E::AllowlistProofInvalid), 6018);
    let (a, b) = (wallets[0], wallets[5]);
    let mut flipped = tree::proof(&wallets, &a);
    flipped[0][0] ^= 0x01;
    let cases: Vec<(Pubkey, Vec<[u8; 32]>)> = vec![
        (a, tree::proof(&wallets, &b)),       // another member's proof
        (a, flipped),                         // one entry flipped
        (f.buyer, tree::proof(&wallets, &a)), // a non-member with a member's proof
        (a, vec![]),                          // a member with an empty proof on an 8-list
    ];
    for (who, proof) in cases {
        let r = gated_buy(&f, &m, &pool, &who, 1, None, proof, &wallets);
        assert_eq!(
            custom_error(&r),
            Some(err(E::AllowlistProofInvalid)),
            "{who}"
        );
        assert_nothing_moved(&r, &f, &pool);
    }
}

#[test]
fn buy_allowlist_single_wallet_list_takes_an_empty_proof() {
    // PROGRAM §6.4: one wallet → root = leaf(w); proof(w) = [].
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let wallets = vec![f.buyer_2];
    let pool = allowlist_pool(&f, &wallets);
    assert_eq!(pool.allowlist_root, allowlist::leaf(&to_a(&f.buyer_2)));
    let r = gated_buy(&f, &m, &pool, &f.buyer_2, 1, None, vec![], &[]);
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
    let r = gated_buy(&f, &m, &pool, &f.buyer, 1, None, vec![], &[]);
    assert_eq!(custom_error(&r), Some(err(E::AllowlistProofInvalid)));
}

/// A proof of `depth` planted siblings and the root they fold to from `wallet`'s leaf.
fn planted_chain(wallet: &Pubkey, depth: usize) -> (Vec<[u8; 32]>, [u8; 32]) {
    let proof: Vec<[u8; 32]> = (0..depth)
        .map(|i| {
            let mut s = [0x50u8; 32];
            s[31] = i as u8;
            s
        })
        .collect();
    let root = proof.iter().fold(allowlist::leaf(&to_a(wallet)), |acc, s| {
        allowlist::node(&acc, s)
    });
    (proof, root)
}

#[test]
fn buy_allowlist_proof_of_33_entries_is_invalid() {
    // PROGRAM §6.4: len(proof) ≤ 32, checked before the fold; MAX_PROOF_LEN = 32.
    assert_eq!(allowlist::MAX_PROOF_LEN, 32);
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    // The root is the fold of all 33 entries, so only the length bound can refuse this proof
    // (a 33rd entry that breaks the hash would be refused by the fold alone and would not
    // tell the bound apart from it).
    let (proof, root) = planted_chain(&f.buyer, 33);
    let mut pool = fresh_pool(&f, &f.expected_config(), &sol_params(0));
    pool.access_type = AccessType::Allowlist;
    pool.allowlist_root = root;
    assert_eq!(
        proof
            .iter()
            .fold(allowlist::leaf(&to_a(&f.buyer)), |acc, s| allowlist::node(&acc, s)),
        root
    );
    assert!(!allowlist::verify(&root, &to_a(&f.buyer), &proof));
    let r = gated_buy(&f, &m, &pool, &f.buyer, 1, None, proof, &[]);
    assert_eq!(custom_error(&r), Some(err(E::AllowlistProofInvalid)));
    assert_nothing_moved(&r, &f, &pool);
}

#[test]
fn buy_allowlist_depth_32_proof_verifies() {
    // PROGRAM §6.4 at the limit: 32 entries fold to the planted root (the bench row's shape).
    let f = Fixture::new();
    let mut m = mollusk_for_pools(T0, SLOT_HASH);
    m.compute_budget.compute_unit_limit = 400_000;
    let (proof, root) = planted_chain(&f.buyer, 32);
    let mut pool = fresh_pool(&f, &f.expected_config(), &sol_params(0));
    pool.access_type = AccessType::Allowlist;
    pool.allowlist_root = root;
    let r = gated_buy(&f, &m, &pool, &f.buyer, 1, None, proof, &[]);
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
    assert_eq!(boxes_of(&r, &f, &f.buyer), 1);
}

#[test]
fn buy_public_ignores_the_gate_key_slot_and_the_proof() {
    // PROGRAM §4.3: the slot is ignored on pools that are not Link, the proof on pools that
    // are not Allowlist.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let (pool, _) = open_pool(&f);
    let r = gated_buy(
        &f,
        &m,
        &pool,
        &f.buyer,
        1,
        Some((f.buyer_2, true)),
        vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        &[],
    );
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
    assert_eq!(boxes_of(&r, &f, &f.buyer), 1);
}

#[test]
fn buy_link_ignores_the_proof_and_allowlist_ignores_the_slot() {
    // PROGRAM §4.3 "so one client code path serves every pool".
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let link = link_pool(&f);
    let r = gated_buy(
        &f,
        &m,
        &link,
        &f.buyer,
        1,
        Some((gate_key_g(), true)),
        vec![[0xEE; 32]; 5],
        &[],
    );
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);

    let wallets = allowlist_wallets(8);
    let pool = allowlist_pool(&f, &wallets);
    let r = gated_buy(
        &f,
        &m,
        &pool,
        &wallets[3],
        1,
        Some((f.buyer_2, true)),
        tree::proof(&wallets, &wallets[3]),
        &wallets,
    );
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
}

#[test]
fn buy_gating_runs_after_the_count_and_cap_checks() {
    // PROGRAM §4.3 order: count (NothingToBuy 6024) and the own-box cap (OwnBoxLimit) come
    // before gating; ARCHITECTURE › Limits: 5 own boxes.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = link_pool(&f);
    let r = gated_buy(&f, &m, &pool, &f.buyer, 0, None, vec![], &[]);
    assert_eq!(custom_error(&r), Some(err(E::NothingToBuy)));
    assert_eq!(err(E::NothingToBuy), 6024);
    // The creator holds 5 already (link_pool): the cap fires even with G signing.
    let r = gated_buy(
        &f,
        &m,
        &pool,
        &f.creator,
        1,
        Some((gate_key_g(), true)),
        vec![],
        &[],
    );
    assert_eq!(custom_error(&r), Some(err(E::OwnBoxLimit)));
}

#[test]
fn buy_link_and_allowlist_on_an_ore_pool() {
    // PROGRAM §5.4 with §4.3 gating: the token moves on a gated success.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let buyer_ata = Pubkey::new_from_array([0x7A; 32]);
    let path = TokenPath {
        mint: Some(f.ore_mint),
        token_account: Some(buyer_ata),
        token_program: Some(token_program_id()),
    };

    let mut link = fresh_pool(&f, &f.expected_config(), &ore_params(0));
    link.access_type = AccessType::Link;
    link.gate_key = to_a(&gate_key_g());
    let mut accounts = ore_pool_accounts(&f, &m, &link, 0, buyer_ata, 10 * PRICE_ORE);
    accounts.push((gate_key_g(), system_account(0)));
    let r = m.process_instruction(
        &buy_ix_gated(&f.buyer, &link, 1, path, Some((gate_key_g(), true)), vec![]),
        &accounts,
    );
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
    assert_eq!(
        decode_token_amount(account_of(&r, &vault_pda(&pool_key(&f)).0)),
        PRICE_ORE
    );

    let wallets = vec![f.buyer, f.buyer_2, f.sponsor];
    let mut allow = fresh_pool(&f, &f.expected_config(), &ore_params(0));
    allow.access_type = AccessType::Allowlist;
    allow.allowlist_root = tree::root(&wallets);
    let accounts = ore_pool_accounts(&f, &m, &allow, 0, buyer_ata, 10 * PRICE_ORE);
    let r = m.process_instruction(
        &buy_ix_gated(
            &f.buyer,
            &allow,
            1,
            path,
            None,
            tree::proof(&wallets, &f.buyer),
        ),
        &accounts,
    );
    assert!(r.program_result.is_ok(), "{:?}", r.program_result);
    assert_eq!(
        decode_token_amount(account_of(&r, &vault_pda(&pool_key(&f)).0)),
        PRICE_ORE
    );
    assert_eq!(
        decode_token_amount(account_of(&r, &buyer_ata)),
        9 * PRICE_ORE
    );
}

#[test]
fn buy_gated_is_composable() {
    // PROGRAM §10 Composability, on the gated path.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = link_pool(&f);
    let accounts = pool_accounts_with_wallets(&f, &m, &pool, rent_for(0) + 5 * PRICE, 1, &[]);
    let noop = solana_instruction::Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], // SystemInstruction::Transfer { lamports: 1 }
        vec![
            solana_instruction::AccountMeta::new(f.buyer, true),
            solana_instruction::AccountMeta::new(f.admin, false),
        ],
    );
    let buy = buy_ix_gated(
        &f.buyer,
        &pool,
        1,
        TokenPath::default(),
        Some((gate_key_g(), true)),
        vec![],
    );
    let alone = m.process_instruction_chain(&[buy.clone()], &accounts);
    let sandwiched = m.process_instruction_chain(&[noop.clone(), buy, noop], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_pool(account_of(&alone, &pool_key(&f))),
        decode_pool(account_of(&sandwiched, &pool_key(&f)))
    );
}

#[test]
fn gating_events_and_layout_are_unchanged() {
    // PROGRAM §3.3: Pool::SIZE 1,442, access_type 186, gate_key 187, allowlist_root 219;
    // §8: 65 errors (6000–6064); the IDL stays 26 / 6 / 24 / 65 (layout.rs and errors.rs
    // hold the per-field and per-code assertions).
    assert_eq!(Pool::SIZE, 1_442);
    assert_eq!(err(E::VarCommitMismatch), 6064);
    let idl: serde_json::Value =
        serde_json::from_str(include_str!("../../../idl/mybarpool.json")).unwrap();
    let n = |k: &str| idl[k].as_array().unwrap().len();
    assert_eq!(
        (n("instructions"), n("accounts"), n("events"), n("errors")),
        (26, 6, 24, 65)
    );
    let buy = idl["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "buy")
        .unwrap();
    assert_eq!(buy["args"][1]["name"], "allowlist_proof");
    let gate = buy["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "gate_key")
        .unwrap();
    assert_eq!(gate["optional"], true);
}
