import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  API_SPORTS_TEAM_IDS,
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
    it(`${source}: every one of our 32 teams appears exactly once, no two ids share a team`, () => {
      const values = Object.values(table);
      expect(values).toHaveLength(TEAMS.length);
      for (const team of TEAMS) expect(values).toContain(team.id);
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
      "apiSports",
      "espn",
      "kalshi",
      "nflverse",
      "polymarket",
    ]);
  });

  it("Sportradar (Polymarket's homeTeam/awayTeam): the 26 teams of 2026-10-11 (ATL and BAL from the night game), LAR and BUF from 2026-10-12, and CAR, DAL, KC, TB from Sportradar's aliases; WAS, JAX, NO as ours; the other sources' spellings absent", () => {
    expect(Object.keys(SPORTRADAR_ABBREVIATIONS)).toHaveLength(32);
    expect(resolveSourceTeam("polymarket", "CAR")).toBe(4);
    expect(resolveSourceTeam("polymarket", "DAL")).toBe(8);
    expect(resolveSourceTeam("polymarket", "KC")).toBe(15);
    expect(resolveSourceTeam("polymarket", "TB")).toBe(29);
    expect(resolveSourceTeam("polymarket", "ATL")).toBe(1);
    expect(resolveSourceTeam("polymarket", "BAL")).toBe(2);
    expect(resolveSourceTeam("polymarket", "LAR")).toBe(17);
    expect(resolveSourceTeam("polymarket", "BUF")).toBe(3);
    expect(resolveSourceTeam("polymarket", "WAS")).toBe(31);
    expect(resolveSourceTeam("polymarket", "JAX")).toBe(14);
    expect(resolveSourceTeam("polymarket", "NO")).toBe(22);
    expect(resolveSourceTeam("polymarket", "LAC")).toBe(16);
    for (const other of ["LA", "WSH", "JAC", "NOLA"])
      expect(resolveSourceTeam("polymarket", other)).toBeUndefined();
  });

  it("API-Sports: the 32 numeric ids, each the team nflverse's code names; the feed's own team list rebuilds the table; the conference pseudo-teams 33 and 34 are absent", () => {
    const list = JSON.parse(
      readFileSync(new URL("./fixtures/apisports-teams-2024.json", import.meta.url), "utf8"),
    ) as { results: number; response: Array<{ id: number; code: string | null }> };
    expect(list.results).toBe(34);
    const fromFeed: Record<string, number> = {};
    for (const t of list.response) {
      if (t.code === null) {
        expect([33, 34]).toContain(t.id);
        continue;
      }
      const ours = NFLVERSE_CODES[t.code];
      expect(ours, `code ${t.code} is not one of nflverse's`).toBeDefined();
      fromFeed[String(t.id)] = ours!;
    }
    expect(API_SPORTS_TEAM_IDS).toEqual(fromFeed);
    expect(Object.keys(API_SPORTS_TEAM_IDS)).toHaveLength(32);
    const codes = list.response.filter((t) => t.code !== null).map((t) => t.code);
    expect([...codes].sort()).toEqual(Object.keys(NFLVERSE_CODES).sort());
    expect(resolveSourceTeam("apiSports", 31)).toBe(17); // LA, the Rams
    expect(resolveSourceTeam("apiSports", "20")).toBe(3); // BUF
    expect(resolveSourceTeam("apiSports", 2)).toBe(14); // JAX
    expect(resolveSourceTeam("apiSports", 12)).toBe(25); // PHI
    expect(resolveSourceTeam("apiSports", 33)).toBeUndefined();
    expect(resolveSourceTeam("apiSports", 34)).toBeUndefined();
    expect(resolveSourceTeam("apiSports", "LA")).toBeUndefined();
  });
});
