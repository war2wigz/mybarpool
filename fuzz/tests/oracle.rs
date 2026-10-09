//! Harness correctness (build plan Step 8 › Fuzzer): three planted bugs prove the oracle bites,
//! each against synthetic contexts and outcomes (no program change, no Mollusk).
//!
//! 1. A success that moves lamports to a stranger fails O3 (destinations).
//! 2. A success that leaves the vault one lamport short fails O4 (the §3.4 discrepancy moved).
//! 3. A failure that changed an account fails O2 (atomic).
//!
//! Each planted bug's healthy twin passes, so the tests pin the oracle's edge, not just its
//! presence.

use anchor_lang::prelude::Pubkey;
use anchor_lang::{AccountSerialize, Discriminator};
use mybarpool::{AccessType, PayoutPreset, Pool, PoolStatus};
use mybarpool_fuzz::model::{Account, Context, Key, Meta, Outcome, Result as Res};
use mybarpool_fuzz::mutate::SYSTEM_PROGRAM;
use mybarpool_fuzz::oracle::{check, vault_rent_floor};

const PRICE: u64 = 50_000_000; // ARCHITECTURE › Buying: 0.05 SOL
const POOL: Key = [0xA0; 32];
const VAULT: Key = [0xA1; 32];
const CREATOR: Key = [0xC0; 32];
const BUYER: Key = [0xB0; 32];
const STRANGER: Key = [0x55; 32];

/// An `Open` SOL pool with two boxes sold to the buyer and the fees of the worked example.
fn pool(status: PoolStatus, sold: u8) -> Pool {
    let mut owners = [Pubkey::default(); 25];
    for o in owners.iter_mut().take(usize::from(sold)) {
        *o = Pubkey::new_from_array(BUYER);
    }
    Pool {
        game: Pubkey::new_from_array([0x61; 32]),
        creator: Pubkey::new_from_array(CREATOR),
        nonce: 1,
        token: 0,
        mint: Pubkey::default(),
        token_program: Pubkey::default(),
        vault: Pubkey::new_from_array(VAULT),
        price: PRICE,
        preset: PayoutPreset::Standard,
        access_type: AccessType::Public,
        gate_key: Pubkey::default(),
        allowlist_root: [0; 32],
        creator_addon_bps: 200,
        integrator: Pubkey::default(),
        integrator_bps: 0,
        platform_fee: 62_500_000,
        creator_fee: 87_500_000,
        integrator_fee: 0,
        status,
        sold,
        owners,
        creator_boxes: 0,
        sponsored_total: 0,
        sponsor_count: 0,
        sponsorships_open: 0,
        var: Pubkey::default(),
        var_end_at: 0,
        sampled_slot: 0,
        sampled_hash: [0; 32],
        var_replacements: 0,
        drawn: false,
        home_axis: [0; 10],
        away_axis: [0; 10],
        prize_pool: 0,
        quarter_prize: [0; 4],
        quarters_settled: 0,
        winning_box: [255; 4],
        fees_paid: false,
        unpaid_prize_pool: 0,
        returned: 0,
        split_amount: 0,
        cancelled_by_admin: false,
        abandoned: false,
        created_at: 1_800_000_000,
        locked_at: 0,
        bump: 254,
        vault_bump: 253,
        var_commit: [0; 32],
        reserved: [0; 96],
    }
}

fn pool_account(p: &Pool) -> Account {
    let mut data = Vec::with_capacity(Pool::SIZE);
    p.try_serialize(&mut data).expect("serialise");
    data.resize(Pool::SIZE, 0);
    Account {
        key: POOL,
        lamports: 11_000_000,
        data,
        owner: mybarpool::ID.to_bytes(),
        executable: false,
    }
}

fn system(key: Key, lamports: u64) -> Account {
    Account {
        key,
        lamports,
        data: Vec::new(),
        owner: SYSTEM_PROGRAM,
        executable: false,
    }
}

/// A `return_boxes`-shaped context: the keeper, the pool, its vault, the buyer and a stranger.
fn context(accounts: Vec<Account>) -> Context {
    Context {
        program_id: mybarpool::ID.to_bytes(),
        metas: accounts
            .iter()
            .enumerate()
            .map(|(i, a)| Meta {
                key: a.key,
                is_signer: i == 0,
                is_writable: true,
            })
            .collect(),
        data: <mybarpool::instruction::ReturnBoxes as Discriminator>::DISCRIMINATOR.to_vec(),
        accounts,
        unix_timestamp: 1_800_000_000,
        compute_unit_limit: 200_000,
    }
}

