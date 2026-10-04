//! `WalletOverride`, PROGRAM §3.6. Seeds `["override", wallet]`; admin-created.
//! When present it replaces both config limits for that wallet.

use anchor_lang::prelude::*;

use crate::constants::MAX_OWN_BOXES_ABSOLUTE;
use crate::errors::MybarpoolError;

/// PROGRAM §3.6 `WalletOverride`: per-wallet creator limits. 43 bytes.
///
/// Layout: discriminator 0 (8); `wallet` 8 (32); `max_open_pools` 40; `max_own_boxes` 41;
/// `bump` 42. No `reserved` field (ARCHITECTURE › Open source names the pool and config
/// accounts as the padded ones).
#[account]
#[derive(InitSpace, Debug, PartialEq, Eq)]
pub struct WalletOverride {
    /// The wallet these limits apply to; also the PDA seed.
    pub wallet: Pubkey,
    /// Open pools per game for this wallet; ≥ 1.
    pub max_open_pools: u8,
    /// Boxes in its own pool for this wallet; 1–MAX_OWN_BOXES_ABSOLUTE.
    pub max_own_boxes: u8,
    /// PDA bump.
    pub bump: u8,
}

impl WalletOverride {
    /// Account size: 8-byte discriminator + 35.
    pub const SIZE: usize = 8 + Self::INIT_SPACE;

    /// `max_open_pools ≥ 1`, `1 ≤ max_own_boxes ≤ MAX_OWN_BOXES_ABSOLUTE` (PROGRAM §3.6, §3.1).
    pub fn validate(&self) -> Result<()> {
        require!(self.max_open_pools >= 1, MybarpoolError::InvalidConfig);
        require!(self.max_own_boxes >= 1, MybarpoolError::InvalidConfig);
        require!(
            self.max_own_boxes <= MAX_OWN_BOXES_ABSOLUTE,
            MybarpoolError::InvalidConfig
        );
        Ok(())
    }
}
