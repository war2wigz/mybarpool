//! PROGRAM §6.2 axis shuffle, as a pure function so the vector test can call
//! it directly. `sha256` is `solana_sha256_hasher::hashv`, the same call §6.1
//! uses; Entropy's formulas (§4.4) are keccak and live in `entropy.rs` — the
//! two are never mixed. The reference implementation is `drawAxes` in
//! `packages/shared`, cross-tested on `axes.json` and `axes-bulk.json`.

use solana_sha256_hasher::hashv;

use crate::constants::{AXIS_LABEL_AWAY, AXIS_LABEL_HOME};

/// Digits per axis.
pub const DIGITS: usize = 10;

/// PROGRAM §6.2 `axis(value, label)`:
///
/// ```text
/// seed = sha256(value || label)
/// a = [0..9]
/// for i in 9 down to 1:
///     r = sha256(seed || [i])
///     j = u64_le(r[0..8]) mod (i + 1)
///     swap(a[i], a[j])
/// ```
///
/// Lane `l` (0–4) covers digits `a[l]` and `a[l + 5]`. The only labels are `b"home"` and
/// `b"away"` (domain separation: the two axes are never a copy of one another).
pub fn axis(value: &[u8; 32], label: &[u8]) -> [u8; DIGITS] {
    let seed = hashv(&[value, label]).to_bytes();
    let mut a: [u8; DIGITS] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    for i in (1..DIGITS).rev() {
        // `i` is 1–9, so the cast to u8 is exact and `i + 1` never overflows.
        let r = hashv(&[&seed, &[i as u8]]).to_bytes();
        let mut word = [0u8; 8];
        word.copy_from_slice(&r[0..8]);
        // `i + 1 ≤ 10`, so the remainder fits in usize on every target.
        let j = (u64::from_le_bytes(word) % (i as u64 + 1)) as usize;
        a.swap(i, j);
    }
    a
}

/// PROGRAM §6.2: `(axis(value, b"home"), axis(value, b"away"))`.
pub fn draw_axes(value: &[u8; 32]) -> ([u8; DIGITS], [u8; DIGITS]) {
    (axis(value, AXIS_LABEL_HOME), axis(value, AXIS_LABEL_AWAY))
}
