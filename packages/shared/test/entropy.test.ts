/**
 * Entropy bytes (PROGRAM §4.4) against the live ORE `Var` fetched from mainnet
 * (`BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E`, slot 454,331,258, Step 5
 * first task) and against keccak vectors computed independently (pycryptodome):
 * the Rust `entropy.rs` tests use the same hard-coded vectors, which ties the
 * two languages together.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { fromHex, toHex } from "../src/bytes.js";
import {
  decodeVar,
  encodeClose,
  encodeNext,
  encodeOpen,
  encodeReveal,
  encodeSample,
  encodeVar,
  entropyValue,
  EntropyInstruction,
  fallbackHash,
  keccak,
  VAR_LEN,
  VAR_OFFSETS,
  varSeeds,
} from "../src/entropy.js";

const VAR_BYTES = new Uint8Array(
  readFileSync(fileURLToPath(new URL("./fixtures/var-BWCaDY96.bin", import.meta.url))),
);

/** `bs58` without a dependency: 32 bytes → the mainnet address. */
function base58(bytes: Uint8Array): string {
  const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  let n = 0n;
  for (const b of bytes) n = (n << 8n) | BigInt(b);
  let out = "";
  while (n > 0n) {
    out = ALPHABET[Number(n % 58n)] + out;
    n /= 58n;
  }
  for (const b of bytes) {
    if (b !== 0) break;
    out = "1" + out;
  }
  return out;
}

describe("Entropy Var (regolith-labs/entropy f26ae03)", () => {
  it("decodes the live ORE Var: discriminator zero, ORE Board authority, id 0, the provider key", () => {
    expect(VAR_BYTES).toHaveLength(VAR_LEN);
    const v = decodeVar(VAR_BYTES);
    expect(base58(v.authority)).toBe("BrcSxdp1nXFzou1YyDnQJcPNBNHgoypZmTsyKBSLLXzi");
    expect(v.id).toBe(0n);
    expect(base58(v.provider)).toBe("AKBXJ7jQ2DiqLQKzgPn791r1ZVNvLchTFH6kpesPAAWF");
    expect(v.isAuto).toBe(0n);
    expect(v.samples).toBeGreaterThan(0n);
    expect(v.endAt).toBeGreaterThan(v.startAt);
    // As fetched: rolled by `Next`, waiting for its window (committed, unsampled, unrevealed).
    expect(v.commit.some((b) => b !== 0)).toBe(true);
    expect(v.seed.every((b) => b === 0)).toBe(true);
  });

  it("encodeVar is the inverse of decodeVar and places every field at the §4.4 offset", () => {
    const v = decodeVar(VAR_BYTES);
    expect(encodeVar(v)).toEqual(VAR_BYTES);
    expect(VAR_OFFSETS).toEqual({
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
    expect(() => decodeVar(VAR_BYTES.subarray(8))).toThrow(RangeError); // the 232-byte legacy shape
    const bad = Uint8Array.from(VAR_BYTES);
    bad[0] = 1;
    expect(() => decodeVar(bad)).toThrow(RangeError);
  });

  it("keccak vectors match an independent implementation", () => {
    expect(toHex(keccak(new Uint8Array(0)))).toBe(
      "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470",
    );
    expect(toHex(fallbackHash(1000n))).toBe(
      "21c9650f3e9e5e2b94468fe9f1a4351613d2f305993af006c012c30ea53117e9",
    );
    expect(
      toHex(entropyValue(new Uint8Array(32).fill(0x11), new Uint8Array(32).fill(0x22), 1n)),
    ).toBe("afb7ecc0aa543bf6cd7d2deac8d34b0476b669b383df70b55622236745e78744");
    // A different samples count is a different value: Reveal binds all three inputs.
    expect(
      entropyValue(new Uint8Array(32).fill(0x11), new Uint8Array(32).fill(0x22), 2n),
    ).not.toEqual(fromHex("afb7ecc0aa543bf6cd7d2deac8d34b0476b669b383df70b55622236745e78744"));
  });

  it("instruction data: Open, Sample, Reveal, Next, Close", () => {
    expect(EntropyInstruction).toEqual({ Open: 0, Close: 1, Next: 2, Reveal: 4, Sample: 5 });
    const commit = new Uint8Array(32).fill(0xab);
    const open = encodeOpen({ id: 7n, commit, isAuto: false, samples: 1n, endAt: 1000n });
    expect(open).toHaveLength(1 + 8 + 32 + 1 + 8 + 8);
    expect([...open.subarray(0, 9)]).toEqual([0, 7, 0, 0, 0, 0, 0, 0, 0]);
    expect(open.subarray(9, 41)).toEqual(commit);
    expect(open[41]).toBe(0);
    expect([...open.subarray(42, 50)]).toEqual([1, 0, 0, 0, 0, 0, 0, 0]);
    expect([...open.subarray(50, 58)]).toEqual([0xe8, 0x03, 0, 0, 0, 0, 0, 0]);
    expect([...encodeSample()]).toEqual([5]);
    expect([...encodeReveal(commit)]).toEqual([4, ...commit]);
    expect([...encodeNext(1000n)]).toEqual([2, 0xe8, 0x03, 0, 0, 0, 0, 0, 0]);
    expect([...encodeClose()]).toEqual([1]);
  });

  it("varSeeds: 'var', authority, id u64 LE", () => {
    const seeds = varSeeds(new Uint8Array(32).fill(0xaa), 7n);
    expect(seeds.map(toHex)).toEqual(["766172", "aa".repeat(32), "0700000000000000"]);
  });
});
