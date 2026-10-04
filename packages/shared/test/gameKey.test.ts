import { describe, expect, it } from "vitest";

import { toHex } from "../src/bytes.js";
import {
  GAME_KEY_BYTES,
  PRESEASON_WEEK_MAX,
  PRESEASON_WEEK_MIN,
  SUPER_BOWL_WEEK,
  TEAM_COUNT,
  assertGameKey,
  decodeGameKey,
  encodeGameKey,
  gameRecordSeeds,
  isPreseasonWeek,
} from "../src/gameKey.js";
import { Prng } from "./prng.js";

// PROGRAM §2: 2026 season, week 1, home KC (15), away BAL (2).
const KC_BAL = { season: 2026, week: 1, home: 15, away: 2 };

describe("GameKey (PROGRAM §2)", () => {
  it("encodes {2026, 1, 15, 2} as [EA 07 01 0F 02] and round-trips", () => {
    const bytes = encodeGameKey(KC_BAL);
    expect(bytes).toHaveLength(GAME_KEY_BYTES);
    expect([...bytes]).toEqual([0xea, 0x07, 0x01, 0x0f, 0x02]);
    expect(decodeGameKey(bytes)).toEqual(KC_BAL);
  });

  it("accepts weeks 1–22 and 101–103, rejects 0, 23, 100, 104", () => {
    expect(SUPER_BOWL_WEEK).toBe(22);
    expect(PRESEASON_WEEK_MIN).toBe(101);
    expect(PRESEASON_WEEK_MAX).toBe(103);
    for (const week of [1, 18, 19, 20, 21, 22, 101, 102, 103]) {
      expect(() => assertGameKey({ ...KC_BAL, week })).not.toThrow();
    }
    for (const week of [0, 23, 100, 104, -1, 1.5]) {
      expect(() => assertGameKey({ ...KC_BAL, week })).toThrow(/week/);
    }
    expect(isPreseasonWeek(101)).toBe(true);
    expect(isPreseasonWeek(18)).toBe(false);
  });

  it("rejects home === away and teams outside 0–31", () => {
    expect(TEAM_COUNT).toBe(32);
    expect(() => assertGameKey({ ...KC_BAL, away: 15 })).toThrow(/differ/);
    expect(() => assertGameKey({ ...KC_BAL, home: 32 })).toThrow(/home/);
    expect(() => assertGameKey({ ...KC_BAL, away: -1 })).toThrow(/away/);
    expect(() => assertGameKey({ ...KC_BAL, season: 70_000 })).toThrow(/season/);
    expect(() => decodeGameKey(new Uint8Array(4))).toThrow(/5 bytes/);
    // Decoding validates too: week 0 is not a game.
    expect(() => decodeGameKey(Uint8Array.of(0xea, 0x07, 0x00, 0x0f, 0x02))).toThrow(/week/);
  });

  it("round-trips 1,000 random valid keys", () => {
    const rng = new Prng("gameKey");
    for (let t = 0; t < 1_000; t++) {
      const home = rng.int(32);
      let away = rng.int(32);
      if (away === home) away = (away + 1) % 32;
      const week = rng.int(2) === 0 ? rng.between(1, 22) : rng.between(101, 103);
      const key = { season: rng.between(2026, 2100), week, home, away };
      expect(decodeGameKey(encodeGameKey(key))).toEqual(key);
    }
  });
});

describe("GameRecord seeds (PROGRAM §3.2)", () => {
  it('["game", season u16, week u8, home u8, away u8, scheduled_kickoff i64], all little-endian', () => {
    // Sun 2026-09-13 17:00:00 UTC = 1 789 318 800.
    const seeds = gameRecordSeeds(KC_BAL, 1_789_318_800n);
    expect(seeds).toHaveLength(6);
    expect(seeds.map(toHex)).toEqual([
      "67616d65", // "game"
      "ea07", // 2026
      "01",
      "0f",
      "02",
      "90d6a66a00000000", // 1789318800 as i64 LE
    ]);
  });

  it("rejects an invalid key and a kickoff outside i64", () => {
    expect(() => gameRecordSeeds({ ...KC_BAL, week: 0 }, 0n)).toThrow(/week/);
    expect(() => gameRecordSeeds(KC_BAL, 1n << 63n)).toThrow(/i64/);
    expect(toHex(gameRecordSeeds(KC_BAL, -1n)[5]!)).toBe("ffffffffffffffff");
  });
});
