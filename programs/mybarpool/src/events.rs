//! Events, PROGRAM §7. Emitted with `emit_cpi!` (a self-CPI), never `emit!`,
//! so a result is never truncated out of the logs. Fields are only ever
//! appended. Every event carries the Unix time from the Clock sysvar.

use anchor_lang::prelude::*;

use crate::constants::{PayoutPreset, TOKEN_COUNT};
use crate::state::{AccessType, GameKey, GameStatus, PlatformConfig, TokenRule};

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

/// PROGRAM §7 `PoolCreated`: `create_pool` wrote a `Pool`. Exactly the §7 fields: `nonce` and
/// `vault` derive from the pool address and the IDL seeds.
#[event]
pub struct PoolCreated {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The `GameRecord`.
    pub game: Pubkey,
    /// The creator.
    pub creator: Pubkey,
    /// PROGRAM §2 token index.
    pub token: u8,
    /// The mint; default for SOL.
    pub mint: Pubkey,
    /// Per box, base units.
    pub price: u64,
    /// PROGRAM §1 payout preset.
    pub preset: PayoutPreset,
    /// PROGRAM §3.3 access type.
    pub access_type: AccessType,
    /// 0–500.
    pub creator_addon_bps: u16,
    /// Default when unset.
    pub integrator: Pubkey,
    /// 0–500; 0 when unset.
    pub integrator_bps: u16,
    /// PROGRAM §5.1.
    pub platform_fee: u64,
    /// PROGRAM §5.1.
    pub creator_fee: u64,
    /// PROGRAM §5.1.
    pub integrator_fee: u64,
}

/// PROGRAM §7 `BoxesBought`: a purchase landed (by `buy`, or by `create_pool` for the creator).
#[event]
pub struct BoxesBought {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The buyer.
    pub buyer: Pubkey,
    /// 0-based box indices in assignment order (labels 1–25 are made in `packages/shared`).
    pub boxes: Vec<u8>,
    /// Boxes bought.
    pub count: u8,
    /// `pool.sold` after this purchase.
    pub sold_after: u8,
}

/// PROGRAM §7 `PoolLocked`: the 25th box sold.
#[event]
pub struct PoolLocked {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// `pool.locked_at`, the same instant.
    pub locked_at: i64,
}

/// PROGRAM §7 `Sponsored`: a `sponsor` call landed.
#[event]
pub struct Sponsored {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The sponsor wallet.
    pub sponsor: Pubkey,
    /// This call's amount, base units.
    pub amount: u64,
    /// `pool.sponsored_total` after this call.
    pub sponsored_total: u64,
}

/// PROGRAM §7 `GateKeyRotated`: the creator rotated a `Link` pool's gate key. The new key is
/// public information; only signatures from it matter, so it is not in the event.
#[event]
pub struct GateKeyRotated {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
}

/// PROGRAM §7 `VarSet`: `set_var` bound a `Var` to the pool.
#[event]
pub struct VarSet {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The `Var` bound.
    pub var: Pubkey,
    /// The `Var`'s `end_at`, the slot whose hash will be sampled.
    pub end_at: u64,
    /// The `Var`'s `commit`, now recorded on the pool (Step 5b).
    pub commit: [u8; 32],
}

/// PROGRAM §7 `VarSampled`: `sample_var` recorded a hash it matched against SlotHashes.
#[event]
pub struct VarSampled {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The `Var`.
    pub var: Pubkey,
    /// Whoever signed `sample_var` (anyone may).
    pub sampler: Pubkey,
    /// The slot `sample_var` ran in.
    pub slot: u64,
    /// The `Var`'s `end_at`.
    pub end_at: u64,
    /// The hash recorded, equal to the SlotHashes entry for `end_at`.
    pub slot_hash: [u8; 32],
}

/// PROGRAM §7 `VarReplaced`: the admin bound a replacement `Var`.
#[event]
pub struct VarReplaced {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The `Var` abandoned (its address stays in history).
    pub old_var: Pubkey,
    /// The `Var` bound instead.
    pub new_var: Pubkey,
    /// The new `Var`'s `end_at`.
    pub end_at: u64,
    /// `pool.var_replacements` after this call (1 or 2).
    pub replacements: u8,
    /// The replacement's `commit`, now recorded on the pool (Step 5b).
    pub commit: [u8; 32],
}

