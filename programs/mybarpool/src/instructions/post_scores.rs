//! `post_scores`, PROGRAM §4.2 and ARCHITECTURE › Trust model (ordering, the
//! 15-minute floors) and Payouts (cumulative scores; the fourth post is the
//! final score after any overtime). Signer: the keeper.

use anchor_lang::prelude::*;

use crate::constants::{CONFIG_SEED, GAME_SEED, MIN_QUARTER_SECONDS, QUARTERS};
use crate::errors::MybarpoolError;
use crate::events::ScoresPosted;
use crate::state::{GameRecord, GameStatus, PlatformConfig};

#[derive(Accounts)]
#[event_cpi]
pub struct PostScores<'info> {
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

pub fn handle_post_scores(
    ctx: Context<PostScores>,
    quarter: u8,
    home: u16,
    away: u16,
    is_final: bool,
    had_overtime: bool,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let game = &mut ctx.accounts.game;

    // PROGRAM §4.2: status first, so terminal records are inert whatever the arguments.
    game.require_scheduled()?;

    // Order: exactly the next quarter. With status Scheduled, quarters_posted ≤ 3, so a passing
    // quarter is 1–4 and `quarter - 1` / `quarter - 2` below cannot underflow.
    let next = game
        .quarters_posted
        .checked_add(1)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(quarter == next, MybarpoolError::QuarterOutOfOrder);
    let slot = usize::from(
        quarter
            .checked_sub(1)
            .ok_or(MybarpoolError::QuarterOutOfOrder)?,
    );
    require!(
        slot < usize::from(QUARTERS),
        MybarpoolError::QuarterOutOfOrder
    );

    // Timing: the 15-minute floor after the recorded kickoff (Q1) or the previous post.
    let floor_from = if quarter == 1 {
        game.recorded_kickoff
    } else {
        let previous = slot
            .checked_sub(1)
            .ok_or(MybarpoolError::QuarterOutOfOrder)?;
        *game
            .posted_at
            .get(previous)
            .ok_or(MybarpoolError::QuarterOutOfOrder)?
    };
    let earliest = floor_from
        .checked_add(MIN_QUARTER_SECONDS)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(now >= earliest, MybarpoolError::QuarterTooSoon);

    // Cumulative scores never decrease.
    if quarter > 1 {
        let previous = slot
            .checked_sub(1)
            .ok_or(MybarpoolError::QuarterOutOfOrder)?;
        let previous_home = *game
            .home_score
            .get(previous)
            .ok_or(MybarpoolError::QuarterOutOfOrder)?;
        let previous_away = *game
            .away_score
            .get(previous)
            .ok_or(MybarpoolError::QuarterOutOfOrder)?;
        require!(
            home >= previous_home && away >= previous_away,
            MybarpoolError::ScoreDecreased
        );
    }

    // The final flag marks the fourth post and nothing else; overtime is only ever final.
    let fourth = quarter == QUARTERS;
    require!(is_final == fourth, MybarpoolError::FinalFlagMismatch);
    require!(is_final || !had_overtime, MybarpoolError::FinalFlagMismatch);

    *game
        .home_score
        .get_mut(slot)
        .ok_or(MybarpoolError::QuarterOutOfOrder)? = home;
    *game
        .away_score
        .get_mut(slot)
        .ok_or(MybarpoolError::QuarterOutOfOrder)? = away;
    *game
        .posted_at
        .get_mut(slot)
        .ok_or(MybarpoolError::QuarterOutOfOrder)? = now;
    game.quarters_posted = quarter;
    if fourth {
        game.status = GameStatus::Final;
        game.final_had_overtime = had_overtime;
    }

    emit_cpi!(ScoresPosted {
        time: now,
        game: game.key(),
        quarter,
        home,
        away,
        is_final,
        had_overtime,
    });
    Ok(())
}
