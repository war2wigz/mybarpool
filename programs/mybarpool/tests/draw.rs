//! The draw, PROGRAM §4.4, under Mollusk with the Clock and SlotHashes
//! sysvars set directly and the real Entropy bytecode loaded as a second
//! program. Every spec number cites its line. Run after
//! `anchor build --arch v3`.
//!
//! The standard window: `END_AT = 1 000`, the sysvar holding
//! `[(END_AT + 2, h2), (END_AT + 1, h1), (END_AT, END_HASH)]`, the clock at
//! `END_AT + 3`. Pools are `Locked` with all 25 boxes sold to `buyer_2`.

mod common;

use anchor_lang::error::ErrorCode;
use common::*;
use mollusk_svm::result::Check;
use mybarpool::axes::axis;
use mybarpool::constants::{AXIS_LABEL_AWAY, AXIS_LABEL_HOME};
use mybarpool::entropy::fallback_hash;
use mybarpool::{
    DigitsDrawn, MybarpoolError as E, Pool, PoolStatus, VarReplaced, VarSampled, VarSet,
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

/// A Mollusk at `END_AT + 3` with the standard window; `entropy` loads the real program.
fn draw_mollusk(entropy: bool) -> mollusk_svm::Mollusk {
    let mut m = mollusk_for_draw(END_AT + 3, &standard_window());
    if entropy {
        with_entropy(&mut m);
    }
    m
}

// ---------------------------------------------------------------------------
// set_var
// ---------------------------------------------------------------------------

#[test]
fn set_var_binds_a_fresh_var_and_changes_nothing_else() {
    // PROGRAM §4.4 set_var: sets pool.var and var_end_at; emits VarSet.
    let f = Fixture::new();
    let m = mollusk_for_draw(900, &[]);
    let var = f.var_key();
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let fields = f.fresh_var(END_AT);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &fields)));
    let result = m.process_and_validate_instruction(
        &set_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::success()],
    );
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.var, to_a(&var));
    assert_eq!(after.var_end_at, END_AT);
    let mut expected = pool;
    expected.var = to_a(&var);
    expected.var_end_at = END_AT;
    assert_eq!(after, expected);
    assert_eq!(event_names(&result), ["VarSet"]);
    let event: VarSet = emitted_event(&result).expect("VarSet");
    assert_eq!(
        (event.pool, event.var, event.end_at),
        (to_a(&pool_key(&f)), to_a(&var), END_AT)
    );
    assert_eq!(event.time, T0);
}

#[test]
fn set_var_needs_a_locked_pool() {
    // PROGRAM §9: Open, Drawn, Returned, Settled, Split are not Locked.
    let f = Fixture::new();
    let m = mollusk_for_draw(900, &[]);
    let var = f.var_key();
    let fields = f.fresh_var(END_AT);
    for status in [
        PoolStatus::Open,
        PoolStatus::Drawn,
        PoolStatus::Returned,
        PoolStatus::Settled,
        PoolStatus::Split,
    ] {
        let sold = if status == PoolStatus::Open { 3 } else { 25 };
        let pool = pool_with(&f, status, sold, &f.buyer_2);
        let accounts = draw_accounts(&f, &m, &pool, Some((&var, &fields)));
        m.process_and_validate_instruction(
            &set_var_ix(&f.keeper, &pool, &var),
            &accounts,
            &[Check::err(custom(err(E::PoolNotLocked)))],
        );
    }
}

#[test]
fn set_var_twice_is_var_already_set() {
    // PROGRAM §4.4 "One call per pool".
    let f = Fixture::new();
    let m = mollusk_for_draw(900, &[]);
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    m.process_and_validate_instruction(
        &set_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::VarAlreadySet)))],
    );
}

#[test]
fn set_var_refuses_what_is_not_an_entropy_var() {
    // this brief: wrong owner (constraint), legacy 232-byte length, non-zero discriminator.
    let f = Fixture::new();
    let m = mollusk_for_draw(900, &[]);
    let var = f.var_key();
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let fields = f.fresh_var(END_AT);

    let mut system_owned = var_account(&fields);
    system_owned.owner = Pubkey::default();
    let mut legacy = var_account(&fields);
    legacy.data = legacy.data[8..].to_vec();
    let mut bad_disc = var_account(&fields);
    bad_disc.data[0] = 1;
    for planted in [system_owned, legacy, bad_disc] {
        let mut accounts = draw_accounts(&f, &m, &pool, None);
        set_account(&mut accounts, var, planted);
        m.process_and_validate_instruction(
            &set_var_ix(&f.keeper, &pool, &var),
            &accounts,
            &[Check::err(custom(err(E::VarNotEntropy)))],
        );
    }
}