/// PROGRAM §7 `QuarterSettled`: one quarter paid (or recorded, on a zero share). The three fee
/// fields are the amounts moved in **this** call — the pool's stored fees when `fees_paid_now`,
/// zero otherwise (the stored amounts are in `PoolCreated`). Clients show a box as won only on
/// this event.
#[event]
pub struct QuarterSettled {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// 1–4.
    pub quarter: u8,
    /// Cumulative home score at the end of the quarter (PROGRAM §3.2).
    pub home: u16,
    /// Cumulative away score.
    pub away: u16,
    /// The winning box, 0-based (PROGRAM §6.3). Named `box_index` here because `box` is a Rust
    /// keyword and Anchor would carry the raw identifier `r#box` into the IDL; §7's `box`.
    pub box_index: u8,
    /// The box's owner at settlement, who was paid `amount`.
    pub winner: Pubkey,
    /// `quarter_prize[quarter − 1]`; 0 on a zero-share quarter.
    pub amount: u64,
    /// True on the call that moved the fees (the first non-zero prize).
    pub fees_paid_now: bool,
    /// `platform_fee` moved in this call, else 0.
    pub platform_fee: u64,
    /// `creator_fee` moved in this call, else 0.
    pub creator_fee: u64,
    /// `integrator_fee` moved in this call, else 0.
    pub integrator_fee: u64,
}

/// PROGRAM §7 `PoolClosed`: a terminal pool's vault and account closed to `destination`.
#[event]
pub struct PoolClosed {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address (now closed).
    pub pool: Pubkey,
    /// `fee_wallet`, or the creator on an abandoned pool.
    pub destination: Pubkey,
    /// The amount swept: the SPL vault's token balance; for SOL, the lamports above the vault's
    /// own rent-exempt minimum (that minimum and the pool's rent go to the same destination but
    /// are rent, not dust).
    pub dust: u64,
}

/// PROGRAM §7 `BoxesReturned`: `return_boxes` paid one owner the purchase price of their
/// unreturned boxes (§4.6).
#[event]
pub struct BoxesReturned {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The box owner paid.
    pub owner: Pubkey,
    /// The boxes paid in this call, 0-based, ascending.
    pub boxes: Vec<u8>,
    /// The sum paid in this call.
    pub amount: u64,
}

/// PROGRAM §7 `BoxesSplit`: `split` paid one owner `split_amount` per unreturned box (§4.6).
#[event]
pub struct BoxesSplit {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The box owner paid.
    pub owner: Pubkey,
    /// The boxes paid in this call, 0-based, ascending.
    pub boxes: Vec<u8>,
    /// The sum paid in this call.
    pub amount: u64,
}

/// PROGRAM §7 `BoxesReclaimed`: a box owner took their own boxes back after the 30-day clock
/// (§4.6 `reclaim`).
#[event]
pub struct BoxesReclaimed {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The signer, paid for their own boxes.
    pub owner: Pubkey,
    /// The boxes paid in this call, 0-based, ascending.
    pub boxes: Vec<u8>,
    /// The sum paid in this call.
    pub amount: u64,
}

/// PROGRAM §7 `SponsorshipReturned`: a sponsorship paid back in full to its wallet and its
/// account closed (`return_sponsorship` or `reclaim_sponsorship`, §4.6).
#[event]
pub struct SponsorshipReturned {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// `sponsorship.wallet`, the only possible destination.
    pub sponsor: Pubkey,
    /// The amount returned.
    pub amount: u64,
}

/// PROGRAM §7 `SponsorshipClosed`: a committed sponsorship's account closed on a terminal pool
/// (`close_sponsorship`, §4.6); the amount stays in the pool.
#[event]
pub struct SponsorshipClosed {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// `sponsorship.wallet`, who receives the account's rent.
    pub sponsor: Pubkey,
    /// The committed amount, informational.
    pub amount: u64,
}

/// PROGRAM §7 `PoolCancelled`: the admin moved an unpaid pool to `Returned` (§4.6 `cancel_pool`).
#[event]
pub struct PoolCancelled {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
}

/// PROGRAM §7 `DigitsDrawn`: `draw` derived both axes from the `Var`'s value.
#[event]
pub struct DigitsDrawn {
    /// Unix time of the write.
    pub time: i64,
    /// The pool's address.
    pub pool: Pubkey,
    /// The `Var` drawn on.
    pub var: Pubkey,
    /// The revealed Entropy value.
    pub value: [u8; 32],
    /// Home (column) digits; lane `l` = positions `l`, `l + 5`.
    pub home_axis: [u8; 10],
    /// Away (row) digits.
    pub away_axis: [u8; 10],
}
