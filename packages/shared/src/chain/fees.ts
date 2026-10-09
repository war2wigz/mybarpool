/**
 * Compute and fee numbers (DESIGN §10.4). The compute-unit limit a client sets is simulated
 * usage plus 20 % plus 30,000 units for instructions a wallet may append; the priority fee is a
 * small micro-lamport price adjusted by the recent-fee API and capped so a `buy` shows as
 * "Network fee ~0.00001 SOL"; the base fee is one constant per signature.
 */
import type { Address } from "@solana/kit";

import type { MyBarPoolClient } from "./client.js";

/** DESIGN §10.4: 20 % headroom over the simulated compute units. */
export const CU_HEADROOM_RATIO = 0.2;
/** DESIGN §10.4: 30,000 units reserved for instructions a wallet may append. */
export const CU_WALLET_RESERVE = 30_000;
/** The runtime's per-transaction compute-unit maximum; the limit is clamped to it. */
export const CU_MAX = 1_400_000;
/** 8 KiB over the simulated loaded-accounts data size, for accounts created on the way. */
export const DATA_SIZE_HEADROOM = 8 * 1024;
/** The data a token account takes when the instruction creates one (165 bytes, rounded up). */
export const TOKEN_ACCOUNT_BYTES = 256;
/** The floor of the suggested priority fee: a quiet network must not produce a zero fee. */
export const PRIORITY_FEE_FLOOR_MICRO_LAMPORTS = 1_000n;
/**
 * The cap of the suggested priority fee: at a `buy` limit of about 200,000 units, 50,000
 * micro-lamports per unit is 10,000 lamports, the "~0.00001 SOL" the copy shows.
 */
export const PRIORITY_FEE_CAP_MICRO_LAMPORTS = 50_000n;
/** The runtime's fee per signature since genesis, in lamports. */
export const LAMPORTS_PER_SIGNATURE = 5_000n;

/** `ceil(unitsConsumed × 1.2) + 30,000`, clamped to the runtime maximum. */
export function computeUnitLimitFor(unitsConsumed: number): number {
  const withHeadroom = Math.ceil(unitsConsumed * (1 + CU_HEADROOM_RATIO)) + CU_WALLET_RESERVE;
  return Math.min(withHeadroom, CU_MAX);
}

/** `simulated + createdAccountBytes + 8 KiB`. */
export function loadedAccountsDataSizeLimitFor(simulated: number, createdAccountBytes = 0): number {
  return simulated + createdAccountBytes + DATA_SIZE_HEADROOM;
}

/**
 * A price in micro-lamports per compute unit as a total in lamports for `computeUnitLimit`
 * units, rounded up (the conversion between the legacy price and the v1 total; the displayed
 * fee is the same either way).
 */
export function priorityFeeLamportsFor(
  priceMicroLamports: bigint | number,
  computeUnitLimit: number,
): bigint {
  const price = BigInt(priceMicroLamports);
  const total = price * BigInt(computeUnitLimit);
  return (total + 999_999n) / 1_000_000n;
}

/** Bounds for {@link suggestPriorityFee}; both default to the exported constants. */
export interface PriorityFeeBounds {
  floorMicroLamports?: bigint;
  capMicroLamports?: bigint;
}

/**
 * The median of `getRecentPrioritizationFees(writableAccounts)` over the returned slots, as
 * micro-lamports per compute unit, clamped to `[floor, cap]`. An empty answer is the floor.
 */
export async function suggestPriorityFee(
  client: MyBarPoolClient,
  writableAccounts: readonly Address[],
  bounds: PriorityFeeBounds = {},
): Promise<bigint> {
  const floor = bounds.floorMicroLamports ?? PRIORITY_FEE_FLOOR_MICRO_LAMPORTS;
  const cap = bounds.capMicroLamports ?? PRIORITY_FEE_CAP_MICRO_LAMPORTS;
  const recent = await client.rpc.getRecentPrioritizationFees([...writableAccounts]).send();
  const fees = recent
    .map((r) => BigInt(r.prioritizationFee))
    .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const median = fees.length === 0 ? floor : fees[Math.floor(fees.length / 2)]!;
  return median < floor ? floor : median > cap ? cap : median;
}

/** What a prepared transaction costs the fee payer, for the "Network fee ~" line. */
export interface NetworkFee {
  /** `signatureCount × LAMPORTS_PER_SIGNATURE`. */
  baseFeeLamports: bigint;
  /** The priority total in lamports. */
  priorityFeeLamports: bigint;
  /** The sum. */
  totalLamports: bigint;
}

/** The network fee of a prepared transaction from its signature count and priority total. */
export function networkFeeLamports(prepared: {
  signatureCount: number;
  priorityFeeLamports: bigint;
}): NetworkFee {
  const baseFeeLamports = BigInt(prepared.signatureCount) * LAMPORTS_PER_SIGNATURE;
  return {
    baseFeeLamports,
    priorityFeeLamports: prepared.priorityFeeLamports,
    totalLamports: baseFeeLamports + prepared.priorityFeeLamports,
  };
}
