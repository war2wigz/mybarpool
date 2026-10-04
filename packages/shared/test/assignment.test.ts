import { describe, expect, it } from "vitest";

import { PUBKEY_BYTES, assignBoxes, isUnowned, type Owner } from "../src/assignment.js";
import { BOXES } from "../src/boxes.js";
import { toHex } from "../src/bytes.js";
import { Prng } from "./prng.js";

const EMPTY: Owner[] = Array.from({ length: BOXES }, () => null);
const ZERO_PUBKEY = new Uint8Array(PUBKEY_BYTES);

function ownedCount(owners: readonly Owner[]): number {
  return owners.filter((o) => !isUnowned(o)).length;
}

describe("assignBoxes (PROGRAM §6.1)", () => {
  const rng = new Prng("assignment");

  it("treats null and 32 zero bytes (Pubkey::default()) as unowned", () => {
    expect(isUnowned(null)).toBe(true);
    expect(isUnowned(ZERO_PUBKEY)).toBe(true);
    expect(isUnowned(rng.pubkey())).toBe(false);
    expect(() => isUnowned(new Uint8Array(31))).toThrow(RangeError);
  });

  it("assigns exactly count distinct free boxes and does not mutate the input", () => {
    const owners: Owner[] = [...EMPTY];
    owners[3] = rng.pubkey();
    owners[10] = rng.pubkey();
    const snapshot = owners.map((o) => (o === null ? null : toHex(o)));
    const buyer = rng.pubkey();
    const result = assignBoxes({ slothash: rng.bytes(32), buyer, sold: 2, count: 3, owners });

    expect(result.boxes).toHaveLength(3);
    expect(new Set(result.boxes).size).toBe(3);
    for (const b of result.boxes) {
      expect(b).not.toBe(3);
      expect(b).not.toBe(10);
      expect(toHex(result.owners[b] as Uint8Array)).toBe(toHex(buyer));
    }
    expect(ownedCount(result.owners)).toBe(5);
    expect(owners.map((o) => (o === null ? null : toHex(o)))).toEqual(snapshot);
  });

  it("is deterministic for equal inputs and changes when any input byte changes", () => {
    const slothash = rng.bytes(32);
    const buyer = rng.pubkey();
    const a = assignBoxes({ slothash, buyer, sold: 0, count: 5, owners: EMPTY });
    const b = assignBoxes({ slothash, buyer, sold: 0, count: 5, owners: EMPTY });
    expect(a.boxes).toEqual(b.boxes);

    let differs = 0;
    const trials = 64;
    for (let t = 0; t < trials; t++) {
      const flipped = Uint8Array.from(slothash);
      flipped[t % 32]! ^= 1 << (t % 8);
      const c = assignBoxes({ slothash: flipped, buyer, sold: 0, count: 5, owners: EMPTY });
      if (c.boxes.join() !== a.boxes.join()) differs++;
    }
    // A single flipped bit re-seeds the shuffle; identical outputs happen only by chance.
    expect(differs).toBeGreaterThan(trials * 0.9);
  });

  it("fills the grid with count = 25 − sold", () => {
    const result = assignBoxes({
      slothash: rng.bytes(32),
      buyer: rng.pubkey(),
      sold: 0,
      count: 25,
      owners: EMPTY,
    });
    expect([...result.boxes].sort((x, y) => x - y)).toEqual(
      Array.from({ length: BOXES }, (_, i) => i),
    );
    expect(ownedCount(result.owners)).toBe(BOXES);
  });

  it("never returns a taken box across 1,000 random purchases", () => {
    for (let t = 0; t < 1_000; t++) {
      const owners: Owner[] = [...EMPTY];
      const sold = rng.int(BOXES);
      const taken = new Set<number>();
      while (taken.size < sold) taken.add(rng.int(BOXES));
      for (const i of taken) owners[i] = rng.pubkey();
      const count = rng.between(1, BOXES - sold);
      const { boxes, owners: after } = assignBoxes({
        slothash: rng.bytes(32),
        buyer: rng.pubkey(),
        sold,
        count,
        owners,
      });
      expect(boxes).toHaveLength(count);
      expect(new Set(boxes).size).toBe(count);
      for (const b of boxes) expect(taken.has(b)).toBe(false);
      expect(ownedCount(after)).toBe(sold + count);
    }
  });

  it("scatters a 3-box buy on a fresh grid: never labels 1-2-3 across 100 seeded runs", () => {
    // Build plan Step 4 acceptance; ARCHITECTURE › Buying "never 1-2-3".
    let consecutive = 0;
    for (let t = 0; t < 100; t++) {
      const { boxes } = assignBoxes({
        slothash: rng.bytes(32),
        buyer: rng.pubkey(),
        sold: 0,
        count: 3,
        owners: EMPTY,
      });
      if ([...boxes].sort((x, y) => x - y).join() === "0,1,2") consecutive++;
    }
    expect(consecutive).toBe(0);
  });

  it("rejects bad inputs", () => {
    const buyer = rng.pubkey();
    const slothash = rng.bytes(32);
    expect(() =>
      assignBoxes({ slothash: new Uint8Array(31), buyer, sold: 0, count: 1, owners: EMPTY }),
    ).toThrow(RangeError);
    expect(() =>
      assignBoxes({ slothash, buyer: new Uint8Array(33), sold: 0, count: 1, owners: EMPTY }),
    ).toThrow(RangeError);
    expect(() =>
      assignBoxes({ slothash, buyer: ZERO_PUBKEY, sold: 0, count: 1, owners: EMPTY }),
    ).toThrow(RangeError);
    expect(() =>
      assignBoxes({ slothash, buyer, sold: 0, count: 1, owners: EMPTY.slice(1) }),
    ).toThrow(RangeError);
    expect(() => assignBoxes({ slothash, buyer, sold: 1, count: 1, owners: EMPTY })).toThrow(
      /sold/,
    );
    expect(() => assignBoxes({ slothash, buyer, sold: 0, count: 0, owners: EMPTY })).toThrow(
      /count/,
    );
    expect(() => assignBoxes({ slothash, buyer, sold: 0, count: 26, owners: EMPTY })).toThrow(
      /count/,
    );
    const full: Owner[] = EMPTY.map(() => rng.pubkey());
    expect(() => assignBoxes({ slothash, buyer, sold: 25, count: 1, owners: full })).toThrow(
      /count/,
    );
  });
});
