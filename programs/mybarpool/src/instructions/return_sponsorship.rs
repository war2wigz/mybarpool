//! `return_sponsorship`, PROGRAM §4.6: once a pool is `Returned` with its fees
//! untaken, the keeper returns each sponsorship in full to the wallet the
//! `Sponsorship` account recorded, and the account closes to that wallet.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::{CONFIG_SEED, POOL_SEED, SPONSORSHIP_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::SponsorshipReturned;
use crate::instructions::return_boxes::spl_plumbing;
use crate::payout::VaultCtx;
use crate::state::{PlatformConfig, Pool, PoolStatus, Sponsorship};
use crate::vault::pool_signer_seeds;

#[derive(Accounts)]
#[event_cpi]
pub struct ReturnSponsorship<'info> {
    /// The keeper, `config.score_authority`; pays the rent of the sponsor's associated token
    /// account when this call creates it.
    #[account(mut)]
    pub score_authority: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; `has_one = score_authority`.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = score_authority @ MybarpoolError::Unauthorized,
    )]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields; must be `Returned` with fees
    /// untaken.
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: seeds + the stored `vault_bump` pin it to `pool.vault`.
    /// PROGRAM §3.4 vault; the sponsorship comes out of it.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// PROGRAM §3.7 `Sponsorship` for this pool and `sponsor`; closed to `sponsor` on success.
    #[account(
        mut,
        seeds = [SPONSORSHIP_SEED, pool.key().as_ref(), sponsorship.wallet.as_ref()],
        bump = sponsorship.bump,
        has_one = pool,
        close = sponsor,
    )]
    pub sponsorship: Account<'info, Sponsorship>,
    /// CHECK: `address = sponsorship.wallet`, the only destination a return can use.
    /// The sponsor's wallet; receives the sponsorship (SOL) and the account's rent.
    #[account(mut, address = sponsorship.wallet)]
    pub sponsor: UncheckedAccount<'info>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// CHECK: compared to the sponsor's derived associated token account in the handler.
    /// SPL pools only.
    #[account(mut)]
    pub sponsor_token_account: Option<UncheckedAccount<'info>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// The associated-token program, a fixed address; SPL pools only.
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// For the SOL transfer out of the vault and any ATA rent.
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` names `config` here: it is read-only and the lint reads a MIR
/// temporary derived from a field read as a write (the Step 2 false positive). Shown by
/// `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_return_sponsorship(ctx: Context<ReturnSponsorship>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let ReturnSponsorship {
        score_authority,
        pool,
        vault,
        sponsorship,
        sponsor,
        mint,
        sponsor_token_account,
        token_program,
        associated_token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.6: only a Returned pool with its fees untaken returns sponsorships in full.
    require!(!pool.fees_paid, MybarpoolError::FeesAlreadyPaid);
    require!(
        pool.status == PoolStatus::Returned,
        MybarpoolError::NotReturnable
    );
    let spl = spl_plumbing(pool, mint, token_program, associated_token_program)?;

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

/// The named token account of a sponsorship or reclaim payout: on an SPL pool it is required
/// (`ConstraintAccountIsNone`) and must be the wallet's derived ATA (`RequireKeysEqViolated`);
/// on a SOL pool it is `None` and never read.
pub(crate) fn sponsor_token_account_checked<'info>(
    ctx: &VaultCtx<'_, 'info>,
    wallet: &Pubkey,
    token_account: &Option<UncheckedAccount<'info>>,
) -> Result<Option<AccountInfo<'info>>> {
    let Some(expected) = ctx.ata_of(wallet) else {
        return Ok(None);
    };
    let account = token_account
        .as_ref()
        .ok_or(ErrorCode::ConstraintAccountIsNone)?;
    require_keys_eq!(account.key(), expected);
    Ok(Some(account.to_account_info()))
}
