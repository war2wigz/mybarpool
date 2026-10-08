//! Money leaving the vault (PROGRAM §5.4), shared by every instruction that
//! pays someone: `settle` (Step 6), and the §4.6 returns, splits and reclaims
//! (Step 7). One implementation routes SOL and SPL, creates the recipient's
//! associated token account idempotently when funds move to it, and skips a
//! zero amount, so the audit reads one function for every outbound transfer.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::vault::{create_ata_idempotent, transfer_out_sol, transfer_out_spl};

/// The SPL plumbing of a payout, present only for an SPL pool.
pub(crate) struct Spl<'a, 'info> {
    pub mint: &'a InterfaceAccount<'info, Mint>,
    pub token_program: &'a Interface<'info, TokenInterface>,
    pub associated_token_program: &'a Program<'info, AssociatedToken>,
}

/// One payment out of the vault to a verified wallet (PROGRAM §5.4): SOL by
/// `system_program::transfer` signed with the vault seeds, directly to the wallet; SPL by
/// `transfer_checked` signed with the pool seeds to the wallet's associated token account,
/// `create_idempotent`-ed first with `payer` covering its rent. Nothing is created and nothing
/// moves when `amount == 0`. The caller has already verified `wallet` against the pool or the
/// config and `token_account` against the ATA derivation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pay_out<'info>(
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
