/**
 * Price ladders, ARCHITECTURE › Buying and PROGRAM §3.1 `TokenRule`.
 *
 * Each token has a minimum, a step and a maximum in base units; the program
 * rejects any price that is not `min + k × step` within the cap. Money is
 * `bigint` throughout; nothing here touches `Number` on an amount.
 */
import { BOXES } from "./boxes.js";
import { assertU64 } from "./bytes.js";

/** PROGRAM §2 token index. */
export enum TokenIndex {
  SOL = 0,
  SKR = 1,
  ORE = 2,
}

/** The price fields of a `TokenRule`, in base units. */
export interface PriceLadder {
  readonly decimals: number;
  readonly minPrice: bigint;
  readonly step: bigint;
  readonly maxPrice: bigint;
}

/** `10^decimals` as a bigint. */
export function pow10(decimals: number): bigint {
  if (!Number.isInteger(decimals) || decimals < 0 || decimals > 38) {
    throw new RangeError(`decimals out of range 0–38: ${decimals}`);
  }
  return 10n ** BigInt(decimals);
}

/** PROGRAM §3.1 invariants: `min ≥ 1`, `step ≥ 1`, `min ≤ max`, `(max − min) % step == 0`; everything a u64. */
export function validateLadder(ladder: PriceLadder): PriceLadder {
  pow10(ladder.decimals);
  assertU64(ladder.minPrice, "minPrice");
  assertU64(ladder.step, "step");
  assertU64(ladder.maxPrice, "maxPrice");
  if (ladder.minPrice < 1n) throw new RangeError("minPrice must be ≥ 1");
  if (ladder.step < 1n) throw new RangeError("step must be ≥ 1");
  if (ladder.minPrice > ladder.maxPrice) throw new RangeError("minPrice must be ≤ maxPrice");
  if ((ladder.maxPrice - ladder.minPrice) % ladder.step !== 0n) {
    throw new RangeError("(maxPrice − minPrice) must be a multiple of step");
  }
  return ladder;
}

/** `price == min + k × step` for some integer `k`, and `price ≤ max` (PROGRAM §4.3 `create_pool`). */
export function isValidPrice(ladder: PriceLadder, amount: bigint): boolean {
  validateLadder(ladder);
  if (amount < ladder.minPrice || amount > ladder.maxPrice) return false;
  return (amount - ladder.minPrice) % ladder.step === 0n;
}

/** Upper bound on `priceLadderSteps`; the real ladders have 20 (SOL, ORE) and 50 (SKR). */
export const MAX_LADDER_STEPS = 10_000n;

/** Every valid price on the ladder, ascending (the create flow's stepper, DESIGN §4.5). */
export function priceLadderSteps(ladder: PriceLadder): bigint[] {
  validateLadder(ladder);
  const count = (ladder.maxPrice - ladder.minPrice) / ladder.step + 1n;
  if (count > MAX_LADDER_STEPS) {
    throw new RangeError(`ladder has ${count} steps, more than ${MAX_LADDER_STEPS}`);
  }
  const steps: bigint[] = [];
  for (let p = ladder.minPrice; p <= ladder.maxPrice; p += ladder.step) steps.push(p);
  return steps;
}

/** Initial per-pool sponsorship cap: `25 × max_price` (PROGRAM §3.1 `max_sponsorship`). */
export function initialMaxSponsorship(ladder: PriceLadder): bigint {
  validateLadder(ladder);
  return assertU64(BigInt(BOXES) * ladder.maxPrice, "maxSponsorship");
}

/** A ladder stated in whole tokens, for tokens whose decimals arrive at build time. */
export interface WholeTokenLadder {
  readonly minPrice: bigint;
  readonly step: bigint;
  readonly maxPrice: bigint;
}

/** ARCHITECTURE › Buying table, in whole tokens. SOL/ORE are not whole-token ladders (0.05 steps) and are given in base units below. */
export const INITIAL_LADDERS_WHOLE: { readonly SKR: WholeTokenLadder } = {
  // ARCHITECTURE › Buying: SKR 100 / 100 / 5,000
  SKR: { minPrice: 100n, step: 100n, maxPrice: 5_000n },
};

function scaled(whole: WholeTokenLadder, decimals: number): PriceLadder {
  const unit = pow10(decimals);
  return validateLadder({
    decimals,
    minPrice: whole.minPrice * unit,
    step: whole.step * unit,
    maxPrice: whole.maxPrice * unit,
  });
}

/** SKR's decimals are per mint and unknown at build time (PROGRAM §3.1); the caller supplies them. */
export function skrLadder(decimals: number): PriceLadder {
  return scaled(INITIAL_LADDERS_WHOLE.SKR, decimals);
}

/** SOL has 9 decimals (lamports). */
export const SOL_DECIMALS = 9;
/** ORE has 11 decimals. */
export const ORE_DECIMALS = 11;

/** ARCHITECTURE › Buying table in base units: SOL and ORE `0.05 / 0.05 / 1`. */
export const INITIAL_LADDERS: { readonly SOL: PriceLadder; readonly ORE: PriceLadder } = {
  SOL: validateLadder({
    decimals: SOL_DECIMALS,
    minPrice: 50_000_000n, // 0.05 SOL
    step: 50_000_000n, // 0.05 SOL
    maxPrice: 1_000_000_000n, // 1 SOL
  }),
  ORE: validateLadder({
    decimals: ORE_DECIMALS,
    minPrice: 5_000_000_000n, // 0.05 ORE
    step: 5_000_000_000n, // 0.05 ORE
    maxPrice: 100_000_000_000n, // 1 ORE
  }),
};
