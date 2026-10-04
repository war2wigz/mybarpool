import { describe, expect, it } from "vitest";

import { drawAxes, type Digits } from "../src/axes.js";
import { BOXES, colOf, rowOf } from "../src/boxes.js";
import { boxDigitPairs, winningBox } from "../src/winner.js";
import { Prng } from "./prng.js";

// Identity axes: lane l covers digits l and l + 5, so column = home % 5, row = away % 5.
const IDENTITY: Digits = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

describe("winningBox (PROGRAM §6.3)", () => {
  const rng = new Prng("winner");

  it("home is the column (across the top), away the row (down the side): box = row × 5 + col", () => {
    // home 14 → digit 4 → lane 4 (col 4); away 10 → digit 0 → lane 0 (row 0); box = 0 × 5 + 4.
    expect(winningBox({ home: 14, away: 10, homeAxis: IDENTITY, awayAxis: IDENTITY })).toBe(4);
    // Swapped scores land in a different box: col 0, row 4 → 20.
    expect(winningBox({ home: 10, away: 14, homeAxis: IDENTITY, awayAxis: IDENTITY })).toBe(20);
    // Digit 7 sits at position 7 → lane 2 (col 2); digit 3 → lane 3 (row 3) → 17.
    expect(winningBox({ home: 7, away: 3, homeAxis: IDENTITY, awayAxis: IDENTITY })).toBe(17);
  });

  it("returns exactly one box in 0–24 for every (home, away) in 0–999 × 0–999 against random axes", () => {
    const { home: homeAxis, away: awayAxis } = drawAxes(rng.bytes(32));
    let outOfRange = 0;
    const hits = new Array<number>(BOXES).fill(0);
    for (let home = 0; home < 1000; home++) {
      for (let away = 0; away < 1000; away++) {
        const box = winningBox({ home, away, homeAxis, awayAxis });
        if (!Number.isInteger(box) || box < 0 || box >= BOXES) outOfRange++;
        else hits[box]!++;
      }
    }
    expect(outOfRange).toBe(0);
    // Every box has a 4% chance (ARCHITECTURE › Grid): 4 of 100 digit pairs × 100 × 100 score pairs each.
    for (const n of hits) expect(n).toBe(40_000);
  });

  it("equals the box whose digit pairs contain (home % 10, away % 10), for 50 random axes", () => {
    for (let t = 0; t < 50; t++) {
      const { home: homeAxis, away: awayAxis } = drawAxes(rng.bytes(32));
      for (let home = 0; home < 100; home++) {
        for (let away = 0; away < 100; away++) {
          const box = winningBox({ home, away, homeAxis, awayAxis });
          const pairs = boxDigitPairs(box, homeAxis, awayAxis);
          expect(pairs.some(([h, a]) => h === home % 10 && a === away % 10)).toBe(true);
        }
      }
    }
  });

  it("every box covers exactly 4 digit pairs and the 25 boxes partition the 100 pairs", () => {
    // ARCHITECTURE › Grid: "Each box covers 4 of the 100 possible last-digit pairs".
    for (let t = 0; t < 100; t++) {
      const { home: homeAxis, away: awayAxis } = drawAxes(rng.bytes(32));
      const seen = new Set<string>();
      for (let box = 0; box < BOXES; box++) {
        const pairs = boxDigitPairs(box, homeAxis, awayAxis);
        expect(pairs).toHaveLength(4);
        for (const [h, a] of pairs) seen.add(`${h},${a}`);
      }
      expect(seen.size).toBe(100);
    }
  });

  it("boxDigitPairs uses the column's home digits and the row's away digits", () => {
    const { home: homeAxis, away: awayAxis } = drawAxes(rng.bytes(32));
    for (let box = 0; box < BOXES; box++) {
      const col = colOf(box);
      const row = rowOf(box);
      const homeDigits = new Set([homeAxis[col], homeAxis[col + 5]]);
      const awayDigits = new Set([awayAxis[row], awayAxis[row + 5]]);
      for (const [h, a] of boxDigitPairs(box, homeAxis, awayAxis)) {
        expect(homeDigits.has(h)).toBe(true);
        expect(awayDigits.has(a)).toBe(true);
      }
    }
  });

  it("rejects scores outside u16 and non-integers", () => {
    expect(() => winningBox({ home: -1, away: 0, homeAxis: IDENTITY, awayAxis: IDENTITY })).toThrow(
      RangeError,
    );
    expect(() =>
      winningBox({ home: 0, away: 65_536, homeAxis: IDENTITY, awayAxis: IDENTITY }),
    ).toThrow(RangeError);
    expect(() =>
      winningBox({ home: 1.5, away: 0, homeAxis: IDENTITY, awayAxis: IDENTITY }),
    ).toThrow(RangeError);
    expect(winningBox({ home: 65_535, away: 65_535, homeAxis: IDENTITY, awayAxis: IDENTITY })).toBe(
      0,
    );
  });

  it("rejects an axis that is not a permutation of 0–9 instead of matching the first duplicate", () => {
    const duplicated = [0, 1, 2, 3, 4, 5, 6, 7, 8, 8] as unknown as Digits;
    expect(() =>
      winningBox({ home: 0, away: 0, homeAxis: duplicated, awayAxis: IDENTITY }),
    ).toThrow(/permutation/);
    expect(() =>
      winningBox({ home: 0, away: 0, homeAxis: IDENTITY, awayAxis: duplicated }),
    ).toThrow(/permutation/);
  });
});