#[test]
fn set_var_with_another_provider_is_var_provider_mismatch() {
    // PROGRAM §4.4 var.provider == config.entropy_provider.
    let f = Fixture::new();
    let m = mollusk_for_draw(900, &[]);
    let var = f.var_key();
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let fields = VarFields {
        provider: f.stranger,
        ..f.fresh_var(END_AT)
    };
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &fields)));
    m.process_and_validate_instruction(
        &set_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::VarProviderMismatch)))],
    );
}

#[test]
fn set_var_freshness_each_clause_is_var_not_fresh() {
    // PROGRAM §4.4: commit != 0; seed, slot_hash, value == 0; samples == 1; is_auto == 0;
    // end_at > Clock.slot. Nine cases, one clause each, at slot 900.
    let f = Fixture::new();
    let m = mollusk_for_draw(900, &[]);
    let var = f.var_key();
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let fresh = f.fresh_var(END_AT);
    let cases = [
        VarFields {
            commit: [0; 32],
            ..fresh
        },
        VarFields {
            seed: [1; 32],
            ..fresh
        },
        VarFields {
            slot_hash: [1; 32],
            ..fresh
        },
        VarFields {
            value: [1; 32],
            ..fresh
        },
        VarFields {
            samples: 2,
            ..fresh
        },
        VarFields {
            samples: 0,
            ..fresh
        },
        VarFields {
            is_auto: 1,
            ..fresh
        },
        VarFields {
            end_at: 900,
            ..fresh
        },
        VarFields {
            end_at: 899,
            ..fresh
        },
    ];
    for (i, fields) in cases.iter().enumerate() {
        let accounts = draw_accounts(&f, &m, &pool, Some((&var, fields)));
        let r = m.process_instruction(&set_var_ix(&f.keeper, &pool, &var), &accounts);
        assert_eq!(custom_error(&r), Some(err(E::VarNotFresh)), "case {i}");
    }
}

#[test]
fn set_var_by_the_admin_or_a_stranger_is_unauthorized() {
    // PROGRAM §10 Authority: the keeper key only.
    let f = Fixture::new();
    let m = mollusk_for_draw(900, &[]);
    let var = f.var_key();
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    for signer in [f.admin, f.stranger] {
        m.process_and_validate_instruction(
            &set_var_ix(&signer, &pool, &var),
            &accounts,
            &[Check::err(custom(err(E::Unauthorized)))],
        );
    }
}

// ---------------------------------------------------------------------------
// sample_var
// ---------------------------------------------------------------------------

#[test]
fn sample_var_cpis_entropy_sample_and_records_the_verified_hash() {
    // PROGRAM §4.4 sample_var, the CPI case: the real bytecode writes END_HASH, the handler
    // re-decodes, matches it against the sysvar and records slot + hash.
    let f = Fixture::new();
    let m = draw_mollusk(true);
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    let result = m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::success()],
    );
    // The Var account itself: the real program wrote the sysvar entry.
    assert_eq!(&account_of(&result, &var).data[144..176], &END_HASH);
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.sampled_slot, END_AT + 3);
    assert_eq!(after.sampled_hash, END_HASH);
    assert_eq!(after.status, PoolStatus::Locked);
    // The Entropy CPI is an inner instruction beside the event CPI; event_payloads sees one event.
    assert!(result.inner_instructions.len() >= 2);
    assert_eq!(event_names(&result), ["VarSampled"]);
    let event: VarSampled = emitted_event(&result).expect("VarSampled");
    assert_eq!((event.var, event.sampler), (to_a(&var), to_a(&f.keeper)));
    assert_eq!(
        (event.slot, event.end_at, event.slot_hash),
        (END_AT + 3, END_AT, END_HASH)
    );
}

#[test]
fn the_reload_test_sampled_hash_is_the_sysvar_entry_though_the_var_was_zero_before_the_cpi() {
    // The test that fails if the re-decode after the CPI is missing (PROGRAM §4.4). The planted
    // Var has slot_hash == 0; only a decode after the CPI can see END_HASH. Without the second
    // decode the handler reads zero and fails with SampleWindowMissed.
    let f = Fixture::new();
    let m = draw_mollusk(true);
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let fields = f.fresh_var(END_AT);
    assert_eq!(fields.slot_hash, [0u8; 32]);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &fields)));
    let result = m.process_and_validate_instruction(
        &sample_var_ix(&f.stranger, &pool, &var),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(
        decode_pool(account_of(&result, &pool_key(&f))).sampled_hash,
        END_HASH
    );
}

