//! `sponsor`, `rotate_gate_key` and `close_counter`, PROGRAM §4.3 and §3.5,
//! under Mollusk. Every spec number cites its line. Run after
//! `anchor build --arch v3`.

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::Check;
use mybarpool::{AccessType, GateKeyRotated, MybarpoolError as E, Pool, PoolStatus, Sponsored};
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

fn open_pool(f: &Fixture) -> Pool {
    fresh_pool(f, &f.expected_config(), &sol_params(0))
}

fn expect_sponsor(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    config: &mybarpool::PlatformConfig,
    pool: &Pool,
    sponsor: &Pubkey,
    amount: u64,
    expect: Result<(), u32>,
) -> mollusk_svm::result::InstructionResult {
    let accounts = pool_accounts(f, m, config, pool, rent_for(0), 1);
    let check = match expect {
        Ok(()) => Check::success(),
        Err(code) => Check::err(custom(code)),
    };
    m.process_and_validate_instruction(
        &sponsor_ix(sponsor, pool, amount, TokenPath::default()),
        &accounts,
        &[check],
    )
}

// ---------------------------------------------------------------------------
// sponsor
// ---------------------------------------------------------------------------

#[test]
fn sponsor_creates_the_account_adds_to_the_vault_and_touches_nothing_else() {
    // PROGRAM §3.7, §4.3; ARCHITECTURE › Sponsorship "No fee": a sponsorship is not a box.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = open_pool(&f);
    let result = expect_sponsor(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        &f.sponsor,
        PRICE,
        Ok(()),
    );

    let key = pool_key(&f);
    let s_key = sponsorship_pda(&key, &f.sponsor).0;
    let account = account_of(&result, &s_key);
    assert_eq!(account.data.len(), 81); // PROGRAM §3.7: 8 + 73
    let s = decode_sponsorship(account);
    assert_eq!(s.pool, to_a(&key));
    assert_eq!(s.wallet, to_a(&f.sponsor));
    assert_eq!(s.amount, PRICE);
    assert_eq!(s.bump, sponsorship_pda(&key, &f.sponsor).1);

    let after = decode_pool(account_of(&result, &key));
    assert_eq!(after.sponsored_total, PRICE);
    assert_eq!(after.sponsor_count, 1);
    assert_eq!(after.sponsorships_open, 1);
    assert_eq!(after.owners, pool.owners);
    assert_eq!(after.sold, pool.sold);
    assert_eq!(after.creator_boxes, pool.creator_boxes);
    assert_eq!(
        (after.platform_fee, after.creator_fee, after.integrator_fee),
        (pool.platform_fee, pool.creator_fee, pool.integrator_fee)
    );
    assert_eq!(
        account_of(&result, &vault_pda(&key).0).lamports,
        rent_for(0) + PRICE
    );
    assert_eq!(
        decode_counter(account_of(
            &result,
            &counter_pda(&f.creator, &standard_game()).0
        ))
        .open_count,
        1
    );

    assert_eq!(event_names(&result), ["Sponsored"]);
    let event: Sponsored = emitted_event(&result).expect("Sponsored");
    assert_eq!(
        (event.sponsor, event.amount, event.sponsored_total),
        (to_a(&f.sponsor), PRICE, PRICE)
    );
    assert_eq!(event.pool, to_a(&key));
}

#[test]
fn a_second_sponsor_call_tops_up_and_another_wallet_is_a_new_account() {
    // PROGRAM §3.7 "topped up by later ones"; sponsor_count counts distinct wallets.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = open_pool(&f);
    let accounts = pool_accounts(&f, &m, &f.expected_config(), &pool, rent_for(0), 1);
    let first = m.process_and_validate_instruction(
        &sponsor_ix(&f.sponsor, &pool, PRICE, TokenPath::default()),
        &accounts,
        &[Check::success()],
    );
    let second = m.process_and_validate_instruction(
        &sponsor_ix(&f.sponsor, &pool, 2 * PRICE, TokenPath::default()),
        &first.resulting_accounts,
        &[Check::success()],
    );
    let key = pool_key(&f);
    let s = decode_sponsorship(account_of(&second, &sponsorship_pda(&key, &f.sponsor).0));
    assert_eq!(s.amount, 3 * PRICE);
    let after = decode_pool(account_of(&second, &key));
    assert_eq!((after.sponsor_count, after.sponsored_total), (1, 3 * PRICE));
    assert_eq!(
        account_of(&second, &sponsorship_pda(&key, &f.sponsor).0).lamports,
        account_of(&first, &sponsorship_pda(&key, &f.sponsor).0).lamports,
        "no second rent"
    );

    let third = m.process_and_validate_instruction(
        &sponsor_ix(&f.buyer, &pool, PRICE, TokenPath::default()),
        &second.resulting_accounts,
        &[Check::success()],
    );
    let after = decode_pool(account_of(&third, &key));
    assert_eq!(
        (
            after.sponsor_count,
            after.sponsorships_open,
            after.sponsored_total
        ),
        (2, 2, 4 * PRICE)
    );
    assert_eq!(
        account_of(&third, &vault_pda(&key).0).lamports,
        rent_for(0) + 4 * PRICE
    );
}