fn lines(before: &Context, after: &Outcome) -> Vec<&'static str> {
    check(before, after).into_iter().map(|v| v.line).collect()
}

#[test]
fn a_success_that_pays_a_stranger_fails_o3() {
    let p = pool(PoolStatus::Returned, 2);
    let keeper = system([0x11; 32], 1_000_000_000);
    let before = context(vec![
        keeper.clone(),
        pool_account(&p),
        system(VAULT, vault_rent_floor() + 2 * PRICE),
        system(BUYER, 10),
        system(STRANGER, 10),
    ]);

    // Healthy: the vault pays the buyer (a box owner) one box's price; the pool records it.
    let mut paid = pool(PoolStatus::Returned, 2);
    paid.returned = 0b01;
    let healthy = Outcome {
        result: Res::Success,
        accounts: vec![
            keeper.clone(),
            pool_account(&paid),
            system(VAULT, vault_rent_floor() + PRICE),
            system(BUYER, 10 + PRICE),
            system(STRANGER, 10),
        ],
        compute_units: 1,
    };
    assert!(
        check(&before, &healthy).is_empty(),
        "{:?}",
        check(&before, &healthy)
    );

    // Planted: the same lamports go to the stranger.
    let planted = Outcome {
        result: Res::Success,
        accounts: vec![
            keeper,
            pool_account(&paid),
            system(VAULT, vault_rent_floor() + PRICE),
            system(BUYER, 10),
            system(STRANGER, 10 + PRICE),
        ],
        compute_units: 1,
    };
    let found = lines(&before, &planted);
    assert!(found.contains(&"O3 destinations"), "{found:?}");
    assert!(!found.contains(&"O3 lamports conserved"), "{found:?}");
}

#[test]
fn a_success_that_leaves_the_vault_one_lamport_short_fails_o4() {
    let p = pool(PoolStatus::Open, 2);
    let keeper = system([0x11; 32], 1_000_000_000);
    let before = context(vec![
        keeper.clone(),
        pool_account(&p),
        system(VAULT, vault_rent_floor() + 2 * PRICE),
        system(CREATOR, 10),
    ]);

    // Healthy: nothing moved, the invariant holds exactly as before.
    let healthy = Outcome {
        result: Res::Success,
        accounts: before.accounts.clone(),
        compute_units: 1,
    };
    assert!(check(&before, &healthy).is_empty());

    // Planted: one lamport leaks from the vault to the creator (a permitted destination, so
    // O3 is silent; conservation holds) while the pool's state says nothing was paid.
    let planted = Outcome {
        result: Res::Success,
        accounts: vec![
            keeper,
            pool_account(&p),
            system(VAULT, vault_rent_floor() + 2 * PRICE - 1),
            system(CREATOR, 11),
        ],
        compute_units: 1,
    };
    let found = lines(&before, &planted);
    assert_eq!(found, vec!["O4 vault invariant"]);
}

#[test]
fn a_failure_that_changed_an_account_fails_o2() {
    let p = pool(PoolStatus::Open, 2);
    let keeper = system([0x11; 32], 1_000_000_000);
    let before = context(vec![
        keeper.clone(),
        pool_account(&p),
        system(VAULT, vault_rent_floor() + 2 * PRICE),
        system(BUYER, 10),
    ]);

    // Healthy: a refusal that left everything as it was.
    let healthy = Outcome {
        result: Res::Custom(6046), // NotReturnable
        accounts: before.accounts.clone(),
        compute_units: 1,
    };
    assert!(check(&before, &healthy).is_empty());

    // Planted: the refusal, but the buyer gained a lamport from the vault.
    let planted = Outcome {
        result: Res::Custom(6046),
        accounts: vec![
            keeper,
            pool_account(&p),
            system(VAULT, vault_rent_floor() + 2 * PRICE - 1),
            system(BUYER, 11),
        ],
        compute_units: 1,
    };
    let found = lines(&before, &planted);
    assert!(found.iter().all(|l| *l == "O2 atomic"), "{found:?}");
    assert_eq!(found.len(), 2); // the vault and the buyer

    // And a program that did not complete is O1, whatever else happened.
    let crashed = Outcome {
        result: Res::FailedToComplete,
        accounts: before.accounts.clone(),
        compute_units: 200_000,
    };
    assert_eq!(lines(&before, &crashed), vec!["O1 no crash"]);
}
