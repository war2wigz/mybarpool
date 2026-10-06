//! `Sponsorship`, PROGRAM §3.7: one per (pool, sponsor wallet). The wallet it
//! records is the only destination a return can ever use.

use anchor_lang::prelude::*;

/// PROGRAM §3.7 `Sponsorship`. 81 bytes = 8 + 73.
///
/// Seeds `["sponsorship", pool, wallet]`. Created by the first `sponsor` from that wallet (the
/// sponsor pays rent), topped up by later ones; closed in Step 7 with rent to `wallet`.
///
/// Layout: discriminator 0 (8); `pool` 8 (32); `wallet` 40 (32); `amount` 72 (8); `bump` 80.
#[account]
#[derive(InitSpace, Debug, PartialEq, Eq)]
pub struct Sponsorship {
    /// The pool; part of the seeds.
    pub pool: Pubkey,
    /// The sponsor; part of the seeds; the only possible return destination.
    pub wallet: Pubkey,
    /// Cumulative amount in the pool's token, base units.
    pub amount: u64,
    /// PDA bump.
    pub bump: u8,
}

impl Sponsorship {
    /// Account size: 8-byte discriminator + 73.
    pub const SIZE: usize = 8 + Self::INIT_SPACE;
}
