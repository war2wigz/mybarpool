//! Game instructions, PROGRAM §4.2, under Mollusk with an explicit clock.
//! Every spec number cites its line. Run after `anchor build --arch v3`.
//!
//! `now` is always `T0` unless a test says otherwise; the standard game is
//! KC hosting DAL in week 1 of 2026 with `SCHEDULED = T0 + 86 400`.

mod common;

use common::*;
use mollusk_svm::result::Check;
use mybarpool::constants::{KICKOFF_UPDATE_BOUND, MIN_QUARTER_SECONDS};
use mybarpool::{
    GameCreated, GameKey, GameMarked, GameRecord, GameStatus, KickoffUpdated, MybarpoolError as E,
    ScoresPosted,
};
use solana_pubkey::Pubkey;

fn custom(code: u32) -> solana_program_error::ProgramError {
    solana_program_error::ProgramError::Custom(code)
}

fn standard_game() -> Pubkey {
    game_pda(&standard_key(), SCHEDULED).0
}

// ---------------------------------------------------------------------------
// create_game
// ---------------------------------------------------------------------------

#[test]
fn create_game_by_the_keeper_writes_every_field_and_emits_game_created() {
    let f = Fixture::new();
    let m = mollusk_at(T0);
    let (pda, bump) = game_pda(&standard_key(), SCHEDULED);
    let ix = create_game_ix(&f.keeper, standard_key(), SCHEDULED);
    let result =
        m.process_and_validate_instruction(&ix, &game_accounts(&f, None), &[Check::success()]);

    let account = account_of(&result, &pda);
    assert_eq!(account.data.len(), 153); // PROGRAM §3.2: 8 + 145
    assert_eq!(account.owner, program_id());
    let game = decode_game(account);
    assert_eq!(game, standard_record());
    assert_eq!(game.key, standard_key());
    assert_eq!(game.scheduled_kickoff, SCHEDULED);
    assert_eq!(game.recorded_kickoff, SCHEDULED); // PROGRAM §4.2: recorded_kickoff = scheduled_kickoff
    assert_eq!(game.status, GameStatus::Scheduled);
    assert_eq!(game.quarters_posted, 0);
    assert_eq!(game.home_score, [0; 4]);
    assert_eq!(game.away_score, [0; 4]);
    assert_eq!(game.posted_at, [0; 4]);
    assert!(!game.final_had_overtime);
    assert_eq!(game.marked_at, 0); // PROGRAM §3.2: 0 if never
    assert_eq!(game.bump, bump);
    assert!(game.reserved.iter().all(|&b| b == 0));

    let event: GameCreated = emitted_event(&result).expect("GameCreated self-CPI");
    assert_eq!(event.time, T0);
    assert_eq!(event.game, to_a(&pda));
    assert_eq!(event.key, standard_key());
    assert_eq!(event.scheduled_kickoff, SCHEDULED);
    assert_eq!(emitted_event_count(&result), 1);
}

#[test]
fn create_game_by_the_admin_or_a_stranger_is_unauthorized() {
    // PROGRAM §4.2 "signer: keeper"; §10 Authority.
    let f = Fixture::new();
    let m = mollusk_at(T0);
    for signer in [f.admin, f.stranger] {
        let ix = create_game_ix(&signer, standard_key(), SCHEDULED);
        let result = m.process_and_validate_instruction(
            &ix,
            &game_accounts(&f, None),
            &[Check::err(custom(err(E::Unauthorized)))],
        );
        assert_eq!(account_of(&result, &standard_game()).data.len(), 0);
    }
}

#[test]
fn create_game_at_or_before_now_is_kickoff_in_past() {
    // PROGRAM §4.2: scheduled_kickoff > now.
    let f = Fixture::new();
    let m = mollusk_at(T0);
    for kickoff in [T0, T0 - 1] {
        let mut accounts = game_accounts(&f, None);
        accounts.push((game_pda(&standard_key(), kickoff).0, system_account(0)));
        m.process_and_validate_instruction(
            &create_game_ix(&f.keeper, standard_key(), kickoff),
            &accounts,
            &[Check::err(custom(err(E::KickoffInPast)))],
        );
    }
    let mut accounts = game_accounts(&f, None);
    accounts.push((game_pda(&standard_key(), T0 + 1).0, system_account(0)));
    m.process_and_validate_instruction(
        &create_game_ix(&f.keeper, standard_key(), T0 + 1),
        &accounts,
        &[Check::success()],
    );
}

