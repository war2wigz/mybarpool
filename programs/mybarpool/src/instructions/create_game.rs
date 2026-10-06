//! `create_game`, PROGRAM §4.2. Signer and payer: the keeper
//! (`config.score_authority`). Creates the `GameRecord` (§3.2) for a game key
//! and scheduled kickoff; a rescheduled game is a new record.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, GAME_SEED};
use crate::errors::MybarpoolError;
use crate::events::GameCreated;
use crate::state::{GameKey, GameRecord, GameStatus, PlatformConfig};

#[derive(Accounts)]
#[instruction(key: GameKey, scheduled_kickoff: i64)]
#[event_cpi]
pub struct CreateGame<'info> {
    /// The keeper, `config.score_authority`; pays the record's rent.
    #[account(mut)]
    pub score_authority: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; read for `score_authority` and `preseason_enabled`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = score_authority @ MybarpoolError::Unauthorized,
    )]
    pub config: Account<'info, PlatformConfig>,
    /// PROGRAM §3.2 `GameRecord` at the six seeds; one per (key, scheduled kickoff).
    #[account(
        init,
        payer = score_authority,
        space = GameRecord::SIZE,
        seeds = [
            GAME_SEED,
            &key.season.to_le_bytes(),
            &[key.week],
            &[key.home],
            &[key.away],
            &scheduled_kickoff.to_le_bytes(),
        ],
        bump,
    )]
    pub game: Account<'info, GameRecord>,
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` names `config` here: the handler only reads
/// `config.preseason_enabled`, and the lint treats a MIR temporary derived from a field read as
/// a write (the Step 2 false positive on `PlatformConfig::validate`). Shown by
/// `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without this line:
/// "account `config` is mutated in the instruction but is not declared with `#[account(mut)]`".
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_create_game(
    ctx: Context<CreateGame>,
    key: GameKey,
    scheduled_kickoff: i64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(scheduled_kickoff > now, MybarpoolError::KickoffInPast);
    key.validate(ctx.accounts.config.preseason_enabled)?;

    let game = &mut ctx.accounts.game;
    game.key = key;
    game.scheduled_kickoff = scheduled_kickoff;
    game.recorded_kickoff = scheduled_kickoff;
    game.status = GameStatus::Scheduled;
    game.quarters_posted = 0;
    game.home_score = [0; 4];
    game.away_score = [0; 4];
    game.posted_at = [0; 4];
    game.final_had_overtime = false;
    game.marked_at = 0;
    game.bump = ctx.bumps.game;
    game.reserved = [0; 64];

    emit_cpi!(GameCreated {
        time: now,
        game: game.key(),
        key,
        scheduled_kickoff,
    });
    Ok(())
}
