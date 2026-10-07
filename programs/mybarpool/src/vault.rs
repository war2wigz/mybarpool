//! The vault, PROGRAM §3.4 and §5.4: its two creation paths and the two
//! inbound transfers, shared by `create_pool`, `buy` and `sponsor` so money
//! enters a pool through one implementation. Nothing here moves money out.
//!
//! SOL: a system-owned PDA with no data (`["vault", pool]`), funded with its
//! rent-exempt minimum by the creator; `system_program::transfer` in and, in
//! Steps 6–7, out signed with the vault seeds. SPL: a token account at the
//! same PDA, `owner = pool`, created by `create_account` signed with the
//! vault seeds and `initialize_account3`; `transfer_checked` in and, later,
//! out signed with the pool seeds.

use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::token_2022::spl_token_2022::extension::{
    BaseStateWithExtensions, ExtensionType, StateWithExtensions,
};
use anchor_spl::token_2022::spl_token_2022::state::{
    Account as TokenAccount2022, Mint as Mint2022,
};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface};

use crate::constants::VAULT_SEED;
use crate::errors::MybarpoolError;

/// SOL vault: the creator funds the PDA with `minimum_balance(0)` so it is rent-exempt with no
/// data and stays system-owned (PROGRAM §3.4). Returns the lamports moved.
pub fn fund_sol_vault<'info>(
    creator: &Signer<'info>,
    vault: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<u64> {
    let rent = Rent::get()?.minimum_balance(0);
    transfer_in_sol(creator, vault, system_program, rent)?;
    Ok(rent)
}

/// SPL vault: a token account for `mint` at the vault PDA, `authority = pool`, under the mint's
/// own token program. The account length follows the mint's extensions (165 for a mint with no
/// account-side requirements, ORE included); `create_account` is signed with the vault seeds,
/// the only place in this step that signs with them.
pub fn create_spl_vault<'info>(
    creator: &Signer<'info>,
    vault: &AccountInfo<'info>,
    pool: &AccountInfo<'info>,
    mint: &InterfaceAccount<'info, Mint>,
    token_program: &Interface<'info, TokenInterface>,
    system_program: &AccountInfo<'info>,
    vault_seeds: &[&[u8]],
) -> Result<()> {
    let space = vault_token_account_len(mint, token_program)?;
    let lamports = Rent::get()?.minimum_balance(space);
    let space_u64 = u64::try_from(space).map_err(|_| MybarpoolError::MathOverflow)?;
    if vault.lamports() == 0 {
        system_program::create_account(
            CpiContext::new_with_signer(
                *system_program.key,
                system_program::CreateAccount {
                    from: creator.to_account_info(),
                    to: vault.clone(),
                },
                &[vault_seeds],
            ),
            lamports,
            space_u64,
            token_program.key,
        )?;
    } else {
        // The PDA already holds lamports (anyone can send to a derivable address), and the
        // System program's create_account refuses a funded target (Step 4 audit M1). The
        // sequence Anchor 1.2's `init` uses instead: top up to the rent minimum, allocate,
        // assign — the last two signed with the vault seeds.
        let shortfall = lamports.saturating_sub(vault.lamports());
        if shortfall > 0 {
            transfer_in_sol(creator, vault, system_program, shortfall)?;
        }
        system_program::allocate(
            CpiContext::new_with_signer(
                *system_program.key,
                system_program::Allocate {
                    account_to_allocate: vault.clone(),
                },
                &[vault_seeds],
            ),
            space_u64,
        )?;
        system_program::assign(
            CpiContext::new_with_signer(
                *system_program.key,
                system_program::Assign {
                    account_to_assign: vault.clone(),
                },
                &[vault_seeds],
            ),
            token_program.key,
        )?;
    }
    token_interface::initialize_account3(CpiContext::new(
        *token_program.key,
        token_interface::InitializeAccount3 {
            account: vault.clone(),
            mint: mint.to_account_info(),
            authority: pool.clone(),
        },
    ))
}

/// Token account length for a vault of `mint`: the base 165 bytes plus whatever account-side
/// extensions the mint's own extensions require (Token-2022), computed rather than assumed.
fn vault_token_account_len(
    mint: &InterfaceAccount<'_, Mint>,
    token_program: &Interface<'_, TokenInterface>,
) -> Result<usize> {
    let mint_extensions: Vec<ExtensionType> = if *token_program.key == anchor_spl::token_2022::ID {
        let info = mint.to_account_info();
        let data = info.try_borrow_data()?;
        StateWithExtensions::<Mint2022>::unpack(&data)?.get_extension_types()?
    } else {
        Vec::new()
    };
    let required = ExtensionType::get_required_init_account_extensions(&mint_extensions);
    Ok(ExtensionType::try_calculate_account_len::<TokenAccount2022>(&required)?)
}

/// SOL in: `from` → vault by `system_program::transfer`. `from` is typed `Signer` so no caller
/// can pass an account that did not sign.
///
/// `missing_signer_validation` resolves CPI signers to fields of the handler's `Accounts`
/// struct; in a free function it falls back to the parameter's source text and can never match
/// it to a `Signer` field, so it reports the parameter (`from: &Signer<'info>`) itself. The
/// type is the validation. Shown by `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all
/// --workspace -- --lib` without this line. (`transfer_in_spl` is not flagged: the lint reads
/// `transfer_checked`'s authority through `to_account_info()` differently.)
#[cfg_attr(
    dylint_lib = "missing_signer_validation",
    allow(missing_signer_validation)
)]
pub fn transfer_in_sol<'info>(
    from: &Signer<'info>,
    vault: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    system_program::transfer(
        CpiContext::new(
            *system_program.key,
            system_program::Transfer {
                from: from.to_account_info(),
                to: vault.clone(),
            },
        ),
        amount,
    )
}

/// SPL in: `from_token_account` → vault by `transfer_checked` with the mint's decimals, through
/// the pool's token program. `authority` (the buyer or sponsor) is typed `Signer`.
pub fn transfer_in_spl<'info>(
    from_token_account: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    vault: &AccountInfo<'info>,
    authority: &Signer<'info>,
    token_program: &Interface<'info, TokenInterface>,
    amount: u64,
) -> Result<()> {
    token_interface::transfer_checked(
        CpiContext::new(
            *token_program.key,
            token_interface::TransferChecked {
                from: from_token_account.to_account_info(),
                mint: mint.to_account_info(),
                to: vault.clone(),
                authority: authority.to_account_info(),
            },
        ),
        amount,
        mint.decimals,
    )
}

/// The vault seeds with the stored bump, for `create_spl_vault`'s signer.
pub fn vault_signer_seeds<'a>(pool: &'a Pubkey, bump: &'a [u8; 1]) -> [&'a [u8]; 3] {
    [VAULT_SEED, pool.as_ref(), bump]
}
