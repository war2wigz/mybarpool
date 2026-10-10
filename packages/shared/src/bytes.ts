/**
 * Byte helpers shared by the algorithm modules. Internal: not re-exported.
 *
 * `sha256` is `@noble/hashes` (synchronous, pure JS, runs in React Native where
 * WebCrypto is absent); PROGRAM §6 says `sha256` is the Solana `hashv` syscall,
 * which hashes the concatenation of its inputs, so every call here concatenates
 * first and hashes once.
 */
import { sha256 as nobleSha256 } from "@noble/hashes/sha2.js";

/** The largest u64. */
export const U64_MAX = (1n << 64n) - 1n;

export function concatBytes(...parts: readonly Uint8Array[]): Uint8Array {
  let length = 0;
  for (const part of parts) length += part.length;
  const out = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function sha256(...parts: readonly Uint8Array[]): Uint8Array {
  return nobleSha256(concatBytes(...parts));
}

/** `u64_le(r[0..8])` in PROGRAM §6: the first eight bytes as a little-endian unsigned integer. */
export function u64le(bytes: Uint8Array): bigint {
  if (bytes.length < 8) throw new RangeError("u64le needs at least 8 bytes");
  let value = 0n;
  for (let i = 7; i >= 0; i--) {
    value = (value << 8n) | BigInt(bytes[i]!);
  }
  return value;
}

export function u8(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0 || value > 0xff) {
    throw new RangeError(`not a u8: ${value}`);
  }
  return Uint8Array.of(value);
}

export function u16le(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff) {
    throw new RangeError(`not a u16: ${value}`);
  }
  return Uint8Array.of(value & 0xff, value >>> 8);
}

export function i64le(value: bigint): Uint8Array {
  if (value < -(1n << 63n) || value > (1n << 63n) - 1n) {
    throw new RangeError(`not an i64: ${value}`);
  }
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigInt64(0, value, true);
  return out;
}

/** ASCII label bytes (`b"home"`, `b"away"`, `b"game"`); no TextEncoder so it runs everywhere. */
export function ascii(text: string): Uint8Array {
  const out = new Uint8Array(text.length);
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code > 0x7f) throw new RangeError(`not ASCII: ${text}`);
    out[i] = code;
  }
  return out;
}

/** The program's `u64` cannot hold what JS `bigint` can; every money output passes through this. */
export function assertU64(value: bigint, what = "value"): bigint {
  if (value < 0n || value > U64_MAX) {
    throw new RangeError(`${what} does not fit in u64: ${value}`);
  }
  return value;
}

export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** Lower-case hex of `bytes`. */
export function toHex(bytes: Uint8Array): string {
  let out = "";
  for (const byte of bytes) out += byte.toString(16).padStart(2, "0");
  return out;
}

/** Bytes of a hex string (an even number of hex digits, optional `0x`). */
export function fromHex(hex: string): Uint8Array {
  if (hex.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(hex)) {
    throw new RangeError("not a hex string");
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}