fn expect_create_error(f: &Fixture, m: &mollusk_svm::Mollusk, key: GameKey, code: u32) {
    let mut accounts = game_accounts(f, None);
    accounts.push((game_pda(&key, SCHEDULED).0, system_account(0)));
    m.process_and_validate_instruction(
        &create_game_ix(&f.keeper, key, SCHEDULED),
        &accounts,
        &[Check::err(custom(code))],
    );
}

#[test]
fn create_game_with_a_bad_week_or_team_is_invalid_game_key() {
    // PROGRAM §2: week 1–22 or 101–103; home, away 0–31 and different.
    let f = Fixture::new();
    let m = mollusk_at(T0);
    let k = standard_key();
    for week in [0u8, 23, 100, 104] {
        expect_create_error(&f, &m, GameKey { week, ..k }, err(E::InvalidGameKey));
    }
    expect_create_error(
        &f,
        &m,
        GameKey { away: k.home, ..k },
        err(E::InvalidGameKey),
    );
    expect_create_error(&f, &m, GameKey { home: 32, ..k }, err(E::InvalidGameKey));
    expect_create_error(&f, &m, GameKey { away: 32, ..k }, err(E::InvalidGameKey));
}

#[test]
fn create_game_preseason_weeks_need_preseason_enabled() {
    // PROGRAM §3.1 preseason_enabled; §2 preseason weeks 101–103.
    let f = Fixture::new();
    let m = mollusk_at(T0);
    let k = standard_key();
    expect_create_error(&f, &m, GameKey { week: 101, ..k }, err(E::InvalidGameKey));

    let mut config = f.expected_config();
    config.preseason_enabled = true;
    for week in [101u8, 22, 103] {
        let key = GameKey { week, ..k };
        let mut accounts = base_accounts(&f, Some(&config));
        accounts.push((game_pda(&key, SCHEDULED).0, system_account(0)));
        m.process_and_validate_instruction(
            &create_game_ix(&f.keeper, key, SCHEDULED),
            &accounts,
            &[Check::success()],
        );
    }
}

#[test]
fn create_game_ignores_paused() {
    // PROGRAM §3.1: paused affects create_pool, buy and sponsor and nothing else.
    let f = Fixture::new();
    let m = mollusk_at(T0);
    let mut config = f.expected_config();
    config.paused = true;
    let mut accounts = base_accounts(&f, Some(&config));
    accounts.push((standard_game(), system_account(0)));
    m.process_and_validate_instruction(
        &create_game_ix(&f.keeper, standard_key(), SCHEDULED),
        &accounts,
        &[Check::success()],
    );
}

#[test]
fn create_game_twice_fails_and_a_different_kickoff_is_a_different_record() {
    // PROGRAM §3.2 "One per scheduled game"; §2 "gets a new record".
    let f = Fixture::new();
    let m = mollusk_at(T0);
    let ix = create_game_ix(&f.keeper, standard_key(), SCHEDULED);
    let first = m.process_instruction(&ix, &game_accounts(&f, None));
    assert!(first.program_result.is_ok());
    let second = m.process_instruction(&ix, &first.resulting_accounts);
    assert!(!second.program_result.is_ok(), "account already exists");

    let later = SCHEDULED + 3_600;
    let other_pda = game_pda(&standard_key(), later).0;
    assert_ne!(other_pda, standard_game());
    let mut accounts = first.resulting_accounts.clone();
    accounts.push((other_pda, system_account(0)));
    let result = m.process_and_validate_instruction(
        &create_game_ix(&f.keeper, standard_key(), later),
        &accounts,
        &[Check::success()],
    );
    let a = decode_game(account_of(&result, &standard_game()));
    let b = decode_game(account_of(&result, &other_pda));
    assert_eq!(a.key, b.key);
    assert_eq!(a.scheduled_kickoff, SCHEDULED);
    assert_eq!(b.scheduled_kickoff, later);
    assert_ne!(a.bump, 0);
}

#[test]
fn create_game_at_a_foreign_address_fails_the_seeds_check() {
    let f = Fixture::new();
    let m = mollusk_at(T0);
    let wrong = Pubkey::new_unique();
    let mut ix = create_game_ix(&f.keeper, standard_key(), SCHEDULED);
    ix.accounts[2].pubkey = wrong;
    let mut accounts = game_accounts(&f, None);
    accounts.push((wrong, system_account(0)));
    let result = m.process_instruction(&ix, &accounts);
    assert!(!result.program_result.is_ok());
}

// ---------------------------------------------------------------------------
// update_kickoff
// ---------------------------------------------------------------------------

