//! `close_pool`, PROGRAM §4.5: close a terminal pool with no open sponsorship,
//! sweeping the vault's remaining balance (the dust, and for SOL the vault's
//! own rent) and the pool account's rent to one verified destination —
//! `fee_wallet`, or the creator when the pool was abandoned (ARCHITECTURE ›
//! Pool creation). Permissionless: the signer only pays for a missing
//! destination token account. In Step 6 only the `Settled` branch is reachable
//! through the program; the `Returned`/`Split` checks are written now and
//! proven on planted pools until Step 7 reaches them.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::{get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::token_2022::spl_token_2022::extension::StateWithExtensions;
use anchor_spl::token_2022::spl_token_2022::state::Account as TokenAccount2022;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::constants::{CONFIG_SEED, POOL_SEED, VAULT_SEED};
use crate::errors::MybarpoolError;
use crate::events::PoolClosed;
use crate::state::{PlatformConfig, Pool, PoolStatus};
use crate::vault::{
    close_spl_vault, create_ata_idempotent, pool_signer_seeds, transfer_out_sol, transfer_out_spl,
};

#[derive(Accounts)]
#[event_cpi]
pub struct ClosePool<'info> {
    /// Anyone; pays the rent of the destination's associated token account when it is missing
    /// and the vault holds tokens. Signs nothing else.
    #[account(mut)]
    pub payer: Signer<'info>,
    /// PROGRAM §3.1 `PlatformConfig`; read for `fee_wallet`.
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// PROGRAM §3.3 `Pool`, re-derived from its own fields; closed by the handler to the
    /// destination (`pool.close(...)`, since the destination depends on `abandoned`).
    #[account(
        mut,
        seeds = [POOL_SEED, pool.game.as_ref(), pool.creator.as_ref(), &pool.nonce.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,
    /// CHECK: seeds + the stored `vault_bump` pin it to `pool.vault`.
    /// PROGRAM §3.4 vault; SOL: its whole balance moves out and the runtime reaps it; SPL: the
    /// token balance is swept, then the account is closed with the pool PDA as authority.
    #[account(mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: `address = config.fee_wallet`.
    /// The platform's destination (PROGRAM §3.1); always passed so the account list is static.
    #[account(mut, address = config.fee_wallet @ MybarpoolError::FeeAccountMismatch)]
    pub fee_wallet: UncheckedAccount<'info>,
    /// CHECK: `address = pool.creator`.
    /// The creator; the destination when the pool was abandoned (ARCHITECTURE › Pool creation).
    #[account(mut, address = pool.creator @ MybarpoolError::FeeAccountMismatch)]
    pub creator: UncheckedAccount<'info>,
    /// The pool's mint (`address = pool.mint`); SPL pools only.
    #[account(address = pool.mint)]
    pub mint: Option<InterfaceAccount<'info, Mint>>,
    /// CHECK: compared in the handler to the destination's ATA for the pool's mint.
    /// The destination's associated token account; SPL pools only; created idempotently only
    /// when the vault holds tokens to sweep.
    #[account(mut)]
    pub destination_token_account: Option<UncheckedAccount<'info>>,
    /// The pool's token program (checked against `pool.token_program`); SPL pools only.
    pub token_program: Option<Interface<'info, TokenInterface>>,
    /// The associated-token program, a fixed address; SPL pools only.
    pub associated_token_program: Option<Program<'info, AssociatedToken>>,
    /// For the SOL sweep out of the vault and any ATA rent.
    pub system_program: Program<'info, System>,
}

/// `missing_mut_constraint` names `config` here: read-only, the field-read false positive
/// (Step 2 onward). Shown by `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace
/// -- --lib` without this line.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn handle_close_pool(ctx: Context<ClosePool>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let ClosePool {
        payer,
        pool,
        vault,
        fee_wallet,
        creator,
        mint,
        destination_token_account,
        token_program,
        associated_token_program,
        system_program,
        ..
    } = ctx.accounts;

    // PROGRAM §4.5 checks, in order.
    pool.require_terminal()?;
    require!(
        pool.sponsorships_open == 0,
        MybarpoolError::SponsorshipsStillOpen
    );
    if pool.status != PoolStatus::Settled {
        // Returned / Split: every sold box must have been returned, split or reclaimed.
        require!(
            pool.all_sold_returned(),
            MybarpoolError::BoxesStillOutstanding
        );
    }
    let destination = if pool.abandoned {
        creator.to_account_info()
    } else {
        fee_wallet.to_account_info()
    };

    let pool_key = pool.key();
    let nonce_bytes = pool.nonce.to_le_bytes();
    let pool_bump = [pool.bump];
    let pool_seeds = pool_signer_seeds(&pool.game, &pool.creator, &nonce_bytes, &pool_bump);
    let vault_info = vault.to_account_info();
    let pool_info = pool.to_account_info();
    let system_info = system_program.to_account_info();

    let dust = if pool.token == 0 {
        // SOL: the lamports above the vault's own rent-exempt minimum are the dust; the whole
        // balance, rent included, moves in one transfer so the account ends at zero and the
        // runtime reaps it.
        let balance = vault_info.lamports();
        let dust = balance
            .checked_sub(Rent::get()?.minimum_balance(0))
            .ok_or(MybarpoolError::MathOverflow)?;
        transfer_out_sol(
            &vault_info,
            &destination,
            &system_info,
            &pool_key,
            pool.vault_bump,
            balance,
        )?;
        dust
    } else {
        let mint = mint.as_ref().ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let token_program = token_program
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let associated_token_program = associated_token_program
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        let destination_ata = destination_token_account
            .as_ref()
            .ok_or(ErrorCode::ConstraintAccountIsNone)?;
        require_keys_eq!(token_program.key(), pool.token_program);
        require_keys_eq!(
            destination_ata.key(),
            get_associated_token_address_with_program_id(
                destination.key,
                &mint.key(),
                &token_program.key()
            ),
            MybarpoolError::FeeAccountMismatch
        );
        // The token balance, read before any CPI.
        let dust = {
            let data = vault_info.try_borrow_data()?;
            StateWithExtensions::<TokenAccount2022>::unpack(&data)?
                .base
                .amount
        };
        if dust > 0 {
            create_ata_idempotent(
                &payer.to_account_info(),
                &destination_ata.to_account_info(),
                &destination,
                &mint.to_account_info(),
                &system_info,
                &token_program.to_account_info(),
                &associated_token_program.to_account_info(),
            )?;
            transfer_out_spl(
                &vault_info,
                mint,
                &destination_ata.to_account_info(),
                &pool_info,
                token_program,
                &pool_seeds,
                dust,
            )?;
        }
        close_spl_vault(
            &vault_info,
            &destination,
            &pool_info,
            token_program,
            &pool_seeds,
        )?;
        dust
    };

    emit_cpi!(PoolClosed {
        time: now,
        pool: pool_key,
        destination: *destination.key,
        dust,
    });
    // The pool account last, to the same destination (Anchor's `AccountsClose`).
    pool.close(destination)?;
    Ok(())
}
