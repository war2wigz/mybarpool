/**
 * The game-ID mapping shape for the three score sources (ARCHITECTURE › Games,
 * Score integrity: API-Sports, the Sportradar ID the Polymarket stream carries,
 * ESPN). Shape and pure helpers only; the scores service fills the map in Step
 * 10. A game is identified on-chain by its `GameKey`, never by a source's ID
 * (PROGRAM §2).
 */
import type { GameKey } from "./gameKey.js";

export type ScoreSource = "apiSports" | "sportradar" | "espn";

export const SCORE_SOURCES: readonly ScoreSource[] = ["apiSports", "sportradar", "espn"];

/** A game's IDs at each source; any may be unknown. */
export interface SourceGameIds {
  /** API-Sports `game.id`. */
  readonly apiSports?: number;
  /** The Sportradar game ID the Polymarket stream carries. */
  readonly sportradar?: string;
  /** ESPN event ID. */
  readonly espn?: string;
}

/** `sourceKey(source, id)` → `GameKey`. */
export type GameIdMap = ReadonlyMap<string, GameKey>;

export function sourceKey(source: ScoreSource, id: number | string): string {
  if (!SCORE_SOURCES.includes(source)) throw new RangeError(`unknown score source: ${source}`);
  const text = typeof id === "number" ? id.toString() : id;
  if (text.length === 0) throw new RangeError("source id must not be empty");
  return `${source}:${text}`;
}

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
      ["apiSports", ids.apiSports],
      ["sportradar", ids.sportradar],
      ["espn", ids.espn],
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

export function sameGameKey(a: GameKey, b: GameKey): boolean {
  return a.season === b.season && a.week === b.week && a.home === b.home && a.away === b.away;
}
