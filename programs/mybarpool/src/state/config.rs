//! `PlatformConfig`, PROGRAM §3.1. One per deployment, seeds `["config"]`.

use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::MybarpoolError;

/// PROGRAM §3.1 `TokenRule`: one per token index (0 SOL, 1 SKR, 2 ORE). 98 bytes.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub struct TokenRule {
    /// Pools may be created in this token.
    pub enabled: bool,
    /// The mint; `Pubkey::default()` for SOL and for a token whose mint is not yet known.
    pub mint: Pubkey,
    /// Token or Token-2022; `Pubkey::default()` for SOL.
    pub token_program: Pubkey,
    /// Base-unit decimals: 9 SOL, 11 ORE, SKR per mint.
    pub decimals: u8,
    /// Minimum box price, base units; ≥ 1.
    pub min_price: u64,
    /// Price step, base units; ≥ 1; `(max_price − min_price) % step == 0`.
    pub step: u64,
    /// Maximum box price, base units; ≥ `min_price`.
    pub max_price: u64,
    /// Per-pool sponsorship cap, base units; initial `25 × max_price`.
    pub max_sponsorship: u64,
}

impl TokenRule {
    /// Serialized size: 1 + 32 + 32 + 1 + 8 × 4.
    pub const SIZE: usize = Self::INIT_SPACE;

    /// A rule with no mint: SOL (index 0) or a disabled placeholder.
    pub fn has_default_mint(&self) -> bool {
        self.mint == Pubkey::default()
    }

    /// PROGRAM §3.1 ladder invariants: `min_price ≥ 1`, `step ≥ 1`, `min_price ≤ max_price`,
    /// `(max_price − min_price) % step == 0`. `max_sponsorship` has no invariant beyond `u64`.
    pub fn validate_ladder(&self) -> Result<()> {
        require!(self.min_price >= 1, MybarpoolError::InvalidConfig);
        require!(self.step >= 1, MybarpoolError::InvalidConfig);
        require!(
            self.min_price <= self.max_price,
            MybarpoolError::InvalidConfig
        );
        let span = self
            .max_price
            .checked_sub(self.min_price)
            .ok_or(MybarpoolError::MathOverflow)?;
        let rem = span
            .checked_rem(self.step)
            .ok_or(MybarpoolError::MathOverflow)?;
        require!(rem == 0, MybarpoolError::InvalidConfig);
        Ok(())
    }

    /// Token-shape rules (PROGRAM §2 "Token index", §3.1 notes, §5.4), for token `index`.
    /// Index 0 is native SOL: default mint and program, 9 decimals. Any other index with a
    /// default mint is a disabled placeholder (how SKR ships until its mint is known) and must
    /// have a default program too. A non-default mint needs a non-default program.
    pub fn validate_shape(&self, index: usize) -> Result<()> {
        if index == 0 {
            require!(self.has_default_mint(), MybarpoolError::InvalidConfig);
            require!(
                self.token_program == Pubkey::default(),
                MybarpoolError::InvalidConfig
            );
            require!(self.decimals == SOL_DECIMALS, MybarpoolError::InvalidConfig);
        } else if self.has_default_mint() {
            require!(
                self.token_program == Pubkey::default(),
                MybarpoolError::InvalidConfig
            );
            require!(!self.enabled, MybarpoolError::InvalidConfig);
        } else {
            require!(
                self.token_program != Pubkey::default(),
                MybarpoolError::InvalidConfig
            );
        }
        Ok(())
    }
}

