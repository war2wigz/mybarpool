//! `close_wallet_override`, PROGRAM §4.1. Signer: admin; rent to admin.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, OVERRIDE_SEED};
use crate::errors::MybarpoolError;
use crate::events::OverrideClosed;
use crate::state::{PlatformConfig, WalletOverride};

#[derive(Accounts)]
#[instruction(wallet: Pubkey)]
#[event_cpi]
pub struct CloseWalletOverride<'info> {
    /// `config.admin`; receives the rent.
    #[account(mut)]
    pub admin: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ MybarpoolError::Unauthorized,
    )]
    pub config: Account<'info, PlatformConfig>,
    /// PROGRAM §3.6 `WalletOverride` for `wallet`; closed, rent to admin.
    #[account(
        mut,
        seeds = [OVERRIDE_SEED, wallet.as_ref()],
        bump = wallet_override.bump,
        close = admin,
        constraint = wallet_override.wallet == wallet @ MybarpoolError::InvalidConfig,
    )]
    pub wallet_override: Account<'info, WalletOverride>,
}

pub fn handle_close_wallet_override(
    ctx: Context<CloseWalletOverride>,
    wallet: Pubkey,
) -> Result<()> {
    let closed = &ctx.accounts.wallet_override;
    let time = Clock::get()?.unix_timestamp;
    emit_cpi!(OverrideClosed {
        time,
        wallet,
        max_open_pools: closed.max_open_pools,
        max_own_boxes: closed.max_own_boxes,
    });
    Ok(())
}
