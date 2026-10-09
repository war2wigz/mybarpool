//! `create_pool`, PROGRAM §4.3, under Mollusk. Every spec number cites its
//! line. Run after `anchor build --arch v3`. The standard game's kickoff is
//! `SCHEDULED = T0 + 86 400`, so sales are open at `T0`.

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::Check;
use mybarpool::{
    AccessType, BoxesBought, CreatePoolParams, MybarpoolError as E, PayoutPreset, Pool,
    PoolCreated, PoolStatus,
};
use solana_pubkey::Pubkey;

fn custom(code: u32) -> solana_program_error::ProgramError {
    solana_program_error::ProgramError::Custom(code)
}

fn anchor(code: ErrorCode) -> u32 {
    code as u32
}

// ---------------------------------------------------------------------------
// The happy path
// ---------------------------------------------------------------------------

#[test]
fn create_pool_sol_writes_every_field_funds_the_vault_and_creates_the_counter() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let config = f.expected_config();
    let ix = create_pool_ix(
        &f.creator,
        &standard_game(),
        sol_params(0),
        TokenPath::default(),
    );
    let result =
        m.process_and_validate_instruction(&ix, &pool_base(&f, &m, &config), &[Check::success()]);

    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;
    let account = account_of(&result, &pool_key);
    assert_eq!(account.data.len(), 1442); // PROGRAM §3.3: 8 + 1,434
    let pool = decode_pool(account);
    let expected = fresh_pool(&f, &config, &sol_params(0));
    assert_eq!(pool, expected);
    // ARCHITECTURE › Fees worked example: 0.05 SOL boxes, 2 % add-on → 0.0625 / 0.0875 / 0.
    assert_eq!(pool.platform_fee, 62_500_000);
    assert_eq!(pool.creator_fee, 87_500_000);
    assert_eq!(pool.integrator_fee, 0);
    assert_eq!(pool.winning_box, [255; 4]); // PROGRAM §3.3: 255 until settled
    assert_eq!(pool.created_at, T0);
    assert_eq!(pool.status, PoolStatus::Open);
    assert_eq!(pool.vault, to_a(&vault_pda(&pool_key).0));

    let vault = account_of(&result, &vault_pda(&pool_key).0);
    assert_eq!(vault.lamports, rent_for(0)); // PROGRAM §3.4: its own rent-exempt minimum
    assert_eq!(vault.owner, Pubkey::default()); // system-owned
    assert!(vault.data.is_empty());

    let counter = decode_counter(account_of(
        &result,
        &counter_pda(&f.creator, &standard_game()).0,
    ));
    assert_eq!(counter.creator, to_a(&f.creator));
    assert_eq!(counter.game, to_a(&standard_game()));
    assert_eq!(counter.open_count, 1); // PROGRAM §3.5: incremented on create
    assert_eq!(counter.bump, counter_pda(&f.creator, &standard_game()).1);

    assert_eq!(event_names(&result), ["PoolCreated"]);
    let event: PoolCreated = emitted_event(&result).expect("PoolCreated");
    assert_eq!(event.time, T0);
    assert_eq!(event.pool, to_a(&pool_key));
    assert_eq!(event.creator, to_a(&f.creator));
    assert_eq!(
        (event.token, event.price, event.preset),
        (0, PRICE, PayoutPreset::Standard)
    );
    assert_eq!(event.access_type, AccessType::Public);
    assert_eq!(
        (event.platform_fee, event.creator_fee, event.integrator_fee),
        (62_500_000, 87_500_000, 0)
    );
}

