/**
 * Source team tables (ARCHITECTURE › Games, Score integrity): each score source's own team
 * identifier → our team id (0–31, `teams.ts`). The one place a source's team id is mapped;
 * the scores service resolves through these and never through names or abbreviations of
 * its own.
 *
 * Keyed by the source's id rather than its abbreviation where the source has one, because
 * abbreviations drift between vendors (ESPN writes Washington as `WSH`, our table `WAS`).
 */
import { teamByAbbreviation } from "./teams.js";

const id = (abbreviation: string): number => {
  const team = teamByAbbreviation(abbreviation);
  if (!team) throw new RangeError(`unknown team ${abbreviation}`);
  return team.id;
};

/**
 * ESPN `competitors[].team.id` (a decimal string on the scoreboard) → team id. Read from the
 * scoreboard on 2026-10-10; ESPN's ids are stable across seasons.
 */
export const ESPN_TEAM_IDS: Readonly<Record<string, number>> = Object.freeze({
  "22": id("ARI"),
  "1": id("ATL"),
  "33": id("BAL"),
  "2": id("BUF"),
  "29": id("CAR"),
  "3": id("CHI"),
  "4": id("CIN"),
  "5": id("CLE"),
  "6": id("DAL"),
  "7": id("DEN"),
  "8": id("DET"),
  "9": id("GB"),
  "34": id("HOU"),
  "11": id("IND"),
  "30": id("JAX"),
  "12": id("KC"),
  "24": id("LAC"),
  "14": id("LAR"),
  "13": id("LV"),
  "15": id("MIA"),
  "16": id("MIN"),
  "17": id("NE"),
  "18": id("NO"),
  "19": id("NYG"),
  "20": id("NYJ"),
  "21": id("PHI"),
  "23": id("PIT"),
  "26": id("SEA"),
  "25": id("SF"),
  "27": id("TB"),
  "10": id("TEN"),
  "28": id("WAS"), // ESPN's abbreviation is WSH
});

/** Every source table the package carries, by the name the scores service uses. */
export const SOURCE_TEAM_TABLES: Readonly<Record<"espn", Readonly<Record<string, number>>>> =
  Object.freeze({ espn: ESPN_TEAM_IDS });

/**
 * Our team id for a source's team identifier, or `undefined` when the source does not list it
 * (a game whose teams do not resolve is dropped with an alert, never inferred).
 */
export function resolveSourceTeam(
  source: keyof typeof SOURCE_TEAM_TABLES,
  sourceTeamId: string | number,
): number | undefined {
  return SOURCE_TEAM_TABLES[source][String(sourceTeamId)];
}
