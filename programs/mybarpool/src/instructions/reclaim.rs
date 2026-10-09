//! `reclaim`, PROGRAM §4.6: thirty days after the scheduled kickoff, a box
//! owner takes back what a keeper never returned — the purchase price while the
//! fees are untaken, the split share once they are. The first reclaim on an
//! unresolved pool marks it `abandoned`, so `close_pool` later sweeps the
//! remainder to the creator instead of the platform.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::{BOXES, COUNTER_SEED, POOL_SEED, RECLAIM_DELAY, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::BoxesReclaimed;
use crate::instructions::return_boxes::spl_plumbing;
use crate::instructions::return_sponsorship::sponsor_token_account_checked;
use crate::payout::VaultCtx;
use crate::state::{CreatorCounter, GameRecord, Pool, PoolStatus};
use crate::vault::pool_signer_seeds;

#[derive(Accounts)]
#[event_cpi]
pub struct Reclaim<'info> {
    /// A box owner of the pool; pays the rent of their own associated token account when this
    /// call creates it.
    #[account(mut)]
    pub box_owner: Signer<'info>,
    /// PROGRAM §3.2 `GameRecord`, the pool's game (`has_one`); read for the scheduled kickoff.
    pub game: Account<'info, GameRecord>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields; anything but `Settled`.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
        has_one = game,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: seeds + the stored `vault_bump` pin it to `pool.vault`.
    /// PROGRAM §3.4 vault; the reclaim comes out of it.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
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
    /// CHECK: compared to the owner's derived associated token account in the handler. SPL
    /// pools only.
    #[account(mut)]
    pub box_owner_token_account: Option<UncheckedAccount<'info>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// The associated-token program, a fixed address; SPL pools only.
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// For the SOL transfer out of the vault and any ATA rent.
    pub system_program: Program<'info, System>,
}

pub fn handle_reclaim(ctx: Context<Reclaim>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let Reclaim {
        box_owner,
        game,
        pool,
        vault,
        counter,
        mint,
        box_owner_token_account,
        token_program,
        associated_token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.6 checks, in order.
    pool.require_not_settled()?;
    let reclaim_at = game
        .scheduled_kickoff
        .checked_add(RECLAIM_DELAY)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(now >= reclaim_at, MybarpoolError::ReclaimTooEarly);
    let owner = box_owner.key();
    require!(pool.owners.contains(&owner), MybarpoolError::NotOwner);
    let boxes = pool.unreturned_boxes_of(&owner);
    require!(!boxes.is_empty(), MybarpoolError::NothingToReturn);
    let spl = spl_plumbing(pool, mint, token_program, associated_token_program)?;

    // (1) An unresolved pool is abandoned: out of Open through the counter, then to the
    // terminal status its fees decide.
    if pool.is_unresolved() {
        pool.abandoned = true;
        if pool.status == PoolStatus::Open {
            counter
                .as_mut()
                .ok_or(ErrorCode::ConstraintAccountIsNone)?
                .decrement()?;
        }
        if pool.fees_paid {
            pool.split_amount = pool
                .unpaid_prize_pool
                .checked_div(u64::from(BOXES))
                .ok_or(MybarpoolError::MathOverflow)?;
            pool.status = PoolStatus::Split;
        } else {
            pool.status = PoolStatus::Returned;
        }
    }

    // (2) The owner's boxes at the price the fees decide, paid by the vault, ATA rent by the
    // owner.
    let pool_key = pool.key();
    let (game_key, creator_key) = (pool.game, pool.creator);
    let nonce_bytes = pool.nonce.to_le_bytes();
    let pool_bump = [pool.bump];
    let pool_seeds = pool_signer_seeds(&game_key, &creator_key, &nonce_bytes, &pool_bump);
    let vault_info = vault.to_account_info();
    let pool_info = pool.to_account_info();
    let payer = box_owner.to_account_info();
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
    let token_account = sponsor_token_account_checked(&vault_ctx, &owner, box_owner_token_account)?;

    let per_box = if pool.fees_paid {
        pool.split_amount
    } else {
        pool.price
    };
    let count = u64::try_from(boxes.len()).map_err(|_| MybarpoolError::MathOverflow)?;
    let amount = count
        .checked_mul(per_box)
        .ok_or(MybarpoolError::MathOverflow)?;
    vault_ctx.pay(&box_owner.to_account_info(), token_account.as_ref(), amount)?;
    for &b in &boxes {
        pool.mark_returned(b);
    }
    if pool.fees_paid {
        pool.unpaid_prize_pool = pool
            .unpaid_prize_pool
            .checked_sub(amount)
            .ok_or(MybarpoolError::MathOverflow)?;
    }

    emit_cpi!(BoxesReclaimed {
        time: now,
        pool: pool_key,
        owner,
        boxes,
        amount,
    });
    Ok(())
}
