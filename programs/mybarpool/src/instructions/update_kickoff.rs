//! `update_kickoff`, PROGRAM §4.2 and ARCHITECTURE › Buying ("Kickoff updates,
//! precisely"). Signer: the keeper. Moves `recorded_kickoff`; the scheduled
//! kickoff the record was seeded with never changes.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, GAME_SEED, KICKOFF_UPDATE_BOUND};
use crate::errors::MybarpoolError;
use crate::events::KickoffUpdated;
use crate::state::{GameRecord, PlatformConfig};

#[derive(Accounts)]
#[event_cpi]
pub struct UpdateKickoff<'info> {
    /// The keeper, `config.score_authority`.
    pub score_authority: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = score_authority @ MybarpoolError::Unauthorized,
    )]
    pub config: Account<'info, PlatformConfig>,
    /// PROGRAM §3.2 `GameRecord`, re-derived from its own key and scheduled kickoff.
    #[account(
        mut,
        seeds = [
            GAME_SEED,
            &game.key.season.to_le_bytes(),
            &[game.key.week],
            &[game.key.home],
            &[game.key.away],
            &game.scheduled_kickoff.to_le_bytes(),
        ],
        bump = game.bump,
    )]
    pub game: Account<'info, GameRecord>,
}

pub fn handle_update_kickoff(ctx: Context<UpdateKickoff>, new_time: i64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let game = &mut ctx.accounts.game;

    // PROGRAM §4.2, all required, in this order.
    game.require_scheduled()?;
    require!(
        game.quarters_posted == 0,
        MybarpoolError::KickoffUpdateTooLate
    );
    require!(
        now < game.recorded_kickoff,
        MybarpoolError::KickoffUpdateTooLate
    );
    require!(new_time > now, MybarpoolError::KickoffInPast);
    // The bound is against the scheduled kickoff, never the recorded one (ARCHITECTURE › Trust model).
    let latest = game
        .scheduled_kickoff
        .checked_add(KICKOFF_UPDATE_BOUND)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(new_time <= latest, MybarpoolError::KickoffOutOfBounds);

    let old = game.recorded_kickoff;
    game.recorded_kickoff = new_time;

    emit_cpi!(KickoffUpdated {
        time: now,
        game: game.key(),
        old,
        new: new_time,
    });
    Ok(())
}
