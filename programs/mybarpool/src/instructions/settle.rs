//! `settle(quarter)`, PROGRAM §4.5: the winning box computed in the program
//! (§6.3) from the pool's stored axes and the game record's cumulative score,
//! the passed winner verified against it, the prizes fixed on the first call
//! (§5.2), the quarter's prize paid, and — in the same transaction as the
//! first non-zero prize, never before, never again — the three fees (§5.1,
//! ARCHITECTURE › Fees "no fee leaves the vault until the pool has paid a
//! prize"). The first instruction that moves money out: every destination is
//! one the program verified against the pool or the config here.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::{get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::{CONFIG_SEED, POOL_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::QuarterSettled;
use crate::money::{prize_pool, quarter_prizes, FeeAmounts};
use crate::state::{GameRecord, PlatformConfig, Pool, PoolStatus};
use crate::vault::{create_ata_idempotent, pool_signer_seeds, transfer_out_sol, transfer_out_spl};
use crate::winner::winning_box;

#[derive(Accounts)]
#[event_cpi]
pub struct Settle<'info> {
    /// The keeper, `config.score_authority`; pays the rent of any associated token account this
    /// call creates (PROGRAM §4.5 "The keeper is the payer for any ATA creation").
    #[account(mut)]
    pub score_authority: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; `has_one = score_authority`, read for `fee_wallet`. The
    /// fee amounts moved are the pool's stored ones, never the config's bps.
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = score_authority @ MybarpoolError::Unauthorized,
    )]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.2 `GameRecord`, the pool's game (`has_one`); read for `quarters_posted` and
    /// the cumulative scores.
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
    /// PROGRAM §3.4 vault at `["vault", pool]`; SOL: system-owned, pays by `transfer` signed
    /// with the vault seeds; SPL: the pool's token account, pays by `transfer_checked` with the
    /// pool PDA as authority.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: compared in the handler to `pool.owners[winning box]` (`WinnerMismatch`).
    /// The winning box's owner: a payout destination, never typed `SystemAccount` (PROGRAM §10
    /// Account types). Computed in the program; this account is only verified against it.
    #[account(mut)]
    pub winner: UncheckedAccount<'info>,
    /// CHECK: `address = config.fee_wallet`.
    /// The platform's fee destination (PROGRAM §3.1); SOL directly, SPL to its ATA.
    #[account(mut, address = config.fee_wallet @ MybarpoolError::FeeAccountMismatch)]
    pub fee_wallet: UncheckedAccount<'info>,
    /// CHECK: `address = pool.creator`.
    /// The creator, paid `creator_fee` with the first prize (PROGRAM §5.1).
    #[account(mut, address = pool.creator @ MybarpoolError::FeeAccountMismatch)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: compared in the handler to `pool.integrator` when the pool has one.
    /// The integrator wallet; required and verified when `pool.integrator != default`, never
    /// read otherwise.
    #[account(mut)]
    pub integrator: Option<UncheckedAccount<'info>>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// CHECK: compared in the handler to the winner's ATA for the pool's mint and token program.
    /// The winner's associated token account; SPL pools only; created idempotently when the
    /// prize is non-zero.
    #[account(mut)]
    pub winner_token_account: Option<UncheckedAccount<'info>>,
    /// CHECK: compared in the handler to `fee_wallet`'s ATA.
    /// `fee_wallet`'s associated token account; SPL pools only; created on the fee-paying call.
    #[account(mut)]
    pub fee_token_account: Option<UncheckedAccount<'info>>,
    /// CHECK: compared in the handler to the creator's ATA.
    /// The creator's associated token account; SPL pools only; created on the fee-paying call.
    #[account(mut)]
    pub creator_token_account: Option<UncheckedAccount<'info>>,
    /// CHECK: compared in the handler to the integrator's ATA when the pool has an integrator.
    /// The integrator's associated token account; SPL pools with an integrator only.
    #[account(mut)]
    pub integrator_token_account: Option<UncheckedAccount<'info>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// The associated-token program, a fixed address; SPL pools only.
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// For the SOL transfers out of the vault and any ATA rent.
    pub system_program: Program<'info, System>,
}

/// The SPL plumbing of a settlement, present only for an SPL pool.
struct Spl<'a, 'info> {
    mint: &'a InterfaceAccount<'info, Mint>,
    token_program: &'a Interface<'info, TokenInterface>,
    associated_token_program: &'a Program<'info, AssociatedToken>,
}

