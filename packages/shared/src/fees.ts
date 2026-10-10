/**
 * Fees and prizes, PROGRAM §1 (constants, presets) and §5 (money);
 * ARCHITECTURE › Fees, Payouts, Sponsorship.
 *
 * Every amount is `bigint` base units; `floor` is `bigint` division; every
 * output passes `assertU64` so the vectors never hold a value the program's
 * `u64` would reject. Basis points are plain integers (the program's `u16`).
 */
import { BOXES } from "./boxes.js";
import { assertU64 } from "./bytes.js";

/** PROGRAM §1: ceiling on the platform share. */
export const PLATFORM_BPS_MAX = 500;
/** PROGRAM §1: ceiling on the creator base share. */
export const CREATOR_BPS_MAX = 500;
/** PROGRAM §1: ceiling on `creator_addon_bps + integrator_bps`. */
export const ADDON_BUDGET_BPS_MAX = 500;
/** PROGRAM §1: ceiling on platform + creator base + add-on budget. */
export const TOTAL_BPS_MAX = 1500;
/** Basis points in a whole: 10,000. */
export const BPS_DENOMINATOR = 10_000n;

/** PROGRAM §1 payout presets; the discriminant is the on-chain value. */
export enum PayoutPreset {
  Standard = 0,
  Even = 1,
  FinalOnly = 2,
}

/** A payout preset's per-quarter percentages, summing to 100. */
export type Split = readonly [q1: number, q2: number, q3: number, final: number];

/** PROGRAM §1 preset table, in percent. */
export const PRESET_SPLITS: Readonly<Record<PayoutPreset, Split>> = Object.freeze({
  [PayoutPreset.Standard]: [20, 20, 20, 40],
  [PayoutPreset.Even]: [25, 25, 25, 25],
  [PayoutPreset.FinalOnly]: [0, 0, 0, 100],
});

/** Whether `value` is one of the PROGRAM §1 presets. */
export function isPayoutPreset(value: number): value is PayoutPreset {
  return (
    value === PayoutPreset.Standard ||
    value === PayoutPreset.Even ||
    value === PayoutPreset.FinalOnly
  );
}

/** Checks that `value` is a preset and returns it typed. */
export function assertPayoutPreset(value: number): PayoutPreset {
  if (!isPayoutPreset(value)) throw new RangeError(`invalid payout preset: ${value}`);
  return value;
}

function assertBps(value: number, max: number, what: string): number {
  if (!Number.isInteger(value) || value < 0 || value > max) {
    throw new RangeError(`${what} must be 0–${max} bps, got ${value}`);
  }
  return value;
}

/** The platform's fee settings from `PlatformConfig` (PROGRAM §3.1). */
export interface FeeConfig {
  /** Platform share, ≤ `PLATFORM_BPS_MAX` (initial 500). */
  readonly platformBps: number;
  /** Creator base share, ≤ `CREATOR_BPS_MAX` (initial 500). */
  readonly creatorBps: number;
  /** Add-on budget the creator add-on and integrator fee share, ≤ `ADDON_BUDGET_BPS_MAX` (initial 500). */
  readonly addonBudgetBps: number;
}

/** What a creator chose for a pool: the add-on and the integrator share. */
export interface PoolFeeChoice {
  /** Creator add-on, 0–500 (ARCHITECTURE › Fees). */
  readonly creatorAddonBps: number;
  /** Integrator fee, 0–500; 0 when unset. */
  readonly integratorBps: number;
}

/**
 * PROGRAM §3.1 invariants on config plus the §4.3 `create_pool` check on the add-on budget.
 * `platform + creator + addon_budget ≤ TOTAL_BPS_MAX` follows from the three per-field ceilings
 * (500 + 500 + 500 = 1500); the program checks it explicitly, here it is implied and pinned by a test.
 */
export function validateFeeConfig(config: FeeConfig, choice: PoolFeeChoice): void {
  assertBps(config.platformBps, PLATFORM_BPS_MAX, "platformBps");
  assertBps(config.creatorBps, CREATOR_BPS_MAX, "creatorBps");
  assertBps(config.addonBudgetBps, ADDON_BUDGET_BPS_MAX, "addonBudgetBps");
  assertBps(choice.creatorAddonBps, ADDON_BUDGET_BPS_MAX, "creatorAddonBps");
  assertBps(choice.integratorBps, ADDON_BUDGET_BPS_MAX, "integratorBps");
  if (choice.creatorAddonBps + choice.integratorBps > config.addonBudgetBps) {
    throw new RangeError(
      `creatorAddonBps + integratorBps exceeds the add-on budget of ${config.addonBudgetBps} bps`,
    );
  }
}

/** `P = 25 × price` (PROGRAM §5.1). */
export function pot(price: bigint): bigint {
  assertU64(price, "price");
  if (price < 1n) throw new RangeError("price must be ≥ 1 base unit");
  return assertU64(BigInt(BOXES) * price, "pot");
}

/** Input to {@link feeAmounts}: the price and every fee rate. */
export interface FeeAmountsInput extends PoolFeeChoice {
  readonly price: bigint;
  readonly platformBps: number;
  readonly creatorBps: number;
}