#[test]
fn create_pool_with_initial_boxes_buys_them_for_the_creator_at_full_price() {
    // PROGRAM §4.3 "runs the buy logic"; ARCHITECTURE › Pool creation: full price into the pot.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let config = f.expected_config();
    let accounts = pool_base(&f, &m, &config);
    let creator_before = accounts
        .iter()
        .find(|(k, _)| *k == f.creator)
        .unwrap()
        .1
        .lamports;
    let ix = create_pool_ix(
        &f.creator,
        &standard_game(),
        sol_params(2),
        TokenPath::default(),
    );
    let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);

    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;
    let pool = decode_pool(account_of(&result, &pool_key));
    assert_eq!(pool.sold, 2);
    assert_eq!(pool.creator_boxes, 2);
    assert_eq!(
        pool.owners
            .iter()
            .filter(|o| **o == to_a(&f.creator))
            .count(),
        2
    );
    assert_eq!(pool.status, PoolStatus::Open);
    let vault = account_of(&result, &vault_pda(&pool_key).0);
    assert_eq!(vault.lamports, rent_for(0) + 2 * PRICE);
    let rent_paid = rent_for(Pool::SIZE) + rent_for(0) + rent_for(mybarpool::CreatorCounter::SIZE);
    assert_eq!(
        account_of(&result, &f.creator).lamports,
        creator_before - rent_paid - 2 * PRICE
    );

    assert_eq!(event_names(&result), ["PoolCreated", "BoxesBought"]);
    let bought: BoxesBought = emitted_event(&result).expect("BoxesBought");
    assert_eq!(bought.buyer, to_a(&f.creator));
    assert_eq!((bought.count, bought.sold_after), (2, 2));
    assert_eq!(bought.boxes.len(), 2);
    for b in &bought.boxes {
        assert_eq!(pool.owners[usize::from(*b)], to_a(&f.creator));
    }
}

#[test]
fn create_pool_with_an_integrator_stores_all_three_fees() {
    // PROGRAM §5.1: 1.25 SOL pot, 500 / (500 + 200) / 300 bps.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let params = CreatePoolParams {
        integrator: to_a(&f.integrator),
        integrator_bps: 300,
        ..sol_params(0)
    };
    let ix = create_pool_ix(&f.creator, &standard_game(), params, TokenPath::default());
    let result = m.process_and_validate_instruction(
        &ix,
        &pool_base(&f, &m, &f.expected_config()),
        &[Check::success()],
    );
    let pool = decode_pool(account_of(
        &result,
        &pool_pda(&standard_game(), &f.creator, NONCE).0,
    ));
    assert_eq!(
        (pool.platform_fee, pool.creator_fee, pool.integrator_fee),
        (62_500_000, 87_500_000, 37_500_000)
    );
    assert_eq!(pool.integrator, to_a(&f.integrator));
    let event: PoolCreated = emitted_event(&result).expect("PoolCreated");
    assert_eq!(
        (event.integrator, event.integrator_bps, event.integrator_fee),
        (to_a(&f.integrator), 300, 37_500_000)
    );
}

#[test]
fn create_pool_twice_with_the_same_nonce_fails_and_a_new_nonce_is_a_new_pool() {
    // PROGRAM §3.3: the PDA must not already exist; one creator, several pools, one counter.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let config = f.expected_config();
    let ix = create_pool_ix(
        &f.creator,
        &standard_game(),
        sol_params(0),
        TokenPath::default(),
    );
    let first =
        m.process_and_validate_instruction(&ix, &pool_base(&f, &m, &config), &[Check::success()]);
    let second = m.process_instruction(&ix, &first.resulting_accounts);
    assert!(!second.program_result.is_ok(), "account already exists");

    let params = CreatePoolParams {
        nonce: NONCE + 1,
        ..sol_params(0)
    };
    let other = pool_pda(&standard_game(), &f.creator, NONCE + 1).0;
    let mut accounts = first.resulting_accounts.clone();
    accounts.push((other, system_account(0)));
    accounts.push((vault_pda(&other).0, system_account(0)));
    let result = m.process_and_validate_instruction(
        &create_pool_ix(&f.creator, &standard_game(), params, TokenPath::default()),
        &accounts,
        &[Check::success()],
    );
    assert_ne!(other, pool_pda(&standard_game(), &f.creator, NONCE).0);
    let counter = decode_counter(account_of(
        &result,
        &counter_pda(&f.creator, &standard_game()).0,
    ));
    assert_eq!(counter.open_count, 2);
}

// ---------------------------------------------------------------------------
// Negatives, one clause each
// ---------------------------------------------------------------------------

