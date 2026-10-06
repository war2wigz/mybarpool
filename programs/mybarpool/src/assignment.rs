//! PROGRAM §6.1 box assignment, as a pure function so the vector tests can
//! call it directly. `sha256` is the Solana `hashv` syscall; the reference
//! implementation is `assignBoxes` in `packages/shared`, and the two are
//! cross-tested on the shared vector files.

use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;

use crate::constants::BOXES;
use crate::errors::MybarpoolError;

/// PROGRAM §6.1. `owners` holds the 25 box owners (`Pubkey::default()` = unsold); `sold` is the
/// count before this purchase and must equal the owned count; `count` is 1–(25 − sold). Writes
/// `buyer` into the assigned slots and returns their 0-based indices in assignment order.
///
/// ```text
/// seed      = sha256(slothash || buyer || [sold] || [count])
/// remaining = [b for b in 0..25 if owners[b] == default]    // ascending
/// for k in 0..count:
///     r   = sha256(seed || [k])
///     idx = u64_le(r[0..8]) mod len(remaining)
///     box = remaining.remove(idx)                              // removes and shifts
///     owners[box] = buyer
/// ```
pub fn assign_boxes(
    slothash: &[u8; 32],
    buyer: &Pubkey,
    sold: u8,
    count: u8,
    owners: &mut [Pubkey; BOXES as usize],
) -> Result<Vec<u8>> {
    let mut remaining: Vec<u8> = Vec::with_capacity(usize::from(BOXES));
    for (index, owner) in owners.iter().enumerate() {
        if *owner == Pubkey::default() {
            remaining.push(u8::try_from(index).map_err(|_| MybarpoolError::MathOverflow)?);
        }
    }
    // `sold` is the caller's count of owned boxes; the two must agree or the purchase is
    // inconsistent (PROGRAM §6.1 "sold ... before this purchase").
    let owned = BOXES
        .checked_sub(u8::try_from(remaining.len()).map_err(|_| MybarpoolError::MathOverflow)?)
        .ok_or(MybarpoolError::MathOverflow)?;
    require!(owned == sold, MybarpoolError::TooManyBoxes);
    require!(count >= 1, MybarpoolError::NothingToBuy);
    require!(
        usize::from(count) <= remaining.len(),
        MybarpoolError::TooManyBoxes
    );

    let seed = hashv(&[slothash, buyer.as_ref(), &[sold], &[count]]).to_bytes();
    let mut boxes = Vec::with_capacity(usize::from(count));
    for k in 0..count {
        let r = hashv(&[&seed, &[k]]).to_bytes();
        let mut word = [0u8; 8];
        word.copy_from_slice(r.get(0..8).ok_or(MybarpoolError::MathOverflow)?);
        let len = u64::try_from(remaining.len()).map_err(|_| MybarpoolError::MathOverflow)?;
        let idx = usize::try_from(u64::from_le_bytes(word) % len)
            .map_err(|_| MybarpoolError::MathOverflow)?;
        require!(idx < remaining.len(), MybarpoolError::MathOverflow);
        let chosen = remaining.remove(idx);
        *owners
            .get_mut(usize::from(chosen))
            .ok_or(MybarpoolError::MathOverflow)? = *buyer;
        boxes.push(chosen);
    }
    Ok(boxes)
}
