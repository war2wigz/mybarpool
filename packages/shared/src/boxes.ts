/**
 * Box numbering (ARCHITECTURE › Grid, Buying › Numbering; PROGRAM conventions).
 *
 * The program stores indices 0–24; people see labels 1–25, box 1 top-left,
 * numbering left to right then down. This file is the only place in the
 * product that adds or subtracts 1 on a box number.
 */

/** Boxes per pool: a 5×5 grid (PROGRAM §1 `BOXES`). */
export const BOXES = 25;

/** Positions per axis; lane `l` holds the digits at positions `l` and `l + 5` (PROGRAM §1 `LANES`). */
export const LANES = 5;

/** Settlement periods; the fourth is the final score (PROGRAM §1 `QUARTERS`). */
export const QUARTERS = 4;

/** 0-based box index as the program stores it. */
export type BoxIndex = number;

/** 1-based box label as a person sees it. */
export type BoxLabel = number;

/** Checks that `index` is an integer in 0–24 and returns it typed. */
export function assertBoxIndex(index: number): BoxIndex {
  if (!Number.isInteger(index) || index < 0 || index >= BOXES) {
    throw new RangeError(`box index out of range 0–${BOXES - 1}: ${index}`);
  }
  return index;
}

/** Checks that `label` is an integer in 1–25 and returns it typed. */
export function assertBoxLabel(label: number): BoxLabel {
  if (!Number.isInteger(label) || label < 1 || label > BOXES) {
    throw new RangeError(`box label out of range 1–${BOXES}: ${label}`);
  }
  return label;
}

/** Checks that `lane` is an integer in 0–4 (a row or a column) and returns it. */
export function assertLane(lane: number): number {
  if (!Number.isInteger(lane) || lane < 0 || lane >= LANES) {
    throw new RangeError(`lane out of range 0–${LANES - 1}: ${lane}`);
  }
  return lane;
}

/** Index 0–24 → label 1–25. */
export function toLabel(index: BoxIndex): BoxLabel {
  return assertBoxIndex(index) + 1;
}

/** Label 1–25 → index 0–24. */
export function toIndex(label: BoxLabel): BoxIndex {
  return assertBoxLabel(label) - 1;
}

/** Row 0–4 (away team, down the side). */
export function rowOf(index: BoxIndex): number {
  return Math.floor(assertBoxIndex(index) / LANES);
}

/** Column 0–4 (home team, across the top). */
export function colOf(index: BoxIndex): number {
  return assertBoxIndex(index) % LANES;
}

/** `box = row × 5 + col` (PROGRAM §6.3). */
export function indexAt(row: number, col: number): BoxIndex {
  return assertLane(row) * LANES + assertLane(col);
}
