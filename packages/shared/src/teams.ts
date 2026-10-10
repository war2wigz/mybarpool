/**
 * The 32 NFL teams in the frozen PROGRAM §2 order (sorted by abbreviation;
 * index = team id). Adding a team is a table change and a shared-package major.
 *
 * Colours are the teams' published primary and secondary brand colours as
 * `#rrggbb`, used only inside grid axes and team chips (DESIGN §1 Palette).
 * No club logos exist anywhere: a team is shown as a generic helmet shape in
 * these two colours beside its abbreviation or name (DESIGN §9, §10.11).
 */

export interface Team {
  /** Index into `TEAMS`; the `home`/`away` value in a `GameKey`. */
  readonly id: number;
  readonly abbreviation: string;
  readonly city: string;
  readonly name: string;
  readonly colors: { readonly primary: string; readonly secondary: string };
}

function team(
  id: number,
  abbreviation: string,
  city: string,
  name: string,
  primary: string,
  secondary: string,
): Team {
  return { id, abbreviation, city, name, colors: { primary, secondary } };
}

/** The frozen 32-team table, ids 1–32 (PROGRAM §2). */
export const TEAMS: readonly Team[] = Object.freeze([
  team(0, "ARI", "Arizona", "Cardinals", "#97233F", "#000000"),
  team(1, "ATL", "Atlanta", "Falcons", "#A71930", "#000000"),
  team(2, "BAL", "Baltimore", "Ravens", "#241773", "#000000"),
  team(3, "BUF", "Buffalo", "Bills", "#00338D", "#C60C30"),
  team(4, "CAR", "Carolina", "Panthers", "#0085CA", "#101820"),
  team(5, "CHI", "Chicago", "Bears", "#0B162A", "#C83803"),
  team(6, "CIN", "Cincinnati", "Bengals", "#FB4F14", "#000000"),
  team(7, "CLE", "Cleveland", "Browns", "#311D00", "#FF3C00"),
  team(8, "DAL", "Dallas", "Cowboys", "#003594", "#869397"),
  team(9, "DEN", "Denver", "Broncos", "#FB4F14", "#002244"),
  team(10, "DET", "Detroit", "Lions", "#0076B6", "#B0B7BC"),
  team(11, "GB", "Green Bay", "Packers", "#203731", "#FFB612"),
  team(12, "HOU", "Houston", "Texans", "#03202F", "#A71930"),
  team(13, "IND", "Indianapolis", "Colts", "#002C5F", "#A2AAAD"),
  team(14, "JAX", "Jacksonville", "Jaguars", "#006778", "#D7A22A"),
  team(15, "KC", "Kansas City", "Chiefs", "#E31837", "#FFB81C"),
  team(16, "LAC", "Los Angeles", "Chargers", "#0080C6", "#FFC20E"),
  team(17, "LAR", "Los Angeles", "Rams", "#003594", "#FFA300"),
  team(18, "LV", "Las Vegas", "Raiders", "#000000", "#A5ACAF"),
  team(19, "MIA", "Miami", "Dolphins", "#008E97", "#FC4C02"),
  team(20, "MIN", "Minnesota", "Vikings", "#4F2683", "#FFC62F"),
  team(21, "NE", "New England", "Patriots", "#002244", "#C60C30"),
  team(22, "NO", "New Orleans", "Saints", "#D3BC8D", "#101820"),
  team(23, "NYG", "New York", "Giants", "#0B2265", "#A71930"),
  team(24, "NYJ", "New York", "Jets", "#125740", "#000000"),
  team(25, "PHI", "Philadelphia", "Eagles", "#004C54", "#A5ACAF"),
  team(26, "PIT", "Pittsburgh", "Steelers", "#FFB612", "#101820"),
  team(27, "SEA", "Seattle", "Seahawks", "#002244", "#69BE28"),
  team(28, "SF", "San Francisco", "49ers", "#AA0000", "#B3995D"),
  team(29, "TB", "Tampa Bay", "Buccaneers", "#D50A0A", "#FF7900"),
  team(30, "TEN", "Tennessee", "Titans", "#0C2340", "#4B92DB"),
  team(31, "WAS", "Washington", "Commanders", "#5A1414", "#FFB612"),
]);

const BY_ABBREVIATION: ReadonlyMap<string, Team> = new Map(TEAMS.map((t) => [t.abbreviation, t]));

/** The team with this abbreviation, or `undefined`. */
export function teamByAbbreviation(abbreviation: string): Team | undefined {
  return BY_ABBREVIATION.get(abbreviation.toUpperCase());
}

/** The team with this id (1–32); throws `RangeError` otherwise. */
export function teamById(id: number): Team {
  const found = TEAMS[id];
  if (found === undefined)
    throw new RangeError(`team id out of range 0–${TEAMS.length - 1}: ${id}`);
  return found;
}