#[test]
fn sample_var_on_a_third_party_sampled_var_records_without_a_cpi() {
    // PROGRAM §4.4 "whether this call sampled or someone else did". No Entropy program loaded:
    // a CPI would fail, so success proves none happened.
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(
        &f,
        &m,
        &pool,
        Some((&var, &f.sampled_var(END_AT, END_HASH))),
    );
    let result = m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::success()],
    );
    assert_eq!(result.inner_instructions.len(), 1, "only the event CPI");
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(
        (after.sampled_slot, after.sampled_hash),
        (END_AT + 3, END_HASH)
    );
}

#[test]
fn sample_var_refuses_a_hash_the_sysvar_does_not_confirm() {
    // PROGRAM §4.4: a different hash for end_at, and the fallback with end_at gone, are both
    // SampleWindowMissed (ARCHITECTURE › Randomness "two ways").
    let f = Fixture::new();
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);

    let m = draw_mollusk(false);
    let accounts = draw_accounts(
        &f,
        &m,
        &pool,
        Some((&var, &f.sampled_var(END_AT, [0xAA; 32]))),
    );
    m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::SampleWindowMissed)))],
    );

    // Window closed: the sysvar's oldest entry is END_AT + 1 and the Var carries the fallback.
    let closed = mollusk_for_draw(
        END_AT + 600,
        &[(END_AT + 2, [0xE3; 32]), (END_AT + 1, [0xE2; 32])],
    );
    let fb = f.sampled_var(END_AT, fallback_hash(END_AT));
    let accounts = draw_accounts(&f, &closed, &pool, Some((&var, &fb)));
    closed.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::SampleWindowMissed)))],
    );
}

#[test]
fn sample_var_after_the_window_lets_entropy_write_the_fallback_and_then_refuses_it() {
    // PROGRAM §4.4: the CPI writes keccak(end_at); our check fails; the whole transaction
    // fails, so the Var write is rolled back too.
    let f = Fixture::new();
    let mut m = mollusk_for_draw(
        END_AT + 600,
        &[(END_AT + 2, [0xE3; 32]), (END_AT + 1, [0xE2; 32])],
    );
    with_entropy(&mut m);
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    let result = m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::SampleWindowMissed)))],
    );
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!((after.sampled_slot, after.sampled_hash), (0, [0u8; 32]));
    assert_eq!(
        &account_of(&result, &var).data[144..176],
        &[0u8; 32],
        "rolled back"
    );
}

#[test]
fn sample_var_before_end_at_is_sample_too_early() {
    // PROGRAM §4.4 Clock.slot ≥ var.end_at; this brief: 6062.
    let f = Fixture::new();
    let m = mollusk_for_draw(END_AT - 1, &standard_window());
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::SampleTooEarly)))],
    );
    assert_eq!(err(E::SampleTooEarly), 6062);
    assert_eq!(err(E::VarAlreadySampled), 6063);
}

#[test]
fn sample_var_binding_errors_var_not_set_var_mismatch_var_already_sampled() {
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let sampled = f.sampled_var(END_AT, END_HASH);

    let unbound = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let accounts = draw_accounts(&f, &m, &unbound, Some((&var, &sampled)));
    m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &unbound, &var),
        &accounts,
        &[Check::err(custom(err(E::VarNotSet)))],
    );

    let other = var_pda(&f.keeper, VAR_ID + 1).0;
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&var, &sampled)));
    set_account(
        &mut accounts,
        other,
        var_account(&VarFields {
            id: VAR_ID + 1,
            ..sampled
        }),
    );
    m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &pool, &other),
        &accounts,
        &[Check::err(custom(err(E::VarMismatch)))],
    );

    let already = sampled_pool(&f, &var, END_AT, END_AT + 2, END_HASH);
    let accounts = draw_accounts(&f, &m, &already, Some((&var, &sampled)));
    m.process_and_validate_instruction(
        &sample_var_ix(&f.keeper, &already, &var),
        &accounts,
        &[Check::err(custom(err(E::VarAlreadySampled)))],
    );
}

#[test]
fn sample_var_is_permissionless() {
    // this brief: any signer; VarSampled.sampler is whoever signed.
    let f = Fixture::new();
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    for signer in [f.stranger, f.admin] {
        let m = draw_mollusk(true);
        let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
        let result = m.process_and_validate_instruction(
            &sample_var_ix(&signer, &pool, &var),
            &accounts,
            &[Check::success()],
        );
        let event: VarSampled = emitted_event(&result).expect("VarSampled");
        assert_eq!(event.sampler, to_a(&signer));
    }
}

