//! `rotate_gate_key`, PROGRAM §4.3. Signer: the pool's creator. Only `Link`
//! pools have a gate key; the pool's status does not matter and nothing is
//! read from `config` or `game`.

use anchor_lang::prelude::*;

use crate::constants::POOL_SEED;
use crate::errors::MybarpoolError;
use crate::events::GateKeyRotated;
use crate::state::{AccessType, Pool};

#[derive(Accounts)]
#[event_cpi]
pub struct RotateGateKey<'info> {
    /// `pool.creator`.
    pub creator: Signer<'info>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields; `has_one = creator`.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
        has_one = creator @ MybarpoolError::Unauthorized,
    )]
    pub pool: Box<Account<'info, Pool>>,
}

pub fn handle_rotate_gate_key(ctx: Context<RotateGateKey>, new_key: Pubkey) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let pool = &mut ctx.accounts.pool;
    require!(
        pool.access_type == AccessType::Link,
        MybarpoolError::InvalidAccessType
    );
    require_keys_neq!(new_key, Pubkey::default(), MybarpoolError::GateKeyMissing);
    pool.gate_key = new_key;
    emit_cpi!(GateKeyRotated {
        time: now,
        pool: pool.key(),
    });
    Ok(())
}
