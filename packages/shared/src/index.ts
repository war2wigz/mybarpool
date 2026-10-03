/**
 * MyBarPool client SDK.
 *
 * Step 0 of the build plan: an empty package with a build and a test runner.
 * Step 1 adds the pure math (box labelling, assignment, axis shuffle, winner,
 * price ladder, fees, team table); Step 9 adds the chain layer generated from
 * the program's IDL. Nothing here talks to an RPC yet.
 *
 * Boxes are the unit. People see labels 1–25; the program stores indices
 * 0–24; this package is the only place the two are converted.
 */

/** Boxes in a pool: a 5×5 grid. */
export const BOXES = 25;

/** Lanes per axis; lane `l` holds the digits at positions `l` and `l + 5`. */
export const LANES = 5;

/** Settlement periods; the fourth is the final score. */
export const QUARTERS = 4;

/** Package version, read by clients that want to log what they shipped with. */
export const SDK_VERSION = "0.0.0";
