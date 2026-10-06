import { describe, expect, it } from "vitest";

import { toHex } from "../src/bytes.js";
import { counterSeeds, poolSeeds, sponsorshipSeeds, vaultSeeds } from "../src/seeds.js";

const A = new Uint8Array(32).fill(0xaa);
const B = new Uint8Array(32).fill(0xbb);
const hex = (parts: Uint8Array[]) => parts.map(toHex);

describe("pool-side PDA seeds (PROGRAM §3.3–§3.7)", () => {
  it("poolSeeds: 'pool', game, creator, nonce u64 LE", () => {
    const seeds = poolSeeds(A, B, 7n);
    expect(hex(seeds)).toEqual(["706f6f6c", toHex(A), toHex(B), "0700000000000000"]);
    expect(hex(poolSeeds(A, B, 0xffff_ffff_ffff_ffffn))[3]).toBe("ffffffffffffffff");
    expect(() => poolSeeds(A, B, -1n)).toThrow(RangeError);
    expect(() => poolSeeds(A, B, 1n << 64n)).toThrow(RangeError);
    expect(() => poolSeeds(A.subarray(1), B, 1n)).toThrow(RangeError);
  });

  it("vaultSeeds: 'vault', pool", () => {
    expect(hex(vaultSeeds(A))).toEqual(["7661756c74", toHex(A)]);
  });

  it("counterSeeds: 'counter', creator, game — creator first", () => {
    expect(hex(counterSeeds(A, B))).toEqual(["636f756e746572", toHex(A), toHex(B)]);
  });

  it("sponsorshipSeeds: 'sponsorship', pool, wallet", () => {
    expect(hex(sponsorshipSeeds(A, B))).toEqual(["73706f6e736f7273686970", toHex(A), toHex(B)]);
  });
});