fn expect_update(
    f: &Fixture,
    now: i64,
    record: &GameRecord,
    new_time: i64,
    expect: Result<(), u32>,
) -> mollusk_svm::result::InstructionResult {
    let m = mollusk_at(now);
    let game = game_pda(&record.key, record.scheduled_kickoff).0;
    let ix = update_kickoff_ix(&f.keeper, &game, new_time);
    let check = match expect {
        Ok(()) => Check::success(),
        Err(code) => Check::err(custom(code)),
    };
    m.process_and_validate_instruction(&ix, &game_accounts(f, Some(record)), &[check])
}

#[test]
fn update_kickoff_later_moves_recorded_only_and_emits_kickoff_updated() {
    // PROGRAM §4.2: sets recorded_kickoff; scheduled_kickoff is the seed and never changes.
    let f = Fixture::new();
    let new_time = SCHEDULED + 3_600;
    let result = expect_update(&f, T0, &standard_record(), new_time, Ok(()));
    let game = decode_game(account_of(&result, &standard_game()));
    assert_eq!(game.recorded_kickoff, new_time);
    assert_eq!(game.scheduled_kickoff, SCHEDULED);
    assert_eq!(game.status, GameStatus::Scheduled);
    let event: KickoffUpdated = emitted_event(&result).expect("KickoffUpdated");
    assert_eq!(event.time, T0);
    assert_eq!(event.game, to_a(&standard_game()));
    assert_eq!((event.old, event.new), (SCHEDULED, new_time));
    assert_eq!(emitted_event_count(&result), 1);
}

#[test]
fn update_kickoff_earlier_is_allowed() {
    // PROGRAM §4.2: "Earlier moves are allowed under the same checks."
    let f = Fixture::new();
    let result = expect_update(&f, T0, &standard_record(), SCHEDULED - 3_600, Ok(()));
    assert_eq!(
        decode_game(account_of(&result, &standard_game())).recorded_kickoff,
        SCHEDULED - 3_600
    );
}

#[test]
fn update_kickoff_to_the_same_time_is_a_no_op_that_still_emits() {
    let f = Fixture::new();
    let result = expect_update(&f, T0, &standard_record(), SCHEDULED, Ok(()));
    let event: KickoffUpdated = emitted_event(&result).expect("KickoffUpdated");
    assert_eq!((event.old, event.new), (SCHEDULED, SCHEDULED));
}

#[test]
fn update_kickoff_bound_is_72_hours_after_the_scheduled_kickoff() {
    // PROGRAM §1: KICKOFF_UPDATE_BOUND = 259 200; §4.2: new_time ≤ scheduled_kickoff + bound.
    let f = Fixture::new();
    assert_eq!(KICKOFF_UPDATE_BOUND, 259_200);
    expect_update(
        &f,
        T0,
        &standard_record(),
        SCHEDULED + KICKOFF_UPDATE_BOUND,
        Ok(()),
    );
    expect_update(
        &f,
        T0,
        &standard_record(),
        SCHEDULED + KICKOFF_UPDATE_BOUND + 1,
        Err(err(E::KickoffOutOfBounds)),
    );
}

#[test]
fn update_kickoff_bound_does_not_follow_a_prior_move() {
    // Audit focus: the bound is measured from scheduled_kickoff, not from the moved recorded one.
    let f = Fixture::new();
    let mut moved = standard_record();
    moved.recorded_kickoff = SCHEDULED + 3_600;
    expect_update(
        &f,
        T0,
        &moved,
        SCHEDULED + KICKOFF_UPDATE_BOUND + 1,
        Err(err(E::KickoffOutOfBounds)),
    );
    // Measured from recorded_kickoff it would have fit; from scheduled_kickoff it is exactly at the bound.
    expect_update(&f, T0, &moved, SCHEDULED + KICKOFF_UPDATE_BOUND, Ok(()));
}

#[test]
fn update_kickoff_to_now_or_earlier_is_kickoff_in_past() {
    // PROGRAM §4.2: new_time > now.
    let f = Fixture::new();
    for new_time in [T0, T0 - 1] {
        expect_update(
            &f,
            T0,
            &standard_record(),
            new_time,
            Err(err(E::KickoffInPast)),
        );
    }
}

#[test]
fn update_kickoff_once_recorded_kickoff_has_arrived_is_too_late() {
    // PROGRAM §4.2: now < recorded_kickoff.
    let f = Fixture::new();
    for now in [SCHEDULED, SCHEDULED + 1] {
        expect_update(
            &f,
            now,
            &standard_record(),
            now + 3_600,
            Err(err(E::KickoffUpdateTooLate)),
        );
    }
    expect_update(
        &f,
        SCHEDULED - 1,
        &standard_record(),
        SCHEDULED + 3_600,
        Ok(()),
    );
}