#[test]
fn below_one_box_price_is_sponsorship_too_small() {
    // PROGRAM §4.3: amount ≥ price.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = open_pool(&f);
    let c = f.expected_config();
    expect_sponsor(
        &f,
        &m,
        &c,
        &pool,
        &f.sponsor,
        PRICE - 1,
        Err(err(E::SponsorshipTooSmall)),
    );
    expect_sponsor(
        &f,
        &m,
        &c,
        &pool,
        &f.sponsor,
        0,
        Err(err(E::SponsorshipTooSmall)),
    );
}

#[test]
fn the_per_token_cap_is_exact_and_read_from_the_config_now() {
    // PROGRAM §3.1 max_sponsorship (25 × max_price = 25 SOL); §4.3 sponsored_total + amount ≤ cap.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let cap = c.tokens[0].max_sponsorship;
    let mut pool = open_pool(&f);
    pool.sponsored_total = 10 * PRICE;
    pool.sponsor_count = 1;
    pool.sponsorships_open = 1;
    let room = cap - pool.sponsored_total;
    expect_sponsor(
        &f,
        &m,
        &c,
        &pool,
        &f.sponsor,
        room + 1,
        Err(err(E::SponsorshipCapExceeded)),
    );
    let ok = expect_sponsor(&f, &m, &c, &pool, &f.sponsor, room, Ok(()));
    assert_eq!(
        decode_pool(account_of(&ok, &pool_key(&f))).sponsored_total,
        cap
    );

    // Lower the cap in the config: pools already open see the new value.
    let mut lowered = c;
    lowered.tokens[0].max_sponsorship = 12 * PRICE;
    expect_sponsor(
        &f,
        &m,
        &lowered,
        &pool,
        &f.sponsor,
        3 * PRICE,
        Err(err(E::SponsorshipCapExceeded)),
    );
    expect_sponsor(&f, &m, &lowered, &pool, &f.sponsor, 2 * PRICE, Ok(()));
}

#[test]
fn sponsor_is_allowed_on_open_locked_and_drawn_and_refused_on_terminal_pools() {
    // PROGRAM §4.3, §9: Open, Locked, Drawn; Returned, Settled, Split are terminal.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    for status in [PoolStatus::Locked, PoolStatus::Drawn] {
        let pool = pool_with(&f, status, 25, &f.buyer_2);
        expect_sponsor(&f, &m, &c, &pool, &f.sponsor, PRICE, Ok(()));
    }
    for status in [PoolStatus::Returned, PoolStatus::Settled, PoolStatus::Split] {
        let pool = pool_with(&f, status, 25, &f.buyer_2);
        expect_sponsor(
            &f,
            &m,
            &c,
            &pool,
            &f.sponsor,
            PRICE,
            Err(err(E::PoolNotOpen)),
        );
    }
}

#[test]
fn sponsor_after_kickoff_on_a_suspended_game_or_while_paused_is_refused() {
    let f = Fixture::new();
    let pool = open_pool(&f);
    let c = f.expected_config();
    let m = mollusk_for_pools(SCHEDULED, SLOT_HASH);
    expect_sponsor(
        &f,
        &m,
        &c,
        &pool,
        &f.sponsor,
        PRICE,
        Err(err(E::SalesClosed)),
    );

    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut record = standard_record();
    record.status = mybarpool::GameStatus::Suspended;
    record.marked_at = T0 - 60;
    let mut accounts = pool_accounts(&f, &m, &c, &pool, rent_for(0), 1);
    set_account(&mut accounts, standard_game(), game_record_account(&record));
    m.process_and_validate_instruction(
        &sponsor_ix(&f.sponsor, &pool, PRICE, TokenPath::default()),
        &accounts,
        &[Check::err(custom(err(E::GameNotScheduled)))],
    );

    let mut paused = c;
    paused.paused = true;
    expect_sponsor(
        &f,
        &m,
        &paused,
        &pool,
        &f.sponsor,
        PRICE,
        Err(err(E::Paused)),
    );
}

