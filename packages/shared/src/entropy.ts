/**
 * Regolith Entropy as the program reads it (PROGRAM §4.4), Kit-free bytes
 * like `seeds.ts`: the `Var` layout, the instruction data, and the two
 * keccak formulas. Declared from `regolith-labs/entropy` at `f26ae03`
 * (`api/src/state/var.rs`, `api/src/instruction.rs`, `program/src/sample.rs`,
 * `program/src/reveal.rs`), the commit `verify.osec.io` reports as deployed.
 *
 * The 240-byte account is steel's 8-byte discriminator (`Var = 0`, so all
 * zero) followed by the `#[repr(C)]` struct, every field naturally aligned.
 */
import { keccak_256 } from "@noble/hashes/sha3.js";

import { ascii, assertU64, concatBytes } from "./bytes.js";

/** PROGRAM §1 `ENTROPY_PROGRAM`, base58. */
export const ENTROPY_PROGRAM = "3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X";
export const VAR_LEN = 240;
export const VAR_DISCRIMINATOR: Uint8Array = new Uint8Array(8);
const PUBKEY_BYTES = 32;

/** `EntropyInstruction` discriminants at `f26ae03`; `Open` is refused by the deployed program. */
export const EntropyInstruction = Object.freeze({
  Open: 0,
  Close: 1,
  Next: 2,
  Reveal: 4,
  Sample: 5,
});

export interface Var {
  readonly authority: Uint8Array;
  readonly id: bigint;
  readonly provider: Uint8Array;
  readonly commit: Uint8Array;
  readonly seed: Uint8Array;
  readonly slotHash: Uint8Array;
  readonly value: Uint8Array;
  readonly samples: bigint;
  readonly isAuto: bigint;
  readonly startAt: bigint;
  readonly endAt: bigint;
}

/** Byte offsets within the 240-byte account, discriminator included. */
export const VAR_OFFSETS = Object.freeze({
  discriminator: 0,
  authority: 8,
  id: 40,
  provider: 48,
  commit: 80,
  seed: 112,
  slotHash: 144,
  value: 176,
  samples: 208,
  isAuto: 216,
  startAt: 224,
  endAt: 232,
});

function assertPubkey(bytes: Uint8Array, what: string): Uint8Array {
  if (bytes.length !== PUBKEY_BYTES) {
    throw new RangeError(`${what} must be ${PUBKEY_BYTES} bytes, got ${bytes.length}`);
  }
  return bytes;
}

function assert32(bytes: Uint8Array, what: string): Uint8Array {
  if (bytes.length !== 32) throw new RangeError(`${what} must be 32 bytes, got ${bytes.length}`);
  return bytes;
}

function u64le(value: bigint, what: string): Uint8Array {
  assertU64(value, what);
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, value, true);
  return out;
}

function readU64(bytes: Uint8Array, at: number): bigint {
  return new DataView(bytes.buffer, bytes.byteOffset).getBigUint64(at, true);
}

/** `["var", authority, id u64 LE]` under `ENTROPY_PROGRAM`. */
export function varSeeds(authority: Uint8Array, id: bigint): Uint8Array[] {
  return [ascii("var"), assertPubkey(authority, "authority"), u64le(id, "id")];
}

export function keccak(bytes: Uint8Array): Uint8Array {
  return keccak_256(bytes);
}

/** `keccak(end_at.to_le_bytes())`: what `Sample` writes when `end_at` is no longer in SlotHashes. */
export function fallbackHash(endAt: bigint): Uint8Array {
  return keccak_256(u64le(endAt, "endAt"));
}

/** `keccak(slot_hash ‖ seed ‖ samples.to_le_bytes())`: what `Reveal` writes as `value`. */
export function entropyValue(slotHash: Uint8Array, seed: Uint8Array, samples: bigint): Uint8Array {
  return keccak_256(
    concatBytes(assert32(slotHash, "slotHash"), assert32(seed, "seed"), u64le(samples, "samples")),
  );
}

export function decodeVar(bytes: Uint8Array): Var {
  if (bytes.length !== VAR_LEN) {
    throw new RangeError(`Var must be ${VAR_LEN} bytes, got ${bytes.length}`);
  }
  if (!bytes.subarray(0, 8).every((b) => b === 0)) {
    throw new RangeError("Var discriminator must be zero");
  }
  const o = VAR_OFFSETS;
  return {
    authority: bytes.slice(o.authority, o.authority + 32),
    id: readU64(bytes, o.id),
    provider: bytes.slice(o.provider, o.provider + 32),
    commit: bytes.slice(o.commit, o.commit + 32),
    seed: bytes.slice(o.seed, o.seed + 32),
    slotHash: bytes.slice(o.slotHash, o.slotHash + 32),
    value: bytes.slice(o.value, o.value + 32),
    samples: readU64(bytes, o.samples),
    isAuto: readU64(bytes, o.isAuto),
    startAt: readU64(bytes, o.startAt),
    endAt: readU64(bytes, o.endAt),
  };
}

/** The inverse of `decodeVar`: the 240 bytes `Open` (and the later instructions) would leave. */
export function encodeVar(fields: Var): Uint8Array {
  const out = new Uint8Array(VAR_LEN);
  const o = VAR_OFFSETS;
  out.set(assertPubkey(fields.authority, "authority"), o.authority);
  out.set(u64le(fields.id, "id"), o.id);
  out.set(assertPubkey(fields.provider, "provider"), o.provider);
  out.set(assert32(fields.commit, "commit"), o.commit);
  out.set(assert32(fields.seed, "seed"), o.seed);
  out.set(assert32(fields.slotHash, "slotHash"), o.slotHash);
  out.set(assert32(fields.value, "value"), o.value);
  out.set(u64le(fields.samples, "samples"), o.samples);
  out.set(u64le(fields.isAuto, "isAuto"), o.isAuto);
  out.set(u64le(fields.startAt, "startAt"), o.startAt);
  out.set(u64le(fields.endAt, "endAt"), o.endAt);
  return out;
}

/** `Open`: `[0] ‖ id ‖ commit ‖ is_auto (u8) ‖ samples ‖ end_at`. Refused by the deployed program. */
export function encodeOpen(args: {
  id: bigint;
  commit: Uint8Array;
  isAuto: boolean;
  samples: bigint;
  endAt: bigint;
}): Uint8Array {
  return concatBytes(
    Uint8Array.of(EntropyInstruction.Open),
    u64le(args.id, "id"),
    assert32(args.commit, "commit"),
    Uint8Array.of(args.isAuto ? 1 : 0),
    u64le(args.samples, "samples"),
    u64le(args.endAt, "endAt"),
  );
}

/** `Sample`: `[5]`. Accounts `[signer (s), var (w), SlotHashes]`. */
export function encodeSample(): Uint8Array {
  return Uint8Array.of(EntropyInstruction.Sample);
}

/** `Reveal`: `[4] ‖ seed`. Accounts `[signer (s), var (w)]`. */
export function encodeReveal(seed: Uint8Array): Uint8Array {
  return concatBytes(Uint8Array.of(EntropyInstruction.Reveal), assert32(seed, "seed"));
}

/** `Next`: `[2] ‖ end_at`. Accounts `[authority (s), var (w)]`. */
export function encodeNext(endAt: bigint): Uint8Array {
  return concatBytes(Uint8Array.of(EntropyInstruction.Next), u64le(endAt, "endAt"));
}

/** `Close`: `[1]`. Accounts `[authority (w,s), var (w), system]` (three, per the processor). */
export function encodeClose(): Uint8Array {
  return Uint8Array.of(EntropyInstruction.Close);
}
