//! `close_counter`, PROGRAM §3.5: close a `CreatorCounter` at zero open
//! pools, rent to `fee_wallet`. Permissionless: no signer in the context.
//! Lands in Step 4 because its acceptance list requires the counter to
//! close at zero (BUILD-PLAN Step 6 is amended).

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, COUNTER_SEED};
use crate::errors::MybarpoolError;
use crate::state::{CreatorCounter, PlatformConfig};

#[derive(Accounts)]
pub struct CloseCounter<'info> {
    /// PROGRAM §3.5 `CreatorCounter`, re-derived from its own fields; closed to `fee_wallet`.
    #[account(
        mut,
        close = fee_wallet,
        seeds = [COUNTER_SEED, counter.creator.as_ref(), counter.game.as_ref()],
        bump = counter.bump,
    )]
    pub counter: Account<'info, CreatorCounter>,
    /// PROGRAM §3.1 `PlatformConfig`; names the rent destination.
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = fee_wallet)]
    pub config: Account<'info, PlatformConfig>,
    /// CHECK: `has_one = fee_wallet` on `config`; a lamport destination only.
    /// `config.fee_wallet` by `has_one`; a destination only, never `SystemAccount`
    /// (PROGRAM §10: a wallet whose owner may change must still be payable).
    #[account(mut)]
    pub fee_wallet: UncheckedAccount<'info>,
}

pub fn handle_close_counter(ctx: Context<CloseCounter>) -> Result<()> {
    require!(
        ctx.accounts.counter.open_count == 0,
        MybarpoolError::CounterNotEmpty
    );
    Ok(())
}
