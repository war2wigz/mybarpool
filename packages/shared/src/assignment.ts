/**
 * Box assignment, PROGRAM §6.1. The reference the program matches in Step 4.
 *
 * ```
 * seed      = sha256(slothash || buyer || [sold] || [count])
 * remaining = [b for b in 0..25 if owners[b] == default]    // ascending
 * for k in 0..count:
 *     r   = sha256(seed || [k])
 *     idx = u64_le(r[0..8]) mod len(remaining)
 *     box = remaining.remove(idx)                              // removes and shifts
 *     owners[box] = buyer
 * ```
 */
import { BOXES, type BoxIndex } from "./boxes.js";
import { sha256, u64le, u8 } from "./bytes.js";

export const PUBKEY_BYTES = 32;

/** A box owner: 32 pubkey bytes, or unowned. `null` and 32 zero bytes (`Pubkey::default()`) both mean unowned. */
export type Owner = Uint8Array | null;

export type Owners = readonly Owner[];

export interface AssignBoxesInput {
  /** 32 bytes: the hash of the most recent `SlotHashes` entry. */
  readonly slothash: Uint8Array;
  /** 32 bytes: the buyer's pubkey. */
  readonly buyer: Uint8Array;
  /** Boxes sold before this purchase, 0–24; must equal the owned count in `owners`. */
  readonly sold: number;
  /** Boxes to assign, 1 ≤ count ≤ 25 − sold. */
  readonly count: number;
  /** 25 entries; unowned boxes are `null` or 32 zero bytes. Not mutated. */
  readonly owners: Owners;
}

export interface AssignBoxesResult {
  /** The assigned indices, in assignment order. */
  readonly boxes: readonly BoxIndex[];
  /** The 25 owners after the purchase; assigned boxes hold a copy of `buyer`. */
  readonly owners: readonly Owner[];
}

export function isUnowned(owner: Owner): boolean {
  if (owner === null) return true;
  if (owner.length !== PUBKEY_BYTES) {
    throw new RangeError(`owner must be ${PUBKEY_BYTES} bytes, got ${owner.length}`);
  }
  return owner.every((byte) => byte === 0);
}

function assertPubkey(bytes: Uint8Array, what: string): void {
  if (bytes.length !== PUBKEY_BYTES) {
    throw new RangeError(`${what} must be ${PUBKEY_BYTES} bytes, got ${bytes.length}`);
  }
}

export function assignBoxes(input: AssignBoxesInput): AssignBoxesResult {
  const { slothash, buyer, sold, count, owners } = input;
  assertPubkey(slothash, "slothash");
  assertPubkey(buyer, "buyer");
  if (owners.length !== BOXES) {
    throw new RangeError(`owners must have ${BOXES} entries, got ${owners.length}`);
  }
  if (isUnowned(buyer)) throw new RangeError("buyer must not be the default pubkey");

  const remaining: BoxIndex[] = [];
  for (let b = 0; b < BOXES; b++) {
    if (isUnowned(owners[b]!)) remaining.push(b);
  }
  const owned = BOXES - remaining.length;
  if (!Number.isInteger(sold) || sold !== owned) {
    throw new RangeError(`sold (${sold}) must equal the number of owned boxes (${owned})`);
  }
  if (!Number.isInteger(count) || count < 1 || count > remaining.length) {
    throw new RangeError(`count must be 1–${remaining.length}, got ${count}`);
  }

  const seed = sha256(slothash, buyer, u8(sold), u8(count));
  const next: Owner[] = owners.map((o) => (o === null ? null : Uint8Array.from(o)));
  const boxes: BoxIndex[] = [];
  for (let k = 0; k < count; k++) {
    const r = sha256(seed, u8(k));
    const idx = Number(u64le(r) % BigInt(remaining.length));
    const [box] = remaining.splice(idx, 1);
    boxes.push(box!);
    next[box!] = Uint8Array.from(buyer);
  }
  return { boxes, owners: next };
}
