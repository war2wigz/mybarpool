//! PROGRAM §6.4: the allowlist Merkle tree, verification side. `packages/shared` builds roots
//! and proofs (`allowlist.ts`); this module only folds a proof and compares. Pure, no accounts;
//! cross-tested through `packages/shared/src/vectors/allowlist.json` in `tests/vectors.rs`.
//!
//! ```text
//! leaf(w)    = sha256([0x00] || w)
//! node(a, b) = sha256([0x01] || min(a, b) || max(a, b))     // bytewise order, no direction bits
//! verify     = fold(leaf(w), proof, node) == root, with len(proof) ≤ 32
//! ```
//!
//! The leading byte separates leaves from nodes so no interior node can be presented as a
//! wallet; sorted pairs mean a proof carries only sibling hashes. `sha256` is the Solana
//! `hashv` syscall, the same call §6.1 makes.

use anchor_lang::prelude::Pubkey;
use solana_sha256_hasher::hashv;

/// PROGRAM §6.4: a proof longer than this is invalid before anything is hashed.
pub const MAX_PROOF_LEN: usize = 32;

/// The §6.4 leaf prefix.
pub const LEAF_PREFIX: u8 = 0x00;
/// The §6.4 interior-node prefix.
pub const NODE_PREFIX: u8 = 0x01;

/// `leaf(w) = sha256([0x00] || w)`.
pub fn leaf(wallet: &Pubkey) -> [u8; 32] {
    hashv(&[&[LEAF_PREFIX], wallet.as_ref()]).to_bytes()
}

/// `node(a, b) = sha256([0x01] || min(a, b) || max(a, b))`, the pair in bytewise order.
pub fn node(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    hashv(&[&[NODE_PREFIX], lo, hi]).to_bytes()
}

/// `fold(leaf(wallet), proof, node) == root`; `false` for a proof longer than
/// [`MAX_PROOF_LEN`] without hashing anything. One wallet in the list means `root == leaf`
/// and an empty proof.
pub fn verify(root: &[u8; 32], wallet: &Pubkey, proof: &[[u8; 32]]) -> bool {
    if proof.len() > MAX_PROOF_LEN {
        return false;
    }
    let computed = proof
        .iter()
        .fold(leaf(wallet), |acc, sibling| node(&acc, sibling));
    computed == *root
}
