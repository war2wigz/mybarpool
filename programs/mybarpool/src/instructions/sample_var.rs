//! `sample_var`, PROGRAM §4.4. Signer: anyone (`sampler`). CPIs Entropy
//! `Sample` if the `Var` is unsampled, then independently proves against the
//! SlotHashes sysvar that the hash the `Var` now carries is the real hash of
//! `end_at` (not the `keccak(end_at)` fallback) and records slot and hash on
//! the pool. The only instruction with a CPI, and the only one any wallet may
//! sign: it can only ever record a hash that matches the sysvar.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use solana_sdk_ids::sysvar;

use crate::constants::{ENTROPY_PROGRAM, POOL_SEED};
use crate::entropy::{sample_instruction, Var};
use crate::errors::MybarpoolError;
use crate::events::VarSampled;
use crate::slot_hashes::hash_for_slot;
use crate::state::Pool;

#[derive(Accounts)]
#[event_cpi]
pub struct SampleVar<'info> {
    /// Any wallet; the CPI's signer and nothing else.
    pub sampler: Signer<'info>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: owner `ENTROPY_PROGRAM` by constraint, `== pool.var` and every field by the
    /// handler. Writable because Entropy's `Sample` writes `slot_hash`.
    #[account(mut, owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy)]
    pub var: UncheckedAccount<'info>,
    /// CHECK: the SlotHashes sysvar by address; read by Entropy's `Sample` and again by this
    /// handler's own `hash_for_slot`.
    #[account(address = sysvar::slot_hashes::ID)]
    pub slot_hashes: UncheckedAccount<'info>,
    /// CHECK: the Entropy program by address (PROGRAM §1 `ENTROPY_PROGRAM`), executable; the
    /// CPI target is this constant, never a caller-supplied key.
    #[account(address = ENTROPY_PROGRAM, executable)]
    pub entropy_program: UncheckedAccount<'info>,
}

pub fn handle_sample_var(ctx: Context<SampleVar>) -> Result<()> {
    let clock = Clock::get()?;
    let SampleVar {
        sampler,
        pool,
        var,
        slot_hashes,
        ..
    } = ctx.accounts;

    // PROGRAM §4.4 checks, in order.
    pool.require_locked()?;
    require_keys_neq!(pool.var, Pubkey::default(), MybarpoolError::VarNotSet);
    require_keys_eq!(var.key(), pool.var, MybarpoolError::VarMismatch);
    require!(pool.sampled_slot == 0, MybarpoolError::VarAlreadySampled);
    let before = Var::try_from_account(var)?;
    require!(clock.slot >= before.end_at, MybarpoolError::SampleTooEarly);

    if before.slot_hash == [0u8; 32] {
        invoke(
            &sample_instruction(sampler.key, var.key),
            &[
                sampler.to_account_info(),
                var.to_account_info(),
                slot_hashes.to_account_info(),
            ],
        )?;
    }
    // The §4.4 "reload": `before` was decoded before the CPI and is dropped here; the `Var` is
    // decoded again from the account's live buffer. Without this the program would read a zero
    // `slot_hash` and fail the check below against fresh data (the Mollusk reload test).
    let v = Var::try_from_account(var)?;
    require!(v.slot_hash != [0u8; 32], MybarpoolError::SampleWindowMissed);
    // Our own proof: the sysvar still holds end_at and its entry is the hash the Var carries.
    // Absent, or present and different (the keccak(end_at) fallback included), is a missed window.
    let entry = hash_for_slot(slot_hashes, v.end_at)?;
    require!(
        entry == Some(v.slot_hash),
        MybarpoolError::SampleWindowMissed
    );

    pool.sampled_slot = clock.slot;
    pool.sampled_hash = v.slot_hash;

    emit_cpi!(VarSampled {
        time: clock.unix_timestamp,
        pool: pool.key(),
        var: var.key(),
        sampler: sampler.key(),
        slot: clock.slot,
        end_at: v.end_at,
        slot_hash: v.slot_hash,
    });
    Ok(())
}
