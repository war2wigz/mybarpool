//! PROGRAM §6.3, the winning box, as a pure function. Columns are the home
//! team across the top, rows the away team down the side; lane `l` of an
//! axis holds the digits at positions `l` and `l + 5`. The reference
//! implementation is `winningBox` in `packages/shared`; the two are
//! cross-tested on `winner.json` and `winner-bulk.json`.

use anchor_lang::prelude::*;

use crate::axes::DIGITS;
use crate::constants::LANES;
use crate::errors::MybarpoolError;

/// The lane `l` in `0..5` with `digit ∈ {axis[l], axis[l + 5]}`. A loop, not a table. On a
/// drawn pool the axes are permutations of 0–9, so a digit is always found; a missing one is
/// `MathOverflow`, never a panic (the standing rule: no `unwrap` on account data).
pub fn lane_of(axis: &[u8; DIGITS], digit: u8) -> Result<u8> {
    for l in 0..LANES {
        let lane = usize::from(l);
        if axis[lane] == digit || axis[lane + usize::from(LANES)] == digit {
            return Ok(l);
        }
    }
    Err(MybarpoolError::MathOverflow.into())
}

/// PROGRAM §6.3: `col = lane_of(home_axis, home mod 10)`, `row = lane_of(away_axis, away mod 10)`,
/// `box = row × 5 + col` (0–24; the label people see is `box + 1`). Exactly one box matches
/// for any pair of scores.
pub fn winning_box(
    home: u16,
    away: u16,
    home_axis: &[u8; DIGITS],
    away_axis: &[u8; DIGITS],
) -> Result<u8> {
    // `% 10` is below 256, so the casts are exact.
    let col = lane_of(home_axis, (home % 10) as u8)?;
    let row = lane_of(away_axis, (away % 10) as u8)?;
    row.checked_mul(LANES)
        .and_then(|v| v.checked_add(col))
        .ok_or_else(|| MybarpoolError::MathOverflow.into())
}