#[test]
fn update_kickoff_after_a_post_is_too_late() {
    // PROGRAM §4.2: quarters_posted == 0. Fixture-only: Q1 cannot land before recorded_kickoff.
    let f = Fixture::new();
    let mut record = record_with_quarters(1);
    record.recorded_kickoff = SCHEDULED + 86_400; // make the "now < recorded_kickoff" clause pass
    expect_update(
        &f,
        T0,
        &record,
        SCHEDULED + 3_600,
        Err(err(E::KickoffUpdateTooLate)),
    );
}

#[test]
fn update_kickoff_on_a_terminal_record_is_game_not_scheduled() {
    // PROGRAM §4.2: status == Scheduled; §9: terminal states.
    let f = Fixture::new();
    for status in [
        GameStatus::Postponed,
        GameStatus::Cancelled,
        GameStatus::Suspended,
    ] {
        let mut record = standard_record();
        record.status = status;
        record.marked_at = T0 - 1;
        expect_update(
            &f,
            T0,
            &record,
            SCHEDULED + 3_600,
            Err(err(E::GameNotScheduled)),
        );
    }
    let mut final_record = record_with_quarters(4);
    final_record.recorded_kickoff = SCHEDULED + 86_400;
    expect_update(
        &f,
        T0,
        &final_record,
        SCHEDULED + 3_600,
        Err(err(E::GameNotScheduled)),
    );
}

#[test]
fn update_kickoff_by_the_admin_or_a_stranger_is_unauthorized() {
    let f = Fixture::new();
    let m = mollusk_at(T0);
    for signer in [f.admin, f.stranger] {
        let ix = update_kickoff_ix(&signer, &standard_game(), SCHEDULED + 3_600);
        let result = m.process_and_validate_instruction(
            &ix,
            &game_accounts(&f, Some(&standard_record())),
            &[Check::err(custom(err(E::Unauthorized)))],
        );
        assert_eq!(
            decode_game(account_of(&result, &standard_game())),
            standard_record()
        );
    }
}

// ---------------------------------------------------------------------------
// post_scores
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn expect_post(
    f: &Fixture,
    now: i64,
    record: &GameRecord,
    quarter: u8,
    home: u16,
    away: u16,
    is_final: bool,
    had_overtime: bool,
    expect: Result<(), u32>,
) -> mollusk_svm::result::InstructionResult {
    let m = mollusk_at(now);
    let game = game_pda(&record.key, record.scheduled_kickoff).0;
    let ix = post_scores_ix(
        &f.keeper,
        &game,
        quarter,
        home,
        away,
        is_final,
        had_overtime,
    );
    let check = match expect {
        Ok(()) => Check::success(),
        Err(code) => Check::err(custom(code)),
    };
    m.process_and_validate_instruction(&ix, &game_accounts(f, Some(record)), &[check])
}

#[test]
fn post_scores_q1_needs_15_minutes_after_recorded_kickoff() {
    // PROGRAM §1: MIN_QUARTER_SECONDS = 900; §4.2: now ≥ recorded_kickoff + MIN_QUARTER_SECONDS.
    let f = Fixture::new();
    assert_eq!(MIN_QUARTER_SECONDS, 900);
    expect_post(
        &f,
        SCHEDULED + 899,
        &standard_record(),
        1,
        7,
        3,
        false,
        false,
        Err(err(E::QuarterTooSoon)),
    );
    let now = SCHEDULED + 900;
    let result = expect_post(&f, now, &standard_record(), 1, 7, 3, false, false, Ok(()));
    let game = decode_game(account_of(&result, &standard_game()));
    assert_eq!(game.home_score[0], 7);
    assert_eq!(game.away_score[0], 3);
    assert_eq!(game.posted_at[0], now);
    assert_eq!(game.quarters_posted, 1);
    assert_eq!(game.status, GameStatus::Scheduled);
    assert_eq!(game.home_score[1..], [0; 3]);
    let event: ScoresPosted = emitted_event(&result).expect("ScoresPosted");
    assert_eq!(event.time, now);
    assert_eq!(event.game, to_a(&standard_game()));
    assert_eq!(
        (
            event.quarter,
            event.home,
            event.away,
            event.is_final,
            event.had_overtime
        ),
        (1, 7, 3, false, false)
    );
    assert_eq!(emitted_event_count(&result), 1);
}

