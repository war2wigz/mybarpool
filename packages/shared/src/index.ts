/**
 * MyBarPool client SDK: the pure core.
 *
 * Box labelling, the reference implementations of the on-chain algorithms
 * (PROGRAM §6), the money arithmetic (PROGRAM §5), display formatting
 * (DESIGN §10.3) and the static tables. No RPC, no wallet; the chain layer
 * generated from the program's IDL arrives in Step 9.
 *
 * Boxes are the unit. People see labels 1–25; the program stores indices
 * 0–24; `boxes.ts` is the only place the two are converted.
 */
export * from "./boxes.js";
export * from "./assignment.js";
export * from "./axes.js";
export * from "./winner.js";
export * from "./gameKey.js";
export * from "./seeds.js";
export * from "./entropy.js";
export * from "./teams.js";
export * from "./price.js";
export * from "./fees.js";
export * from "./format.js";
export * from "./sources.js";
export * from "./sponsors.js";
export * from "./allowlist.js";
export { assertU64, U64_MAX, toHex, fromHex } from "./bytes.js";

/** Package version, read by clients that want to log what they shipped with. */
export const SDK_VERSION = "0.0.0";
