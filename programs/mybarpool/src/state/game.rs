//! `GameRecord`, PROGRAM §3.2, and the two types it is built from: the
//! canonical game key (§2) and the game status (§3.2, §9). One record per
//! scheduled game, seeded by the key *and* the scheduled kickoff, so a
//! rescheduled game gets a fresh record (ARCHITECTURE › Games). Created and
//! rent-paid by the keeper; never closed.

use anchor_lang::prelude::*;

use crate::constants::QUARTERS;
use crate::errors::MybarpoolError;

/// PROGRAM §2 game key: season, week, home, away. Five bytes on the wire (`season` u16 LE,
/// then `week`, `home`, `away`), the encoding `encodeGameKey` in `@mybarpool/shared` produces.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameKey {
    /// The year the regular season starts in (2026 for the 2026–27 season).
    pub season: u16,
    /// 1–18 regular season; 19 Wild Card, 20 Divisional, 21 Conference Championships,
    /// 22 Super Bowl. Preseason, when enabled, is 101–103 so it can never collide.
    pub week: u8,
    /// Home team: index 0–31 into the frozen team table (PROGRAM §2).
    pub home: u8,
    /// Away team: index 0–31 into the frozen team table (PROGRAM §2).
    pub away: u8,
}

/// PROGRAM §2 week bounds.
pub const REGULAR_SEASON_FIRST_WEEK: u8 = 1;
/// PROGRAM §2: week 22 is the Super Bowl, the last postseason week.
pub const SUPER_BOWL_WEEK: u8 = 22;
/// PROGRAM §2: preseason weeks are 101–103.
pub const PRESEASON_FIRST_WEEK: u8 = 101;
/// PROGRAM §2: preseason weeks are 101–103.
pub const PRESEASON_LAST_WEEK: u8 = 103;
/// PROGRAM §2: team indices are 0–31.
pub const TEAM_COUNT: u8 = 32;

impl GameKey {
    /// Serialised size: 2 + 1 + 1 + 1.
    pub const SIZE: usize = Self::INIT_SPACE;

    /// PROGRAM §4.2 `create_game` key rules: `week` 1–22, or 101–103 only with
    /// `preseason_enabled`; `home != away`; both < 32. Every violation is `InvalidGameKey`.
    ///
    /// `missing_mut_constraint` treats `self` as an Accounts struct and the `self.away` read in
    /// the comparison as a write (the same false positive Step 2 saw on `PlatformConfig::validate`).
    #[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
    pub fn validate(&self, preseason_enabled: bool) -> Result<()> {
        let regular_or_postseason =
            (REGULAR_SEASON_FIRST_WEEK..=SUPER_BOWL_WEEK).contains(&self.week);
        let preseason = (PRESEASON_FIRST_WEEK..=PRESEASON_LAST_WEEK).contains(&self.week);
        require!(
            regular_or_postseason || (preseason && preseason_enabled),
            MybarpoolError::InvalidGameKey
        );
        require!(self.home < TEAM_COUNT, MybarpoolError::InvalidGameKey);
        require!(self.away < TEAM_COUNT, MybarpoolError::InvalidGameKey);
        require!(self.home != self.away, MybarpoolError::InvalidGameKey);
        Ok(())
    }
}

/// PROGRAM §3.2 `GameStatus`; the discriminant is the on-chain byte. PROGRAM §9: `Scheduled`
/// is the only live state; the other four are terminal.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Debug, PartialEq, Eq)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum GameStatus {
    /// Created and not yet final or marked; sales, kickoff updates and posts are possible.
    Scheduled = 0,
    /// Marked by the admin before any post; pools are returned (ARCHITECTURE › Returns and splits).
    Postponed = 1,
    /// Marked by the admin before any post; pools are returned.
    Cancelled = 2,
    /// Marked by the admin at any time before `Final`; unpaid prizes are split.
    Suspended = 3,
    /// Set by the fourth `post_scores`; the final score is on the record.
    Final = 4,
}

/// PROGRAM §3.2 `GameRecord`. 153 bytes = 8 + 145.
///
/// Seeds `["game", season u16 LE, week u8, home u8, away u8, scheduled_kickoff i64 LE]`, the
/// six `gameRecordSeeds` in `@mybarpool/shared` produces, in that order.
///
/// Layout: discriminator 0 (8); `key` 8 (5: `season` 8, `week` 10, `home` 11, `away` 12);
/// `scheduled_kickoff` 13; `recorded_kickoff` 21; `status` 29; `quarters_posted` 30;
/// `home_score` 31 (4 × u16); `away_score` 39 (4 × u16); `posted_at` 47 (4 × i64);
/// `final_had_overtime` 79; `marked_at` 80; `bump` 88; `reserved` 89 (64).
#[account]
#[derive(InitSpace, Debug, PartialEq, Eq)]
pub struct GameRecord {
    /// PROGRAM §2 game key; part of the seeds.
    pub key: GameKey,
    /// Seed value; never changes. The 72-hour update bound and the 30-day reclaim clock are
    /// measured from this, never from `recorded_kickoff` (ARCHITECTURE › Trust model).
    pub scheduled_kickoff: i64,
    /// Starts equal to `scheduled_kickoff`; moved by `update_kickoff`. Sales close at this time.
    pub recorded_kickoff: i64,
    /// PROGRAM §3.2 `GameStatus`, one byte on-chain.
    pub status: GameStatus,
    /// 0–4: how many `post_scores` have landed.
    pub quarters_posted: u8,
    /// Cumulative home score at the end of Q1..Q3 and final (ARCHITECTURE › Payouts).
    pub home_score: [u16; QUARTERS as usize],
    /// Same for away.
    pub away_score: [u16; QUARTERS as usize],
    /// Unix time each post landed; 0 until it does.
    pub posted_at: [i64; QUARTERS as usize],
    /// Informational, set on the final post.
    pub final_had_overtime: bool,
    /// Unix time `mark_game` ran; 0 if never.
    pub marked_at: i64,
    /// PDA bump.
    pub bump: u8,
    /// PROGRAM §3 preamble: padding so fields can be appended without a migration.
    pub reserved: [u8; 64],
}

impl GameRecord {
    /// Account size: 8-byte discriminator + 145.
    pub const SIZE: usize = 8 + Self::INIT_SPACE;

    /// `status == Scheduled`, else `GameNotScheduled` (PROGRAM §4.2; §9 terminal states are
    /// inert). Every instruction that writes a record, and Step 4's `create_pool`, calls this
    /// before anything else.
    pub fn require_scheduled(&self) -> Result<()> {
        require!(
            self.status == GameStatus::Scheduled,
            MybarpoolError::GameNotScheduled
        );
        Ok(())
    }
}
