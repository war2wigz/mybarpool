//! `set_var`, PROGRAM §4.4. Signer: the keeper. Binds a fresh, committed,
//! unsampled `Var` from the configured provider to a `Locked` pool. One call
//! per pool; `replace_var` is the admin's only way to change it.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, ENTROPY_PROGRAM, POOL_SEED};
use crate::entropy::Var;
use crate::errors::MybarpoolError;
use crate::events::VarSet;
use crate::state::{PlatformConfig, Pool};

#[derive(Accounts)]
#[event_cpi]
pub struct SetVar<'info> {
    /// The keeper, `config.score_authority`.
    pub score_authority: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; read for `entropy_provider`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = score_authority @ MybarpoolError::Unauthorized,
    )]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: owner `ENTROPY_PROGRAM` by constraint; length, discriminator and every field by
    /// `Var::try_from_account` in the handler. Not an Anchor account (steel layout).
    #[account(owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy)]
    pub var: UncheckedAccount<'info>,
}

/// PROGRAM §4.4's freshness checks, shared with `replace_var`: provider, committed, unsampled,
/// unrevealed, `samples == 1`, `is_auto == 0`, `end_at` ahead of `slot`. Each is one `require!`
/// so a test can isolate it.
pub fn require_fresh(v: &Var, config: &PlatformConfig, slot: u64) -> Result<()> {
    require_keys_eq!(
        v.provider,
        config.entropy_provider,
        MybarpoolError::VarProviderMismatch
    );
    require!(v.commit != [0u8; 32], MybarpoolError::VarNotFresh);
    require!(
        v.seed == [0u8; 32] && v.slot_hash == [0u8; 32] && v.value == [0u8; 32],
        MybarpoolError::VarNotFresh
    );
    require!(v.samples == 1, MybarpoolError::VarNotFresh);
    require!(v.is_auto == 0, MybarpoolError::VarNotFresh);
    require!(v.end_at > slot, MybarpoolError::VarNotFresh);
    Ok(())
}

pub fn handle_set_var(ctx: Context<SetVar>) -> Result<()> {
    let clock = Clock::get()?;
    let SetVar {
        config, pool, var, ..
    } = ctx.accounts;

    // PROGRAM §4.4 checks, in order.
    pool.require_locked()?;
    require_keys_eq!(pool.var, Pubkey::default(), MybarpoolError::VarAlreadySet);
    let v = Var::try_from_account(var)?;
    require_fresh(&v, config, clock.slot)?;

    pool.var = var.key();
    pool.var_end_at = v.end_at;
    // PROGRAM §4.4 (Step 5b): the commit is fixed on the pool before the end slot; `draw`
    // checks the revealed seed against this copy, never against the Var's own field.
    pool.var_commit = v.commit;

    emit_cpi!(VarSet {
        time: clock.unix_timestamp,
        pool: pool.key(),
        var: var.key(),
        end_at: v.end_at,
        commit: v.commit,
    });
    Ok(())
}
