//! MyBarPool: NFL boxes on Solana.
//!
//! Build plan Steps 2–5: `PlatformConfig`, `WalletOverride`, every PROGRAM
//! §1 constant, the complete §8 error enum, the four §4.1 admin
//! instructions, `GameRecord` with the four §4.2 game instructions, `Pool`,
//! its vault, `CreatorCounter` and `Sponsorship` with the §4.3 instructions
//! that fill them, the §4.4 draw over an Entropy `Var` on the platform's own
//! deployment, and (Step 6) the §4.5 settlement — `settle` and `close_pool`,
//! the first instructions that move money out, every destination verified
//! against the pool or the config in the same instruction, and (Step 7) the
//! §4.6 returns — `return_boxes`, `return_sponsorship`, `cancel_pool`,
//! `split`, `reclaim`, `reclaim_sponsorship` and `close_sponsorship`, every
//! return the full purchase price or the recorded sponsorship, every
//! destination read from the pool or the `Sponsorship`, never from the
//! caller — each built and audited against `docs/PROGRAM.md`.
#![allow(unexpected_cfgs)]

pub mod allowlist;
pub mod assignment;
pub mod axes;
pub mod constants;
pub mod entropy;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod money;
pub mod payout;
pub mod slot_hashes;
pub mod state;
pub mod vault;
pub mod winner;

use anchor_lang::prelude::*;

pub use constants::*;
pub use errors::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("3jk4YM9xnpoMYK3nuaUHZ59SCDxwExfT9EHxwP2wRMBw");

#[program]
pub mod mybarpool {
    use super::*;

    /// PROGRAM §4.1 `initialize`: create `PlatformConfig`. Signer: the program's upgrade
    /// authority, which becomes `admin`. Emits `ConfigUpdated`.
    pub fn initialize(ctx: Context<Initialize>, params: InitializeParams) -> Result<()> {
        instructions::initialize::handle_initialize(ctx, params)
    }

    /// PROGRAM §4.1 `update_config`: any subset of config fields; §3.1 invariants judged on the
    /// whole result. Signer: admin. Emits `ConfigUpdated`.
    pub fn update_config(ctx: Context<UpdateConfig>, params: UpdateConfigParams) -> Result<()> {
        instructions::update_config::handle_update_config(ctx, params)
    }

    /// PROGRAM §4.1 `set_wallet_override`: create or update a `WalletOverride`. Signer: admin.
    /// Emits `OverrideSet`.
    pub fn set_wallet_override(
        ctx: Context<SetWalletOverride>,
        wallet: Pubkey,
        max_open_pools: u8,
        max_own_boxes: u8,
    ) -> Result<()> {
        instructions::set_wallet_override::handle_set_wallet_override(
            ctx,
            wallet,
            max_open_pools,
            max_own_boxes,
        )
    }

    /// PROGRAM §4.1 `close_wallet_override`: close it, rent to admin. Signer: admin.
    /// Emits `OverrideClosed`.
    pub fn close_wallet_override(ctx: Context<CloseWalletOverride>, wallet: Pubkey) -> Result<()> {
        instructions::close_wallet_override::handle_close_wallet_override(ctx, wallet)
    }

    /// PROGRAM §4.2 `create_game`: create the `GameRecord` for `key` at `scheduled_kickoff`
    /// (must be in the future; §2 key rules, preseason only with `preseason_enabled`).
    /// Signer and payer: the keeper. Emits `GameCreated`.
    pub fn create_game(
        ctx: Context<CreateGame>,
        key: GameKey,
        scheduled_kickoff: i64,
    ) -> Result<()> {
        instructions::create_game::handle_create_game(ctx, key, scheduled_kickoff)
    }

    /// PROGRAM §4.2 `update_kickoff`: move `recorded_kickoff` to `new_time` while the record is
    /// `Scheduled`, nothing is posted, the recorded kickoff is still ahead, `new_time` is in the
    /// future and within 72 hours of the scheduled kickoff. Signer: the keeper. Emits
    /// `KickoffUpdated`.
    pub fn update_kickoff(ctx: Context<UpdateKickoff>, new_time: i64) -> Result<()> {
        instructions::update_kickoff::handle_update_kickoff(ctx, new_time)
    }

    /// PROGRAM §4.2 `post_scores`: post the cumulative score at the end of `quarter` (1–4, in
    /// order, at least 15 minutes after kickoff or the previous post, never decreasing;
    /// `is_final` on the fourth only). The fourth post sets `Final`. Signer: the keeper. Emits
    /// `ScoresPosted`.
    pub fn post_scores(
        ctx: Context<PostScores>,
        quarter: u8,
        home: u16,
        away: u16,
        is_final: bool,
        had_overtime: bool,
    ) -> Result<()> {
        instructions::post_scores::handle_post_scores(
            ctx,
            quarter,
            home,
            away,
            is_final,
            had_overtime,
        )
    }