/// PROGRAM §3.1 `PlatformConfig`: the account the rest of the program reads. 698 bytes.
///
/// Layout (offsets from the start of the account data): discriminator 0 (8); `admin` 8;
/// `score_authority` 40; `entropy_provider` 72; `fee_wallet` 104; `platform_bps` 136;
/// `creator_bps` 138; `addon_budget_bps` 140; `default_preset` 142; `max_open_pools` 143;
/// `max_own_boxes` 144; `preseason_enabled` 145; `paused` 146; `tokens` 147 (three
/// `TokenRule` of 98 bytes at 147, 245, 343); `bump` 441; `reserved` 442 (256).
#[account]
#[derive(InitSpace, Debug, PartialEq, Eq)]
pub struct PlatformConfig {
    /// PROGRAM §3.1: the Squads vault on mainnet. May not be the default pubkey.
    pub admin: Pubkey,
    /// PROGRAM §3.1: the keeper's KMS key.
    pub score_authority: Pubkey,
    /// PROGRAM §3.1: the Entropy provider whose commits `set_var` accepts.
    pub entropy_provider: Pubkey,
    /// PROGRAM §3.1: platform fee destination (SOL directly; SPL to its ATA).
    pub fee_wallet: Pubkey,
    /// PROGRAM §3.1: ≤ PLATFORM_BPS_MAX; initial 500.
    pub platform_bps: u16,
    /// PROGRAM §3.1: ≤ CREATOR_BPS_MAX; initial 500.
    pub creator_bps: u16,
    /// PROGRAM §3.1: ≤ ADDON_BUDGET_BPS_MAX; initial 500.
    pub addon_budget_bps: u16,
    /// PROGRAM §3.1: which preset clients pre-select; initial 0.
    pub default_preset: u8,
    /// PROGRAM §3.1: open pools per creator per game; ≥ 1; initial 3.
    pub max_open_pools: u8,
    /// PROGRAM §3.1: creator's boxes in own pool; 1–MAX_OWN_BOXES_ABSOLUTE; initial 5.
    pub max_own_boxes: u8,
    /// PROGRAM §3.1: initial false; `create_game` rejects weeks ≥ 101 when false.
    pub preseason_enabled: bool,
    /// PROGRAM §3.1: when true, `create_pool`, `buy` and `sponsor` fail; nothing else is affected.
    pub paused: bool,
    /// PROGRAM §3.1: indexed by token index (0 SOL, 1 SKR, 2 ORE).
    pub tokens: [TokenRule; TOKEN_COUNT],
    /// PDA bump.
    pub bump: u8,
    /// Padding so fields can be appended without a migration.
    pub reserved: [u8; 256],
}

impl PlatformConfig {
    /// Account size: 8-byte discriminator + 690.
    pub const SIZE: usize = 8 + Self::INIT_SPACE;

    /// Byte offset of `tokens` within the account data (discriminator included).
    pub const TOKENS_OFFSET: usize = 147;

    /// PROGRAM §3.1 invariants, enforced on every write, judged on the whole config.
    /// `missing_mut_constraint` reads `self.tokens.iter()` as a write; `validate` is `&self`.
    #[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
    pub fn validate(&self) -> Result<()> {
        require!(
            self.admin != Pubkey::default(),
            MybarpoolError::InvalidConfig
        );
        require!(
            self.platform_bps <= PLATFORM_BPS_MAX,
            MybarpoolError::InvalidConfig
        );
        require!(
            self.creator_bps <= CREATOR_BPS_MAX,
            MybarpoolError::InvalidConfig
        );
        require!(
            self.addon_budget_bps <= ADDON_BUDGET_BPS_MAX,
            MybarpoolError::InvalidConfig
        );
        let total = self
            .platform_bps
            .checked_add(self.creator_bps)
            .and_then(|s| s.checked_add(self.addon_budget_bps))
            .ok_or(MybarpoolError::MathOverflow)?;
        require!(total <= TOTAL_BPS_MAX, MybarpoolError::InvalidConfig);
        PayoutPreset::try_from(self.default_preset)?;
        require!(self.max_open_pools >= 1, MybarpoolError::InvalidConfig);
        require!(self.max_own_boxes >= 1, MybarpoolError::InvalidConfig);
        require!(
            self.max_own_boxes <= MAX_OWN_BOXES_ABSOLUTE,
            MybarpoolError::InvalidConfig
        );
        for (index, rule) in self.tokens.iter().enumerate() {
            rule.validate_ladder()?;
            rule.validate_shape(index)?;
        }
        Ok(())
    }
}
