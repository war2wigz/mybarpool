import { describe, expect, it } from "vitest";

import { TEAMS, teamByAbbreviation, teamById } from "../src/teams.js";

// PROGRAM §2 team table order.
const ORDER =
  "ARI, ATL, BAL, BUF, CAR, CHI, CIN, CLE, DAL, DEN, DET, GB, HOU, IND, JAX, KC, LAC, LAR, LV, MIA, MIN, NE, NO, NYG, NYJ, PHI, PIT, SEA, SF, TB, TEN, WAS".split(
    ", ",
  );

describe("team table (PROGRAM §2)", () => {
  it("has 32 teams in the frozen order, ids equal to index", () => {
    expect(TEAMS).toHaveLength(32);
    expect(TEAMS.map((t) => t.abbreviation)).toEqual(ORDER);
    expect(TEAMS[0]!.abbreviation).toBe("ARI");
    expect(TEAMS[31]!.abbreviation).toBe("WAS");
    TEAMS.forEach((t, i) => expect(t.id).toBe(i));
    expect(new Set(TEAMS.map((t) => t.id)).size).toBe(32);
  });

  it("is sorted by abbreviation", () => {
    const sorted = [...ORDER].sort();
    expect(ORDER).toEqual(sorted);
  });

  it("every team has a city, a name and two #rrggbb colours", () => {
    for (const t of TEAMS) {
      expect(t.city.length).toBeGreaterThan(0);
      expect(t.name.length).toBeGreaterThan(0);
      expect(t.colors.primary).toMatch(/^#[0-9A-F]{6}$/);
      expect(t.colors.secondary).toMatch(/^#[0-9A-F]{6}$/);
      expect(t.colors.primary).not.toBe(t.colors.secondary);
    }
  });

  it("looks up by abbreviation (case-insensitive) and by id", () => {
    expect(teamByAbbreviation("KC")?.name).toBe("Chiefs");
    expect(teamByAbbreviation("kc")?.id).toBe(15);
    expect(teamByAbbreviation("XYZ")).toBeUndefined();
    expect(teamById(2).abbreviation).toBe("BAL");
    expect(() => teamById(32)).toThrow(RangeError);
    expect(() => teamById(-1)).toThrow(RangeError);
  });

  it("is frozen", () => {
    expect(Object.isFrozen(TEAMS)).toBe(true);
  });
});
