import { describe, expect, it } from "vitest";

import { ESPN_TEAM_IDS, resolveSourceTeam, SOURCE_TEAM_TABLES } from "../src/sourceTeams.js";
import { TEAMS } from "../src/teams.js";

describe("source team tables (ARCHITECTURE › Games)", () => {
  for (const [source, table] of Object.entries(SOURCE_TEAM_TABLES)) {
    it(`${source}: every one of our 32 teams appears exactly once, no two ids share a team`, () => {
      const values = Object.values(table);
      expect(values).toHaveLength(TEAMS.length);
      expect(new Set(values).size).toBe(TEAMS.length);
      for (const team of TEAMS) expect(values).toContain(team.id);
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
});
