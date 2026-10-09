/**
 * Allowlist root and proof, PROGRAM §6.4. The reference the program verifies against
 * (`allowlist::verify` in the program folds a proof; this module builds roots and proofs).
 *
 * ```
 * leaf(w)      = sha256([0x00] || w)
 * node(a, b)   = sha256([0x01] || min(a, b) || max(a, b))      // bytewise order; no direction bits
 * level 0      = [leaf(w) for w in wallets]                      // sorted bytewise, deduplicated
 * level k + 1  = [node(l[2i], l[2i+1]) for pairs]; an unpaired last element is carried up unchanged
 * root         = the single element of the top level; one wallet → root = leaf(w)
 * proof(w)     = the sibling at each level, bottom up (no entry for a level where it was carried up)
 * verify       = fold(leaf(w), proof, node) == root, with len(proof) ≤ 32
 * ```
 *
 * The leading byte separates leaves from nodes so no interior node can be passed off as a
 * wallet; sorted pairs keep a proof to the sibling hashes alone; the canonical list order means
 * two clients given the same wallets compute the same root and anyone can check a root against
 * a published list.
 */
import { sha256 } from "./bytes.js";

/** PROGRAM §6.4: a proof longer than this is invalid before anything is hashed. */
export const ALLOWLIST_MAX_PROOF = 32;

const LEAF_PREFIX = Uint8Array.of(0x00);
const NODE_PREFIX = Uint8Array.of(0x01);

function compareBytes(a: Uint8Array, b: Uint8Array): number {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const d = a[i]! - b[i]!;
    if (d !== 0) return d;
  }
  return a.length - b.length;
}

function assertWallet(wallet: Uint8Array): void {
  if (wallet.length !== 32) throw new RangeError(`a wallet is 32 bytes, got ${wallet.length}`);
}

/** `leaf(w) = sha256([0x00] || w)`. */
export function allowlistLeaf(wallet: Uint8Array): Uint8Array {
  assertWallet(wallet);
  return sha256(LEAF_PREFIX, wallet);
}

/** `node(a, b) = sha256([0x01] || min(a, b) || max(a, b))`. */
export function allowlistNode(a: Uint8Array, b: Uint8Array): Uint8Array {
  return compareBytes(a, b) <= 0 ? sha256(NODE_PREFIX, a, b) : sha256(NODE_PREFIX, b, a);
}

/**
 * The wallet list as §6.4 hashes it: sorted bytewise ascending, duplicates removed. The other
 * functions call this, so callers may pass any order with repeats. Empty in → empty out.
 */
export function canonicalAllowlist(wallets: readonly Uint8Array[]): Uint8Array[] {
  for (const w of wallets) assertWallet(w);
  const sorted = [...wallets].sort(compareBytes);
  const out: Uint8Array[] = [];
  for (const w of sorted) {
    const last = out[out.length - 1];
    if (last === undefined || compareBytes(last, w) !== 0) out.push(w);
  }
  return out;
}

function nextLevel(level: readonly Uint8Array[]): Uint8Array[] {
  const out: Uint8Array[] = [];
  for (let i = 0; i < level.length; i += 2) {
    const right = level[i + 1];
    out.push(right === undefined ? level[i]! : allowlistNode(level[i]!, right));
  }
  return out;
}

/** The §6.4 root over `wallets`. Throws on an empty list (an `Allowlist` pool needs a root ≠ 0). */
export function allowlistRoot(wallets: readonly Uint8Array[]): Uint8Array {
  let level = canonicalAllowlist(wallets).map(allowlistLeaf);
  if (level.length === 0) throw new RangeError("an allowlist needs at least one wallet");
  while (level.length > 1) level = nextLevel(level);
  return level[0]!;
}

/**
 * The §6.4 proof for `wallet`: the sibling at each level, bottom up, with no entry where the
 * node was carried up unpaired. Throws when `wallet` is not in the list.
 */
export function allowlistProof(wallets: readonly Uint8Array[], wallet: Uint8Array): Uint8Array[] {
  assertWallet(wallet);
  const list = canonicalAllowlist(wallets);
  let index = list.findIndex((w) => compareBytes(w, wallet) === 0);
  if (index < 0) throw new RangeError("the wallet is not on the allowlist");
  let level = list.map(allowlistLeaf);
  const proof: Uint8Array[] = [];
  while (level.length > 1) {
    const sibling = level[index ^ 1];
    if (sibling !== undefined) proof.push(sibling);
    level = nextLevel(level);
    index >>= 1;
  }
  return proof;
}

/** `fold(leaf(wallet), proof, node) == root`, false for more than 32 entries. */
export function verifyAllowlistProof(
  root: Uint8Array,
  wallet: Uint8Array,
  proof: readonly Uint8Array[],
): boolean {
  if (proof.length > ALLOWLIST_MAX_PROOF) return false;
  let acc = allowlistLeaf(wallet);
  for (const sibling of proof) acc = allowlistNode(acc, sibling);
  return compareBytes(acc, root) === 0;
}
