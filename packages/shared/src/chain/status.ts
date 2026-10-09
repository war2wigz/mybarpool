/**
 * DESIGN §10.2 derived states and the §10.1 "leading vs won" distinction, from chain state
 * only. `now` is always a parameter in seconds; the SDK never reads a clock.
 *
 * The live-feed rows of §10.2 ("Delayed", the "Verifying…" tag) depend on the scores feed and
 * are the app's, layered on top of what this module returns.
 */
import type { Address } from "@solana/kit";

import { assertDigits, type Digits } from "../axes.js";
import { BOXES, QUARTERS, toLabel, type BoxLabel } from "../boxes.js";
import { winningBox } from "../winner.js";
import { GameStatus, PoolStatus, type GameRecord, type Pool } from "../generated/index.js";
import { DEFAULT_ADDRESS } from "./pdas.js";

/** PROGRAM §1 `RECLAIM_DELAY`: 30 days after the scheduled kickoff a pool can be reclaimed. */
export const RECLAIM_DELAY_SECONDS = 2_592_000n;

/** Why a pool is `RETURNED` (DESIGN §10.2 header copy). */
export type ReturnReason = "unfilled" | "postponed" | "cancelled" | "cancelledByAdmin";

/** The §10.2 rows that chain state alone decides. */
export type DisplayStatus =
  | { kind: "OPEN" }
  | { kind: "LOCKED_DRAWING" }
  | { kind: "LOCKED_WAITING" }
  | { kind: "LIVE" }
  | { kind: "SETTLED" }
  | { kind: "RETURNED"; reason: ReturnReason }
  | { kind: "SPLIT" }
  | { kind: "ABANDONED" };

/** PROGRAM §4.6's "why returned": the game record first, then the admin flag, else unfilled. */
export function returnReason(pool: Pool, game: GameRecord): ReturnReason {
  if (game.status === GameStatus.Postponed) return "postponed";
  if (game.status === GameStatus.Cancelled) return "cancelled";
  if (pool.cancelledByAdmin) return "cancelledByAdmin";
  return "unfilled";
}

/**
 * DESIGN §10.2, the chain-only rows. `ABANDONED` is an unresolved pool (`Open`, `Locked`,
 * `Drawn`) past `scheduled_kickoff + 30 d`, when "Reclaim your share" applies to every owner;
 * a `Returned`/`Split` pool with `abandoned` set shows as `RETURNED`/`SPLIT` here, and whether
 * a given wallet still holds an unreturned box is {@link unreturnedBoxesOf}. A `Drawn` pool
 * after kickoff is `LIVE` until `settle` confirms the final quarter, whatever the game record
 * says (the keeper's settlement, not the game's `Final`, is what moves the pool on).
 */
export function displayStatus(pool: Pool, game: GameRecord, now: bigint): DisplayStatus {
  switch (pool.status) {
    case PoolStatus.Open:
    case PoolStatus.Locked:
    case PoolStatus.Drawn:
      if (now >= game.scheduledKickoff + RECLAIM_DELAY_SECONDS) return { kind: "ABANDONED" };
      if (pool.status === PoolStatus.Open) return { kind: "OPEN" };
      if (!pool.drawn) return { kind: "LOCKED_DRAWING" };
      return now < game.recordedKickoff ? { kind: "LOCKED_WAITING" } : { kind: "LIVE" };
    case PoolStatus.Settled:
      return { kind: "SETTLED" };
    case PoolStatus.Returned:
      return { kind: "RETURNED", reason: returnReason(pool, game) };
    case PoolStatus.Split:
      return { kind: "SPLIT" };
  }
}

/** The pool's axes as digits, `undefined` until drawn. */
export function axesOf(pool: Pool): { home: Digits; away: Digits } | undefined {
  if (!pool.drawn) return undefined;
  return { home: assertDigits([...pool.homeAxis]), away: assertDigits([...pool.awayAxis]) };
}

/**
 * ARCHITECTURE › Live scores vs. results: "Live scores (from the scores service over
 * WebSocket): display only. The grid may highlight the box that would currently win as
 * 'leading', clearly marked provisional, and it changes with every score update." The label
 * the given scores point at, `undefined` until the pool is drawn. Never a result: a box is won
 * only by {@link wonBoxes}.
 */
