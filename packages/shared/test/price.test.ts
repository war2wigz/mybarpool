import { describe, expect, it } from "vitest";

import {
  INITIAL_LADDERS,
  INITIAL_LADDERS_WHOLE,
  ORE_DECIMALS,
  SOL_DECIMALS,
  TokenIndex,
  initialMaxSponsorship,
  isValidPrice,
  pow10,
  priceLadderSteps,
  skrLadder,
  validateLadder,
} from "../src/price.js";

const SOL = 10n ** 9n;
const ORE = 10n ** 11n;

describe("price ladders (ARCHITECTURE › Buying table; PROGRAM §3.1)", () => {
  it("token indices are PROGRAM §2's", () => {
    expect(TokenIndex.SOL).toBe(0);
    expect(TokenIndex.SKR).toBe(1);
    expect(TokenIndex.ORE).toBe(2);
  });

  it("SOL: 0.05 / 0.05 / 1 at 9 decimals", () => {
    expect(SOL_DECIMALS).toBe(9);
    expect(INITIAL_LADDERS.SOL).toEqual({
      decimals: 9,
      minPrice: 50_000_000n,
      step: 50_000_000n,
      maxPrice: SOL,
    });
    const steps = priceLadderSteps(INITIAL_LADDERS.SOL);
    expect(steps).toHaveLength(20); // 0.05, 0.10, …, 1.00
    expect(steps[0]).toBe(50_000_000n);
    expect(steps[1]).toBe(100_000_000n);
    expect(steps[19]).toBe(SOL);
    for (const p of steps) expect(isValidPrice(INITIAL_LADDERS.SOL, p)).toBe(true);
    expect(isValidPrice(INITIAL_LADDERS.SOL, 70_000_000n)).toBe(false); // 0.07 SOL is off the ladder
    expect(isValidPrice(INITIAL_LADDERS.SOL, 0n)).toBe(false);
    expect(isValidPrice(INITIAL_LADDERS.SOL, 1_050_000_000n)).toBe(false); // 1.05 SOL is over the cap
    expect(isValidPrice(INITIAL_LADDERS.SOL, 49_999_999n)).toBe(false);
  });

  it("ORE: the same ladder as SOL at 11 decimals", () => {
    expect(ORE_DECIMALS).toBe(11);
    expect(INITIAL_LADDERS.ORE).toEqual({
      decimals: 11,
      minPrice: 5n * 10n ** 9n,
      step: 5n * 10n ** 9n,
      maxPrice: ORE,
    });
    expect(priceLadderSteps(INITIAL_LADDERS.ORE)).toHaveLength(20);
    expect(isValidPrice(INITIAL_LADDERS.ORE, 7n * 10n ** 9n)).toBe(false);
  });

  it("SKR: 100 / 100 / 5,000 whole tokens, scaled by the caller's decimals", () => {
    expect(INITIAL_LADDERS_WHOLE.SKR).toEqual({ minPrice: 100n, step: 100n, maxPrice: 5_000n });
    for (const decimals of [0, 6, 9]) {
      const ladder = skrLadder(decimals);
      const unit = 10n ** BigInt(decimals);
      expect(ladder).toEqual({
        decimals,
        minPrice: 100n * unit,
        step: 100n * unit,
        maxPrice: 5_000n * unit,
      });
      expect(priceLadderSteps(ladder)).toHaveLength(50);
      expect(isValidPrice(ladder, 2_500n * unit)).toBe(true);
      expect(isValidPrice(ladder, 2_550n * unit)).toBe(false);
      expect(isValidPrice(ladder, 5_100n * unit)).toBe(false);
    }
  });

  it("initial sponsorship cap is 25 × max price (PROGRAM §3.1 max_sponsorship)", () => {
    expect(initialMaxSponsorship(INITIAL_LADDERS.SOL)).toBe(25n * SOL);
    expect(initialMaxSponsorship(INITIAL_LADDERS.ORE)).toBe(25n * ORE);
    expect(initialMaxSponsorship(skrLadder(6))).toBe(125_000n * 10n ** 6n);
  });

  it("validateLadder enforces the PROGRAM §3.1 invariants", () => {
    const ok = { decimals: 9, minPrice: 1n, step: 1n, maxPrice: 10n };
    expect(validateLadder(ok)).toBe(ok);
    expect(() => validateLadder({ ...ok, minPrice: 0n })).toThrow(/minPrice must be ≥ 1/);
    expect(() => validateLadder({ ...ok, step: 0n })).toThrow(/step must be ≥ 1/);
    expect(() => validateLadder({ ...ok, minPrice: 11n })).toThrow(/minPrice must be ≤ maxPrice/);
    expect(() => validateLadder({ ...ok, step: 4n })).toThrow(/multiple of step/); // (10 − 1) % 4 ≠ 0
    expect(() => validateLadder({ ...ok, maxPrice: 1n << 64n })).toThrow(/u64/);
    expect(() => validateLadder({ ...ok, minPrice: -1n })).toThrow(/u64/);
    expect(() => validateLadder({ ...ok, decimals: 39 })).toThrow(/decimals/);
    expect(() => validateLadder({ ...ok, decimals: 1.5 })).toThrow(/decimals/);
  });

  it("pow10 is exact bigint", () => {
    expect(pow10(0)).toBe(1n);
    expect(pow10(9)).toBe(SOL);
    expect(pow10(11)).toBe(ORE);
    expect(() => pow10(-1)).toThrow(RangeError);
  });
});
