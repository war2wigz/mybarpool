import { describe, expect, it } from "vitest";

import {
  ESPN_TEAM_IDS,
  KALSHI_TEAM_IDS,
  NFLVERSE_CODES,
  resolveSourceTeam,
  SOURCE_TEAM_TABLES,
  SPORTRADAR_ABBREVIATIONS,
} from "../src/sourceTeams.js";
import { TEAMS } from "../src/teams.js";

describe("source team tables (ARCHITECTURE › Games)", () => {
  for (const [source, table] of Object.entries(SOURCE_TEAM_TABLES)) {
    // The Sportradar column is pinned from recordings as teams appear; every id present maps
    // to a distinct team, and the table is complete once the season has shown all 32.
    const complete = source !== "polymarket";
    it(`${source}: ${complete ? "every one of our 32 teams appears exactly once" : "every id present maps to a distinct team"}, no two ids share a team`, () => {
      const values = Object.values(table);
      if (complete) {
        expect(values).toHaveLength(TEAMS.length);
        for (const team of TEAMS) expect(values).toContain(team.id);
      }
      expect(new Set(values).size).toBe(values.length);
      for (const v of values) expect(Number.isInteger(v) && v >= 0 && v < TEAMS.length).toBe(true);
    });
  }

  it("ESPN: Washington is id 28 (ESPN's WSH), Houston 34, Baltimore 33", () => {
    expect(ESPN_TEAM_IDS["28"]).toBe(31);
    expect(ESPN_TEAM_IDS["34"]).toBe(12);
    expect(ESPN_TEAM_IDS["33"]).toBe(2);
    expect(resolveSourceTeam("espn", 28)).toBe(31);
    expect(resolveSourceTeam("espn", "28")).toBe(31);
    expect(resolveSourceTeam("espn", "31")).toBeUndefined(); // no such ESPN id (the Browns are 5)
    expect(resolveSourceTeam("espn", "")).toBeUndefined();
  });

  it("Kalshi: 32 UUIDs; Washington and the Saints by id, never by title prefix", () => {
    expect(Object.keys(KALSHI_TEAM_IDS).every((k) => /^[0-9a-f-]{36}$/.test(k))).toBe(true);
    expect(resolveSourceTeam("kalshi", "aa65dda6-9cb8-4c8b-812b-d0c1feaae54f")).toBe(31); // WAS
    expect(resolveSourceTeam("kalshi", "f7c2cd06-61bf-469e-984f-3ef56cb8f3d7")).toBe(22); // NO
    expect(resolveSourceTeam("kalshi", "WAS")).toBeUndefined();
  });

  it("nflverse: LA is the Rams, JAX and WAS as ours", () => {
    expect(NFLVERSE_CODES["LA"]).toBe(17);
    expect(NFLVERSE_CODES["LAR"]).toBeUndefined();
    expect(resolveSourceTeam("nflverse", "JAX")).toBe(14);
    expect(resolveSourceTeam("nflverse", "WAS")).toBe(31);
    expect(Object.keys(SOURCE_TEAM_TABLES).sort()).toEqual([
      "espn",
      "kalshi",
      "nflverse",
      "polymarket",
    ]);
  });

  it("Sportradar (Polymarket's homeTeam/awayTeam): the 24 teams of 2026-10-11; WAS, JAX, NO as ours; the unseen eight absent", () => {
    expect(Object.keys(SPORTRADAR_ABBREVIATIONS)).toHaveLength(24);
    expect(resolveSourceTeam("polymarket", "WAS")).toBe(31);
    expect(resolveSourceTeam("polymarket", "JAX")).toBe(14);
    expect(resolveSourceTeam("polymarket", "NO")).toBe(22);
    expect(resolveSourceTeam("polymarket", "LAC")).toBe(16);
    for (const unseen of ["ATL", "BAL", "BUF", "CAR", "DAL", "KC", "LAR", "LA", "TB", "WSH", "JAC"])
      expect(resolveSourceTeam("polymarket", unseen)).toBeUndefined();
  });
});
