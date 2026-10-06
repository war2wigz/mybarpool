//! `sponsor`, PROGRAM §4.3 and ARCHITECTURE › Sponsorship: any wallet adds to
//! a pool's prize pool before kickoff. Never a box, never a fee base: this
//! handler touches none of `owners`, `sold`, `creator_boxes`, the fee fields
//! or the counter.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{CONFIG_SEED, POOL_SEED, SPONSORSHIP_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::Sponsored;
use crate::state::{GameRecord, PlatformConfig, Pool, Sponsorship};
use crate::vault::{transfer_in_sol, transfer_in_spl};

#[derive(Accounts)]
#[event_cpi]
pub struct Sponsor<'info> {
    /// The sponsor; pays `amount`, the `Sponsorship` rent on first call, and the transaction fee.
    #[account(mut)]
    pub sponsor: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; read for `paused` and the token's `max_sponsorship`.
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.2 `GameRecord`, the pool's game (`has_one`); read for status and kickoff.
    pub game: Account<'info, GameRecord>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
        has_one = game,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: seeds + the stored `vault_bump` pin it to `pool.vault`.
    /// PROGRAM §3.4 vault at `["vault", pool]` with the stored bump; `pool.vault` is the
    /// only address `sponsor` accepts for it.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// PROGRAM §3.7 `Sponsorship` at `["sponsorship", pool, sponsor]`; created on first call.
    #[account(
        init_if_needed,
        payer = sponsor,
        space = Sponsorship::SIZE,
        seeds = [SPONSORSHIP_SEED, pool.key().as_ref(), sponsor.key().as_ref()],
        bump,
    )]
    pub sponsorship: Account<'info, Sponsorship>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// The sponsor's token account for the mint; SPL pools only.
    #[account(mut, token::mint = mint)]
    pub sponsor_token_account: Option<InterfaceAccount<'info, TokenAccount>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` names `config` and `game` here: both are read-only and the lint
/// reads a MIR temporary derived from a field read as a write (the Step 2 false positive).
/// Shown by `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without
/// this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_sponsor(ctx: Context<Sponsor>, amount: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let sponsorship_bump = ctx.bumps.sponsorship;
    let Sponsor {
        sponsor,
        config,
        game,
        pool,
        vault,
        sponsorship,
        mint,
        sponsor_token_account,
        token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.3 checks, in order.
    require!(!config.paused, MybarpoolError::Paused);
    pool.require_sponsorable()?;
    require!(now < game.recorded_kickoff, MybarpoolError::SalesClosed);
    game.require_scheduled()?;

    let spl = if pool.token != 0 {
        let mint = mint.as_ref().ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let from = sponsor_token_account
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let token_program = token_program
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(token_program.key(), pool.token_program);
        Some((mint, from, token_program))
    } else {
        None
    };

    require!(amount >= pool.price, MybarpoolError::SponsorshipTooSmall);
    // The cap as configured now (PROGRAM §3.1 "adjustable"), not as it was at creation.
    let cap = config
        .tokens
        .get(usize::from(pool.token))
        .ok_or(MybarpoolError::TokenDisabled)?
        .max_sponsorship;
    let sponsored_total = pool
        .sponsored_total
        .checked_add(amount)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(
        sponsored_total <= cap,
        MybarpoolError::SponsorshipCapExceeded
    );

    let pool_key = pool.key();
    let sponsor_key = sponsor.key();
    let vault_info = vault.to_account_info();
    let system_info = system_program.to_account_info();
    match spl {
        None => transfer_in_sol(sponsor, &vault_info, &system_info, amount)?,
        Some((mint, from, token_program)) => {
            transfer_in_spl(from, mint, &vault_info, sponsor, token_program, amount)?
        }
    }

    if sponsorship.wallet == Pubkey::default() {
        sponsorship.pool = pool_key;
        sponsorship.wallet = sponsor_key;
        sponsorship.bump = sponsorship_bump;
        sponsorship.amount = 0;
        pool.sponsor_count = pool
            .sponsor_count
            .checked_add(1)
            .ok_or(MybarpoolError::MathOverflow)?;
        pool.sponsorships_open = pool
            .sponsorships_open
            .checked_add(1)
            .ok_or(MybarpoolError::MathOverflow)?;
    }
    sponsorship.amount = sponsorship
        .amount
        .checked_add(amount)
        .ok_or(MybarpoolError::MathOverflow)?;
    pool.sponsored_total = sponsored_total;

    emit_cpi!(Sponsored {
        time: now,
        pool: pool_key,
        sponsor: sponsor_key,
        amount,
        sponsored_total,
    });
    Ok(())
}
