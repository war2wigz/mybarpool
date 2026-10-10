/**
 * Source team tables (ARCHITECTURE › Games, Score integrity): each score source's own team
 * identifier → our team id (0–31, `teams.ts`). The one place a source's team id is mapped;
 * the scores service resolves through these and never through names or abbreviations of
 * its own.
 *
 * Keyed by the source's id rather than its label where the source has one, because labels
 * drift between vendors (ESPN writes Washington as `WSH`, our table `WAS`; nflverse writes the
 * Rams as `LA`). The Sportradar abbreviations the Polymarket stream carries follow once the
 * first NFL recording has shown them.
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

/**
 * Kalshi team UUIDs (`details.home_team_id` / `away_team_id` on a football milestone) → team
 * id. Read from two weeks of NFL milestones on 2026-10-10 and checked against the week's list
 * before committing; the titles' prefixes are not stable (`NO` and `NOLA` both appeared), which
 * is why the UUID is the key.
 */
export const KALSHI_TEAM_IDS: Readonly<Record<string, number>> = Object.freeze({
  "d73dae4f-1f08-4b5e-bf23-4e25e9b6a69c": id("ARI"),
  "75685be6-4388-4bba-8bcb-d0b0c24feb50": id("ATL"),
  "a69a57af-f1a1-4d44-ab62-e220ebaa2a5c": id("BAL"),
  "f9acd396-ba35-4cae-a431-a44b2af707b0": id("BUF"),
  "e37050ce-354f-4dd0-b2d8-0bd1e9650eb7": id("CAR"),
  "9a77500a-1945-495e-b6f5-5c35bedaa8b9": id("CHI"),
  "b007a1ab-bca6-42c7-8706-62607a605866": id("CIN"),
  "05cc62a4-bd74-4b2c-ab7b-1b177b9f6790": id("CLE"),
  "62b97b7c-5503-461c-b328-fd8543219b22": id("DAL"),
  "0aa02fd7-1bb1-474b-98e1-5379d0a191e3": id("DEN"),
  "f95a55a7-8472-4ffa-be20-14547e3c32ff": id("DET"),
  "08c52d1f-7ce1-445f-be72-97de0554ecc8": id("GB"),
  "96edbad3-1be5-4528-a3ed-c6e0c28c5fa0": id("HOU"),
  "fe5f8219-fef4-4498-83d5-f6cc6f2cb7bf": id("IND"),
  "61a75ab7-eaf0-4601-b920-86b1e165007d": id("JAX"),
  "64f72720-2e4a-4cc8-a39b-ca148aecb389": id("KC"),
  "611e3786-3bfe-4919-bbd5-bc70fc13d827": id("LAC"),
  "422d5fd7-ad18-4c96-902d-6f7c50e4660f": id("LAR"),
  "8b1ce23e-7d64-4213-85e9-7d12bc8c4d85": id("LV"),
  "4cba614d-2d32-48ef-a066-1524437fbaff": id("MIA"),
  "2279efa0-9284-4224-9647-09b14f6983ce": id("MIN"),
  "d1d64188-5cd5-4feb-97ff-8542c9dbeb43": id("NE"),
  "f7c2cd06-61bf-469e-984f-3ef56cb8f3d7": id("NO"),
  "4fa3f3ec-f7e7-41ec-9383-a40af2d26297": id("NYG"),
  "ccc2af2d-5c4d-4717-8036-5ba6bec5cfbd": id("NYJ"),
  "bc65405b-5bda-425c-b7c5-681c382e6a5a": id("PHI"),
  "6aa76797-7833-45e1-85e3-96e0122801e1": id("PIT"),
  "8e52cc75-cba8-4373-9b62-b535b53b3be8": id("SEA"),
  "bfaa7639-8bdf-47b2-8d28-98d2cdbefbcb": id("SF"),
  "1105fa0d-c798-4020-9e24-f377a334b3d9": id("TB"),
  "7c86629b-eafc-47b4-9204-b46a2d0e9cbb": id("TEN"),
  "aa65dda6-9cb8-4c8b-812b-d0c1feaae54f": id("WAS"),
});

/**
 * nflverse `home_team` / `away_team` codes in `games.csv` → team id. `LA` is the Rams. Used by
 * the next-day result check.
 */
export const NFLVERSE_CODES: Readonly<Record<string, number>> = Object.freeze({
  ARI: id("ARI"),
  ATL: id("ATL"),
  BAL: id("BAL"),
  BUF: id("BUF"),
  CAR: id("CAR"),
  CHI: id("CHI"),
  CIN: id("CIN"),
  CLE: id("CLE"),
  DAL: id("DAL"),
  DEN: id("DEN"),
  DET: id("DET"),
  GB: id("GB"),
  HOU: id("HOU"),
  IND: id("IND"),
  JAX: id("JAX"),
  KC: id("KC"),
  LA: id("LAR"),
  LAC: id("LAC"),
  LV: id("LV"),
  MIA: id("MIA"),
  MIN: id("MIN"),
  NE: id("NE"),
  NO: id("NO"),
  NYG: id("NYG"),
  NYJ: id("NYJ"),
  PHI: id("PHI"),
  PIT: id("PIT"),
  SEA: id("SEA"),
  SF: id("SF"),
  TB: id("TB"),
  TEN: id("TEN"),
  WAS: id("WAS"),
});

/** The source names the scores service uses for its team tables. */
export type TeamTableSource = "espn" | "kalshi" | "nflverse";

/** Every source table the package carries, by the name the scores service uses. */
export const SOURCE_TEAM_TABLES: Readonly<
  Record<TeamTableSource, Readonly<Record<string, number>>>
> = Object.freeze({ espn: ESPN_TEAM_IDS, kalshi: KALSHI_TEAM_IDS, nflverse: NFLVERSE_CODES });

/**
 * Our team id for a source's team identifier, or `undefined` when the source does not list it
 * (a game whose teams do not resolve is dropped with an alert, never inferred).
 */
export function resolveSourceTeam(
  source: TeamTableSource,
  sourceTeamId: string | number,
): number | undefined {
  return SOURCE_TEAM_TABLES[source][String(sourceTeamId)];
}