#[test]
fn post_scores_q1_floor_follows_the_recorded_kickoff() {
    // PROGRAM §4.2: the floor is from recorded_kickoff, which update_kickoff may have moved.
    let f = Fixture::new();
    let mut moved = standard_record();
    moved.recorded_kickoff = SCHEDULED + 3_600;
    expect_post(
        &f,
        SCHEDULED + 900,
        &moved,
        1,
        7,
        3,
        false,
        false,
        Err(err(E::QuarterTooSoon)),
    );
    expect_post(
        &f,
        SCHEDULED + 3_600 + 899,
        &moved,
        1,
        7,
        3,
        false,
        false,
        Err(err(E::QuarterTooSoon)),
    );
    expect_post(
        &f,
        SCHEDULED + 3_600 + 900,
        &moved,
        1,
        7,
        3,
        false,
        false,
        Ok(()),
    );
}

#[test]
fn post_scores_full_game_at_realistic_times_ends_final_with_overtime() {
    // ARCHITECTURE › Payouts: cumulative scores; Q4 is the final after overtime.
    let f = Fixture::new();
    let posts = [
        (1u8, 7u16, 3u16, false, false, 45 * 60),
        (2, 14, 10, false, false, 95 * 60),
        (3, 17, 17, false, false, 140 * 60),
        (4, 24, 20, true, true, 190 * 60),
    ];
    let mut accounts = game_accounts(&f, Some(&standard_record()));
    let mut times = [0i64; 4];
    for (i, (q, home, away, is_final, ot, offset)) in posts.iter().enumerate() {
        let now = SCHEDULED + offset;
        times[i] = now;
        let m = mollusk_at(now);
        let ix = post_scores_ix(
            &f.keeper,
            &standard_game(),
            *q,
            *home,
            *away,
            *is_final,
            *ot,
        );
        let result = m.process_and_validate_instruction(&ix, &accounts, &[Check::success()]);
        let event: ScoresPosted = emitted_event(&result).expect("ScoresPosted");
        assert_eq!((event.quarter, event.time), (*q, now));
        assert_eq!(emitted_event_count(&result), 1);
        accounts = result.resulting_accounts;
    }
    let game = decode_game(
        &accounts
            .iter()
            .find(|(k, _)| *k == standard_game())
            .unwrap()
            .1,
    );
    assert_eq!(game.status, GameStatus::Final);
    assert_eq!(game.quarters_posted, 4);
    assert!(game.final_had_overtime);
    assert_eq!(game.home_score, [7, 14, 17, 24]);
    assert_eq!(game.away_score, [3, 10, 17, 20]);
    assert_eq!(game.posted_at, times);
    assert_eq!(game, {
        let mut expected = record_with_quarters(4);
        expected.posted_at = times;
        expected
    });
}

#[test]
fn post_scores_q2_needs_15_minutes_after_the_q1_post() {
    // PROGRAM §4.2: now ≥ posted_at[quarter − 2] + MIN_QUARTER_SECONDS.
    let f = Fixture::new();
    let after_q1 = record_with_quarters(1);
    let q1_at = after_q1.posted_at[0];
    expect_post(
        &f,
        q1_at + 899,
        &after_q1,
        2,
        14,
        10,
        false,
        false,
        Err(err(E::QuarterTooSoon)),
    );
    let result = expect_post(&f, q1_at + 900, &after_q1, 2, 14, 10, false, false, Ok(()));
    let game = decode_game(account_of(&result, &standard_game()));
    assert_eq!(game.quarters_posted, 2);
    assert_eq!(game.posted_at[1], q1_at + 900);
}

#[test]
fn post_scores_out_of_order_is_quarter_out_of_order() {
    // PROGRAM §4.2: quarter == quarters_posted + 1.
    let f = Fixture::new();
    let after_q1 = record_with_quarters(1);
    let now = after_q1.posted_at[0] + 3_600;
    for (record, quarter) in [
        (&after_q1, 1u8), // repeat
        (&after_q1, 3),   // skip
        (&after_q1, 0),
        (&after_q1, 5),
        (&after_q1, 255),
    ] {
        expect_post(
            &f,
            now,
            record,
            quarter,
            14,
            10,
            quarter == 4,
            false,
            Err(err(E::QuarterOutOfOrder)),
        );
    }
    let fresh = standard_record();
    for quarter in [0u8, 2, 4] {
        expect_post(
            &f,
            SCHEDULED + 3_600,
            &fresh,
            quarter,
            7,
            3,
            quarter == 4,
            false,
            Err(err(E::QuarterOutOfOrder)),
        );
    }
}