export function leadingBox(pool: Pool, home: number, away: number): BoxLabel | undefined {
  const axes = axesOf(pool);
  if (!axes) return undefined;
  return toLabel(winningBox({ home, away, homeAxis: axes.home, awayAxis: axes.away }));
}

/** One settled quarter: the label, who owned it, what it paid. */
export interface WonBox {
  quarter: 1 | 2 | 3 | 4;
  box: BoxLabel;
  winner: Address;
  amount: bigint;
}

/**
 * ARCHITECTURE › Live scores vs. results: "Results (from on-chain settlement events emitted by
 * the program): the only source of truth for winners and payouts." The quarters below
 * `quarters_settled`, from `winning_box`, `quarter_prize` and `owners`; the `QuarterSettled`
 * event carries the same facts (`boxIndex` → `toLabel`) for a client that follows events.
 */
export function wonBoxes(pool: Pool): WonBox[] {
  const won: WonBox[] = [];
  for (let q = 0; q < Math.min(pool.quartersSettled, QUARTERS); q++) {
    const index = pool.winningBox[q]!;
    if (index >= BOXES) continue; // 255 until settled; defensive against a torn read
    won.push({
      quarter: (q + 1) as 1 | 2 | 3 | 4,
      box: toLabel(index),
      winner: pool.owners[index]!,
      amount: pool.quarterPrize[q]!,
    });
  }
  return won;
}

/** The labels of `wallet`'s boxes whose `returned` bit is clear (empty when it owns none). */
export function unreturnedBoxesOf(pool: Pool, wallet: Address): BoxLabel[] {
  const out: BoxLabel[] = [];
  for (let i = 0; i < BOXES; i++) {
    if (pool.owners[i] === wallet && ((pool.returned >>> i) & 1) === 0) out.push(toLabel(i));
  }
  return out;
}

/** One grid cell of {@link PoolView}. */
export interface BoxView {
  label: BoxLabel;
  /** `null` while unsold. */
  owner: Address | null;
  /** The `returned` bit: paid back, split or reclaimed. */
  returned: boolean;
  /** Quarters (1–4) this box has won, from the settled quarters. */
  wonQuarters: number[];
}

/** DESIGN §10.1/§10.2: a pool as the app shows it, every box named by its label. */
export interface PoolView {
  status: DisplayStatus;
  /** 25, in label order (1–25). */
  boxes: BoxView[];
  winners: WonBox[];
  sold: number;
  prizePool: bigint;
  /** Per quarter, fixed at the first settlement; zeros before. */
  quarterPrizes: readonly [bigint, bigint, bigint, bigint];
  fees: { platform: bigint; creator: bigint; integrator: bigint; paid: boolean };
  sponsoredTotal: bigint;
  /** `undefined` until drawn. */
  axes: { home: Digits; away: Digits } | undefined;
}

/** The derived view of a pool; the one hand-written type in the SDK that names boxes. */
export function poolView(pool: Pool, game: GameRecord, now: bigint): PoolView {
  const winners = wonBoxes(pool);
  const boxes: BoxView[] = [];
  for (let i = 0; i < BOXES; i++) {
    const owner = pool.owners[i]!;
    const label = toLabel(i);
    boxes.push({
      label,
      owner: owner === DEFAULT_ADDRESS ? null : owner,
      returned: ((pool.returned >>> i) & 1) === 1,
      wonQuarters: winners.filter((w) => w.box === label).map((w) => w.quarter),
    });
  }
  return {
    status: displayStatus(pool, game, now),
    boxes,
    winners,
    sold: pool.sold,
    prizePool: pool.prizePool,
    quarterPrizes: [
      pool.quarterPrize[0]!,
      pool.quarterPrize[1]!,
      pool.quarterPrize[2]!,
      pool.quarterPrize[3]!,
    ],
    fees: {
      platform: pool.platformFee,
      creator: pool.creatorFee,
      integrator: pool.integratorFee,
      paid: pool.feesPaid,
    },
    sponsoredTotal: pool.sponsoredTotal,
    axes: axesOf(pool),
  };
}
