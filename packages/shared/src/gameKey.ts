/**
 * Canonical game key, PROGRAM §2, and the `GameRecord` seeds, PROGRAM §3.2.
 *
 * Wire encoding (defined here; the program adopts it in Step 3): fields in
 * declaration order, little-endian, 5 bytes: `season` u16, `week` u8, `home` u8,
 * `away` u8.
 */
import { ascii, concatBytes, i64le, u16le, u8 } from "./bytes.js";

/** The canonical game key (PROGRAM §2): season, week, home and away team ids. */
export interface GameKey {
  /** The year the regular season starts in (2026 for the 2026–27 season). */
  readonly season: number;
  /** 1–18 regular season; 19 Wild Card, 20 Divisional, 21 Conference Championships, 22 Super Bowl; 101–103 preseason. */
  readonly week: number;
  /** Home team index 0–31 into `TEAMS`. */
  readonly home: number;
  /** Away team index 0–31 into `TEAMS`. */
  readonly away: number;
}

/** Encoded size of a {@link GameKey}: u16 season, u8 week, u8 home, u8 away. */
export const GAME_KEY_BYTES = 5;
/** Teams in the frozen table. */
export const TEAM_COUNT = 32;
/** Weeks 1–18 are the regular season. */
export const REGULAR_SEASON_WEEKS = 18;
/** Week 22 is the Super Bowl; 19–21 the earlier playoff rounds. */
export const SUPER_BOWL_WEEK = 22;
/** First preseason week number, so it can never collide with the season. */
export const PRESEASON_WEEK_MIN = 101;
/** Last preseason week number. */
export const PRESEASON_WEEK_MAX = 103;

/** Whether `week` is a preseason week (101–103). */
export function isPreseasonWeek(week: number): boolean {
  return week >= PRESEASON_WEEK_MIN && week <= PRESEASON_WEEK_MAX;
}

/** Checks every field of a {@link GameKey} and returns it. */
export function assertGameKey(key: GameKey): GameKey {
  const { season, week, home, away } = key;
  if (!Number.isInteger(season) || season < 0 || season > 0xffff) {
    throw new RangeError(`season must be a u16, got ${season}`);
  }
  if (
    !Number.isInteger(week) ||
    !((week >= 1 && week <= SUPER_BOWL_WEEK) || isPreseasonWeek(week))
  ) {
    throw new RangeError(
      `week must be 1–${SUPER_BOWL_WEEK} or ${PRESEASON_WEEK_MIN}–${PRESEASON_WEEK_MAX}, got ${week}`,
    );
  }
  for (const [name, team] of [
    ["home", home],
    ["away", away],
  ] as const) {
    if (!Number.isInteger(team) || team < 0 || team >= TEAM_COUNT) {
      throw new RangeError(`${name} must be 0–${TEAM_COUNT - 1}, got ${team}`);
    }
  }
  if (home === away) throw new RangeError("home and away must differ");
  return key;
}

/** The 5 little-endian bytes of a {@link GameKey}, the `GameRecord` seed. */
export function encodeGameKey(key: GameKey): Uint8Array {
  assertGameKey(key);
  return concatBytes(u16le(key.season), u8(key.week), u8(key.home), u8(key.away));
}

/** Inverse of {@link encodeGameKey}. */
export function decodeGameKey(bytes: Uint8Array): GameKey {
  if (bytes.length !== GAME_KEY_BYTES) {
    throw new RangeError(`game key must be ${GAME_KEY_BYTES} bytes, got ${bytes.length}`);
  }
  return assertGameKey({
    season: bytes[0]! | (bytes[1]! << 8),
    week: bytes[2]!,
    home: bytes[3]!,
    away: bytes[4]!,
  });
}

/** The six seeds of PROGRAM §3.2, in order: `"game"`, season, week, home, away, scheduled kickoff (i64 LE). */
export function gameRecordSeeds(key: GameKey, scheduledKickoff: bigint): Uint8Array[] {
  assertGameKey(key);
  return [
    ascii("game"),
    u16le(key.season),
    u8(key.week),
    u8(key.home),
    u8(key.away),
    i64le(scheduledKickoff),
  ];
}