#[test]
fn post_scores_lower_than_the_previous_post_is_score_decreased() {
    // PROGRAM §4.2: home ≥ home_score[quarter − 2] and away ≥ away_score[quarter − 2].
    let f = Fixture::new();
    let after_q1 = record_with_quarters(1); // 7–3
    let now = after_q1.posted_at[0] + 3_600;
    expect_post(
        &f,
        now,
        &after_q1,
        2,
        6,
        10,
        false,
        false,
        Err(err(E::ScoreDecreased)),
    );
    expect_post(
        &f,
        now,
        &after_q1,
        2,
        14,
        2,
        false,
        false,
        Err(err(E::ScoreDecreased)),
    );
    // A scoreless quarter is fine.
    expect_post(&f, now, &after_q1, 2, 7, 3, false, false, Ok(()));
}

#[test]
fn post_scores_final_flag_must_match_the_fourth_quarter() {
    // PROGRAM §4.2: is_final true when quarter == 4 and false otherwise; had_overtime needs is_final.
    let f = Fixture::new();
    let after_q3 = record_with_quarters(3);
    expect_post(
        &f,
        after_q3.posted_at[2] + 3_600,
        &after_q3,
        4,
        24,
        20,
        false,
        false,
        Err(err(E::FinalFlagMismatch)),
    );
    for q in 1u8..=3 {
        let record = record_with_quarters(q - 1);
        let now = if q == 1 {
            SCHEDULED + 3_600
        } else {
            record.posted_at[usize::from(q) - 2] + 3_600
        };
        expect_post(
            &f,
            now,
            &record,
            q,
            30, // ≥ every fixture score, so only the flag can fail
            30,
            true,
            false,
            Err(err(E::FinalFlagMismatch)),
        );
    }
    let after_q1 = record_with_quarters(1);
    expect_post(
        &f,
        after_q1.posted_at[0] + 3_600,
        &after_q1,
        2,
        14,
        10,
        false,
        true,
        Err(err(E::FinalFlagMismatch)),
    );
    // Final without overtime is fine too.
    expect_post(
        &f,
        after_q3.posted_at[2] + 3_600,
        &after_q3,
        4,
        24,
        20,
        true,
        false,
        Ok(()),
    );
}

#[test]
fn post_scores_on_a_terminal_record_is_game_not_scheduled() {
    // PROGRAM §4.2: status is checked before order, so a Final record says GameNotScheduled
    // whatever the quarter; §9: terminal states are inert.
    let f = Fixture::new();
    let final_record = record_with_quarters(4);
    let now = final_record.posted_at[3] + 3_600;
    for quarter in [5u8, 1] {
        expect_post(
            &f,
            now,
            &final_record,
            quarter,
            30,
            27,
            quarter == 4,
            false,
            Err(err(E::GameNotScheduled)),
        );
    }
    for status in [GameStatus::Postponed, GameStatus::Suspended] {
        let mut record = standard_record();
        record.status = status;
        record.marked_at = T0;
        expect_post(
            &f,
            SCHEDULED + 3_600,
            &record,
            1,
            7,
            3,
            false,
            false,
            Err(err(E::GameNotScheduled)),
        );
    }
}

#[test]
fn post_scores_by_the_admin_or_a_stranger_is_unauthorized() {
    let f = Fixture::new();
    let m = mollusk_at(SCHEDULED + 3_600);
    for signer in [f.admin, f.stranger] {
        let ix = post_scores_ix(&signer, &standard_game(), 1, 7, 3, false, false);
        let result = m.process_and_validate_instruction(
            &ix,
            &game_accounts(&f, Some(&standard_record())),
            &[Check::err(custom(err(E::Unauthorized)))],
        );
        assert_eq!(
            decode_game(account_of(&result, &standard_game())),
            standard_record()
        );
    }
}

// ---------------------------------------------------------------------------
// mark_game
// ---------------------------------------------------------------------------

fn expect_mark(
    f: &Fixture,
    signer: &Pubkey,
    now: i64,
    record: &GameRecord,
    new_status: GameStatus,
    expect: Result<(), u32>,
) -> mollusk_svm::result::InstructionResult {
    let m = mollusk_at(now);
    let game = game_pda(&record.key, record.scheduled_kickoff).0;
    let ix = mark_game_ix(signer, &game, new_status);
    let check = match expect {
        Ok(()) => Check::success(),
        Err(code) => Check::err(custom(code)),
    };
    m.process_and_validate_instruction(&ix, &game_accounts(f, Some(record)), &[check])
}