fn expect_create(
    f: &Fixture,
    m: &mollusk_svm::Mollusk,
    config: &mybarpool::PlatformConfig,
    params: CreatePoolParams,
    expect: Result<(), u32>,
) -> mollusk_svm::result::InstructionResult {
    let ix = create_pool_ix(&f.creator, &standard_game(), params, TokenPath::default());
    let check = match expect {
        Ok(()) => Check::success(),
        Err(code) => Check::err(custom(code)),
    };
    m.process_and_validate_instruction(&ix, &pool_base(f, m, config), &[check])
}

#[test]
fn price_must_be_on_the_ladder() {
    // ARCHITECTURE › Buying table: SOL 0.05 / 0.05 / 1.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let rule = c.tokens[0];
    for price in [
        rule.min_price - 1,
        rule.min_price + rule.step / 2,
        rule.max_price + rule.step,
    ] {
        expect_create(
            &f,
            &m,
            &c,
            CreatePoolParams {
                price,
                ..sol_params(0)
            },
            Err(err(E::PriceOffLadder)),
        );
    }
    expect_create(
        &f,
        &m,
        &c,
        CreatePoolParams {
            price: rule.max_price,
            ..sol_params(0)
        },
        Ok(()),
    );
}

#[test]
fn a_disabled_or_unknown_token_is_token_disabled() {
    // PROGRAM §4.3 tokens[token].enabled; token ≥ 3 has no rule.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    for token in [1u8, 3, 255] {
        expect_create(
            &f,
            &m,
            &c,
            CreatePoolParams {
                token,
                ..sol_params(0)
            },
            Err(err(E::TokenDisabled)),
        );
    }
}

#[test]
fn an_unknown_preset_or_access_type_byte_does_not_deserialise() {
    // PayoutPreset and AccessType are Anchor enums: a byte outside 0–2 is Anchor 102.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let accounts = pool_base(&f, &m, &f.expected_config());
    let base = create_pool_ix(
        &f.creator,
        &standard_game(),
        sol_params(0),
        TokenPath::default(),
    );
    // CreatePoolParams after the 8-byte discriminator: nonce 8, token 1, price 8, preset 1, access_type 1, ...
    for offset in [8 + 8 + 1 + 8, 8 + 8 + 1 + 8 + 1] {
        let mut ix = base.clone();
        ix.data[offset] = 3;
        m.process_and_validate_instruction(&ix, &accounts, &[Check::err(custom(102))]);
    }
}

#[test]
fn gate_key_and_allowlist_root_must_match_the_access_type() {
    // PROGRAM §4.3: gate_key != default iff Link; allowlist_root != 0 iff Allowlist
    // (InvalidAccessType otherwise). Since Step 8 the two consistent gated shapes are created.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let key = to_a(&Pubkey::new_from_array([0x61; 32]));
    let refused = [
        CreatePoolParams {
            gate_key: key,
            ..sol_params(0)
        },
        CreatePoolParams {
            allowlist_root: [1u8; 32],
            ..sol_params(0)
        },
        CreatePoolParams {
            access_type: AccessType::Link,
            ..sol_params(0)
        },
        CreatePoolParams {
            access_type: AccessType::Allowlist,
            ..sol_params(0)
        },
    ];
    for params in refused {
        expect_create(&f, &m, &c, params, Err(err(E::InvalidAccessType)));
    }
    let created = [
        CreatePoolParams {
            access_type: AccessType::Link,
            gate_key: key,
            ..sol_params(0)
        },
        CreatePoolParams {
            access_type: AccessType::Allowlist,
            allowlist_root: [1u8; 32],
            ..sol_params(0)
        },
    ];
    for params in created {
        expect_create(&f, &m, &c, params, Ok(()));
    }
}

