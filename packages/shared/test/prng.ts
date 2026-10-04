/**
 * Deterministic PRNG for property tests and vector generation (test-only).
 * SHA-256 counter mode over a seed, so runs are reproducible without a
 * dependency; not used in production code.
 */
import { sha256 } from "@noble/hashes/sha2.js";

export class Prng {
  #counter = 0n;
  #seed: Uint8Array;

  constructor(seed: string) {
    this.#seed = sha256(new TextEncoder().encode(seed));
  }

  bytes(length: number): Uint8Array {
    const out = new Uint8Array(length);
    let offset = 0;
    while (offset < length) {
      const counter = new Uint8Array(8);
      new DataView(counter.buffer).setBigUint64(0, this.#counter++, true);
      const block = sha256(new Uint8Array([...this.#seed, ...counter]));
      const take = Math.min(block.length, length - offset);
      out.set(block.subarray(0, take), offset);
      offset += take;
    }
    return out;
  }

  /** Uniform integer in `[0, n)`. */
  int(n: number): number {
    if (!Number.isInteger(n) || n < 1) throw new RangeError(`n must be ≥ 1: ${n}`);
    return Number(this.bigint(BigInt(n)));
  }

  /** Uniform bigint in `[0, n)`. */
  bigint(n: bigint): bigint {
    if (n < 1n) throw new RangeError("n must be ≥ 1");
    const bytes = this.bytes(16);
    let value = 0n;
    for (const b of bytes) value = (value << 8n) | BigInt(b);
    return value % n;
  }

  /** Uniform integer in `[min, max]`. */
  between(min: number, max: number): number {
    return min + this.int(max - min + 1);
  }

  pick<T>(items: readonly T[]): T {
    if (items.length === 0) throw new RangeError("pick from empty list");
    return items[this.int(items.length)]!;
  }

  pubkey(): Uint8Array {
    return this.bytes(32);
  }
}