/// One payment out of the vault to a verified wallet: SOL directly; SPL to the wallet's ATA,
/// created idempotently first (PROGRAM §5.4). Nothing is created when `amount == 0`.
#[allow(clippy::too_many_arguments)]
fn pay<'info>(
    spl: Option<&Spl<'_, 'info>>,
    vault: &AccountInfo<'info>,
    pool_info: &AccountInfo<'info>,
    pool_key: &Pubkey,
    vault_bump: u8,
    pool_seeds: &[&[u8]],
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    wallet: &AccountInfo<'info>,
    token_account: Option<&AccountInfo<'info>>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    match spl {
        None => transfer_out_sol(vault, wallet, system_program, pool_key, vault_bump, amount),
        Some(spl) => {
            let ata = token_account.ok_or(ErrorCode::ConstraintAccountIsNone)?;
            create_ata_idempotent(
                payer,
                ata,
                wallet,
                &spl.mint.to_account_info(),
                system_program,
                &spl.token_program.to_account_info(),
                &spl.associated_token_program.to_account_info(),
            )?;
            transfer_out_spl(
                vault,
                spl.mint,
                ata,
                pool_info,
                spl.token_program,
                pool_seeds,
                amount,
            )
        }
    }
}

/// `missing_mut_constraint` names `config` and `game` here: both are read-only and the lint
/// reads a MIR temporary derived from a field read as a write (the Step 2 false positive).
/// Shown by `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` without
/// this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_settle(ctx: Context<Settle>, quarter: u8) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let Settle {
        score_authority,
        game,
        pool,
        vault,
        winner,
        fee_wallet,
        creator,
        integrator,
        mint,
        winner_token_account,
        fee_token_account,
        creator_token_account,
        integrator_token_account,
        token_program,
        associated_token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.5 checks, in order.
    pool.require_drawn()?;
    let next = pool
        .quarters_settled
        .checked_add(1)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(quarter == next, MybarpoolError::QuarterOutOfOrder);
    require!(
        game.quarters_posted >= quarter,
        MybarpoolError::ScoresNotPosted
    );
    let q = usize::from(quarter - 1); // `quarter ≥ 1` after the ordering check
    let home = game.home_score[q];
    let away = game.away_score[q];
    // PROGRAM §6.3: the winner is computed here and the passed account only verified.
    let b = winning_box(home, away, &pool.home_axis, &pool.away_axis)?;
    require_keys_eq!(
        pool.owners[usize::from(b)],
        winner.key(),
        MybarpoolError::WinnerMismatch
    );
    // The integrator slot: required and verified when the pool has one, never read otherwise.
    let integrator_info = if pool.integrator != Pubkey::default() {
        let integrator = integrator
            .as_ref()
            .ok_or(MybarpoolError::FeeAccountMismatch)?;
        require_keys_eq!(
            integrator.key(),
            pool.integrator,
            MybarpoolError::FeeAccountMismatch
        );
        Some(integrator.to_account_info())
    } else {
        None
    };

    // Token plumbing: for an SPL pool every optional account is required (Anchor's own error),
    // the program is the one the pool recorded, and every token account is the recipient's
    // ATA for the pool's mint and token program — checked here, before any CPI, so a wrong
    // account fails with this program's error (PROGRAM §5.4).
    let spl = if pool.token != 0 {
        let mint = mint.as_ref().ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let token_program = token_program
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let associated_token_program = associated_token_program
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(token_program.key(), pool.token_program);
        let ata = |wallet: &Pubkey| {
            get_associated_token_address_with_program_id(wallet, &mint.key(), &token_program.key())
        };
        let winner_ata = winner_token_account
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(
            winner_ata.key(),
            ata(&winner.key()),
            MybarpoolError::WinnerMismatch
        );
        let fee_ata = fee_token_account
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(
            fee_ata.key(),
            ata(&fee_wallet.key()),
            MybarpoolError::FeeAccountMismatch
        );
        let creator_ata = creator_token_account
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(
            creator_ata.key(),
            ata(&creator.key()),
            MybarpoolError::FeeAccountMismatch
        );
        if integrator_info.is_some() {
            let integrator_ata = integrator_token_account
                .as_ref()
                .ok_or(MybarpoolError::FeeAccountMismatch)?;
            require_keys_eq!(
                integrator_ata.key(),
                ata(&pool.integrator),
                MybarpoolError::FeeAccountMismatch
            );
        }
        Some(Spl {
            mint,
            token_program,
            associated_token_program,
        })
    } else {
        None
    };

    // PROGRAM §4.5 effects, in this order and nothing between.
    // (1) The first settlement fixes the prizes, whatever this quarter's share.
    if pool.quarters_settled == 0 {
        let fees = FeeAmounts {
            platform_fee: pool.platform_fee,
            creator_fee: pool.creator_fee,
            integrator_fee: pool.integrator_fee,
        };
        pool.prize_pool = prize_pool(pool.price, &fees, pool.sponsored_total)?;
        pool.quarter_prize = quarter_prizes(pool.prize_pool, pool.preset)?;
        pool.unpaid_prize_pool = pool.prize_pool;
    }
    // (2)
    pool.winning_box[q] = b;
    // (3) The prize, then — with the first non-zero prize only — the fees, all in this
    // instruction with no early return between them: any failure reverts them all.
    let amount = pool.quarter_prize[q];
    let mut fees_paid_now = false;
    if amount > 0 {
        let pool_key = pool.key();
        let game_key = pool.game;
        let creator_key = pool.creator;
        let nonce_bytes = pool.nonce.to_le_bytes();
        let pool_bump = [pool.bump];
        let pool_seeds = pool_signer_seeds(&game_key, &creator_key, &nonce_bytes, &pool_bump);
        let vault_bump = pool.vault_bump;
        let (platform_fee, creator_fee, integrator_fee) =
            (pool.platform_fee, pool.creator_fee, pool.integrator_fee);
        let vault_info = vault.to_account_info();
        let pool_info = pool.to_account_info();
        let payer = score_authority.to_account_info();
        let system_info = system_program.to_account_info();
        let winner_ta = winner_token_account.as_ref().map(|a| a.to_account_info());

        pay(
            spl.as_ref(),
            &vault_info,
            &pool_info,
            &pool_key,
            vault_bump,
            &pool_seeds,
            &payer,
            &system_info,
            &winner.to_account_info(),
            winner_ta.as_ref(),
            amount,
        )?;
        pool.unpaid_prize_pool = pool
            .unpaid_prize_pool
            .checked_sub(amount)
            .ok_or(MybarpoolError::MathOverflow)?;

        if !pool.fees_paid {
            let fee_ta = fee_token_account.as_ref().map(|a| a.to_account_info());
            let creator_ta = creator_token_account.as_ref().map(|a| a.to_account_info());
            let integrator_ta = integrator_token_account
                .as_ref()
                .map(|a| a.to_account_info());
            pay(
                spl.as_ref(),
                &vault_info,
                &pool_info,
                &pool_key,
                vault_bump,
                &pool_seeds,
                &payer,
                &system_info,
                &fee_wallet.to_account_info(),
                fee_ta.as_ref(),
                platform_fee,
            )?;
            pay(
                spl.as_ref(),
                &vault_info,
                &pool_info,
                &pool_key,
                vault_bump,
                &pool_seeds,
                &payer,
                &system_info,
                &creator.to_account_info(),
                creator_ta.as_ref(),
                creator_fee,
            )?;
            if let Some(integrator_info) = integrator_info.as_ref() {
                pay(
                    spl.as_ref(),
                    &vault_info,
                    &pool_info,
                    &pool_key,
                    vault_bump,
                    &pool_seeds,
                    &payer,
                    &system_info,
                    integrator_info,
                    integrator_ta.as_ref(),
                    integrator_fee,
                )?;
            }
            pool.fees_paid = true;
            fees_paid_now = true;
        }
    }
    // (4)
    pool.quarters_settled = next;
    if pool.quarters_settled == crate::constants::QUARTERS {
        pool.status = PoolStatus::Settled;
    }

    let (platform_fee, creator_fee, integrator_fee) = if fees_paid_now {
        (pool.platform_fee, pool.creator_fee, pool.integrator_fee)
    } else {
        (0, 0, 0)
    };
    emit_cpi!(QuarterSettled {
        time: now,
        pool: pool.key(),
        quarter,
        home,
        away,
        box_index: b,
        winner: winner.key(),
        amount,
        fees_paid_now,
        platform_fee,
        creator_fee,
        integrator_fee,
    });
    Ok(())
}