#[test]
fn create_pool_link_and_allowlist_are_created_with_their_fields() {
    // PROGRAM §3.3: gate_key is the co-signer when Link, else default; allowlist_root the
    // root when Allowlist, else zero. §7 PoolCreated.access_type carries 1 / 2.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let key = to_a(&gate_key_g());
    let root = tree::root(&allowlist_wallets(8));
    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;

    let r = expect_create(
        &f,
        &m,
        &c,
        CreatePoolParams {
            access_type: AccessType::Link,
            gate_key: key,
            ..sol_params(0)
        },
        Ok(()),
    );
    let pool = decode_pool(account_of(&r, &pool_key));
    assert_eq!(pool.access_type, AccessType::Link);
    assert_eq!(pool.gate_key, key);
    assert_eq!(pool.allowlist_root, [0u8; 32]);
    let e: PoolCreated = emitted_event(&r).unwrap();
    assert_eq!(e.access_type, AccessType::Link);
    assert_eq!(AccessType::Link as u8, 1);

    let r = expect_create(
        &f,
        &m,
        &c,
        CreatePoolParams {
            access_type: AccessType::Allowlist,
            allowlist_root: root,
            ..sol_params(0)
        },
        Ok(()),
    );
    let pool = decode_pool(account_of(&r, &pool_key));
    assert_eq!(pool.access_type, AccessType::Allowlist);
    assert_eq!(pool.gate_key, anchor_lang::prelude::Pubkey::default());
    assert_eq!(pool.allowlist_root, root);
    let e: PoolCreated = emitted_event(&r).unwrap();
    assert_eq!(e.access_type, AccessType::Allowlist);
    assert_eq!(AccessType::Allowlist as u8, 2);
}

#[test]
fn create_pool_initial_boxes_on_an_allowlist_pool_are_not_gated() {
    // PROGRAM §4.3: "runs the buy logic ... without the gating step: the creator is setting the
    // gate in this very instruction, and need not be on their own allowlist."
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let wallets = allowlist_wallets(8); // the creator is not among them
    assert!(!wallets.contains(&f.creator));
    let r = expect_create(
        &f,
        &m,
        &c,
        CreatePoolParams {
            access_type: AccessType::Allowlist,
            allowlist_root: tree::root(&wallets),
            ..sol_params(3)
        },
        Ok(()),
    );
    let pool = decode_pool(account_of(
        &r,
        &pool_pda(&standard_game(), &f.creator, NONCE).0,
    ));
    assert_eq!((pool.sold, pool.creator_boxes), (3, 3));
    assert_eq!(event_names(&r), ["PoolCreated", "BoxesBought"]);
}

#[test]
fn addon_budget_is_the_configs_not_the_constant() {
    // PROGRAM §4.3: creator_addon_bps + integrator_bps ≤ config.addon_budget_bps (§1 max 500).
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let with_integrator = |addon: u16, integ: u16| CreatePoolParams {
        creator_addon_bps: addon,
        integrator: to_a(&f.integrator),
        integrator_bps: integ,
        ..sol_params(0)
    };
    expect_create(
        &f,
        &m,
        &c,
        with_integrator(300, 201),
        Err(err(E::AddonBudgetExceeded)),
    );
    expect_create(&f, &m, &c, with_integrator(300, 200), Ok(()));
    let mut lower = c;
    lower.addon_budget_bps = 300;
    expect_create(
        &f,
        &m,
        &lower,
        CreatePoolParams {
            creator_addon_bps: 400,
            ..sol_params(0)
        },
        Err(err(E::AddonBudgetExceeded)),
    );
}

#[test]
fn integrator_and_integrator_bps_must_agree() {
    // PROGRAM §4.3: integrator_bps == 0 iff integrator == default.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    expect_create(
        &f,
        &m,
        &c,
        CreatePoolParams {
            integrator_bps: 100,
            ..sol_params(0)
        },
        Err(err(E::IntegratorMismatch)),
    );
    expect_create(
        &f,
        &m,
        &c,
        CreatePoolParams {
            integrator: to_a(&f.integrator),
            ..sol_params(0)
        },
        Err(err(E::IntegratorMismatch)),
    );
}

