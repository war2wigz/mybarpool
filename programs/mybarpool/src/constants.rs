//! Hard-coded constants, PROGRAM §1. Changing any of these is a program
//! upgrade; nothing in `PlatformConfig` can raise a ceiling, only compare
//! against it (ARCHITECTURE › Open source, "Guarantees to integrators").

use anchor_lang::prelude::*;

/// PROGRAM §1 `BOXES`: boxes per pool.
#[constant]
pub const BOXES: u8 = 25;

/// PROGRAM §1 `LANES`: positions per axis; lane `l` holds digits at positions `l` and `l + 5`.
#[constant]
pub const LANES: u8 = 5;

/// PROGRAM §1 `QUARTERS`: settlement periods; the fourth is the final score.
#[constant]
pub const QUARTERS: u8 = 4;

/// PROGRAM §1 `PLATFORM_BPS_MAX`: ceiling on the platform share.
#[constant]
pub const PLATFORM_BPS_MAX: u16 = 500;

/// PROGRAM §1 `CREATOR_BPS_MAX`: ceiling on the creator base share.
#[constant]
pub const CREATOR_BPS_MAX: u16 = 500;

/// PROGRAM §1 `ADDON_BUDGET_BPS_MAX`: ceiling on `creator_addon_bps + integrator_bps`.
#[constant]
pub const ADDON_BUDGET_BPS_MAX: u16 = 500;

/// PROGRAM §1 `TOTAL_BPS_MAX`: ceiling on platform + creator base + add-on budget.
#[constant]
pub const TOTAL_BPS_MAX: u16 = 1500;

/// PROGRAM §1 `MIN_QUARTER_SECONDS`: a quarter's worth of football takes at least this long in real time.
#[constant]
pub const MIN_QUARTER_SECONDS: i64 = 900;

/// PROGRAM §1 `KICKOFF_UPDATE_BOUND`: 72 hours; how far a recorded kickoff may move from the scheduled one.
#[constant]
pub const KICKOFF_UPDATE_BOUND: i64 = 259_200;

/// PROGRAM §1 `RECLAIM_DELAY`: 30 days; abandoned-pool reclaim opens this long after the scheduled kickoff.
#[constant]
pub const RECLAIM_DELAY: i64 = 2_592_000;

/// PROGRAM §1 `MAX_OWN_BOXES_ABSOLUTE`: upper bound on any configured creator box cap.
#[constant]
pub const MAX_OWN_BOXES_ABSOLUTE: u8 = 25;

/// PROGRAM §1 `ENTROPY_PROGRAM`: Regolith Entropy.
#[constant]
pub const ENTROPY_PROGRAM: Pubkey = pubkey!("3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X");

/// PROGRAM §3.1 `PlatformConfig` seed.
#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

/// PROGRAM §3.6 `WalletOverride` seed prefix; the wallet follows.
#[constant]
pub const OVERRIDE_SEED: &[u8] = b"override";

/// Native SOL (PROGRAM §2 token index 0): nine decimals.
#[constant]
pub const SOL_DECIMALS: u8 = 9;

/// Number of token rules in the config (PROGRAM §2: SOL, SKR, ORE).
pub const TOKEN_COUNT: usize = 3;

/// PROGRAM §1 payout presets; the discriminant is the on-chain value.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PayoutPreset {
    /// Q1 20 / Q2 20 / Q3 20 / Final 40.
    Standard = 0,
    /// 25 / 25 / 25 / 25.
    Even = 1,
    /// Final 100.
    FinalOnly = 2,
}

impl TryFrom<u8> for PayoutPreset {
    type Error = anchor_lang::error::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(PayoutPreset::Standard),
            1 => Ok(PayoutPreset::Even),
            2 => Ok(PayoutPreset::FinalOnly),
            _ => Err(error!(crate::errors::MybarpoolError::InvalidPreset)),
        }
    }
}

/// PROGRAM §1 preset table in percent, indexed by `PayoutPreset as usize`.
#[constant]
pub const PRESET_SPLITS: [[u8; 4]; 3] = [[20, 20, 20, 40], [25, 25, 25, 25], [0, 0, 0, 100]];