#[test]
fn ore_sponsor_moves_tokens_by_transfer_checked() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = fresh_pool(&f, &f.expected_config(), &ore_params(0));
    let key = pool_key(&f);
    let sponsor_ata = Pubkey::new_unique();
    let mut accounts = pool_accounts(&f, &m, &f.expected_config(), &pool, 0, 1);
    set_account(
        &mut accounts,
        vault_pda(&key).0,
        token_account(&f.ore_mint, &key, 0),
    );
    accounts.push((
        sponsor_ata,
        token_account(&f.ore_mint, &f.sponsor, 10 * PRICE_ORE),
    ));
    let path = TokenPath {
        mint: Some(f.ore_mint),
        token_account: Some(sponsor_ata),
        token_program: Some(token_program_id()),
    };
    let result = m.process_and_validate_instruction(
        &sponsor_ix(&f.sponsor, &pool, PRICE_ORE, path),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &vault_pda(&key).0)),
        PRICE_ORE
    );
    assert_eq!(
        decode_token_amount(account_of(&result, &sponsor_ata)),
        9 * PRICE_ORE
    );
    let after = decode_pool(account_of(&result, &key));
    assert_eq!((after.sponsored_total, after.sponsor_count), (PRICE_ORE, 1));
    assert_eq!(event_names(&result), ["Sponsored"]);

    // Plumbing: Token-2022 program for the ORE pool; no mint.
    let r = m.process_instruction(
        &sponsor_ix(
            &f.sponsor,
            &pool,
            PRICE_ORE,
            TokenPath {
                token_program: Some(token_2022_program_id()),
                ..path
            },
        ),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::RequireKeysEqViolated))
    );
    let r = m.process_instruction(
        &sponsor_ix(
            &f.sponsor,
            &pool,
            PRICE_ORE,
            TokenPath { mint: None, ..path },
        ),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::ConstraintAccountIsNone))
    );
}

#[test]
fn the_creator_may_sponsor_their_own_pool_and_it_is_not_a_box() {
    // ARCHITECTURE › Sponsorship: the creator can sponsor; creator_boxes is untouched.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = pool_with(&f, PoolStatus::Open, 2, &f.creator);
    let accounts = pool_accounts(
        &f,
        &m,
        &f.expected_config(),
        &pool,
        rent_for(0) + 2 * PRICE,
        1,
    );
    let result = m.process_and_validate_instruction(
        &sponsor_ix(&f.creator, &pool, PRICE, TokenPath::default()),
        &accounts,
        &[Check::success()],
    );
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.creator_boxes, 2);
    assert_eq!(after.sold, 2);
    assert_eq!((after.sponsored_total, after.sponsor_count), (PRICE, 1));
}

#[test]
fn sponsor_with_the_pool_or_vault_at_a_wrong_address_fails_its_constraint() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let pool = open_pool(&f);
    let base = pool_accounts(&f, &m, &f.expected_config(), &pool, rent_for(0), 1);
    // Account order in Sponsor: sponsor 0, config 1, game 2, pool 3, vault 4, sponsorship 5, ...
    let other_vault = Pubkey::new_unique();
    let mut accounts = base.clone();
    accounts.push((other_vault, sol_vault_account(rent_for(0))));
    let mut ix = sponsor_ix(&f.sponsor, &pool, PRICE, TokenPath::default());
    ix.accounts[4].pubkey = other_vault;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(anchor(ErrorCode::ConstraintSeeds))
    );

    let other_game = game_pda(&standard_key(), SCHEDULED + 3_600).0;
    let mut accounts = base.clone();
    accounts.push((
        other_game,
        game_record_account(&fresh_record(standard_key(), SCHEDULED + 3_600)),
    ));
    let mut ix = sponsor_ix(&f.sponsor, &pool, PRICE, TokenPath::default());
    ix.accounts[2].pubkey = other_game;
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &accounts)),
        Some(anchor(ErrorCode::ConstraintHasOne))
    );
}

// ---------------------------------------------------------------------------
// rotate_gate_key
// ---------------------------------------------------------------------------