#[test]
fn mark_game_sets_status_and_marked_at_and_emits_game_marked() {
    // PROGRAM §4.2: sets status and marked_at; emits GameMarked.
    let f = Fixture::new();
    for status in [
        GameStatus::Postponed,
        GameStatus::Cancelled,
        GameStatus::Suspended,
    ] {
        let result = expect_mark(&f, &f.admin, T0, &standard_record(), status, Ok(()));
        let game = decode_game(account_of(&result, &standard_game()));
        assert_eq!(game.status, status);
        assert_eq!(game.marked_at, T0);
        assert_eq!(game.recorded_kickoff, SCHEDULED);
        assert_eq!(game.quarters_posted, 0);
        let event: GameMarked = emitted_event(&result).expect("GameMarked");
        assert_eq!(event.time, T0);
        assert_eq!(event.game, to_a(&standard_game()));
        assert_eq!(event.status, status);
        assert_eq!(emitted_event_count(&result), 1);
    }
}

#[test]
fn mark_game_with_scheduled_or_final_is_invalid_game_status() {
    // PROGRAM §4.2: new_status ∈ {Postponed, Cancelled, Suspended}.
    let f = Fixture::new();
    for status in [GameStatus::Scheduled, GameStatus::Final] {
        let result = expect_mark(
            &f,
            &f.admin,
            T0,
            &standard_record(),
            status,
            Err(err(E::InvalidGameStatus)),
        );
        assert_eq!(
            decode_game(account_of(&result, &standard_game())),
            standard_record()
        );
    }
}

#[test]
fn mark_game_with_an_unknown_discriminant_does_not_deserialise() {
    // GameStatus is an Anchor enum: a byte outside 0–4 never reaches the handler
    // (Anchor 102, InstructionDidNotDeserialize).
    let f = Fixture::new();
    let m = mollusk_at(T0);
    let mut ix = mark_game_ix(&f.admin, &standard_game(), GameStatus::Suspended);
    let last = ix.data.len() - 1;
    ix.data[last] = 5;
    m.process_and_validate_instruction(
        &ix,
        &game_accounts(&f, Some(&standard_record())),
        &[Check::err(custom(102))],
    );
}

#[test]
fn mark_game_after_a_post_allows_only_suspended() {
    // PROGRAM §4.2, §9: Postponed and Cancelled only while quarters_posted == 0.
    let f = Fixture::new();
    let after_q1 = record_with_quarters(1);
    let now = after_q1.posted_at[0] + 60;
    for status in [GameStatus::Postponed, GameStatus::Cancelled] {
        let result = expect_mark(
            &f,
            &f.admin,
            now,
            &after_q1,
            status,
            Err(err(E::InvalidGameStatus)),
        );
        assert_eq!(decode_game(account_of(&result, &standard_game())), after_q1);
    }
    let result = expect_mark(&f, &f.admin, now, &after_q1, GameStatus::Suspended, Ok(()));
    let game = decode_game(account_of(&result, &standard_game()));
    assert_eq!(game.status, GameStatus::Suspended);
    assert_eq!(game.marked_at, now);
    assert_eq!(game.quarters_posted, 1);
    assert_eq!(game.home_score[0], 7);

    let after_q3 = record_with_quarters(3);
    expect_mark(
        &f,
        &f.admin,
        after_q3.posted_at[2] + 60,
        &after_q3,
        GameStatus::Suspended,
        Ok(()),
    );
}

#[test]
fn mark_game_on_a_marked_record_is_already_marked_and_on_final_is_not_scheduled() {
    // PROGRAM §4.2 "Irreversible".
    let f = Fixture::new();
    for status in [
        GameStatus::Postponed,
        GameStatus::Cancelled,
        GameStatus::Suspended,
    ] {
        let mut record = standard_record();
        record.status = status;
        record.marked_at = T0 - 60;
        for new_status in [GameStatus::Suspended, GameStatus::Cancelled] {
            let result = expect_mark(
                &f,
                &f.admin,
                T0,
                &record,
                new_status,
                Err(err(E::GameAlreadyMarked)),
            );
            assert_eq!(decode_game(account_of(&result, &standard_game())), record);
        }
    }
    let final_record = record_with_quarters(4);
    expect_mark(
        &f,
        &f.admin,
        final_record.posted_at[3] + 60,
        &final_record,
        GameStatus::Suspended,
        Err(err(E::GameNotScheduled)),
    );
}

