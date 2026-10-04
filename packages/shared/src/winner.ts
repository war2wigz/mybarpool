/**
 * Winner, PROGRAM §6.3.
 *
 * ```
 * hd = home mod 10; ad = away mod 10
 * col = the l in 0..5 with hd in {home_axis[l], home_axis[l+5]}
 * row = the l in 0..5 with ad in {away_axis[l], away_axis[l+5]}
 * box = row × 5 + col          // 0–24; label = box + 1
 * ```
 *
 * Columns are the home team (across the top), rows the away team (down the
 * side). Exactly one box matches for any pair of scores.
 */
import { DIGITS, type Digits, assertDigits, laneDigits, laneOfDigit } from "./axes.js";
import { type BoxIndex, colOf, indexAt, rowOf } from "./boxes.js";

export interface WinningBoxInput {
  /** Home team's cumulative score (u16). */
  readonly home: number;
  /** Away team's cumulative score (u16). */
  readonly away: number;
  /** Home digits, across the top: columns. */
  readonly homeAxis: Digits;
  /** Away digits, down the side: rows. */
  readonly awayAxis: Digits;
}

export const SCORE_MAX = 0xffff;

function assertScore(score: number, what: string): number {
  if (!Number.isInteger(score) || score < 0 || score > SCORE_MAX) {
    throw new RangeError(`${what} score must be a u16, got ${score}`);
  }
  return score;
}

/** The 0-based index of the box whose digits match the scores' last digits. */
export function winningBox({ home, away, homeAxis, awayAxis }: WinningBoxInput): BoxIndex {
  assertDigits(homeAxis);
  assertDigits(awayAxis);
  const hd = assertScore(home, "home") % DIGITS;
  const ad = assertScore(away, "away") % DIGITS;
  const col = laneOfDigit(homeAxis, hd);
  const row = laneOfDigit(awayAxis, ad);
  return indexAt(row, col);
}

/** A `[homeDigit, awayDigit]` pair of last digits. */
export type DigitPair = readonly [home: number, away: number];

/** The four `[home, away]` last-digit pairs a box covers (DESIGN §4.3 box sheet). */
export function boxDigitPairs(index: BoxIndex, homeAxis: Digits, awayAxis: Digits): DigitPair[] {
  const [h1, h2] = laneDigits(homeAxis, colOf(index));
  const [a1, a2] = laneDigits(awayAxis, rowOf(index));
  return [
    [h1, a1],
    [h1, a2],
    [h2, a1],
    [h2, a2],
  ];
}