#[test]
fn initial_boxes_is_capped_by_max_own_boxes_with_override_precedence() {
    // ARCHITECTURE › Limits: 5 own boxes; PROGRAM §3.6: the override replaces the config value.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    expect_create(&f, &m, &c, sol_params(6), Err(err(E::OwnBoxLimit)));
    let ok = expect_create(&f, &m, &c, sol_params(5), Ok(()));
    assert_eq!(
        decode_pool(account_of(
            &ok,
            &pool_pda(&standard_game(), &f.creator, NONCE).0
        ))
        .creator_boxes,
        5
    );

    let mut raised = pool_base(&f, &m, &c);
    set_account(
        &mut raised,
        override_pda(&f.creator).0,
        override_account(&f.creator, 3, 10),
    );
    m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            sol_params(6),
            TokenPath::default(),
        ),
        &raised,
        &[Check::success()],
    );
    let mut lowered = pool_base(&f, &m, &c);
    set_account(
        &mut lowered,
        override_pda(&f.creator).0,
        override_account(&f.creator, 3, 2),
    );
    m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            sol_params(3),
            TokenPath::default(),
        ),
        &lowered,
        &[Check::err(custom(err(E::OwnBoxLimit)))],
    );
}

#[test]
fn initial_boxes_25_under_an_override_locks_the_pool_in_the_creating_transaction() {
    // PROGRAM §4.3 effects: counter +1 then −1; events PoolCreated, BoxesBought, PoolLocked.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let mut accounts = pool_base(&f, &m, &c);
    set_account(
        &mut accounts,
        override_pda(&f.creator).0,
        override_account(&f.creator, 25, 25),
    );
    set_account(
        &mut accounts,
        counter_pda(&f.creator, &standard_game()).0,
        counter_account(&f.creator, &standard_game(), 2),
    );
    let result = m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            sol_params(25),
            TokenPath::default(),
        ),
        &accounts,
        &[Check::success()],
    );
    let pool = decode_pool(account_of(
        &result,
        &pool_pda(&standard_game(), &f.creator, NONCE).0,
    ));
    assert_eq!(pool.status, PoolStatus::Locked);
    assert_eq!(pool.locked_at, T0);
    assert_eq!(pool.sold, 25);
    assert_eq!(pool.creator_boxes, 25);
    assert!(pool.owners.iter().all(|o| *o == to_a(&f.creator)));
    let counter = decode_counter(account_of(
        &result,
        &counter_pda(&f.creator, &standard_game()).0,
    ));
    assert_eq!(counter.open_count, 2); // unchanged: +1 −1
    assert_eq!(
        event_names(&result),
        ["PoolCreated", "BoxesBought", "PoolLocked"]
    );
}

#[test]
fn open_pool_limit_with_override_precedence() {
    // ARCHITECTURE › Limits: 3 open pools; PROGRAM §3.5 / §3.6.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let counter_key = counter_pda(&f.creator, &standard_game()).0;
    let ix = create_pool_ix(
        &f.creator,
        &standard_game(),
        sol_params(0),
        TokenPath::default(),
    );

    let mut at_limit = pool_base(&f, &m, &c);
    set_account(
        &mut at_limit,
        counter_key,
        counter_account(&f.creator, &standard_game(), 3),
    );
    m.process_and_validate_instruction(
        &ix,
        &at_limit,
        &[Check::err(custom(err(E::OpenPoolLimit)))],
    );

    let mut raised = at_limit.clone();
    set_account(
        &mut raised,
        override_pda(&f.creator).0,
        override_account(&f.creator, 4, 5),
    );
    let result = m.process_and_validate_instruction(&ix, &raised, &[Check::success()]);
    assert_eq!(
        decode_counter(account_of(&result, &counter_key)).open_count,
        4
    );

    let mut lowered = pool_base(&f, &m, &c);
    set_account(
        &mut lowered,
        counter_key,
        counter_account(&f.creator, &standard_game(), 2),
    );
    set_account(
        &mut lowered,
        override_pda(&f.creator).0,
        override_account(&f.creator, 2, 5),
    );
    m.process_and_validate_instruction(&ix, &lowered, &[Check::err(custom(err(E::OpenPoolLimit)))]);
}

#[test]
fn paused_blocks_create_pool() {
    // PROGRAM §3.1: paused stops create_pool, buy and sponsor.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut c = f.expected_config();
    c.paused = true;
    expect_create(&f, &m, &c, sol_params(0), Err(err(E::Paused)));
}

