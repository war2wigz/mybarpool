//! `draw`, PROGRAM §4.4 and §6.2. Signer: the keeper. Recomputes Entropy's
//! value from the `Var`'s own fields, refuses the fallback, requires the `Var`
//! still carries the hash `sample_var` verified, and derives both axes. The
//! only writer of `home_axis` / `away_axis`.

use anchor_lang::prelude::*;

use crate::axes::draw_axes;
use crate::constants::{CONFIG_SEED, ENTROPY_PROGRAM, POOL_SEED};
use crate::entropy::{expected_value, fallback_hash, Var};
use crate::errors::MybarpoolError;
use crate::events::DigitsDrawn;
use crate::state::{PlatformConfig, Pool, PoolStatus};

#[derive(Accounts)]
#[event_cpi]
pub struct Draw<'info> {
    /// The keeper, `config.score_authority`.
    pub score_authority: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`.
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
    /// CHECK: owner `ENTROPY_PROGRAM` by constraint, `== pool.var` and every field by the handler.
    /// The recorded `Var`, sampled through this program and revealed by its provider.
    #[account(owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy)]
    pub var: UncheckedAccount<'info>,
}

/// `missing_mut_constraint` names `var` here: read-only (decoded by `Var::try_from_account`,
/// never written), the field-read false positive (Step 2 onward). Shown by
/// `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_draw(ctx: Context<Draw>) -> Result<()> {
    let clock = Clock::get()?;
    let Draw { pool, var, .. } = ctx.accounts;

    // PROGRAM §4.4 checks, in order: the value binding to the recorded Var.
    pool.require_locked()?;
    require!(!pool.drawn, MybarpoolError::AlreadyDrawn);
    require_keys_neq!(pool.var, Pubkey::default(), MybarpoolError::VarNotSet);
    require_keys_eq!(var.key(), pool.var, MybarpoolError::VarMismatch);
    require!(pool.sampled_slot != 0, MybarpoolError::VarNotSampledHere);
    let v = Var::try_from_account(var)?;
    // Rolled with Next, re-opened, or rewritten: not the hash this program verified.
    require!(
        v.slot_hash == pool.sampled_hash,
        MybarpoolError::VarNotSampledHere
    );
    // Defence in depth behind sample_var: the predictable fallback is refused by formula.
    require!(
        v.slot_hash != fallback_hash(v.end_at),
        MybarpoolError::VarFallbackHash
    );
    require!(
        v.seed != [0u8; 32] && v.value != [0u8; 32],
        MybarpoolError::VarNotRevealed
    );
    // PROGRAM §4.4 (Step 5b), the commit binding: the seed must hash to the commit this
    // program recorded on the pool before the end slot. The Var's own `commit` field is not
    // read: it is a foreign account the Entropy program may write however it likes; the
    // pool's copy is the one fixed here. With this and the two checks around it, `value` is a
    // function of the recorded commit and the verified slot hash alone, so the Entropy
    // deployment can stop a draw but never steer one.
    require!(
        solana_keccak_hasher::hashv(&[&v.seed]).to_bytes() == pool.var_commit,
        MybarpoolError::VarCommitMismatch
    );
    // `samples` must still be 1 (set_var required it; a value recomputed for an edited count
    // is refused here before the recomputation below).
    require!(v.samples == 1, MybarpoolError::VarNotFresh);
    require!(
        v.value == expected_value(&v.slot_hash, &v.seed, v.samples),
        MybarpoolError::VarNotRevealed
    );

    let (home, away) = draw_axes(&v.value);
    pool.home_axis = home;
    pool.away_axis = away;
    pool.drawn = true;
    pool.status = PoolStatus::Drawn;

    emit_cpi!(DigitsDrawn {
        time: clock.unix_timestamp,
        pool: pool.key(),
        var: var.key(),
        value: v.value,
        home_axis: home,
        away_axis: away,
    });
    Ok(())
}
