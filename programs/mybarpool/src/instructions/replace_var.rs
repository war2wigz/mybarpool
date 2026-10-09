//! `replace_var`, PROGRAM §4.4. Signer: the admin. The capped escape hatch for
//! a missed sample window: binds a fresh `Var` in place of one that was never
//! sampled through this program. Never callable by the keeper; a pool with a
//! verified sample is never re-rolled.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, ENTROPY_PROGRAM, POOL_SEED, VAR_REPLACEMENTS_MAX};
use crate::entropy::Var;
use crate::errors::MybarpoolError;
use crate::events::VarReplaced;
use crate::instructions::set_var::require_fresh;
use crate::state::{PlatformConfig, Pool};

#[derive(Accounts)]
#[event_cpi]
pub struct ReplaceVar<'info> {
    /// `config.admin`.
    pub admin: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; read for `entropy_provider`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ MybarpoolError::Unauthorized,
    )]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: owner `ENTROPY_PROGRAM` by constraint; every `set_var` check by the handler.
    /// The fresh `Var` bound in place of `pool.var`.
    #[account(owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy)]
    pub new_var: UncheckedAccount<'info>,
}

pub fn handle_replace_var(ctx: Context<ReplaceVar>) -> Result<()> {
    let clock = Clock::get()?;
    let ReplaceVar {
        config,
        pool,
        new_var,
        ..
    } = ctx.accounts;

    // PROGRAM §4.4 checks, in order.
    pool.require_locked()?;
    require!(!pool.drawn, MybarpoolError::AlreadyDrawn);
    // PROGRAM §4.4: the admin replaces a Var, never binds the first one; a pool with no Var has
    // no window to miss, and the first binding is the keeper's `set_var` (Step 5 audit L1).
    require_keys_neq!(pool.var, Pubkey::default(), MybarpoolError::VarNotSet);
    require!(
        pool.var_replacements < VAR_REPLACEMENTS_MAX,
        MybarpoolError::TooManyVarReplacements
    );
    // A Var with a verified sample is never abandoned: its value is fixed and only the reveal
    // is outstanding. The only replacement case left is a missed window.
    require!(pool.sampled_slot == 0, MybarpoolError::VarAlreadySampled);
    require_keys_neq!(new_var.key(), pool.var, MybarpoolError::VarAlreadySet);
    let v = Var::try_from_account(new_var)?;
    require_fresh(&v, config, clock.slot)?;

    let old_var = pool.var;
    pool.var = new_var.key();
    pool.var_end_at = v.end_at;
    // A replacement is a fresh commit-reveal: the pool's recorded commit moves with it.
    pool.var_commit = v.commit;
    pool.var_replacements = pool
        .var_replacements
        .checked_add(1)
        .ok_or(MybarpoolError::MathOverflow)?;
    // sampled_slot / sampled_hash are already zero (the check above).

    emit_cpi!(VarReplaced {
        time: clock.unix_timestamp,
        pool: pool.key(),
        old_var,
        new_var: new_var.key(),
        end_at: v.end_at,
        replacements: pool.var_replacements,
        commit: v.commit,
    });
    Ok(())
}