#[test]
fn sample_var_fixed_cpi_target_and_sysvar_by_address() {
    // PROGRAM §10 "fixed CPI targets": the Entropy program by address and executable; the
    // sysvar by address. Account order in SampleVar: sampler 0, pool 1, var 2, slot_hashes 3,
    // entropy_program 4.
    let f = Fixture::new();
    let m = draw_mollusk(true);
    let var = f.var_key();
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let base = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));

    let mut ix = sample_var_ix(&f.keeper, &pool, &var);
    ix.accounts[4].pubkey = Pubkey::default(); // the system program as the Entropy program
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &base)),
        Some(anchor(ErrorCode::ConstraintAddress))
    );

    let mut not_executable = base.clone();
    set_account(&mut not_executable, entropy_id(), system_account(1));
    let ix = sample_var_ix(&f.keeper, &pool, &var);
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &not_executable)),
        Some(anchor(ErrorCode::ConstraintExecutable))
    );

    let mut ix = sample_var_ix(&f.keeper, &pool, &var);
    ix.accounts[3].pubkey = to_m(&solana_sdk_ids::sysvar::clock::ID);
    let mut with_clock = base.clone();
    with_clock.push(m.sysvars.keyed_account_for_clock_sysvar());
    assert_eq!(
        custom_error(&m.process_instruction(&ix, &with_clock)),
        Some(anchor(ErrorCode::ConstraintAddress))
    );
}

#[test]
fn sample_var_needs_a_locked_pool() {
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    for status in [PoolStatus::Open, PoolStatus::Drawn] {
        let mut pool = pool_with(
            &f,
            status,
            if status == PoolStatus::Open { 3 } else { 25 },
            &f.buyer_2,
        );
        pool.var = to_a(&var);
        pool.var_end_at = END_AT;
        let accounts = draw_accounts(
            &f,
            &m,
            &pool,
            Some((&var, &f.sampled_var(END_AT, END_HASH))),
        );
        m.process_and_validate_instruction(
            &sample_var_ix(&f.keeper, &pool, &var),
            &accounts,
            &[Check::err(custom(err(E::PoolNotLocked)))],
        );
    }
}

// ---------------------------------------------------------------------------
// draw
// ---------------------------------------------------------------------------

#[test]
fn draw_derives_both_axes_from_the_revealed_value() {
    // PROGRAM §4.4 draw, §6.2 axes.
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    let accounts = draw_accounts(
        &f,
        &m,
        &pool,
        Some((&var, &f.revealed_var(END_AT, END_HASH, &SEED))),
    );
    let result = m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::success()],
    );
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.status, PoolStatus::Drawn);
    assert!(after.drawn);
    let value = standard_value();
    assert_eq!(after.home_axis, axis(&value, AXIS_LABEL_HOME));
    assert_eq!(after.away_axis, axis(&value, AXIS_LABEL_AWAY));
    for a in [after.home_axis, after.away_axis] {
        let mut sorted = a;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
    assert_ne!(after.home_axis, after.away_axis);
    assert_eq!((after.var, after.var_end_at), (to_a(&var), END_AT));
    assert_eq!(
        (after.sampled_slot, after.sampled_hash),
        (END_AT + 3, END_HASH)
    );
    assert_eq!(event_names(&result), ["DigitsDrawn"]);
    let event: DigitsDrawn = emitted_event(&result).expect("DigitsDrawn");
    assert_eq!((event.var, event.value), (to_a(&var), value));
    assert_eq!(
        (event.home_axis, event.away_axis),
        (after.home_axis, after.away_axis)
    );
}

#[test]
fn draw_refuses_a_var_that_no_longer_carries_the_verified_hash() {
    // PROGRAM §4.4 "value binding": a Var rolled with Next is VarNotSampledHere.
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    // After Next: commit = seed, zero seed/hash/value, samples 0, end_at later.
    let rolled = VarFields {
        commit: SEED,
        seed: [0; 32],
        slot_hash: [0; 32],
        value: [0; 32],
        samples: 0,
        end_at: END_AT + 100,
        ..f.fresh_var(END_AT)
    };
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &rolled)));
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::VarNotSampledHere)))],
    );
    // Revealed, but on a different hash than the one this program verified.
    let other = f.revealed_var(END_AT, [0xBB; 32], &SEED);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &other)));
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::VarNotSampledHere)))],
    );
}

