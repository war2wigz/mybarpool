/**
 * The game-ID mapping shape for the five score sources (ARCHITECTURE › Games,
 * Score integrity: Polymarket's live stream, Kalshi's live data, ESPN's
 * scoreboard, TxODDS's TxLINE feed, and Yahoo's scoreboard). Shape and pure helpers only; the
 * scores service fills the map in Step 10. A game is identified on-chain by
 * its `GameKey`, never by a source's ID (PROGRAM §2).
 */
import type { GameKey } from "./gameKey.js";

/** A live-score source whose game ids map to a {@link GameKey}. */
export type ScoreSource = "polymarket" | "kalshi" | "espn" | "txodds" | "yahoo";

/** Every {@link ScoreSource}. */
export const SCORE_SOURCES: readonly ScoreSource[] = [
  "polymarket",
  "kalshi",
  "espn",
  "txodds",
  "yahoo",
];

/** A game's IDs at each source; any may be unknown. */
export interface SourceGameIds {
  /** Polymarket's `gameId` on its live frames (a small integer, as a string). */
  readonly polymarket?: string;
  /** Kalshi's milestone id (a UUID). */
  readonly kalshi?: string;
  /** ESPN event ID. */
  readonly espn?: string;
  /** TxODDS `FixtureId` (an integer on the TxLINE feed, as a string). */
  readonly txodds?: string;
  /** Yahoo's `gameid` on the editorial scoreboard (`nfl.g.<yyyymmdd><nnn>`). */
  readonly yahoo?: string;
}

/** `sourceKey(source, id)` → `GameKey`. */
export type GameIdMap = ReadonlyMap<string, GameKey>;

/** The map key for a source's game id. */
export function sourceKey(source: ScoreSource, id: number | string): string {
  if (!SCORE_SOURCES.includes(source)) throw new RangeError(`unknown score source: ${source}`);
  const text = typeof id === "number" ? id.toString() : id;
  if (text.length === 0) throw new RangeError("source id must not be empty");
  return `${source}:${text}`;
}

/** The {@link GameKey} a source's game id maps to, or `undefined`. */
export function lookupGameKey(
  map: GameIdMap,
  source: ScoreSource,
  id: number | string,
): GameKey | undefined {
  return map.get(sourceKey(source, id));
}

/** Build a `GameIdMap` from per-game ID sets; a source ID that maps to two games is an error. */
export function buildGameIdMap(entries: Iterable<{ key: GameKey; ids: SourceGameIds }>): GameIdMap {
  const map = new Map<string, GameKey>();
  for (const { key, ids } of entries) {
    const pairs: Array<[ScoreSource, number | string | undefined]> = [
      ["polymarket", ids.polymarket],
      ["kalshi", ids.kalshi],
      ["espn", ids.espn],
      ["txodds", ids.txodds],
      ["yahoo", ids.yahoo],
    ];
    for (const [source, id] of pairs) {
      if (id === undefined) continue;
      const k = sourceKey(source, id);
      const existing = map.get(k);
      if (existing !== undefined && !sameGameKey(existing, key)) {
        throw new RangeError(`${k} maps to two different games`);
      }
      map.set(k, key);
    }
  }
  return map;
}

/** Field-wise equality of two keys. */
export function sameGameKey(a: GameKey, b: GameKey): boolean {
  return a.season === b.season && a.week === b.week && a.home === b.home && a.away === b.away;
}
