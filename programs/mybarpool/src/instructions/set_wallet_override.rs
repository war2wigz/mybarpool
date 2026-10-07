//! `set_wallet_override`, PROGRAM §4.1. Signer: admin. Creates or updates a
//! `WalletOverride` (§3.6).

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, OVERRIDE_SEED};
use crate::errors::MybarpoolError;
use crate::events::OverrideSet;
use crate::state::{PlatformConfig, WalletOverride};

#[derive(Accounts)]
#[instruction(wallet: Pubkey)]
#[event_cpi]
pub struct SetWalletOverride<'info> {
    /// `config.admin`; pays the rent on create.
    #[account(mut)]
    pub admin: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ MybarpoolError::Unauthorized,
    )]
    pub config: Account<'info, PlatformConfig>,
    /// PROGRAM §3.6 `WalletOverride` for `wallet`, created if absent.
    #[account(
        init_if_needed,
        payer = admin,
        space = WalletOverride::SIZE,
        seeds = [OVERRIDE_SEED, wallet.as_ref()],
        bump,
    )]
    pub wallet_override: Account<'info, WalletOverride>,
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` is non-deterministic here: the public Lints job passed on `b1a88d6`
/// and failed on byte-identical code at `c2fc985` (run 37553061357), naming `wallet_override`,
/// which *is* writable (`init_if_needed`; Anchor refuses the literal `mut` beside it, the token
/// the lint looks for). The lint reports from `check_fn`, so the allow has to sit on the
/// handler, not the context, to take effect.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_set_wallet_override(
    ctx: Context<SetWalletOverride>,
    wallet: Pubkey,
    max_open_pools: u8,
    max_own_boxes: u8,
) -> Result<()> {
    // The zero key is how a just-created override reads; an override for it would be
    // "created" on every call (Step 2 audit L1), and no wallet is the zero key anyway.
    require_keys_neq!(wallet, Pubkey::default(), MybarpoolError::InvalidConfig);
    let wallet_override = &mut ctx.accounts.wallet_override;
    let created = wallet_override.wallet == Pubkey::default();
    if created {
        wallet_override.wallet = wallet;
        wallet_override.bump = ctx.bumps.wallet_override;
    } else {
        require_keys_eq!(
            wallet_override.wallet,
            wallet,
            MybarpoolError::InvalidConfig
        );
        require!(
            wallet_override.bump == ctx.bumps.wallet_override,
            MybarpoolError::InvalidConfig
        );
    }
    wallet_override.max_open_pools = max_open_pools;
    wallet_override.max_own_boxes = max_own_boxes;
    wallet_override.validate()?;

    let time = Clock::get()?.unix_timestamp;
    emit_cpi!(OverrideSet {
        time,
        wallet,
        max_open_pools,
        max_own_boxes
    });
    Ok(())
}
