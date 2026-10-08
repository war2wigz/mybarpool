//! Money leaving the vault (PROGRAM §5.4), shared by every instruction that
//! pays someone: `settle` (Step 6), and the §4.6 returns, splits and reclaims
//! (Step 7). One implementation routes SOL and SPL, creates the recipient's
//! associated token account idempotently when funds move to it, and skips a
//! zero amount, so the audit reads one function for every outbound transfer.

use anchor_lang::prelude::*;
use anchor_spl::associated_token::{get_associated_token_address_with_program_id, AssociatedToken};
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::errors::MybarpoolError;
use crate::state::Pool;
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

/// Everything a payout needs besides the recipient: the vault, the pool (as account info and as
/// signer material), the payer for any ATA rent, the System program, and the SPL plumbing.
pub(crate) struct VaultCtx<'a, 'info> {
    pub vault: &'a AccountInfo<'info>,
    pub pool_info: &'a AccountInfo<'info>,
    pub pool_key: Pubkey,
    pub vault_bump: u8,
    pub pool_seeds: &'a [&'a [u8]],
    pub payer: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
    pub spl: Option<Spl<'a, 'info>>,
}

impl<'a, 'info> VaultCtx<'a, 'info> {
    /// `pay_out` with this context.
    pub fn pay(
        &self,
        wallet: &AccountInfo<'info>,
        token_account: Option<&AccountInfo<'info>>,
        amount: u64,
    ) -> Result<()> {
        pay_out(
            self.spl.as_ref(),
            self.vault,
            self.pool_info,
            &self.pool_key,
            self.vault_bump,
            self.pool_seeds,
            self.payer,
            self.system_program,
            wallet,
            token_account,
            amount,
        )
    }

    /// The recipient's associated token account for the pool's mint and token program.
    pub fn ata_of(&self, wallet: &Pubkey) -> Option<Pubkey> {
        self.spl.as_ref().map(|s| {
            get_associated_token_address_with_program_id(
                wallet,
                &s.mint.key(),
                &s.token_program.key(),
            )
        })
    }
}

/// PROGRAM §4.6 owner batches, shared by `return_boxes` and `split`. The remaining accounts are
/// `[owner]*` on SOL and `[owner, ata]*` on SPL (an odd SPL list is `AccountNotEnoughKeys`).
/// For every entry: the boxes `b` with `owners[b] == owner` and bit `b` clear, ascending; none →
/// skipped, no transfer, no event; else `amount = boxes.len() × per_box` (checked), the ATA
/// compared to the derivation **before** the CPI (`RequireKeysEqViolated`), one payment, the
/// bits set, then `on_paid(pool, owner, boxes, amount)` for the event and any bookkeeping.
/// Returns the number of boxes paid over the call. The list decides **which** owners, never how
/// much or to where.
pub(crate) fn pay_owner_batch<'info>(
    pool: &mut Pool,
    ctx: &VaultCtx<'_, 'info>,
    remaining: &[AccountInfo<'info>],
    per_box: u64,
    mut on_paid: impl FnMut(&mut Pool, &Pubkey, Vec<u8>, u64) -> Result<()>,
) -> Result<u8> {
    let stride = if ctx.spl.is_some() { 2 } else { 1 };
    require!(
        remaining.len().is_multiple_of(stride),
        ErrorCode::AccountNotEnoughKeys
    );
    let mut paid: u8 = 0;
    for entry in remaining.chunks_exact(stride) {
        let owner = &entry[0];
        let boxes = pool.unreturned_boxes_of(owner.key);
        if boxes.is_empty() {
            continue;
        }
        let count = u64::try_from(boxes.len()).map_err(|_| MybarpoolError::MathOverflow)?;
        let amount = count
            .checked_mul(per_box)
            .ok_or(MybarpoolError::MathOverflow)?;
        let token_account = if stride == 2 {
            let ata = &entry[1];
            if let Some(derived) = ctx.ata_of(owner.key) {
                require_keys_eq!(*ata.key, derived);
            }
            Some(ata)
        } else {
            None
        };
        ctx.pay(owner, token_account, amount)?;
        for &b in &boxes {
            pool.mark_returned(b);
        }
        paid = paid
            .checked_add(boxes.len() as u8) // ≤ 25
            .ok_or(MybarpoolError::MathOverflow)?;
        on_paid(pool, owner.key, boxes, amount)?;
    }
    Ok(paid)
}
