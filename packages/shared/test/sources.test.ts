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

describe("score-source game-ID map (ARCHITECTURE › Games; PROGRAM §2)", () => {
  it("names the five sources", () => {
    expect(SCORE_SOURCES).toEqual(["polymarket", "kalshi", "espn", "txodds", "yahoo"]);
  });

  it("keys are source-prefixed and never collide across sources", () => {
    expect(sourceKey("polymarket", 19518)).toBe("polymarket:19518");
    expect(sourceKey("kalshi", "7b5c15b6-74e9-4ccb-8589-992d7b5e4968")).toBe(
      "kalshi:7b5c15b6-74e9-4ccb-8589-992d7b5e4968",
    );
    expect(sourceKey("espn", "401671789")).toBe("espn:401671789");
    expect(sourceKey("txodds", 18086885)).toBe("txodds:18086885");
    expect(sourceKey("polymarket", 1)).not.toBe(sourceKey("espn", "1"));
    expect(() => sourceKey("sportradar" as never, 1)).toThrow(/unknown score source/);
    expect(() => sourceKey("espn", "")).toThrow(/empty/);
  });

  it("builds and looks up a map; a source ID that maps to two games is rejected", () => {
    const map = buildGameIdMap([
      { key: KC_BAL, ids: { polymarket: "1", kalshi: "k1", espn: "e1", txodds: "18086885" } },
      { key: PHI_DAL, ids: { polymarket: "2" } },
    ]);
    expect(map.size).toBe(5);
    expect(lookupGameKey(map, "polymarket", 1)).toEqual(KC_BAL);
    expect(lookupGameKey(map, "kalshi", "k1")).toEqual(KC_BAL);
    expect(lookupGameKey(map, "espn", "e1")).toEqual(KC_BAL);
    expect(lookupGameKey(map, "txodds", 18086885)).toEqual(KC_BAL);
    expect(lookupGameKey(map, "polymarket", 2)).toEqual(PHI_DAL);
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
