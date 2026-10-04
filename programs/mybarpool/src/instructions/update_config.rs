//! `update_config`, PROGRAM §4.1. Signer: admin. Any subset of fields; the
//! §3.1 invariants are judged on the whole resulting config.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::constants::{CONFIG_SEED, TOKEN_COUNT};
use crate::errors::MybarpoolError;
use crate::events::ConfigUpdated;
use crate::instructions::mint_check::check_mint;
use crate::state::{PlatformConfig, TokenRule};

/// Every PROGRAM §3.1 field as an option; `None` leaves it unchanged.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct UpdateConfigParams {
    /// New `admin`; takes effect for the next instruction. May not be the default pubkey.
    pub admin: Option<Pubkey>,
    /// PROGRAM §3.1 `score_authority`.
    pub score_authority: Option<Pubkey>,
    /// PROGRAM §3.1 `entropy_provider`.
    pub entropy_provider: Option<Pubkey>,
    /// PROGRAM §3.1 `fee_wallet`.
    pub fee_wallet: Option<Pubkey>,
    /// PROGRAM §3.1 `platform_bps`; up or down, never above PLATFORM_BPS_MAX.
    pub platform_bps: Option<u16>,
    /// PROGRAM §3.1 `creator_bps`; never above CREATOR_BPS_MAX.
    pub creator_bps: Option<u16>,
    /// PROGRAM §3.1 `addon_budget_bps`; never above ADDON_BUDGET_BPS_MAX.
    pub addon_budget_bps: Option<u16>,
    /// PROGRAM §3.1 `default_preset`.
    pub default_preset: Option<u8>,
    /// PROGRAM §3.1 `max_open_pools`.
    pub max_open_pools: Option<u8>,
    /// PROGRAM §3.1 `max_own_boxes`.
    pub max_own_boxes: Option<u8>,
    /// PROGRAM §3.1 `preseason_enabled`.
    pub preseason_enabled: Option<bool>,
    /// PROGRAM §3.1 `paused`.
    pub paused: Option<bool>,
    /// PROGRAM §3.1 `tokens`, by token index; `Some` replaces that rule whole.
    pub tokens: [Option<TokenRule>; TOKEN_COUNT],
}

#[derive(Accounts)]
#[event_cpi]
pub struct UpdateConfig<'info> {
    /// `config.admin`.
    pub admin: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`.
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ MybarpoolError::Unauthorized,
    )]
    pub config: Account<'info, PlatformConfig>,
    /// Mint for token index 1 (SKR), required when `tokens[1]` is `Some` with a non-default mint.
    pub mint_1: Option<InterfaceAccount<'info, Mint>>,
    /// Mint for token index 2 (ORE), required when `tokens[2]` is `Some` with a non-default mint.
    pub mint_2: Option<InterfaceAccount<'info, Mint>>,
}

/// `missing_mut_constraint` reads `ctx.accounts.mint_1.as_ref()` as a write; the mints are
/// read-only and `config` (declared `mut`) is the only account written.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_update_config(ctx: Context<UpdateConfig>, params: UpdateConfigParams) -> Result<()> {
    let config = &mut ctx.accounts.config;
    if let Some(v) = params.admin {
        config.admin = v;
    }
    if let Some(v) = params.score_authority {
        config.score_authority = v;
    }
    if let Some(v) = params.entropy_provider {
        config.entropy_provider = v;
    }
    if let Some(v) = params.fee_wallet {
        config.fee_wallet = v;
    }
    if let Some(v) = params.platform_bps {
        config.platform_bps = v;
    }
    if let Some(v) = params.creator_bps {
        config.creator_bps = v;
    }
    if let Some(v) = params.addon_budget_bps {
        config.addon_budget_bps = v;
    }
    if let Some(v) = params.default_preset {
        config.default_preset = v;
    }
    if let Some(v) = params.max_open_pools {
        config.max_open_pools = v;
    }
    if let Some(v) = params.max_own_boxes {
        config.max_own_boxes = v;
    }
    if let Some(v) = params.preseason_enabled {
        config.preseason_enabled = v;
    }
    if let Some(v) = params.paused {
        config.paused = v;
    }
    for (slot, rule) in config.tokens.iter_mut().zip(params.tokens.iter()) {
        if let Some(r) = rule {
            *slot = *r;
        }
    }

    config.validate()?;
    if params.tokens[1].is_some() {
        check_mint(&config.tokens[1], ctx.accounts.mint_1.as_ref())?;
    }
    if params.tokens[2].is_some() {
        check_mint(&config.tokens[2], ctx.accounts.mint_2.as_ref())?;
    }

    let time = Clock::get()?.unix_timestamp;
    emit_cpi!(ConfigUpdated::from_config(config, time));
    Ok(())
}