#[test]
fn a_game_that_is_not_scheduled_is_game_not_scheduled() {
    // PROGRAM §4.3 game.status == Scheduled (the Step 3 hook).
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    for status in [
        mybarpool::GameStatus::Postponed,
        mybarpool::GameStatus::Cancelled,
        mybarpool::GameStatus::Suspended,
    ] {
        let mut record = standard_record();
        record.status = status;
        record.marked_at = T0 - 60;
        let mut accounts = pool_base(&f, &m, &c);
        set_account(&mut accounts, standard_game(), game_record_account(&record));
        m.process_and_validate_instruction(
            &create_pool_ix(
                &f.creator,
                &standard_game(),
                sol_params(0),
                TokenPath::default(),
            ),
            &accounts,
            &[Check::err(custom(err(E::GameNotScheduled)))],
        );
    }
    let mut accounts = pool_base(&f, &m, &c);
    set_account(
        &mut accounts,
        standard_game(),
        game_record_account(&record_with_quarters(4)),
    );
    m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            sol_params(0),
            TokenPath::default(),
        ),
        &accounts,
        &[Check::err(custom(err(E::GameNotScheduled)))],
    );
}

#[test]
fn sales_close_at_the_recorded_kickoff() {
    // PROGRAM §4.3 now < recorded_kickoff.
    let f = Fixture::new();
    let c = f.expected_config();
    let m = mollusk_for_pools(SCHEDULED, SLOT_HASH);
    expect_create(&f, &m, &c, sol_params(0), Err(err(E::SalesClosed)));
    let m = mollusk_for_pools(SCHEDULED - 1, SLOT_HASH);
    expect_create(&f, &m, &c, sol_params(0), Ok(()));
}

#[test]
fn a_game_record_at_a_foreign_address_fails_the_seeds_check() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let planted = Pubkey::new_unique();
    let mut accounts = pool_base(&f, &m, &f.expected_config());
    accounts.push((planted, game_record_account(&standard_record())));
    let pool = pool_pda(&planted, &f.creator, NONCE).0;
    accounts.push((pool, system_account(0)));
    accounts.push((vault_pda(&pool).0, system_account(0)));
    accounts.push((counter_pda(&f.creator, &planted).0, system_account(0)));
    let ix = create_pool_ix(&f.creator, &planted, sol_params(0), TokenPath::default());
    let result = m.process_instruction(&ix, &accounts);
    assert_eq!(
        custom_error(&result),
        Some(anchor(ErrorCode::ConstraintSeeds))
    );
}

#[test]
fn pre_funded_pdas_do_not_block_creation() {
    // Audit focus "counter can't be griefed": a system account with lamports at the pool,
    // counter or sponsorship address is topped up and assigned by Anchor's init.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;
    let mut accounts = pool_base(&f, &m, &c);
    set_account(&mut accounts, pool_key, system_account(1_000_000));
    set_account(
        &mut accounts,
        counter_pda(&f.creator, &standard_game()).0,
        system_account(1_000_000),
    );
    let result = m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            sol_params(0),
            TokenPath::default(),
        ),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        decode_pool(account_of(&result, &pool_key)).status,
        PoolStatus::Open
    );
    assert_eq!(
        decode_counter(account_of(
            &result,
            &counter_pda(&f.creator, &standard_game()).0
        ))
        .open_count,
        1
    );

    // And the sponsorship slot, through sponsor.
    let pool = decode_pool(account_of(&result, &pool_key));
    let mut accounts = result.resulting_accounts.clone();
    accounts.push((
        sponsorship_pda(&pool_key, &f.sponsor).0,
        system_account(1_000_000),
    ));
    m.process_and_validate_instruction(
        &sponsor_ix(&f.sponsor, &pool, PRICE, TokenPath::default()),
        &accounts,
        &[Check::success()],
    );

    // Step 4 audit M1: an SPL vault PDA holding lamports beforehand. create_account would
    // refuse it; the handler tops up, allocates and assigns instead, and the vault ends as a
    // 165-byte token account owned by the Token program with the creator's first box in it.
    let creator_ata = Pubkey::new_unique();
    let mut accounts = pool_base(&f, &m, &c);
    set_account(
        &mut accounts,
        vault_pda(&pool_key).0,
        system_account(1_000_000),
    );
    accounts.push((
        creator_ata,
        token_account(&f.ore_mint, &f.creator, 10 * PRICE_ORE),
    ));
    let result = m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            ore_params(1),
            ore_path(&f, Some(creator_ata)),
        ),
        &accounts,
        &[Check::success()],
    );
    let vault = account_of(&result, &vault_pda(&pool_key).0);
    assert_eq!(vault.data.len(), 165);
    assert_eq!(vault.owner, token_program_id());
    assert_eq!(decode_token_owner(vault), pool_key);
    assert_eq!(decode_token_amount(vault), PRICE_ORE);
    assert!(vault.lamports >= rent_for(165));
    // Pre-funded above the rent minimum: nothing is taken from the creator for the vault's rent.
    let mut accounts = pool_base(&f, &m, &c);
    set_account(
        &mut accounts,
        vault_pda(&pool_key).0,
        system_account(5_000_000),
    );
    let result = m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            ore_params(0),
            ore_path(&f, None),
        ),
        &accounts,
        &[Check::success()],
    );
    let vault = account_of(&result, &vault_pda(&pool_key).0);
    assert_eq!(vault.lamports, 5_000_000);
    assert_eq!(vault.owner, token_program_id());
}