#[test]
fn draw_refuses_the_fallback_hash_even_when_sampled_hash_is_forged_to_match() {
    // PROGRAM §4.4 VarFallbackHash: defence in depth behind sample_var.
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let fb = fallback_hash(END_AT);
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, fb);
    let forged = f.revealed_var(END_AT, fb, &SEED);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &forged)));
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::VarFallbackHash)))],
    );
}

#[test]
fn draw_refuses_an_unrevealed_or_inconsistent_value() {
    // PROGRAM §4.4 "recomputed": VarNotRevealed ×4.
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    let revealed = f.revealed_var(END_AT, END_HASH, &SEED);
    let mut off_by_one_bit = revealed;
    off_by_one_bit.value[0] ^= 1;
    let cases = [
        f.sampled_var(END_AT, END_HASH),
        VarFields {
            seed: SEED,
            ..f.sampled_var(END_AT, END_HASH)
        },
        off_by_one_bit,
        VarFields {
            samples: 2,
            ..revealed
        }, // value computed for samples == 1
    ];
    for (i, fields) in cases.iter().enumerate() {
        let accounts = draw_accounts(&f, &m, &pool, Some((&var, fields)));
        let r = m.process_instruction(&draw_ix(&f.keeper, &pool, &var), &accounts);
        assert_eq!(custom_error(&r), Some(err(E::VarNotRevealed)), "case {i}");
    }
}

#[test]
fn draw_binding_errors_not_sampled_here_mismatch_not_set() {
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let revealed = f.revealed_var(END_AT, END_HASH, &SEED);

    let unsampled = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &unsampled, Some((&var, &revealed)));
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &unsampled, &var),
        &accounts,
        &[Check::err(custom(err(E::VarNotSampledHere)))],
    );

    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    let other = var_pda(&f.keeper, VAR_ID + 1).0;
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&var, &revealed)));
    set_account(
        &mut accounts,
        other,
        var_account(&VarFields {
            id: VAR_ID + 1,
            ..revealed
        }),
    );
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &pool, &other),
        &accounts,
        &[Check::err(custom(err(E::VarMismatch)))],
    );

    let unbound = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let accounts = draw_accounts(&f, &m, &unbound, Some((&var, &revealed)));
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &unbound, &var),
        &accounts,
        &[Check::err(custom(err(E::VarNotSet)))],
    );
}

#[test]
fn draw_twice_is_pool_not_locked_and_a_planted_drawn_flag_is_already_drawn() {
    // PROGRAM §4.4 "second draw fails": after a draw the status is Drawn; a Locked pool with
    // drawn = true (unreachable) is AlreadyDrawn.
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    let revealed = f.revealed_var(END_AT, END_HASH, &SEED);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &revealed)));
    let first = m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &pool, &var),
        &accounts,
        &[Check::success()],
    );
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &pool, &var),
        &first.resulting_accounts,
        &[Check::err(custom(err(E::PoolNotLocked)))],
    );
    let planted = drawn_pool(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &planted, Some((&var, &revealed)));
    m.process_and_validate_instruction(
        &draw_ix(&f.keeper, &planted, &var),
        &accounts,
        &[Check::err(custom(err(E::AlreadyDrawn)))],
    );
}

#[test]
fn draw_by_the_admin_is_unauthorized() {
    let f = Fixture::new();
    let m = draw_mollusk(false);
    let var = f.var_key();
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    let accounts = draw_accounts(
        &f,
        &m,
        &pool,
        Some((&var, &f.revealed_var(END_AT, END_HASH, &SEED))),
    );
    m.process_and_validate_instruction(
        &draw_ix(&f.admin, &pool, &var),
        &accounts,
        &[Check::err(custom(err(E::Unauthorized)))],
    );
}

// ---------------------------------------------------------------------------
// replace_var
// ---------------------------------------------------------------------------

fn var_b(f: &Fixture) -> (Pubkey, VarFields) {
    (
        var_pda(&f.keeper, VAR_ID + 1).0,
        VarFields {
            id: VAR_ID + 1,
            ..f.fresh_var(END_AT + 1_000)
        },
    )
}