/** The three fees of a pool, fixed at creation (PROGRAM §5). */
export interface FeeAmounts {
  /** `floor(P × platform_bps / 10_000)` */
  readonly platformFee: bigint;
  /** `floor(P × (creator_bps + creator_addon_bps) / 10_000)`: base and add-on are one transfer. */
  readonly creatorFee: bigint;
  /** `floor(P × integrator_bps / 10_000)`; 0 when unset. */
  readonly integratorFee: bigint;
}

/** PROGRAM §5.1, fixed at creation. Fees are computed on the pot only, never on a sponsorship. */
export function feeAmounts(input: FeeAmountsInput): FeeAmounts {
  const P = pot(input.price);
  const platformBps = assertBps(input.platformBps, PLATFORM_BPS_MAX, "platformBps");
  const creatorBps = assertBps(input.creatorBps, CREATOR_BPS_MAX, "creatorBps");
  const creatorAddonBps = assertBps(input.creatorAddonBps, ADDON_BUDGET_BPS_MAX, "creatorAddonBps");
  const integratorBps = assertBps(input.integratorBps, ADDON_BUDGET_BPS_MAX, "integratorBps");
  if (creatorAddonBps + integratorBps > ADDON_BUDGET_BPS_MAX) {
    throw new RangeError(`creatorAddonBps + integratorBps exceeds ${ADDON_BUDGET_BPS_MAX} bps`);
  }
  return {
    platformFee: assertU64((P * BigInt(platformBps)) / BPS_DENOMINATOR, "platformFee"),
    creatorFee: assertU64(
      (P * BigInt(creatorBps + creatorAddonBps)) / BPS_DENOMINATOR,
      "creatorFee",
    ),
    integratorFee: assertU64((P * BigInt(integratorBps)) / BPS_DENOMINATOR, "integratorFee"),
  };
}

/** Input to {@link prizePool}. */
export interface PrizePoolInput {
  readonly pot: bigint;
  readonly fees: FeeAmounts;
  /** Sum of all sponsorships by kickoff; 0 on most pools. No fee is ever computed on it. */
  readonly sponsoredTotal: bigint;
}

/** `prize_pool = P − platform_fee − creator_fee − integrator_fee + sponsored_total` (PROGRAM §5.2). */
export function prizePool({ pot: P, fees, sponsoredTotal }: PrizePoolInput): bigint {
  assertU64(P, "pot");
  assertU64(sponsoredTotal, "sponsoredTotal");
  const afterFees = P - fees.platformFee - fees.creatorFee - fees.integratorFee;
  if (afterFees < 0n) throw new RangeError("fees exceed the pot");
  return assertU64(afterFees + sponsoredTotal, "prizePool");
}

/** The four quarter prizes in base units. */
export type QuarterAmounts = readonly [q1: bigint, q2: bigint, q3: bigint, final: bigint];

/** Result of {@link quarterPrizes}: the four amounts and the dust. */
export interface QuarterPrizes {
  /** `floor(prize_pool × split[q] / 100)` per quarter. */
  readonly quarters: QuarterAmounts;
  /** `prize_pool − Σ quarter_prize`, ≤ 3 base units on any preset; swept to the platform at close. */
  readonly dust: bigint;
}

/** PROGRAM §5.2, fixed at the first settlement. */
export function quarterPrizes(prizePoolAmount: bigint, preset: PayoutPreset): QuarterPrizes {
  assertU64(prizePoolAmount, "prizePool");
  const split = PRESET_SPLITS[assertPayoutPreset(preset)];
  const quarters = split.map((pct) =>
    assertU64((prizePoolAmount * BigInt(pct)) / 100n, "quarterPrize"),
  );
  const paid = quarters.reduce((sum, q) => sum + q, 0n);
  return {
    quarters: quarters as unknown as QuarterAmounts,
    dust: assertU64(prizePoolAmount - paid, "dust"),
  };
}

/** Input to {@link creatorBreakEvenBoxes}. */
export interface BreakEvenInput extends PoolFeeChoice {
  readonly platformBps: number;
  readonly creatorBps: number;
}

/**
 * Boxes a creator must hold for their fee share to cover their own expected
 * losses: creator share × 25 ÷ total fee share (ARCHITECTURE › Limits: 12.5 at
 * the base fee). A plain number for display; it is not money.
 */
export function creatorBreakEvenBoxes(input: BreakEvenInput): number {
  const creatorShare =
    assertBps(input.creatorBps, CREATOR_BPS_MAX, "creatorBps") +
    assertBps(input.creatorAddonBps, ADDON_BUDGET_BPS_MAX, "creatorAddonBps");
  const total =
    assertBps(input.platformBps, PLATFORM_BPS_MAX, "platformBps") +
    creatorShare +
    assertBps(input.integratorBps, ADDON_BUDGET_BPS_MAX, "integratorBps");
  if (total === 0) throw new RangeError("total fee share is zero");
  return (creatorShare * BOXES) / total;
}