#[test]
fn a_strangers_create_pool_cannot_use_the_creators_counter() {
    // PROGRAM §3.5: the counter is per creator; its seeds include the creator.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let mut ix = create_pool_ix(
        &f.stranger,
        &standard_game(),
        sol_params(0),
        TokenPath::default(),
    );
    ix.accounts[5].pubkey = counter_pda(&f.creator, &standard_game()).0; // the creator's counter
    let mut accounts = pool_base(&f, &m, &f.expected_config());
    let pool = pool_pda(&standard_game(), &f.stranger, NONCE).0;
    accounts.push((pool, system_account(0)));
    accounts.push((vault_pda(&pool).0, system_account(0)));
    accounts.push((override_pda(&f.stranger).0, system_account(0)));
    let result = m.process_instruction(&ix, &accounts);
    assert_eq!(
        custom_error(&result),
        Some(anchor(ErrorCode::ConstraintSeeds))
    );
}

// ---------------------------------------------------------------------------
// SPL pools (PROGRAM §3.4, §5.4)
// ---------------------------------------------------------------------------

fn ore_path(f: &Fixture, token_account: Option<Pubkey>) -> TokenPath {
    TokenPath {
        mint: Some(f.ore_mint),
        token_account,
        token_program: Some(token_program_id()),
    }
}

fn skr_path(f: &Fixture, token_account: Option<Pubkey>) -> TokenPath {
    TokenPath {
        mint: Some(f.skr_mint),
        token_account,
        token_program: Some(token_2022_program_id()),
    }
}

#[test]
fn create_pool_ore_makes_a_token_vault_owned_by_the_pool_under_the_token_program() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.expected_config();
    let creator_ata = Pubkey::new_unique();
    let mut accounts = pool_base(&f, &m, &c);
    accounts.push((
        creator_ata,
        token_account(&f.ore_mint, &f.creator, 10 * PRICE_ORE),
    ));
    let result = m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            ore_params(1),
            ore_path(&f, Some(creator_ata)),
        ),
        &accounts,
        &[Check::success()],
    );
    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;
    let pool = decode_pool(account_of(&result, &pool_key));
    assert_eq!(pool.mint, to_a(&f.ore_mint));
    assert_eq!(pool.token_program, anchor_spl::token::ID);
    assert_eq!(pool.sold, 1);
    let vault = account_of(&result, &vault_pda(&pool_key).0);
    assert_eq!(vault.data.len(), 165); // PROGRAM §3.4: a token account
    assert_eq!(vault.owner, token_program_id());
    assert_eq!(decode_token_owner(vault), pool_key); // authority = pool PDA
    assert_eq!(decode_token_amount(vault), PRICE_ORE);
    assert_eq!(
        decode_token_amount(account_of(&result, &creator_ata)),
        9 * PRICE_ORE
    );
    // The transfer_checked CPI sits beside the two events and is not counted as one (audit L2).
    assert_eq!(event_names(&result), ["PoolCreated", "BoxesBought"]);
}