#[test]
fn replace_var_by_the_admin_on_an_unsampled_pool_rebinds_and_counts() {
    // PROGRAM §4.4 replace_var: on a pool whose window was missed (sampled_slot == 0).
    let f = Fixture::new();
    let m = mollusk_for_draw(END_AT + 600, &[]);
    let a = f.var_key();
    let (b, b_fields) = var_b(&f);
    let pool = locked_pool_with_var(&f, &a, END_AT);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&a, &f.fresh_var(END_AT))));
    set_account(&mut accounts, b, var_account(&b_fields));
    let result = m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &pool, &b),
        &accounts,
        &[Check::success()],
    );
    let after = decode_pool(account_of(&result, &pool_key(&f)));
    assert_eq!(after.var, to_a(&b));
    assert_eq!(after.var_end_at, END_AT + 1_000);
    assert_eq!((after.sampled_slot, after.sampled_hash), (0, [0u8; 32]));
    assert_eq!(after.var_replacements, 1);
    assert_eq!(event_names(&result), ["VarReplaced"]);
    let event: VarReplaced = emitted_event(&result).expect("VarReplaced");
    assert_eq!((event.old_var, event.new_var), (to_a(&a), to_a(&b)));
    assert_eq!((event.end_at, event.replacements), (END_AT + 1_000, 1));
}

#[test]
fn replace_var_on_a_sampled_pool_is_var_already_sampled() {
    // this brief: a Var with a verified sample is never abandoned.
    let f = Fixture::new();
    let m = mollusk_for_draw(END_AT + 600, &[]);
    let a = f.var_key();
    let (b, b_fields) = var_b(&f);
    let pool = sampled_pool(&f, &a, END_AT, END_AT + 3, END_HASH);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&a, &f.sampled_var(END_AT, END_HASH))));
    set_account(&mut accounts, b, var_account(&b_fields));
    let result = m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &pool, &b),
        &accounts,
        &[Check::err(custom(err(E::VarAlreadySampled)))],
    );
    assert_eq!(decode_pool(account_of(&result, &pool_key(&f))), pool);
}

#[test]
fn replace_var_is_capped_at_two() {
    // PROGRAM §4.4 "capped at two".
    let f = Fixture::new();
    let m = mollusk_for_draw(END_AT + 600, &[]);
    let a = f.var_key();
    let (b, b_fields) = var_b(&f);
    let c = var_pda(&f.keeper, VAR_ID + 2).0;
    let c_fields = VarFields {
        id: VAR_ID + 2,
        ..f.fresh_var(END_AT + 2_000)
    };
    let d = var_pda(&f.keeper, VAR_ID + 3).0;
    let d_fields = VarFields {
        id: VAR_ID + 3,
        ..f.fresh_var(END_AT + 3_000)
    };
    let pool = locked_pool_with_var(&f, &a, END_AT);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&a, &f.fresh_var(END_AT))));
    for (k, v) in [(b, b_fields), (c, c_fields), (d, d_fields)] {
        set_account(&mut accounts, k, var_account(&v));
    }
    let r1 = m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &pool, &b),
        &accounts,
        &[Check::success()],
    );
    let p1 = decode_pool(account_of(&r1, &pool_key(&f)));
    let r2 = m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &p1, &c),
        &r1.resulting_accounts,
        &[Check::success()],
    );
    let p2 = decode_pool(account_of(&r2, &pool_key(&f)));
    assert_eq!(p2.var_replacements, 2);
    assert_eq!(p2.var, to_a(&c));
    m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &p2, &d),
        &r2.resulting_accounts,
        &[Check::err(custom(err(E::TooManyVarReplacements)))],
    );
}

#[test]
fn replace_var_by_the_keeper_or_a_stranger_is_unauthorized() {
    // PROGRAM §4.4 "Never callable by the keeper".
    let f = Fixture::new();
    let m = mollusk_for_draw(END_AT + 600, &[]);
    let a = f.var_key();
    let (b, b_fields) = var_b(&f);
    let pool = locked_pool_with_var(&f, &a, END_AT);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&a, &f.fresh_var(END_AT))));
    set_account(&mut accounts, b, var_account(&b_fields));
    for signer in [f.keeper, f.stranger] {
        m.process_and_validate_instruction(
            &replace_var_ix(&signer, &pool, &b),
            &accounts,
            &[Check::err(custom(err(E::Unauthorized)))],
        );
    }
}

