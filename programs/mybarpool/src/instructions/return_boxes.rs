//! `return_boxes`, PROGRAM §4.6: one instruction for every return path. On a
//! pool unfilled at kickoff, a game the admin marked postponed, cancelled or
//! suspended before any payout, a pool the admin cancelled, or a continuation
//! of any of those, the keeper passes the owners to pay in this call and the
//! program returns each of them exactly the purchase price of their unreturned
//! boxes (ARCHITECTURE › Returns "a return is always the full purchase price").

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::{CONFIG_SEED, COUNTER_SEED, POOL_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::BoxesReturned;
use crate::payout::{pay_owner_batch, Spl, VaultCtx};
use crate::state::{CreatorCounter, GameRecord, GameStatus, PlatformConfig, Pool, PoolStatus};
use crate::vault::pool_signer_seeds;

#[derive(Accounts)]
#[event_cpi]
pub struct ReturnBoxes<'info> {
    /// The keeper, `config.score_authority`; pays the rent of any associated token account
    /// this call creates.
    #[account(mut)]
    pub score_authority: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; `has_one = score_authority`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = score_authority @ MybarpoolError::Unauthorized,
    )]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.2 `GameRecord`, the pool's game (`has_one`); read for its status and the
    /// recorded kickoff.
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
    /// PROGRAM §3.4 vault; every return comes out of it.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// PROGRAM §3.5 the creator's counter for this game; required and decremented exactly on
    /// the call that takes the pool out of `Open`, never read otherwise.
    #[account(
        mut,
        seeds = [COUNTER_SEED, pool.creator.as_ref(), pool.game.as_ref()],
        bump = counter.bump,
    )]
    pub counter: Option<Account<'info, CreatorCounter>>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// The associated-token program, a fixed address; SPL pools only.
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// For the SOL transfers out of the vault and any ATA rent.
    pub system_program: Program<'info, System>,
    // Remaining accounts: the owner batch, `[owner]*` on SOL, `[owner, ata]*` on SPL.
}

pub fn handle_return_boxes<'info>(ctx: Context<'info, ReturnBoxes<'info>>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let remaining = ctx.remaining_accounts;
    let ReturnBoxes {
        score_authority,
        game,
        pool,
        vault,
        counter,
        mint,
        token_program,
        associated_token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.6: a Settled or Split pool satisfies no precondition, refused before looking.
    require!(
        !matches!(pool.status, PoolStatus::Settled | PoolStatus::Split),
        MybarpoolError::NotReturnable
    );
    let unfilled = pool.status == PoolStatus::Open && now >= game.recorded_kickoff;
    let marked =
        matches!(game.status, GameStatus::Postponed | GameStatus::Cancelled) && !pool.fees_paid;
    let suspended = game.status == GameStatus::Suspended && !pool.fees_paid;
    let cancelled = pool.cancelled_by_admin;
    let continuing = pool.status == PoolStatus::Returned && !pool.fees_paid;
    require!(
        unfilled || marked || suspended || cancelled || continuing,
        MybarpoolError::NotReturnable
    );

    let spl = spl_plumbing(pool, mint, token_program, associated_token_program)?;

    // (1) The first call moves the pool to Returned (and out of Open, through the counter).
    let mut changed = false;
    if pool.status != PoolStatus::Returned {
        if pool.status == PoolStatus::Open {
            counter
                .as_mut()
                .ok_or(ErrorCode::ConstraintAccountIsNone)?
                .decrement()?;
        }
        pool.status = PoolStatus::Returned;
        changed = true;
    }

    // (2) The owner batch at the purchase price.
    let pool_key = pool.key();
    let (game_key, creator_key) = (pool.game, pool.creator);
    let nonce_bytes = pool.nonce.to_le_bytes();
    let pool_bump = [pool.bump];
    let pool_seeds = pool_signer_seeds(&game_key, &creator_key, &nonce_bytes, &pool_bump);
    let vault_info = vault.to_account_info();
    let pool_info = pool.to_account_info();
    let payer = score_authority.to_account_info();
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
    let price = pool.price;
    let paid = pay_owner_batch(
        pool,
        &vault_ctx,
        remaining,
        price,
        |_, owner, boxes, amount| {
            emit_cpi!(BoxesReturned {
                time: now,
                pool: pool_key,
                owner: *owner,
                boxes,
                amount,
            });
            Ok(())
        },
    )?;

    // (3) The success rule: a status change or a box paid, else NothingToReturn.
    require!(changed || paid > 0, MybarpoolError::NothingToReturn);
    Ok(())
}

/// The SPL `Option`s, required together on an SPL pool (Anchor's own errors), `None` on SOL.
pub(crate) fn spl_plumbing<'a, 'info>(
    pool: &Pool,
    mint: &'a Option<InterfaceAccount<'info, Mint>>,
    token_program: &'a Option<Interface<'info, TokenInterface>>,
    associated_token_program: &'a Option<Program<'info, AssociatedToken>>,
) -> Result<Option<Spl<'a, 'info>>> {
    if pool.token == 0 {
        return Ok(None);
    }
    let mint = mint.as_ref().ok_or(ErrorCode::ConstraintAccountIsNone)?;
    let token_program = token_program
        .as_ref()
        .ok_or(ErrorCode::ConstraintAccountIsNone)?;
    let associated_token_program = associated_token_program
        .as_ref()
        .ok_or(ErrorCode::ConstraintAccountIsNone)?;
    require_keys_eq!(token_program.key(), pool.token_program);
    Ok(Some(Spl {
        mint,
        token_program,
        associated_token_program,
    }))
}