#[test]
fn rotate_gate_key_on_a_link_pool_by_the_creator_and_its_three_refusals() {
    // PROGRAM §4.3: access_type == Link, new_key != default; signer: creator.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut link = open_pool(&f);
    link.access_type = AccessType::Link;
    link.gate_key = to_a(&Pubkey::new_unique());
    let accounts = pool_accounts(&f, &m, &f.expected_config(), &link, rent_for(0), 1);
    let new_key = Pubkey::new_unique();
    let result = m.process_and_validate_instruction(
        &rotate_gate_key_ix(&f.creator, &link, &new_key),
        &accounts,
        &[Check::success()],
    );
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.gate_key, to_a(&new_key));
    assert_eq!(event_names(&result), ["GateKeyRotated"]);
    let event: GateKeyRotated = emitted_event(&result).expect("GateKeyRotated");
    assert_eq!((event.time, event.pool), (T0, to_a(&pool_key(&f))));

    m.process_and_validate_instruction(
        &rotate_gate_key_ix(&f.creator, &link, &Pubkey::default()),
        &accounts,
        &[Check::err(custom(err(E::GateKeyMissing)))],
    );
    m.process_and_validate_instruction(
        &rotate_gate_key_ix(&f.buyer, &link, &new_key),
        &accounts,
        &[Check::err(custom(err(E::Unauthorized)))],
    );
    let public = open_pool(&f);
    let accounts = pool_accounts(&f, &m, &f.expected_config(), &public, rent_for(0), 1);
    m.process_and_validate_instruction(
        &rotate_gate_key_ix(&f.creator, &public, &new_key),
        &accounts,
        &[Check::err(custom(err(E::InvalidAccessType)))],
    );
}

#[test]
fn rotate_gate_key_works_while_paused_and_on_a_locked_pool() {
    // PROGRAM §3.1: paused affects three instructions only; §4.3: status does not matter.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut link = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    link.access_type = AccessType::Link;
    link.gate_key = to_a(&Pubkey::new_unique());
    let mut paused = f.expected_config();
    paused.paused = true;
    let accounts = pool_accounts(&f, &m, &paused, &link, rent_for(0) + 25 * PRICE, 0);
    m.process_and_validate_instruction(
        &rotate_gate_key_ix(&f.creator, &link, &Pubkey::new_unique()),
        &accounts,
        &[Check::success()],
    );
}

// ---------------------------------------------------------------------------
// close_counter
// ---------------------------------------------------------------------------

#[test]
fn close_counter_at_zero_sends_rent_to_fee_wallet_without_a_signer() {
    // PROGRAM §3.5: closed when open_count == 0; rent to fee_wallet; permissionless.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let counter_key = counter_pda(&f.creator, &standard_game()).0;
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((
        counter_key,
        counter_account(&f.creator, &standard_game(), 1),
    ));
    accounts.push((f.fee_wallet, system_account(LAMPORTS_PER_SOL)));
    let ix = close_counter_ix(&f.creator, &standard_game(), &f.fee_wallet);
    assert!(
        ix.accounts.iter().all(|a| !a.is_signer),
        "no signer in the context"
    );
    m.process_and_validate_instruction(
        &ix,
        &accounts,
        &[Check::err(custom(err(E::CounterNotEmpty)))],
    );

    set_account(
        &mut accounts,
        counter_key,
        counter_account(&f.creator, &standard_game(), 0),
    );
    let rent = accounts
        .iter()
        .find(|(k, _)| *k == counter_key)
        .unwrap()
        .1
        .lamports;
    let fee_before = accounts
        .iter()
        .find(|(k, _)| *k == f.fee_wallet)
        .map(|(_, a)| a.lamports)
        .unwrap_or(0);
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
    let closed = account_of(&result, &counter_key);
    assert_eq!(closed.lamports, 0);
    assert!(closed.data.is_empty() || closed.data.iter().all(|&b| b == 0));
    assert_eq!(
        account_of(&result, &f.fee_wallet).lamports,
        fee_before + rent
    );
    assert_eq!(rent_for(mybarpool::CreatorCounter::SIZE), 1_405_920); // PROGRAM §3.5 at 74 bytes
    assert_eq!(emitted_event_count(&result), 0); // §7 has no counter event
}

#[test]
fn close_counter_to_a_stranger_is_constraint_has_one() {
    // PROGRAM §10: no instruction can move funds to an account other than the named ones.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let counter_key = counter_pda(&f.creator, &standard_game()).0;
    let mut accounts = base_accounts(&f, Some(&f.expected_config()));
    accounts.push((
        counter_key,
        counter_account(&f.creator, &standard_game(), 0),
    ));
    let r = m.process_instruction(
        &close_counter_ix(&f.creator, &standard_game(), &f.stranger),
        &accounts,
    );
    assert_eq!(custom_error(&r), Some(anchor(ErrorCode::ConstraintHasOne)));
}
