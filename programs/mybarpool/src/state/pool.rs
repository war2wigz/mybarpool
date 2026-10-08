//! `Pool`, PROGRAM §3.3, with its two enums (`PoolStatus`, `AccessType`).
//! The first account that holds money: the vault (§3.4) is a PDA of the pool
//! and the pool PDA is the vault's only authority. `owners` is written by
//! `assign_boxes` (§6.1) and nothing else.

use anchor_lang::prelude::*;

use crate::assignment::assign_boxes;
use crate::constants::{PayoutPreset, BOXES, QUARTERS};
use crate::errors::MybarpoolError;

/// PROGRAM §3.3 `PoolStatus`; the discriminant is the on-chain byte. PROGRAM §9: `Settled`,
/// `Returned` and `Split` are terminal. "Live" is not a state: clients derive it as `Drawn`
/// with `now ≥ game.recorded_kickoff`.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Debug, PartialEq, Eq)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PoolStatus {
    /// Selling; fewer than 25 boxes sold.
    Open = 0,
    /// All 25 boxes sold; waiting for the draw.
    Locked = 1,
    /// Axes drawn; settles quarter by quarter after kickoff.
    Drawn = 2,
    /// All four quarters paid.
    Settled = 3,
    /// Every purchase and sponsorship returned.
    Returned = 4,
    /// Unpaid prizes split equally across the 25 boxes.
    Split = 5,
}

/// PROGRAM §3.3 `AccessType`; the discriminant is the on-chain byte. Step 4 creates `Public`
/// pools only; `Link` and `Allowlist` gating lands in Step 8.
#[derive(AnchorSerialize, AnchorDeserialize, InitSpace, Clone, Copy, Debug, PartialEq, Eq)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum AccessType {
    /// Anyone may buy.
    Public = 0,
    /// `buy` needs `gate_key` as a co-signer.
    Link = 1,
    /// `buy` needs a Merkle proof against `allowlist_root`.
    Allowlist = 2,
}

/// PROGRAM §3.3 `Pool`. 1,442 bytes = 8 + 1,434.
///
/// Seeds `["pool", game_record, creator, nonce u64 LE]`; `nonce` is chosen by the client so one
/// creator can have several pools on one game.
///
/// Layout (offsets from the start of the account data): `game` 8; `creator` 40; `nonce` 72;
/// `token` 80; `mint` 81; `token_program` 113; `vault` 145; `price` 177; `preset` 185;
/// `access_type` 186; `gate_key` 187; `allowlist_root` 219; `creator_addon_bps` 251;
/// `integrator` 253; `integrator_bps` 285; `platform_fee` 287; `creator_fee` 295;
/// `integrator_fee` 303; `status` 311; `sold` 312; `owners` 313 (25 × 32); `creator_boxes` 1113;
/// `sponsored_total` 1114; `sponsor_count` 1122; `sponsorships_open` 1124; `var` 1126;
/// `var_end_at` 1158; `sampled_slot` 1166; `sampled_hash` 1174; `var_replacements` 1206;
/// `drawn` 1207; `home_axis` 1208; `away_axis` 1218; `prize_pool` 1228; `quarter_prize` 1236
/// (4 × 8); `quarters_settled` 1268; `winning_box` 1269; `fees_paid` 1273;
/// `unpaid_prize_pool` 1274; `returned` 1282; `split_amount` 1286; `cancelled_by_admin` 1294;
/// `abandoned` 1295; `created_at` 1296; `locked_at` 1304; `bump` 1312; `vault_bump` 1313;
/// `var_commit` 1314; `reserved` 1346 (96).
#[account]
#[derive(InitSpace, Debug, PartialEq, Eq)]
pub struct Pool {
    /// The `GameRecord` (PROGRAM §3.2); part of the seeds.
    pub game: Pubkey,
    /// The creator; part of the seeds; receives `creator_fee` at the first settlement.
    pub creator: Pubkey,
    /// Client-chosen; part of the seeds.
    pub nonce: u64,
    /// PROGRAM §2 token index: 0 SOL, 1 SKR, 2 ORE.
    pub token: u8,
    /// Copied from `config.tokens[token]` at creation; `Pubkey::default()` for SOL.
    pub mint: Pubkey,
    /// Copied from `config.tokens[token]` at creation; `Pubkey::default()` for SOL.
    pub token_program: Pubkey,
    /// The vault (PROGRAM §3.4) at `["vault", pool]`.
    pub vault: Pubkey,
    /// Per box, base units; on the token's ladder at creation.
    pub price: u64,
    /// PROGRAM §1 payout preset.
    pub preset: PayoutPreset,
    /// PROGRAM §3.3 access type.
    pub access_type: AccessType,
    /// Co-signer for `Link` pools; default otherwise.
    pub gate_key: Pubkey,
    /// Merkle root for `Allowlist` pools; zero otherwise.
    pub allowlist_root: [u8; 32],
    /// 0–500: extra fee to the creator on top of the base share.
    pub creator_addon_bps: u16,
    /// The client that created the pool, when it takes a fee; default when unset.
    pub integrator: Pubkey,
    /// 0–500; 0 when `integrator` is unset.
    pub integrator_bps: u16,
    /// PROGRAM §5.1, fixed at creation.
    pub platform_fee: u64,
    /// PROGRAM §5.1, fixed at creation: base and add-on in one amount.
    pub creator_fee: u64,
    /// PROGRAM §5.1, fixed at creation; 0 when unset.
    pub integrator_fee: u64,
    /// PROGRAM §3.3 / §9 status.
    pub status: PoolStatus,
    /// Boxes sold, 0–25.
    pub sold: u8,
    /// Box owners by 0-based index; `Pubkey::default()` is unsold.
    pub owners: [Pubkey; BOXES as usize],
    /// Boxes the creator holds in this pool, against the creator cap.
    pub creator_boxes: u8,
    /// Sum of all sponsorships (PROGRAM §3.7); never part of the fee base.
    pub sponsored_total: u64,
    /// Distinct sponsor wallets ever.
    pub sponsor_count: u16,
    /// `Sponsorship` accounts not yet closed.
    pub sponsorships_open: u16,
    /// Entropy `Var` recorded at lock (Step 5); default until then.
    pub var: Pubkey,
    /// The `Var`'s `end_at` (Step 5); 0 until then.
    pub var_end_at: u64,
    /// Slot at which `sample_var` ran (Step 5); 0 until then.
    pub sampled_slot: u64,
    /// The slot hash `sample_var` verified against (Step 5); zero until then.
    pub sampled_hash: [u8; 32],
    /// Admin `replace_var` calls so far (Step 5); max 2.
    pub var_replacements: u8,
    /// Axes drawn (Step 5).
    pub drawn: bool,
    /// Home (column) digits after the draw; lane `l` = positions `l`, `l + 5`.
    pub home_axis: [u8; 10],
    /// Away (row) digits after the draw.
    pub away_axis: [u8; 10],
    /// Fixed at the first settlement (Step 6); 0 before.
    pub prize_pool: u64,
    /// Fixed at the first settlement (Step 6).
    pub quarter_prize: [u64; QUARTERS as usize],
    /// 0–4 (Step 6).
    pub quarters_settled: u8,
    /// Winning box per quarter; `NO_WINNING_BOX` (255) until settled.
    pub winning_box: [u8; QUARTERS as usize],
    /// True once the first non-zero prize has been paid (Step 6).
    pub fees_paid: bool,
    /// `prize_pool` minus prizes paid so far (Step 6).
    pub unpaid_prize_pool: u64,
    /// Bitmap over boxes: returned, split or reclaimed (Step 7).
    pub returned: u32,
    /// Per-box amount fixed when the pool enters `Split` or is first reclaimed after a payout (Step 7).
    pub split_amount: u64,
    /// Set by `cancel_pool` (Step 7).
    pub cancelled_by_admin: bool,
    /// Set by the first `reclaim` or `reclaim_sponsorship` (Step 7).
    pub abandoned: bool,
    /// Unix time of `create_pool`.
    pub created_at: i64,
    /// Unix time the 25th box sold; 0 until locked.
    pub locked_at: i64,
    /// PDA bump of the pool.
    pub bump: u8,
    /// PDA bump of the vault.
    pub vault_bump: u8,
    /// PROGRAM §3.3 (Step 5b): the `Var`'s `commit` as recorded by `set_var` / `replace_var`;
    /// `draw` requires `keccak(var.seed)` to equal it. Taken out of `reserved` so no earlier
    /// offset moves. Zero on an unbound pool.
    pub var_commit: [u8; 32],
    /// PROGRAM §3 preamble: padding so fields can be appended without a migration (128
    /// through Step 5).
    pub reserved: [u8; 96],
}