#[test]
fn replace_var_refuses_itself_a_stale_var_and_a_drawn_pool() {
    let f = Fixture::new();
    let m = mollusk_for_draw(END_AT + 600, &[]);
    let a = f.var_key();
    let (b, b_fields) = var_b(&f);
    let pool = locked_pool_with_var(&f, &a, END_AT);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&a, &f.fresh_var(END_AT))));
    set_account(&mut accounts, b, var_account(&b_fields));
    m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &pool, &a),
        &accounts,
        &[Check::err(custom(err(E::VarAlreadySet)))],
    );
    // The set_var errors on new_var: stale (end_at passed), wrong provider, auto.
    for (fields, code) in [
        (
            VarFields {
                end_at: END_AT + 600,
                ..b_fields
            },
            err(E::VarNotFresh),
        ),
        (
            VarFields {
                provider: f.stranger,
                ..b_fields
            },
            err(E::VarProviderMismatch),
        ),
        (
            VarFields {
                is_auto: 1,
                ..b_fields
            },
            err(E::VarNotFresh),
        ),
    ] {
        let mut accounts = accounts.clone();
        set_account(&mut accounts, b, var_account(&fields));
        m.process_and_validate_instruction(
            &replace_var_ix(&f.admin, &pool, &b),
            &accounts,
            &[Check::err(custom(code))],
        );
    }
    let mut drawn = drawn_pool(&f, &a, END_AT);
    drawn.status = PoolStatus::Drawn;
    let mut accounts = draw_accounts(&f, &m, &drawn, Some((&a, &f.fresh_var(END_AT))));
    set_account(&mut accounts, b, var_account(&b_fields));
    m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &drawn, &b),
        &accounts,
        &[Check::err(custom(err(E::PoolNotLocked)))],
    );
    // A Locked pool planted with drawn = true (unreachable): AlreadyDrawn, checked before the cap.
    let planted = drawn_pool(&f, &a, END_AT);
    let mut accounts = draw_accounts(&f, &m, &planted, Some((&a, &f.fresh_var(END_AT))));
    set_account(&mut accounts, b, var_account(&b_fields));
    m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &planted, &b),
        &accounts,
        &[Check::err(custom(err(E::AlreadyDrawn)))],
    );
}

#[test]
fn after_a_replacement_the_new_var_samples_and_draws() {
    // PROGRAM §4.4: the replacement runs the ordinary path; the axes come from its value.
    let f = Fixture::new();
    let a = f.var_key();
    let (b, b_fields) = var_b(&f);
    let b_end = b_fields.end_at;
    let b_hash = [0xB7; 32];
    let pool = locked_pool_with_var(&f, &a, END_AT);

    let m = mollusk_for_draw(END_AT + 600, &[]);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&a, &f.fresh_var(END_AT))));
    set_account(&mut accounts, b, var_account(&b_fields));
    let replaced = m.process_and_validate_instruction(
        &replace_var_ix(&f.admin, &pool, &b),
        &accounts,
        &[Check::success()],
    );
    let p1 = decode_pool(account_of(&replaced, &pool_key(&f)));

    let mut m2 = mollusk_for_draw(b_end + 3, &[(b_end + 1, [0xB8; 32]), (b_end, b_hash)]);
    with_entropy(&mut m2);
    let mut accounts = replaced.resulting_accounts.clone();
    set_account(
        &mut accounts,
        to_m(&solana_sdk_ids::sysvar::slot_hashes::ID),
        m2.sysvars.keyed_account_for_slot_hashes_sysvar().1,
    );
    let sampled = m2.process_and_validate_instruction(
        &sample_var_ix(&f.stranger, &p1, &b),
        &accounts,
        &[Check::success()],
    );
    let p2 = decode_pool(account_of(&sampled, &pool_key(&f)));
    assert_eq!((p2.sampled_slot, p2.sampled_hash), (b_end + 3, b_hash));

    // Reveal happens off-program (the provider); plant the revealed Var and draw.
    let seed_b = [0x33; 32];
    let revealed = VarFields {
        commit: commit_of(&seed_b),
        seed: seed_b,
        value: mybarpool::entropy::expected_value(&b_hash, &seed_b, 1),
        ..VarFields {
            slot_hash: b_hash,
            ..b_fields
        }
    };
    let mut accounts = sampled.resulting_accounts.clone();
    set_account(&mut accounts, b, var_account(&revealed));
    let drawn = m2.process_and_validate_instruction(
        &draw_ix(&f.keeper, &p2, &b),
        &accounts,
        &[Check::success()],
    );
    let p3 = decode_pool(account_of(&drawn, &pool_key(&f)));
    assert_eq!(p3.status, PoolStatus::Drawn);
    assert_eq!(p3.home_axis, axis(&revealed.value, AXIS_LABEL_HOME));
    assert_eq!(p3.var, to_a(&b));
    assert_eq!(p3.var_replacements, 1);
}

// ---------------------------------------------------------------------------
// Composability (PROGRAM §10) and events
// ---------------------------------------------------------------------------

