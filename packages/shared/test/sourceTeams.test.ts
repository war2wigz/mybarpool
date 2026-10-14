import { describe, expect, it } from "vitest";

import {
  ESPN_TEAM_IDS,
  KALSHI_TEAM_IDS,
  NFLVERSE_CODES,
  resolveSourceTeam,
  SOURCE_TEAM_TABLES,
  SPORTRADAR_ABBREVIATIONS,
  TXODDS_TEAMS,
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
      "espn",
      "kalshi",
      "nflverse",
      "polymarket",
      "txodds",
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

  it("TxODDS: the 32 NFL team names from the TxLINE schedule, including dotted L.A. and N.Y. prefixes", () => {
    expect(Object.keys(TXODDS_TEAMS)).toHaveLength(32);
    expect(resolveSourceTeam("txodds", "L.A. Rams")).toBe(17);
    expect(resolveSourceTeam("txodds", "L.A. Chargers")).toBe(16);
    expect(resolveSourceTeam("txodds", "N.Y. Giants")).toBe(23);
    expect(resolveSourceTeam("txodds", "N.Y. Jets")).toBe(24);
    expect(resolveSourceTeam("txodds", "Buffalo Bills")).toBe(3);
    expect(resolveSourceTeam("txodds", "Jacksonville Jaguars")).toBe(14);
    expect(resolveSourceTeam("txodds", "Philadelphia Eagles")).toBe(25);
    expect(resolveSourceTeam("txodds", "Washington Commanders")).toBe(31);
    expect(resolveSourceTeam("txodds", "Los Angeles Rams")).toBeUndefined();
    expect(resolveSourceTeam("txodds", "LAR")).toBeUndefined();
  });
});
