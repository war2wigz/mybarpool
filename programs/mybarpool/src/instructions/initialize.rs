//! `initialize`, PROGRAM §4.1. Signer: the deploying key (the program's upgrade
//! authority), which becomes `admin` until changed.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;
use anchor_spl::token_interface::Mint;

use crate::constants::{CONFIG_SEED, TOKEN_COUNT};
use crate::errors::MybarpoolError;
use crate::events::ConfigUpdated;
use crate::instructions::mint_check::check_mint;
use crate::state::{PlatformConfig, TokenRule};

/// Every PROGRAM §3.1 field except `admin` (the signer), `bump` and `reserved`.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct InitializeParams {
    /// PROGRAM §3.1 `score_authority`: the keeper's key.
    pub score_authority: Pubkey,
    /// PROGRAM §3.1 `entropy_provider`.
    pub entropy_provider: Pubkey,
    /// PROGRAM §3.1 `fee_wallet`.
    pub fee_wallet: Pubkey,
    /// PROGRAM §3.1 `platform_bps`; ≤ PLATFORM_BPS_MAX.
    pub platform_bps: u16,
    /// PROGRAM §3.1 `creator_bps`; ≤ CREATOR_BPS_MAX.
    pub creator_bps: u16,
    /// PROGRAM §3.1 `addon_budget_bps`; ≤ ADDON_BUDGET_BPS_MAX.
    pub addon_budget_bps: u16,
    /// PROGRAM §3.1 `default_preset`; 0, 1 or 2.
    pub default_preset: u8,
    /// PROGRAM §3.1 `max_open_pools`; ≥ 1.
    pub max_open_pools: u8,
    /// PROGRAM §3.1 `max_own_boxes`; 1–MAX_OWN_BOXES_ABSOLUTE.
    pub max_own_boxes: u8,
    /// PROGRAM §3.1 `preseason_enabled`.
    pub preseason_enabled: bool,
    /// PROGRAM §3.1 `paused`.
    pub paused: bool,
    /// PROGRAM §3.1 `tokens`, by token index (0 SOL, 1 SKR, 2 ORE).
    pub tokens: [TokenRule; TOKEN_COUNT],
}

#[derive(Accounts)]
#[event_cpi]
pub struct Initialize<'info> {
    /// The program's upgrade authority; pays for the config and becomes `admin`.
    #[account(mut)]
    pub admin: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`, created here. One per deployment.
    #[account(
        init,
        payer = admin,
        space = PlatformConfig::SIZE,
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Account<'info, PlatformConfig>,
    /// The program's `ProgramData`; its upgrade authority must be the signer.
    #[account(
        seeds = [crate::ID.as_ref()],
        seeds::program = bpf_loader_upgradeable::ID,
        bump,
        constraint = program_data.upgrade_authority_address == Some(admin.key()) @ MybarpoolError::Unauthorized,
    )]
    pub program_data: Account<'info, ProgramData>,
    /// Mint for token index 1 (SKR), when its rule names one.
    pub mint_1: Option<InterfaceAccount<'info, Mint>>,
    /// Mint for token index 2 (ORE), when its rule names one.
    pub mint_2: Option<InterfaceAccount<'info, Mint>>,
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` reads `ctx.accounts.mint_1.as_ref()` as a write; the mints are
/// read-only and `config` is the only account written (it is `init`).
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_initialize(ctx: Context<Initialize>, params: InitializeParams) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.score_authority = params.score_authority;
    config.entropy_provider = params.entropy_provider;
    config.fee_wallet = params.fee_wallet;
    config.platform_bps = params.platform_bps;
    config.creator_bps = params.creator_bps;
    config.addon_budget_bps = params.addon_budget_bps;
    config.default_preset = params.default_preset;
    config.max_open_pools = params.max_open_pools;
    config.max_own_boxes = params.max_own_boxes;
    config.preseason_enabled = params.preseason_enabled;
    config.paused = params.paused;
    config.tokens = params.tokens;
    config.bump = ctx.bumps.config;
    config.reserved = [0u8; 256];

    config.validate()?;
    check_mint(&config.tokens[1], ctx.accounts.mint_1.as_ref())?;
    check_mint(&config.tokens[2], ctx.accounts.mint_2.as_ref())?;

    let time = Clock::get()?.unix_timestamp;
    emit_cpi!(ConfigUpdated::from_config(config, time));
    Ok(())
}