impl Pool {
    /// Account size: 8-byte discriminator + 1,434.
    pub const SIZE: usize = 8 + Self::INIT_SPACE;

    /// `status == Open`, else `PoolNotOpen` (PROGRAM §4.3 `buy`).
    pub fn require_open(&self) -> Result<()> {
        require!(self.status == PoolStatus::Open, MybarpoolError::PoolNotOpen);
        Ok(())
    }

    /// `status == Locked`, else `PoolNotLocked` (PROGRAM §4.4: the four draw instructions).
    pub fn require_locked(&self) -> Result<()> {
        require!(
            self.status == PoolStatus::Locked,
            MybarpoolError::PoolNotLocked
        );
        Ok(())
    }

    /// `status == Drawn`, else `PoolNotDrawn` (PROGRAM §4.5 `settle`; every other status,
    /// `Settled` included).
    pub fn require_drawn(&self) -> Result<()> {
        require!(
            self.status == PoolStatus::Drawn,
            MybarpoolError::PoolNotDrawn
        );
        Ok(())
    }

    /// `status ∈ {Settled, Returned, Split}`, else `PoolNotTerminal` (PROGRAM §4.5
    /// `close_pool`, §9 "Terminal").
    pub fn require_terminal(&self) -> Result<()> {
        require!(
            matches!(
                self.status,
                PoolStatus::Settled | PoolStatus::Returned | PoolStatus::Split
            ),
            MybarpoolError::PoolNotTerminal
        );
        Ok(())
    }

    /// The pool's payout split (PROGRAM §1 preset table).
    pub fn split(&self) -> [u8; QUARTERS as usize] {
        self.preset.split()
    }

    /// `status ∈ {Open, Locked, Drawn}`, else `PoolNotOpen` (PROGRAM §4.3 `sponsor`, §9: a full
    /// pool is usually drawn before kickoff and can still be sponsored).
    pub fn require_sponsorable(&self) -> Result<()> {
        require!(
            matches!(
                self.status,
                PoolStatus::Open | PoolStatus::Locked | PoolStatus::Drawn
            ),
            MybarpoolError::PoolNotOpen
        );
        Ok(())
    }

    /// PROGRAM §6.1 over this pool's `owners`: the only writer of `owners`. Returns the assigned
    /// 0-based indices in assignment order; `sold` is read from the pool and not advanced here.
    pub fn assign_boxes(
        &mut self,
        buyer: &Pubkey,
        count: u8,
        slothash: &[u8; 32],
    ) -> Result<Vec<u8>> {
        assign_boxes(slothash, buyer, self.sold, count, &mut self.owners)
    }
}