#[test]
fn create_pool_skr_under_token_2022() {
    // Audit focus: Token vs Token-2022, decided by the rule.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.config_with_skr();
    let creator_ata = Pubkey::new_unique();
    let mut accounts = pool_base(&f, &m, &c);
    accounts.push((
        creator_ata,
        token_2022_account(&f.skr_mint, &f.creator, 10 * PRICE_SKR),
    ));
    let result = m.process_and_validate_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            skr_params(1),
            skr_path(&f, Some(creator_ata)),
        ),
        &accounts,
        &[Check::success()],
    );
    let pool_key = pool_pda(&standard_game(), &f.creator, NONCE).0;
    let pool = decode_pool(account_of(&result, &pool_key));
    assert_eq!(pool.token_program, anchor_spl::token_2022::ID);
    let vault = account_of(&result, &vault_pda(&pool_key).0);
    assert_eq!(vault.owner, token_2022_program_id());
    assert_eq!(decode_token_amount(vault), PRICE_SKR);
}

#[test]
fn the_stored_token_program_wins_over_the_one_passed() {
    // PROGRAM §5.4, §10: ORE with Token-2022 and SKR with Token are refused.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let c = f.config_with_skr();
    let accounts = pool_base(&f, &m, &c);
    let ore_wrong = TokenPath {
        token_program: Some(token_2022_program_id()),
        ..ore_path(&f, None)
    };
    let r = m.process_instruction(
        &create_pool_ix(&f.creator, &standard_game(), ore_params(0), ore_wrong),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::RequireKeysEqViolated))
    );
    let skr_wrong = TokenPath {
        token_program: Some(token_program_id()),
        ..skr_path(&f, None)
    };
    let r = m.process_instruction(
        &create_pool_ix(&f.creator, &standard_game(), skr_params(0), skr_wrong),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::RequireKeysEqViolated))
    );
    // A third program is refused by the Interface type itself (Anchor's InvalidProgramId).
    let other = Pubkey::new_unique();
    let third = TokenPath {
        token_program: Some(other),
        ..ore_path(&f, None)
    };
    let mut with_other = accounts.clone();
    with_other.push((other, system_account(0)));
    let r = m.process_instruction(
        &create_pool_ix(&f.creator, &standard_game(), ore_params(0), third),
        &with_other,
    );
    assert!(!r.program_result.is_ok());
}

#[test]
fn spl_pool_without_its_accounts_is_constraint_account_is_none() {
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let accounts = pool_base(&f, &m, &f.expected_config());
    let no_mint = TokenPath {
        mint: None,
        ..ore_path(&f, None)
    };
    let r = m.process_instruction(
        &create_pool_ix(&f.creator, &standard_game(), ore_params(0), no_mint),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::ConstraintAccountIsNone))
    );
    let r = m.process_instruction(
        &create_pool_ix(
            &f.creator,
            &standard_game(),
            ore_params(1),
            ore_path(&f, None),
        ),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::ConstraintAccountIsNone))
    );
}

#[test]
fn ore_pool_with_the_skr_mint_is_refused() {
    // PROGRAM §4.3: mint.key() == rule.mint.
    let f = Fixture::new();
    let m = mollusk_for_pools(T0, SLOT_HASH);
    let accounts = pool_base(&f, &m, &f.config_with_skr());
    let wrong_mint = TokenPath {
        mint: Some(f.skr_mint),
        ..ore_path(&f, None)
    };
    let r = m.process_instruction(
        &create_pool_ix(&f.creator, &standard_game(), ore_params(0), wrong_mint),
        &accounts,
    );
    assert_eq!(
        custom_error(&r),
        Some(anchor(ErrorCode::RequireKeysEqViolated))
    );
}
