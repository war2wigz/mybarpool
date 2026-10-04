//! Mint checks for a `TokenRule` (PROGRAM §3.1 notes, §5.4). Shared by
//! `initialize` and `update_config`.

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::extension::{
    BaseStateWithExtensions, ExtensionType, StateWithExtensions,
};
use anchor_spl::token_2022::spl_token_2022::state::Mint as Mint2022;
use anchor_spl::token_interface::Mint;

use crate::errors::MybarpoolError;
use crate::state::TokenRule;

/// Check the mint account a non-default `TokenRule` points at.
///
/// A default mint needs no account (it is ignored if passed). A non-default mint requires the
/// account: its key, owner (the token program) and decimals must match the rule, and under
/// Token-2022 it must carry neither `TransferFeeConfig` nor `TransferHook` (PROGRAM §5.4).
/// Other extensions are allowed. `InterfaceAccount<Mint>` already guarantees the owner is one
/// of the two token programs.
///
/// `missing_mut_constraint` (OtterSec anchor-lints) is snippet-based and reads every borrow of
/// `rule.mint` / `mint.decimals` here as a write; the mint is read-only.
#[cfg_attr(dylint_lib = "missing_mut_constraint", allow(missing_mut_constraint))]
pub fn check_mint(rule: &TokenRule, mint: Option<&InterfaceAccount<Mint>>) -> Result<()> {
    if rule.has_default_mint() {
        return Ok(());
    }
    let mint = mint.ok_or(MybarpoolError::InvalidConfig)?;
    require_keys_eq!(mint.key(), rule.mint, MybarpoolError::InvalidConfig);
    let info = mint.to_account_info();
    require_keys_eq!(
        *info.owner,
        rule.token_program,
        MybarpoolError::InvalidConfig
    );
    require!(
        mint.decimals == rule.decimals,
        MybarpoolError::InvalidConfig
    );

    if *info.owner == anchor_spl::token_2022::ID {
        let data = info.try_borrow_data()?;
        let state = StateWithExtensions::<Mint2022>::unpack(&data)?;
        let extensions = state.get_extension_types()?;
        let unsupported = extensions.iter().any(|e| {
            matches!(
                e,
                ExtensionType::TransferFeeConfig | ExtensionType::TransferHook
            )
        });
        require!(!unsupported, MybarpoolError::UnsupportedMintExtension);
    }
    Ok(())
}
