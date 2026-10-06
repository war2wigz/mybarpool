//! PROGRAM §5.1 fee amounts and the §4.3 price ladder, as pure functions.
//! The reference implementation is `feeAmounts` in `packages/shared`; the
//! two are cross-tested on the shared vector files.

use anchor_lang::prelude::*;

use crate::constants::{BOXES, BPS_DENOMINATOR};
use crate::errors::MybarpoolError;
use crate::state::TokenRule;

/// PROGRAM §5.1: the three amounts fixed at creation, base units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeAmounts {
    /// `floor(P × platform_bps / 10_000)`.
    pub platform_fee: u64,
    /// `floor(P × (creator_bps + creator_addon_bps) / 10_000)`: base and add-on in one amount.
    pub creator_fee: u64,
    /// `floor(P × integrator_bps / 10_000)`; 0 when unset.
    pub integrator_fee: u64,
}

/// PROGRAM §5.1 with `P = BOXES × price`. Products in `u128`, back to `u64` with `try_from`;
/// `MathOverflow` on any failure. Fees are computed on the pot only, never on a sponsorship.
pub fn fee_amounts(
    price: u64,
    platform_bps: u16,
    creator_bps: u16,
    creator_addon_bps: u16,
    integrator_bps: u16,
) -> Result<FeeAmounts> {
    let pot = u128::from(price)
        .checked_mul(u128::from(BOXES))
        .ok_or(MybarpoolError::MathOverflow)?;
    let creator_total_bps = creator_bps
        .checked_add(creator_addon_bps)
        .ok_or(MybarpoolError::MathOverflow)?;
    Ok(FeeAmounts {
        platform_fee: share(pot, platform_bps)?,
        creator_fee: share(pot, creator_total_bps)?,
        integrator_fee: share(pot, integrator_bps)?,
    })
}

fn share(pot: u128, bps: u16) -> Result<u64> {
    let product = pot
        .checked_mul(u128::from(bps))
        .ok_or(MybarpoolError::MathOverflow)?;
    let amount = product
        .checked_div(u128::from(BPS_DENOMINATOR))
        .ok_or(MybarpoolError::MathOverflow)?;
    u64::try_from(amount).map_err(|_| MybarpoolError::MathOverflow.into())
}

/// PROGRAM §4.3: `min_price ≤ price ≤ max_price` and `(price − min_price) % step == 0`, else
/// `PriceOffLadder`. `step ≥ 1` is a §3.1 invariant the config already enforces.
pub fn price_on_ladder(rule: &TokenRule, price: u64) -> Result<()> {
    require!(price >= rule.min_price, MybarpoolError::PriceOffLadder);
    require!(price <= rule.max_price, MybarpoolError::PriceOffLadder);
    let offset = price
        .checked_sub(rule.min_price)
        .ok_or(MybarpoolError::PriceOffLadder)?;
    let remainder = offset
        .checked_rem(rule.step)
        .ok_or(MybarpoolError::PriceOffLadder)?;
    require!(remainder == 0, MybarpoolError::PriceOffLadder);
    Ok(())
}