#[test]
fn set_var_sample_var_and_draw_are_unaffected_by_neighbouring_instructions() {
    let f = Fixture::new();
    let var = f.var_key();
    let noop = solana_instruction::Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], // SystemInstruction::Transfer { lamports: 1 }
        vec![
            solana_instruction::AccountMeta::new(f.keeper, true),
            solana_instruction::AccountMeta::new(f.admin, false),
        ],
    );

    // Mollusk has no ComputeBudget builtin to process a `SetComputeUnitLimit` instruction, so
    // the limit is raised on the Mollusk itself to 400,000, the figure the localnet suite sends.
    let with_limit = |mut m: mollusk_svm::Mollusk| {
        m.compute_budget.compute_unit_limit = 400_000;
        m
    };

    // set_var at slot 900.
    let m = with_limit(mollusk_for_draw(900, &[]));
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    let ix = set_var_ix(&f.keeper, &pool, &var);
    let alone = m.process_instruction_chain(&[ix.clone()], &accounts);
    let sandwiched = m.process_instruction_chain(&[noop.clone(), ix, noop.clone()], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_pool(account_of(&alone, &pool_key(&f))),
        decode_pool(account_of(&sandwiched, &pool_key(&f)))
    );

    // sample_var (CPI case) at END_AT + 3.
    let m = with_limit(draw_mollusk(true));
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    let ix = sample_var_ix(&f.keeper, &pool, &var);
    let alone = m.process_instruction_chain(&[ix.clone()], &accounts);
    let sandwiched = m.process_instruction_chain(&[noop.clone(), ix, noop.clone()], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_pool(account_of(&alone, &pool_key(&f))),
        decode_pool(account_of(&sandwiched, &pool_key(&f)))
    );

    // draw.
    let m = draw_mollusk(false);
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    let accounts = draw_accounts(
        &f,
        &m,
        &pool,
        Some((&var, &f.revealed_var(END_AT, END_HASH, &SEED))),
    );
    let ix = draw_ix(&f.keeper, &pool, &var);
    let alone = m.process_instruction_chain(&[ix.clone()], &accounts);
    let sandwiched = m.process_instruction_chain(&[noop.clone(), ix, noop], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_pool(account_of(&alone, &pool_key(&f))),
        decode_pool(account_of(&sandwiched, &pool_key(&f)))
    );
}

#[test]
fn event_payloads_returns_exactly_the_four_new_events_and_never_the_entropy_cpi() {
    // PROGRAM §7: one event per instruction; the Entropy `Sample` CPI is an inner instruction
    // to another program and `event_payloads` filters on the event authority.
    let f = Fixture::new();
    let var = f.var_key();
    let (b, b_fields) = var_b(&f);

    let m = mollusk_for_draw(900, &[]);
    let pool = pool_with(&f, PoolStatus::Locked, 25, &f.buyer_2);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    let r = m.process_instruction(&set_var_ix(&f.keeper, &pool, &var), &accounts);
    assert_eq!(event_names(&r), ["VarSet"]);
    assert_eq!(emitted_event_count(&r), 1);

    let m = draw_mollusk(true);
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    let r = m.process_instruction(&sample_var_ix(&f.keeper, &pool, &var), &accounts);
    assert_eq!(
        r.inner_instructions.len(),
        2,
        "the Entropy CPI and the event CPI"
    );
    assert_eq!(event_names(&r), ["VarSampled"]);
    assert_eq!(emitted_event_count(&r), 1);

    let m = draw_mollusk(false);
    let pool = sampled_pool(&f, &var, END_AT, END_AT + 3, END_HASH);
    let accounts = draw_accounts(
        &f,
        &m,
        &pool,
        Some((&var, &f.revealed_var(END_AT, END_HASH, &SEED))),
    );
    let r = m.process_instruction(&draw_ix(&f.keeper, &pool, &var), &accounts);
    assert_eq!(event_names(&r), ["DigitsDrawn"]);
    assert_eq!(emitted_event_count(&r), 1);

    let m = mollusk_for_draw(END_AT + 600, &[]);
    let pool = locked_pool_with_var(&f, &var, END_AT);
    let mut accounts = draw_accounts(&f, &m, &pool, Some((&var, &f.fresh_var(END_AT))));
    set_account(&mut accounts, b, var_account(&b_fields));
    let r = m.process_instruction(&replace_var_ix(&f.admin, &pool, &b), &accounts);
    assert_eq!(event_names(&r), ["VarReplaced"]);
    assert_eq!(emitted_event_count(&r), 1);
}

#[test]
fn pool_size_is_unchanged() {
    assert_eq!(Pool::SIZE, 1442); // PROGRAM §3.3; no layout change in Step 5
}
