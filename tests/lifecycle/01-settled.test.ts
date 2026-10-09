/**
 * Lifecycle 01 — settled (build plan Step 8; PROGRAM §4.3–§4.5, §9, §10 Money):
 * a SOL pool, 25 boxes across six buyers (the creator 5), lock → `set_var` → sample → `draw`
 * → Q1–Q4 settlements with the worked example's numbers (ARCHITECTURE › Fees) → `close_pool`
 * by a stranger → `close_counter`. Money in equals money out to the lamport.
 */
import { feeAmounts, PayoutPreset, prizePool, quarterPrizes } from "@mybarpool/shared";
import { beforeAll, describe, expect, it } from "vitest";

import { boxesOf, distinctOwners, Lifecycle, SOL } from "../helpers/lifecycle.js";
import {
  closeCounterInstruction,
  counterPda,
  fetchAccountData,
  fetchLamports,
  PoolStatus,
  send,
  vaultPda,
  type PoolRefs,
} from "../helpers/mybarpool.js";
import { drawLockedPool, fetchPool, fill, PRICE } from "../helpers/scenario.js";

const L = new Lifecycle({ week: 11, buyers: 5 });
let refs: PoolRefs;
const paid: bigint[] = [];
let fees: { platform: bigint; creator: bigint };

describe("lifecycle 01: settled (Surfpool)", { timeout: 120_000 }, () => {
  beforeAll(() => L.setup(), 300_000);

  it("fills: the creator's 5 and four boxes from each of five buyers, Locked at the 25th", async () => {
    // ARCHITECTURE › Buying: 0.05 SOL boxes, pot 1.25 SOL.
    refs = await fill(
      L.scenario(),
      { creatorAddonBps: 200 },
      L.buyers.map((b) => [b, 4] as const),
    );
    const p = await L.expectVaultInvariant(refs);
    expect(p.status).toBe(PoolStatus.Locked);
    expect(p.sold).toBe(25);
    expect(distinctOwners(p)).toHaveLength(6);
    expect(boxesOf(p, L.creator.address)).toHaveLength(5);
    expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault + 25n * PRICE);
    expect(await L.openCount(L.creator.address)).toBe(0);
    fees = { platform: p.platformFee, creator: p.creatorFee };
    // ARCHITECTURE › Fees: 5 % platform 0.0625, 5 % + 2 % creator 0.0875.
    expect(fees.platform).toBe(62_500_000n);
    expect(fees.creator).toBe(87_500_000n);
    const shared = feeAmounts({
      price: PRICE,
      platformBps: 500,
      creatorBps: 500,
      creatorAddonBps: 200,
      integratorBps: 0,
    });
    expect([shared.platformFee, shared.creatorFee]).toEqual([fees.platform, fees.creator]);
  });

  it("draws: Open → set_var → sample_var → Reveal → draw; axes on the pool", async () => {
    await drawLockedPool(L.scenario(), refs);
    const p = await L.expectVaultInvariant(refs);
    expect(p.status).toBe(PoolStatus.Drawn);
    expect(p.drawn).toBe(true);
    expect([...p.homeAxis].sort()).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  });

  it("Q1 moves 0.22 to the winner and 0.0625 + 0.0875 in fees; vault 1.25 → 0.88", async () => {
    // ARCHITECTURE › Fees worked example; PROGRAM §4.5: fees with the first non-zero prize.
    await L.postQuarter(1);
    const winner = await L.expectedWinner(refs.pool, 1);
    const [w0, f0, c0] = await L.balances([winner, L.feeWallet, L.creator.address]);
    const { event } = await L.settle(refs, 1);
    expect(event.amount).toBe(220_000_000n);
    expect(event.feesPaidNow).toBe(true);
    const [w1, f1, c1] = await L.balances([winner, L.feeWallet, L.creator.address]);
    const creatorIsWinner = winner === L.creator.address;
    expect(w1! - w0!).toBe(220_000_000n + (creatorIsWinner ? fees.creator : 0n));
    expect(f1! - f0!).toBe(fees.platform);
    expect(c1! - c0!).toBe(fees.creator + (creatorIsWinner ? 220_000_000n : 0n));
    const p = await L.expectVaultInvariant(refs);
    expect(p.feesPaid).toBe(true);
    expect(p.quartersSettled).toBe(1);
    expect(p.prizePool).toBe(1_100_000_000n);
    expect(p.unpaidPrizePool).toBe(880_000_000n);
    expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault + 880_000_000n);
    paid.push(event.amount);
  });

  it("Q2 and Q3 pay 0.22 each; the fees are not paid again", async () => {
    for (const q of [2, 3]) {
      await L.postQuarter(q);
      const winner = await L.expectedWinner(refs.pool, q);
      const [w0, f0] = await L.balances([winner, L.feeWallet]);
      const { event } = await L.settle(refs, q);
      expect(event.amount).toBe(220_000_000n);
      expect(event.feesPaidNow).toBe(false);
      const [w1, f1] = await L.balances([winner, L.feeWallet]);
      expect(w1! - w0!).toBe(220_000_000n);
      expect(f1! - f0!).toBe(0n);
      const p = await L.expectVaultInvariant(refs);
      expect(p.quartersSettled).toBe(q);
      paid.push(event.amount);
    }
    expect((await fetchPool(refs.pool)).unpaidPrizePool).toBe(440_000_000n);
  });

  it("Q4 pays 0.44 with the final; the pool is Settled with no dust", async () => {
    await L.postQuarter(4);
    const winner = await L.expectedWinner(refs.pool, 4);
    const [w0] = await L.balances([winner]);
    const { event } = await L.settle(refs, 4);
    expect(event.amount).toBe(440_000_000n);
    expect((await L.balances([winner]))[0]! - w0!).toBe(440_000_000n);
    const p = await L.expectVaultInvariant(refs);
    expect(p.status).toBe(PoolStatus.Settled);
    expect(p.quartersSettled).toBe(4);
    expect(p.unpaidPrizePool).toBe(0n);
    paid.push(event.amount);
    const shared = quarterPrizes(
      prizePool({
        pot: 25n * PRICE,
        fees: { platformFee: fees.platform, creatorFee: fees.creator, integratorFee: 0n },
        sponsoredTotal: 0n,
      }),
      PayoutPreset.Standard,
    );
    expect(paid).toEqual([...shared.quarters]);
    expect(shared.dust).toBe(0n);
  });

  it("close_pool by a stranger: vault and pool rent to the fee wallet; money in = money out", async () => {
    const event = await L.closePool(refs, { measure: "close_pool" });
    expect(event.dust).toBe(0n);
    // PROGRAM §10 Money: total out over the pool's life = total in.
    const totalIn = 25n * PRICE;
    const totalOut = paid.reduce((a, b) => a + b, 0n) + fees.platform + fees.creator + event.dust;
    expect(totalOut).toBe(totalIn);
    expect(await fetchAccountData(await vaultPda(refs.pool))).toBeNull();
  });

  it("close_counter at zero by anyone; its rent to the fee wallet", async () => {
    const counter = await counterPda(L.creator.address, L.game);
    const f0 = await fetchLamports(L.feeWallet);
    const sig = await send(
      L.stranger,
      await closeCounterInstruction(L.creator.address, L.game, L.feeWallet),
    );
    await L.measure("close_counter", sig);
    expect(await fetchAccountData(counter)).toBeNull();
    expect((await fetchLamports(L.feeWallet)) - f0).toBe(L.rent.counter);
    expect(L.rent.counter).toBeLessThan(SOL / 100n);
  });

  it("compute units and the clock", () => {
    L.report("01-settled");
  });
});
