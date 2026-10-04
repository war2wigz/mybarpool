import { describe, expect, it } from "vitest";

import {
  SCORE_SOURCES,
  buildGameIdMap,
  lookupGameKey,
  sameGameKey,
  sourceKey,
} from "../src/sources.js";

const KC_BAL = { season: 2026, week: 1, home: 15, away: 2 };
const PHI_DAL = { season: 2026, week: 1, home: 25, away: 8 };

describe("score-source game-ID map (SERVICES §3; PROGRAM §2)", () => {
  it("names the three sources", () => {
    expect(SCORE_SOURCES).toEqual(["apiSports", "sportradar", "espn"]);
  });

  it("keys are source-prefixed and never collide across sources", () => {
    expect(sourceKey("apiSports", 12345)).toBe("apiSports:12345");
    expect(sourceKey("sportradar", "sr:match:1")).toBe("sportradar:sr:match:1");
    expect(sourceKey("espn", "401671789")).toBe("espn:401671789");
    expect(sourceKey("apiSports", 1)).not.toBe(sourceKey("espn", "1"));
    expect(() => sourceKey("kalshi" as never, 1)).toThrow(/unknown score source/);
    expect(() => sourceKey("espn", "")).toThrow(/empty/);
  });

  it("builds and looks up a map; a source ID that maps to two games is rejected", () => {
    const map = buildGameIdMap([
      { key: KC_BAL, ids: { apiSports: 1, sportradar: "sr:1", espn: "e1" } },
      { key: PHI_DAL, ids: { apiSports: 2 } },
    ]);
    expect(map.size).toBe(4);
    expect(lookupGameKey(map, "apiSports", 1)).toEqual(KC_BAL);
    expect(lookupGameKey(map, "sportradar", "sr:1")).toEqual(KC_BAL);
    expect(lookupGameKey(map, "espn", "e1")).toEqual(KC_BAL);
    expect(lookupGameKey(map, "apiSports", 2)).toEqual(PHI_DAL);
    expect(lookupGameKey(map, "espn", "missing")).toBeUndefined();

    expect(() =>
      buildGameIdMap([
        { key: KC_BAL, ids: { espn: "e1" } },
        { key: PHI_DAL, ids: { espn: "e1" } },
      ]),
    ).toThrow(/two different games/);
    // The same game listed twice is fine.
    expect(
      buildGameIdMap([
        { key: KC_BAL, ids: { espn: "e1" } },
        { key: { ...KC_BAL }, ids: { espn: "e1" } },
      ]).size,
    ).toBe(1);
  });

  it("sameGameKey compares all four fields", () => {
    expect(sameGameKey(KC_BAL, { ...KC_BAL })).toBe(true);
    expect(sameGameKey(KC_BAL, { ...KC_BAL, season: 2027 })).toBe(false);
    expect(sameGameKey(KC_BAL, { ...KC_BAL, week: 2 })).toBe(false);
    expect(sameGameKey(KC_BAL, { ...KC_BAL, home: 14 })).toBe(false);
    expect(sameGameKey(KC_BAL, { ...KC_BAL, away: 3 })).toBe(false);
  });
});
