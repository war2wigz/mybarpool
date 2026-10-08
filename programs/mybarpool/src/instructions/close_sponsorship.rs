//! `close_sponsorship`, PROGRAM §4.6: once a pool is `Settled` or `Split`, a
//! sponsorship has been spent inside the prize pool and its account is only
//! rent; anyone closes it to the sponsor's wallet so `close_pool` can follow.

use anchor_lang::prelude::*;

use crate::constants::{POOL_SEED, SPONSORSHIP_SEED};
use crate::errors::MybarpoolError;
use crate::events::SponsorshipClosed;
use crate::state::{Pool, PoolStatus, Sponsorship};

#[derive(Accounts)]
#[event_cpi]
pub struct CloseSponsorship<'info> {
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields; `Settled` or `Split`.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// PROGRAM §3.7 `Sponsorship` for this pool and `sponsor`; closed to `sponsor`.
    #[account(
        mut,
        seeds = [SPONSORSHIP_SEED, pool.key().as_ref(), sponsorship.wallet.as_ref()],
        bump = sponsorship.bump,
        has_one = pool,
        close = sponsor,
    )]
    pub sponsorship: Account<'info, Sponsorship>,
    /// CHECK: `address = sponsorship.wallet`, the rent's only destination.
    #[account(mut, address = sponsorship.wallet)]
    pub sponsor: UncheckedAccount<'info>,
}

pub fn handle_close_sponsorship(ctx: Context<CloseSponsorship>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let CloseSponsorship {
        pool,
        sponsorship,
        sponsor,
        ..
    } = ctx.accounts;

    // PROGRAM §4.6: only a Settled or Split pool has spent its sponsorships.
    require!(
        matches!(pool.status, PoolStatus::Settled | PoolStatus::Split),
        MybarpoolError::PoolNotTerminal
    );
    pool.sponsorships_open = pool
        .sponsorships_open
        .checked_sub(1)
        .ok_or(MybarpoolError::MathOverflow)?;

    emit_cpi!(SponsorshipClosed {
        time: now,
        pool: pool.key(),
        sponsor: sponsor.key(),
        amount: sponsorship.amount,
    });
    Ok(())
}
