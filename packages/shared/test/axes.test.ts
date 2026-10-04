import { describe, expect, it } from "vitest";

import { DIGITS, assertDigits, axis, drawAxes, laneDigits, laneOfDigit } from "../src/axes.js";
import { Prng } from "./prng.js";

function isPermutation(digits: readonly number[]): boolean {
  return (
    digits.length === DIGITS &&
    new Set(digits).size === DIGITS &&
    digits.every((d) => d >= 0 && d < DIGITS)
  );
}

describe("axis shuffle (PROGRAM §6.2)", () => {
  const rng = new Prng("axes");

  it("is a permutation of 0–9 for 1,000 random values, on both labels", () => {
    for (let t = 0; t < 1_000; t++) {
      const value = rng.bytes(32);
      const { home, away } = drawAxes(value);
      expect(isPermutation(home)).toBe(true);
      expect(isPermutation(away)).toBe(true);
    }
  });

  it("home and away are independent shuffles: they differ for the same value (not always equal)", () => {
    // ARCHITECTURE › Randomness: "The axes are never a copy of one another."
    let equal = 0;
    for (let t = 0; t < 200; t++) {
      const { home, away } = drawAxes(rng.bytes(32));
      if (home.join() === away.join()) equal++;
    }
    expect(equal).toBeLessThan(200);
  });

  it("is deterministic and label-sensitive", () => {
    const value = rng.bytes(32);
    expect(axis(value, "home")).toEqual(axis(value, "home"));
    expect(axis(value, "home")).not.toEqual(axis(value, "away"));
  });

  it("each lane covers a[l] and a[l + 5], and the five lanes partition 0–9", () => {
    const digits = axis(rng.bytes(32), "home");
    const covered = new Set<number>();
    for (let lane = 0; lane < 5; lane++) {
      const [a, b] = laneDigits(digits, lane);
      expect(a).toBe(digits[lane]);
      expect(b).toBe(digits[lane + 5]);
      covered.add(a);
      covered.add(b);
      expect(laneOfDigit(digits, a)).toBe(lane);
      expect(laneOfDigit(digits, b)).toBe(lane);
    }
    expect(covered.size).toBe(DIGITS);
  });

  it("rejects a value that is not 32 bytes, bad lanes and bad digits", () => {
    expect(() => axis(new Uint8Array(31), "home")).toThrow(RangeError);
    const digits = axis(rng.bytes(32), "away");
    expect(() => laneDigits(digits, 5)).toThrow(RangeError);
    expect(() => laneOfDigit(digits, 10)).toThrow(RangeError);
    expect(() => laneOfDigit([0, 1, 2, 3, 4, 5, 6, 7, 8, 8] as never, 9)).toThrow(RangeError);
    expect(() => assertDigits([0, 1, 2, 3, 4, 5, 6, 7, 8, 8])).toThrow(RangeError);
    expect(() => assertDigits([0, 1, 2])).toThrow(RangeError);
    expect(assertDigits([9, 8, 7, 6, 5, 4, 3, 2, 1, 0])).toEqual([9, 8, 7, 6, 5, 4, 3, 2, 1, 0]);
  });
});
