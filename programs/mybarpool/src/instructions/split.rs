//! `split`, PROGRAM §4.6: on a game the admin marked suspended after the pool
//! paid a prize, `unpaid_prize_pool` is split equally across the 25 boxes.
//! Fees stay taken and sponsorships stay inside the prize pool (ARCHITECTURE ›
//! Returns "Suspended game"); the keeper passes the owners to pay in this
//! call and each receives `split_amount` per unreturned box.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::{BOXES, CONFIG_SEED, POOL_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::BoxesSplit;
use crate::instructions::return_boxes::spl_plumbing;
use crate::payout::{pay_owner_batch, VaultCtx};
use crate::state::{GameRecord, GameStatus, PlatformConfig, Pool, PoolStatus};
use crate::vault::pool_signer_seeds;

#[derive(Accounts)]
#[event_cpi]
pub struct Split<'info> {
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
    /// PROGRAM §3.2 `GameRecord`, the pool's game (`has_one`); must be `Suspended`.
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
    /// PROGRAM §3.4 vault; every share comes out of it.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// The associated-token program, a fixed address; SPL pools only.
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// For the SOL transfers out of the vault and any ATA rent.
    pub system_program: Program<'info, System>,
    // Remaining accounts: the owner batch. No counter: a Drawn pool is never Open.
}

/// `missing_mut_constraint` names `config` and `game` here: both are read-only and the lint
/// reads a MIR temporary derived from a field read as a write (the Step 2 false positive).
/// Shown by `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without
/// this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_split<'info>(ctx: Context<'info, Split<'info>>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let remaining = ctx.remaining_accounts;
    let Split {
        score_authority,
        game,
        pool,
        vault,
        mint,
        token_program,
        associated_token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.6 checks, in order.
    require!(
        game.status == GameStatus::Suspended,
        MybarpoolError::NotSuspended
    );
    require!(pool.fees_paid, MybarpoolError::NotSplittable);
    require!(
        matches!(pool.status, PoolStatus::Drawn | PoolStatus::Split),
        MybarpoolError::NotSplittable
    );
    let spl = spl_plumbing(pool, mint, token_program, associated_token_program)?;

    // (1) The first call fixes the share and moves the pool to Split.
    let mut changed = false;
    if pool.status == PoolStatus::Drawn {
        pool.split_amount = pool
            .unpaid_prize_pool
            .checked_div(u64::from(BOXES))
            .ok_or(MybarpoolError::MathOverflow)?;
        pool.status = PoolStatus::Split;
        changed = true;
    }

    // (2) The owner batch at the stored share, each share paid subtracted from the unpaid pool.
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
    let per_box = pool.split_amount;
    let paid = pay_owner_batch(
        pool,
        &vault_ctx,
        remaining,
        per_box,
        |pool, owner, boxes, amount| {
            pool.unpaid_prize_pool = pool
                .unpaid_prize_pool
                .checked_sub(amount)
                .ok_or(MybarpoolError::MathOverflow)?;
            emit_cpi!(BoxesSplit {
                time: now,
                pool: pool_key,
                owner: *owner,
                boxes,
                amount,
            });
            Ok(())
        },
    )?;

    // (3) The success rule.
    require!(changed || paid > 0, MybarpoolError::NothingToReturn);
    Ok(())
}
