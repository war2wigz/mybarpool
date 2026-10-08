//! PROGRAM §5.1 fee amounts, §5.2 prizes and the §4.3 price ladder, as pure
//! functions. The reference implementations are `feeAmounts`, `prizePool`
//! and `quarterPrizes` in `packages/shared`; the two sides are cross-tested
//! on the shared vector files.

use anchor_lang::prelude::*;

use crate::constants::{PayoutPreset, BOXES, BPS_DENOMINATOR, QUARTERS};
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

/// PROGRAM §5.2 `prize_pool = P − platform_fee − creator_fee − integrator_fee + sponsored_total`
/// with `P = BOXES × price`: the product in `u128`, every subtraction and the addition checked,
/// back to `u64` with `try_from`; `MathOverflow` on any failure.
pub fn prize_pool(price: u64, fees: &FeeAmounts, sponsored_total: u64) -> Result<u64> {
    let pot = u128::from(price)
        .checked_mul(u128::from(BOXES))
        .ok_or(MybarpoolError::MathOverflow)?;
    let after_fees = pot
        .checked_sub(u128::from(fees.platform_fee))
        .and_then(|v| v.checked_sub(u128::from(fees.creator_fee)))
        .and_then(|v| v.checked_sub(u128::from(fees.integrator_fee)))
        .ok_or(MybarpoolError::MathOverflow)?;
    let total = after_fees
        .checked_add(u128::from(sponsored_total))
        .ok_or(MybarpoolError::MathOverflow)?;
    u64::try_from(total).map_err(|_| MybarpoolError::MathOverflow.into())
}

/// PROGRAM §5.2 `quarter_prize[q] = floor(prize_pool × split[q] / 100)` for the preset's split
/// (PROGRAM §1 preset table); `u128` product, `MathOverflow` on any failure.
pub fn quarter_prizes(prize_pool: u64, preset: PayoutPreset) -> Result<[u64; QUARTERS as usize]> {
    let mut out = [0u64; QUARTERS as usize];
    for (q, share) in preset.split().iter().enumerate() {
        let product = u128::from(prize_pool)
            .checked_mul(u128::from(*share))
            .ok_or(MybarpoolError::MathOverflow)?;
        let amount = product
            .checked_div(100)
            .ok_or(MybarpoolError::MathOverflow)?;
        out[q] = u64::try_from(amount).map_err(|_| MybarpoolError::MathOverflow)?;
    }
    Ok(out)
}

/// PROGRAM §5.2 `dust = prize_pool − Σ quarter_prize` (≤ 3 base units on any preset), checked.
pub fn dust(prize_pool: u64, quarter_prize: &[u64; QUARTERS as usize]) -> Result<u64> {
    let paid = quarter_prize
        .iter()
        .try_fold(0u64, |acc, &p| acc.checked_add(p))
        .ok_or(MybarpoolError::MathOverflow)?;
    prize_pool
        .checked_sub(paid)
        .ok_or_else(|| MybarpoolError::MathOverflow.into())
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
