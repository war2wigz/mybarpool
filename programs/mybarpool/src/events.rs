//! Events, PROGRAM §7. Emitted with `emit_cpi!` (a self-CPI), never `emit!`,
//! so a result is never truncated out of the logs. Fields are only ever
//! appended. Every event carries the Unix time from the Clock sysvar.

use anchor_lang::prelude::*;

use crate::constants::TOKEN_COUNT;
use crate::state::{GameKey, GameStatus, PlatformConfig, TokenRule};

/// PROGRAM §7 `ConfigUpdated`: the full config snapshot after `initialize` or `update_config`.
#[event]
pub struct ConfigUpdated {
    /// Unix time of the write.
    pub time: i64,
    /// PROGRAM §3.1 `admin`.
    pub admin: Pubkey,
    /// PROGRAM §3.1 `score_authority`.
    pub score_authority: Pubkey,
    /// PROGRAM §3.1 `entropy_provider`.
    pub entropy_provider: Pubkey,
    /// PROGRAM §3.1 `fee_wallet`.
    pub fee_wallet: Pubkey,
    /// PROGRAM §3.1 `platform_bps`.
    pub platform_bps: u16,
    /// PROGRAM §3.1 `creator_bps`.
    pub creator_bps: u16,
    /// PROGRAM §3.1 `addon_budget_bps`.
    pub addon_budget_bps: u16,
    /// PROGRAM §3.1 `default_preset`.
    pub default_preset: u8,
    /// PROGRAM §3.1 `max_open_pools`.
    pub max_open_pools: u8,
    /// PROGRAM §3.1 `max_own_boxes`.
    pub max_own_boxes: u8,
    /// PROGRAM §3.1 `preseason_enabled`.
    pub preseason_enabled: bool,
    /// PROGRAM §3.1 `paused`.
    pub paused: bool,
    /// PROGRAM §3.1 `tokens`.
    pub tokens: [TokenRule; TOKEN_COUNT],
}

impl ConfigUpdated {
    pub fn from_config(config: &PlatformConfig, time: i64) -> Self {
        Self {
            time,
            admin: config.admin,
            score_authority: config.score_authority,
            entropy_provider: config.entropy_provider,
            fee_wallet: config.fee_wallet,
            platform_bps: config.platform_bps,
            creator_bps: config.creator_bps,
            addon_budget_bps: config.addon_budget_bps,
            default_preset: config.default_preset,
            max_open_pools: config.max_open_pools,
            max_own_boxes: config.max_own_boxes,
            preseason_enabled: config.preseason_enabled,
            paused: config.paused,
            tokens: config.tokens,
        }
    }
}

/// PROGRAM §7 `OverrideSet`: a `WalletOverride` was created or updated.
#[event]
pub struct OverrideSet {
    /// Unix time of the write.
    pub time: i64,
    /// The wallet the limits apply to.
    pub wallet: Pubkey,
    /// PROGRAM §3.6 `max_open_pools`.
    pub max_open_pools: u8,
    /// PROGRAM §3.6 `max_own_boxes`.
    pub max_own_boxes: u8,
}

/// PROGRAM §7 `OverrideClosed`: a `WalletOverride` was closed; carries the values that were
/// closed so an indexer needs no prior state.
#[event]
pub struct OverrideClosed {
    /// Unix time of the close.
    pub time: i64,
    /// The wallet whose override was closed.
    pub wallet: Pubkey,
    /// The `max_open_pools` that was in force.
    pub max_open_pools: u8,
    /// The `max_own_boxes` that was in force.
    pub max_own_boxes: u8,
}

/// PROGRAM §7 `GameCreated`: a `GameRecord` was created by `create_game`.
#[event]
pub struct GameCreated {
    /// Unix time of the write.
    pub time: i64,
    /// The record's address.
    pub game: Pubkey,
    /// PROGRAM §2 game key.
    pub key: GameKey,
    /// PROGRAM §3.2 `scheduled_kickoff`; also `recorded_kickoff` at creation.
    pub scheduled_kickoff: i64,
}

/// PROGRAM §7 `KickoffUpdated`: `update_kickoff` moved `recorded_kickoff`.
#[event]
pub struct KickoffUpdated {
    /// Unix time of the write.
    pub time: i64,
    /// The record's address.
    pub game: Pubkey,
    /// `recorded_kickoff` before the update.
    pub old: i64,
    /// `recorded_kickoff` after the update.
    pub new: i64,
}

/// PROGRAM §7 `ScoresPosted`: `post_scores` landed a quarter.
#[event]
pub struct ScoresPosted {
    /// Unix time of the write.
    pub time: i64,
    /// The record's address.
    pub game: Pubkey,
    /// 1–4; 4 is the final score.
    pub quarter: u8,
    /// Cumulative home score.
    pub home: u16,
    /// Cumulative away score.
    pub away: u16,
    /// True on the fourth post only.
    pub is_final: bool,
    /// Informational; only ever true with `is_final`.
    pub had_overtime: bool,
}

/// PROGRAM §7 `GameMarked`: the admin marked the record `Postponed`, `Cancelled` or `Suspended`.
#[event]
pub struct GameMarked {
    /// Unix time of the write.
    pub time: i64,
    /// The record's address.
    pub game: Pubkey,
    /// The new status.
    pub status: GameStatus,
}
