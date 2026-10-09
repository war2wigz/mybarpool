//! `reclaim_sponsorship`, PROGRAM §4.6: thirty days after the scheduled
//! kickoff, a sponsor of a pool whose fees were never taken takes their
//! sponsorship back in full; the `Sponsorship` account closes to them. The
//! first reclaim on an unresolved pool marks it `abandoned`.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::{COUNTER_SEED, POOL_SEED, RECLAIM_DELAY, SPONSORSHIP_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::SponsorshipReturned;
use crate::instructions::return_boxes::spl_plumbing;
use crate::instructions::return_sponsorship::sponsor_token_account_checked;
use crate::payout::VaultCtx;
use crate::state::{CreatorCounter, GameRecord, Pool, PoolStatus, Sponsorship};
use crate::vault::pool_signer_seeds;

#[derive(Accounts)]
#[event_cpi]
pub struct ReclaimSponsorship<'info> {
    /// The sponsor, the wallet of the `Sponsorship` (its seed); receives the sponsorship and
    /// the rent, and pays the rent of their own associated token account when this call
    /// creates it.
    #[account(mut)]
    pub sponsor: Signer<'info>,
    /// PROGRAM §3.2 `GameRecord`, the pool's game (`has_one`); read for the scheduled kickoff.
    pub game: Account<'info, GameRecord>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields; fees untaken.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
        has_one = game,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: seeds + the stored `vault_bump` pin it to `pool.vault`.
    /// PROGRAM §3.4 vault; the sponsorship comes out of it.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// PROGRAM §3.7 `Sponsorship` for this pool and the signer; closed to `sponsor` on
    /// success.
    #[account(
        mut,
        seeds = [SPONSORSHIP_SEED, pool.key().as_ref(), sponsor.key().as_ref()],
        bump = sponsorship.bump,
        has_one = pool,
        close = sponsor,
    )]
    pub sponsorship: Account<'info, Sponsorship>,
    /// PROGRAM §3.5 the creator's counter for this game; required and decremented exactly when
    /// the pool is still `Open`, never read otherwise.
    #[account(
        mut,
        seeds = [COUNTER_SEED, pool.creator.as_ref(), pool.game.as_ref()],
        bump = counter.bump,
    )]
    pub counter: Option<Account<'info, CreatorCounter>>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// CHECK: compared to the sponsor's derived associated token account in the handler. SPL
    /// pools only.
    #[account(mut)]
    pub sponsor_token_account: Option<UncheckedAccount<'info>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// The associated-token program, a fixed address; SPL pools only.
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// For the SOL transfer out of the vault and any ATA rent.
    pub system_program: Program<'info, System>,
}

/// `arbitrary_cpi_call` reports the vault's outbound CPIs (`vault.rs`: `transfer_out_sol`,
/// `transfer_out_spl`, `create_ata_idempotent`) against this handler, which reaches them
/// through `payout.rs`: their program id is a parameter and the lint cannot see that it comes
/// from a typed `Program` / checked `Interface` account here. The lint attaches the report to
/// the handler it started from, so the allow lives here; the reproduction is in
/// `AUDIT-READINESS.md`.
#[cfg_attr(dylint_lib = "arbitrary_cpi_call", allow(arbitrary_cpi_call))]
pub fn handle_reclaim_sponsorship(ctx: Context<ReclaimSponsorship>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let ReclaimSponsorship {
        sponsor,
        game,
        pool,
        vault,
        sponsorship,
        counter,
        mint,
        sponsor_token_account,
        token_program,
        associated_token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.6 checks, in order. A sponsorship is reclaimable only while the fees are
    // untaken; the Settled check cannot fail after that but is written as the spec lists it.
    require!(!pool.fees_paid, MybarpoolError::FeesAlreadyPaid);
    let reclaim_at = game
        .scheduled_kickoff
        .checked_add(RECLAIM_DELAY)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(now >= reclaim_at, MybarpoolError::ReclaimTooEarly);
    pool.require_not_settled()?;
    let spl = spl_plumbing(pool, mint, token_program, associated_token_program)?;

    // (1) An unresolved pool is abandoned and Returned (fees are untaken here).
    if pool.is_unresolved() {
        pool.abandoned = true;
        if pool.status == PoolStatus::Open {
            counter
                .as_mut()
                .ok_or(ErrorCode::ConstraintAccountIsNone)?
                .decrement()?;
        }
        pool.status = PoolStatus::Returned;
    }

    // (2) The sponsorship in full, ATA rent by the sponsor.
    let pool_key = pool.key();
    let (game_key, creator_key) = (pool.game, pool.creator);
    let nonce_bytes = pool.nonce.to_le_bytes();
    let pool_bump = [pool.bump];
    let pool_seeds = pool_signer_seeds(&game_key, &creator_key, &nonce_bytes, &pool_bump);
    let vault_info = vault.to_account_info();
    let pool_info = pool.to_account_info();
    let payer = sponsor.to_account_info();
    let system_info = system_program.to_account_info();
    let vault_ctx = VaultCtx {
        vault: &vault_info,
        pool_info: &pool_info,
        pool_key,
        vault_bump: pool.vault_bump,
        pool_seeds: &pool_seeds,
        payer: &payer,
        system_program: &system_info,
        spl,
    };
    let token_account =
        sponsor_token_account_checked(&vault_ctx, &sponsor.key(), sponsor_token_account)?;

    let amount = sponsorship.amount;
    vault_ctx.pay(&sponsor.to_account_info(), token_account.as_ref(), amount)?;
    pool.sponsorships_open = pool
        .sponsorships_open
        .checked_sub(1)
        .ok_or(MybarpoolError::MathOverflow)?;

    emit_cpi!(SponsorshipReturned {
        time: now,
        pool: pool_key,
        sponsor: sponsor.key(),
        amount,
    });
    Ok(())
}
