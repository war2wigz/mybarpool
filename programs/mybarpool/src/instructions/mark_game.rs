//! `mark_game`, PROGRAM §4.2 and §9. Signer: the admin (the multisig). Marks a
//! record `Postponed`, `Cancelled` or `Suspended`, which makes its pools
//! returnable or splittable (ARCHITECTURE › Returns and splits). Irreversible.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, GAME_SEED};
use crate::errors::MybarpoolError;
use crate::events::GameMarked;
use crate::state::{GameRecord, GameStatus, PlatformConfig};

#[derive(Accounts)]
#[event_cpi]
pub struct MarkGame<'info> {
    /// `config.admin`.
    pub admin: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ MybarpoolError::Unauthorized,
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

pub fn handle_mark_game(ctx: Context<MarkGame>, new_status: GameStatus) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let game = &mut ctx.accounts.game;

    // PROGRAM §4.2: new_status ∈ {Postponed, Cancelled, Suspended}; Scheduled and Final are not marks.
    require!(
        matches!(
            new_status,
            GameStatus::Postponed | GameStatus::Cancelled | GameStatus::Suspended
        ),
        MybarpoolError::InvalidGameStatus
    );
    // A Final record is not scheduled; a marked one is already marked (PROGRAM §4.2 "Irreversible").
    match game.status {
        GameStatus::Scheduled => {}
        GameStatus::Final => return Err(MybarpoolError::GameNotScheduled.into()),
        GameStatus::Postponed | GameStatus::Cancelled | GameStatus::Suspended => {
            return Err(MybarpoolError::GameAlreadyMarked.into());
        }
    }
    // PROGRAM §9: Postponed and Cancelled only while quarters_posted == 0; a game with scores
    // can only be suspended.
    if matches!(new_status, GameStatus::Postponed | GameStatus::Cancelled) {
        require!(game.quarters_posted == 0, MybarpoolError::InvalidGameStatus);
    }

    game.status = new_status;
    game.marked_at = now;

    emit_cpi!(GameMarked {
        time: now,
        game: game.key(),
        status: new_status,
    });
    Ok(())
}