#[test]
fn mark_game_by_the_keeper_or_a_stranger_is_unauthorized() {
    // PROGRAM §10 Authority: mark_game is admin-only.
    let f = Fixture::new();
    for signer in [f.keeper, f.stranger] {
        let result = expect_mark(
            &f,
            &signer,
            T0,
            &standard_record(),
            GameStatus::Postponed,
            Err(err(E::Unauthorized)),
        );
        assert_eq!(
            decode_game(account_of(&result, &standard_game())),
            standard_record()
        );
    }
}

#[test]
fn after_suspended_the_record_is_inert() {
    // PROGRAM §9: terminal.
    let f = Fixture::new();
    let marked = expect_mark(
        &f,
        &f.admin,
        T0,
        &standard_record(),
        GameStatus::Suspended,
        Ok(()),
    );
    let m = mollusk_at(SCHEDULED + 3_600);
    m.process_and_validate_instruction(
        &post_scores_ix(&f.keeper, &standard_game(), 1, 7, 3, false, false),
        &marked.resulting_accounts,
        &[Check::err(custom(err(E::GameNotScheduled)))],
    );
    let m = mollusk_at(T0 + 60);
    m.process_and_validate_instruction(
        &update_kickoff_ix(&f.keeper, &standard_game(), SCHEDULED + 3_600),
        &marked.resulting_accounts,
        &[Check::err(custom(err(E::GameNotScheduled)))],
    );
}

#[test]
fn require_scheduled_is_the_hook_for_create_pool() {
    let ok = standard_record();
    assert!(ok.require_scheduled().is_ok());
    for status in [
        GameStatus::Postponed,
        GameStatus::Cancelled,
        GameStatus::Suspended,
        GameStatus::Final,
    ] {
        let mut record = standard_record();
        record.status = status;
        let error = record.require_scheduled().unwrap_err();
        assert!(
            matches!(error, anchor_lang::error::Error::AnchorError(ref e) if e.error_code_number == err(E::GameNotScheduled)),
            "{status:?} must be GameNotScheduled, got {error:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Seeds re-derivation on the three post-creation contexts
// ---------------------------------------------------------------------------

#[test]
fn a_record_at_a_foreign_address_fails_the_seeds_check_on_every_instruction() {
    // The three post-creation contexts re-derive `game` from its own stored fields, so a copy of
    // a record planted at another address is rejected before the handler runs.
    let f = Fixture::new();
    let planted = Pubkey::new_unique();
    let mut accounts = game_accounts(&f, Some(&standard_record()));
    accounts.push((planted, game_record_account(&standard_record())));
    let m = mollusk_at(SCHEDULED + 3_600);
    for ix in [
        update_kickoff_ix(&f.keeper, &planted, SCHEDULED + 7_200),
        post_scores_ix(&f.keeper, &planted, 1, 7, 3, false, false),
        mark_game_ix(&f.admin, &planted, GameStatus::Suspended),
    ] {
        let result = m.process_instruction(&ix, &accounts);
        assert!(!result.program_result.is_ok(), "{ix:?}");
    }
}

// ---------------------------------------------------------------------------
// Composability (PROGRAM §10)
// ---------------------------------------------------------------------------

#[test]
fn post_scores_and_update_kickoff_are_unaffected_by_neighbouring_instructions() {
    let f = Fixture::new();
    let noop = solana_instruction::Instruction::new_with_bytes(
        Pubkey::default(),
        &[2u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0], // SystemInstruction::Transfer { lamports: 1 }
        vec![
            solana_instruction::AccountMeta::new(f.keeper, true),
            solana_instruction::AccountMeta::new(f.admin, false),
        ],
    );

    // update_kickoff before kickoff.
    let m = mollusk_at(T0);
    let accounts = game_accounts(&f, Some(&standard_record()));
    let update = update_kickoff_ix(&f.keeper, &standard_game(), SCHEDULED + 3_600);
    let alone = m.process_instruction_chain(&[update.clone()], &accounts);
    let sandwiched = m.process_instruction_chain(&[noop.clone(), update, noop.clone()], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_game(account_of(&alone, &standard_game())),
        decode_game(account_of(&sandwiched, &standard_game()))
    );

    // post_scores after kickoff.
    let m = mollusk_at(SCHEDULED + 3_600);
    let post = post_scores_ix(&f.keeper, &standard_game(), 1, 7, 3, false, false);
    let alone = m.process_instruction_chain(&[post.clone()], &accounts);
    let sandwiched = m.process_instruction_chain(&[noop.clone(), post, noop], &accounts);
    assert!(alone.program_result.is_ok() && sandwiched.program_result.is_ok());
    assert_eq!(
        decode_game(account_of(&alone, &standard_game())),
        decode_game(account_of(&sandwiched, &standard_game()))
    );
}
