//! `CreatorCounter`, PROGRAM §3.5: open pools per creator per game.

use anchor_lang::prelude::*;

/// PROGRAM §3.5 `CreatorCounter`. 74 bytes = 8 + 66.
///
/// Seeds `["counter", creator, game_record]`. Created by `create_pool` when absent (the creator
/// pays rent) and incremented there; decremented whenever one of the creator's pools on that
/// game leaves `Open`; closed by `close_counter` at zero, rent to `fee_wallet`.
///
/// Layout: discriminator 0 (8); `creator` 8 (32); `game` 40 (32); `open_count` 72; `bump` 73.
#[account]
#[derive(InitSpace, Debug, PartialEq, Eq)]
pub struct CreatorCounter {
    /// The creator; part of the seeds.
    pub creator: Pubkey,
    /// The `GameRecord`; part of the seeds.
    pub game: Pubkey,
    /// Pools in `Open` by this creator on this game.
    pub open_count: u8,
    /// PDA bump.
    pub bump: u8,
}

impl CreatorCounter {
    /// Account size: 8-byte discriminator + 66.
    pub const SIZE: usize = 8 + Self::INIT_SPACE;
}
