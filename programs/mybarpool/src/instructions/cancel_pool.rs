//! `cancel_pool`, PROGRAM §4.6: the admin cancels a pool that has paid nobody
//! yet. The pool goes to `Returned` with `cancelled_by_admin` set, which is the
//! precondition `return_boxes` reads for every later batch; no money moves here.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, COUNTER_SEED, POOL_SEED};
use crate::errors::MybarpoolError;
use crate::events::PoolCancelled;
use crate::state::{CreatorCounter, PlatformConfig, Pool, PoolStatus};

#[derive(Accounts)]
#[event_cpi]
pub struct CancelPool<'info> {
    /// `config.admin`.
    pub admin: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; `has_one = admin`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ MybarpoolError::Unauthorized,
    )]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields; `Open`, `Locked` or `Drawn` with
    /// fees untaken.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// PROGRAM §3.5 the creator's counter for this game; required and decremented exactly when
    /// the pool is `Open`, never read otherwise.
    #[account(
        mut,
        seeds = [COUNTER_SEED, pool.creator.as_ref(), pool.game.as_ref()],
        bump = counter.bump,
    )]
    pub counter: Option<Account<'info, CreatorCounter>>,
}

pub fn handle_cancel_pool(ctx: Context<CancelPool>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let CancelPool { pool, counter, .. } = ctx.accounts;

    // PROGRAM §4.6: only a pool that has paid nobody can be cancelled.
    require!(
        matches!(
            pool.status,
            PoolStatus::Open | PoolStatus::Locked | PoolStatus::Drawn
        ),
        MybarpoolError::NotReturnable
    );
    require!(!pool.fees_paid, MybarpoolError::FeesAlreadyPaid);

    pool.cancelled_by_admin = true;
    if pool.status == PoolStatus::Open {
        counter
            .as_mut()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?
            .decrement()?;
    }
    pool.status = PoolStatus::Returned;

    emit_cpi!(PoolCancelled {
        time: now,
        pool: pool.key(),
    });
    Ok(())
}