    /// PROGRAM §4.2 `mark_game`: mark a `Scheduled` record `Postponed` or `Cancelled` (only
    /// before any post) or `Suspended` (any time before `Final`). Irreversible. Signer: the
    /// admin. Emits `GameMarked`.
    pub fn mark_game(ctx: Context<MarkGame>, new_status: GameStatus) -> Result<()> {
        instructions::mark_game::handle_mark_game(ctx, new_status)
    }

    /// PROGRAM §4.3 `create_pool`: create a `Pool`, its vault and (when absent) the creator's
    /// counter; fix the §5.1 fee amounts; optionally buy the creator's first boxes. Signer and
    /// payer: the creator. Emits `PoolCreated`, then `BoxesBought` and `PoolLocked` as earned.
    pub fn create_pool(ctx: Context<CreatePool>, params: CreatePoolParams) -> Result<()> {
        instructions::create_pool::handle_create_pool(ctx, params)
    }

    /// PROGRAM §4.3 `buy`: buy `count` boxes at the pool's price; positions are assigned by
    /// §6.1 from the SlotHashes sysvar. The 25th box locks the pool. Signer and payer: the
    /// buyer. Gating by `access_type`: a `Link` pool needs the optional `gate_key` account
    /// present and signing; an `Allowlist` pool needs `allowlist_proof`, the §6.4 Merkle proof
    /// for the buyer (at most 32 entries); a `Public` pool ignores both. Emits `BoxesBought`
    /// and, on lock, `PoolLocked`.
    pub fn buy(ctx: Context<Buy>, count: u8, allowlist_proof: Vec<[u8; 32]>) -> Result<()> {
        instructions::buy::handle_buy(ctx, count, allowlist_proof)
    }

    /// PROGRAM §4.3 `sponsor`: add `amount` (≥ one box price, within the token's cap) to the
    /// prize pool of an `Open`, `Locked` or `Drawn` pool before kickoff. Signer and payer: the
    /// sponsor. Emits `Sponsored`.
    pub fn sponsor(ctx: Context<Sponsor>, amount: u64) -> Result<()> {
        instructions::sponsor::handle_sponsor(ctx, amount)
    }

    /// PROGRAM §4.3 `rotate_gate_key`: set a `Link` pool's gate key. Signer: the creator.
    /// Emits `GateKeyRotated`.
    pub fn rotate_gate_key(ctx: Context<RotateGateKey>, new_key: Pubkey) -> Result<()> {
        instructions::rotate_gate_key::handle_rotate_gate_key(ctx, new_key)
    }

    /// PROGRAM §3.5 `close_counter`: close a `CreatorCounter` whose `open_count` is zero, rent to
    /// `config.fee_wallet`. Permissionless.
    pub fn close_counter(ctx: Context<CloseCounter>) -> Result<()> {
        instructions::close_counter::handle_close_counter(ctx)
    }

    /// PROGRAM §4.4 `set_var`: bind a fresh, committed, unsampled Entropy `Var` from the
    /// configured provider to a `Locked` pool. Signer: the keeper. Emits `VarSet`.
    pub fn set_var(ctx: Context<SetVar>) -> Result<()> {
        instructions::set_var::handle_set_var(ctx)
    }

    /// PROGRAM §4.4 `sample_var`: at or after the `Var`'s `end_at`, CPI Entropy `Sample` if
    /// unsampled, then prove against SlotHashes that the hash it carries is the real hash of
    /// `end_at`, and record slot and hash on the pool. Signer: anyone. Emits `VarSampled`.
    pub fn sample_var(ctx: Context<SampleVar>) -> Result<()> {
        instructions::sample_var::handle_sample_var(ctx)
    }

    /// PROGRAM §4.4 `draw`: on a sampled, revealed `Var` that still carries the verified hash
    /// (and not the fallback), recompute the value and derive both axes (§6.2). Signer: the
    /// keeper. Emits `DigitsDrawn`.
    pub fn draw(ctx: Context<Draw>) -> Result<()> {
        instructions::draw::handle_draw(ctx)
    }

    /// PROGRAM §4.4 `replace_var`: bind a fresh `Var` in place of one whose window was missed;
    /// at most twice, never on a pool with a verified sample. Signer: the admin. Emits
    /// `VarReplaced`.
    pub fn replace_var(ctx: Context<ReplaceVar>) -> Result<()> {
        instructions::replace_var::handle_replace_var(ctx)
    }

    /// PROGRAM §4.5 `settle(quarter)`: compute the quarter's winning box from the pool's axes
    /// and the game's cumulative score (§6.3), verify the passed winner is that box's owner,
    /// fix the prizes on the first call (§5.2), pay the quarter's prize, and with the first
    /// non-zero prize pay the three fees (§5.1). Signer: the keeper. Emits `QuarterSettled`.
    pub fn settle(ctx: Context<Settle>, quarter: u8) -> Result<()> {
        instructions::settle::handle_settle(ctx, quarter)
    }

    /// PROGRAM §4.5 `close_pool`: close a terminal pool with no open sponsorship, sweeping the
    /// vault's remaining balance and both accounts' rent to `fee_wallet` (the creator when
    /// abandoned). Permissionless. Emits `PoolClosed`.
    /// `arbitrary_cpi_call` is allowed here for the same reason as on the handler (see
    /// `instructions/close_pool.rs`): the lint attaches its report to the function it started from,
    /// and this entry point reaches the vault's outbound CPIs through the handler.
    #[cfg_attr(dylint_lib = "arbitrary_cpi_call", allow(arbitrary_cpi_call))]
    pub fn close_pool(ctx: Context<ClosePool>) -> Result<()> {
        instructions::close_pool::handle_close_pool(ctx)
    }

    /// PROGRAM §4.6 `return_boxes`: on a pool that is unfilled at kickoff, on a game marked
    /// postponed, cancelled or suspended before any payout, or on a pool the admin cancelled,
    /// move the pool to `Returned` and pay the owners in the remaining accounts the purchase
    /// price of their unreturned boxes.
    pub fn return_boxes<'info>(ctx: Context<'info, ReturnBoxes<'info>>) -> Result<()> {
        instructions::return_boxes::handle_return_boxes(ctx)
    }

    /// PROGRAM §4.6 `return_sponsorship`: on a `Returned` pool with its fees untaken, return
    /// one sponsorship in full to the recorded wallet and close its account.
    /// `arbitrary_cpi_call` is allowed here for the same reason as on the handler (see
    /// `instructions/return_sponsorship.rs`): the lint attaches its report to the function it started from,
    /// and this entry point reaches the vault's outbound CPIs through the handler.
    #[cfg_attr(dylint_lib = "arbitrary_cpi_call", allow(arbitrary_cpi_call))]
    pub fn return_sponsorship(ctx: Context<ReturnSponsorship>) -> Result<()> {
        instructions::return_sponsorship::handle_return_sponsorship(ctx)
    }

    /// PROGRAM §4.6 `cancel_pool`: the admin cancels a pool that has paid nobody; it becomes
    /// `Returned` with `cancelled_by_admin` set, and `return_boxes` pays the owners.
    pub fn cancel_pool(ctx: Context<CancelPool>) -> Result<()> {
        instructions::cancel_pool::handle_cancel_pool(ctx)
    }

    /// PROGRAM §4.6 `split`: on a suspended game after a prize was paid, fix
    /// `split_amount = unpaid_prize_pool / 25`, move the pool to `Split` and pay the owners in
    /// the remaining accounts that share per unreturned box.
    /// `arbitrary_cpi_call` is allowed here for the same reason as on the handler (see
    /// `instructions/split.rs`): the lint attaches its report to the function it started from,
    /// and this entry point reaches the vault's outbound CPIs through the handler.
    #[cfg_attr(dylint_lib = "arbitrary_cpi_call", allow(arbitrary_cpi_call))]
    pub fn split<'info>(ctx: Context<'info, Split<'info>>) -> Result<()> {
        instructions::split::handle_split(ctx)
    }

    /// PROGRAM §4.6 `reclaim`: thirty days after the scheduled kickoff, a box owner takes back
    /// their unreturned boxes (price while fees are untaken, split share once they are); an
    /// unresolved pool becomes `abandoned`.
    pub fn reclaim(ctx: Context<Reclaim>) -> Result<()> {
        instructions::reclaim::handle_reclaim(ctx)
    }

    /// PROGRAM §4.6 `reclaim_sponsorship`: thirty days after the scheduled kickoff, a sponsor
    /// of a pool with its fees untaken takes their sponsorship back and closes its account.
    /// `arbitrary_cpi_call` is allowed here for the same reason as on the handler (see
    /// `instructions/reclaim_sponsorship.rs`): the lint attaches its report to the function it started from,
    /// and this entry point reaches the vault's outbound CPIs through the handler.
    #[cfg_attr(dylint_lib = "arbitrary_cpi_call", allow(arbitrary_cpi_call))]
    pub fn reclaim_sponsorship(ctx: Context<ReclaimSponsorship>) -> Result<()> {
        instructions::reclaim_sponsorship::handle_reclaim_sponsorship(ctx)
    }

    /// PROGRAM §4.6 `close_sponsorship`: on a `Settled` or `Split` pool, close a spent
    /// sponsorship's account to its wallet.
    pub fn close_sponsorship(ctx: Context<CloseSponsorship>) -> Result<()> {
        instructions::close_sponsorship::handle_close_sponsorship(ctx)
    }
}
