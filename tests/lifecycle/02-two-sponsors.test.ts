/**
 * Lifecycle 02 — settled with two sponsors (build plan Step 8; PROGRAM §4.3 `sponsor`, §4.5,
 * §5.4): an ORE pool, two sponsors (one adds twice), fill, draw, four settlements whose
 * `unpaid_prize_pool` includes both sponsorships, both `close_sponsorship` by anyone,
 * `close_pool`, the vault token account closed.
 */
import { PayoutPreset, prizePool, quarterPrizes } from "@mybarpool/shared";
import type { Address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { Lifecycle } from "../helpers/lifecycle.js";
import {
  ata,
  decodeAccount,
  decodeEvent,
  emittedEvents,
  fetchAccountData,
  ORE_MINT,
  PoolStatus,
  send,
  sponsoredDecoder,
  sponsorInstruction,
  sponsorshipDecoder,
  sponsorshipPda,
  TOKEN_PROGRAM,
  tokenAmount,
  vaultPda,
  withRetry,
  type PoolRefs,
  type SplPool,
} from "../helpers/mybarpool.js";
import { drawLockedPool, fetchPool, fill, PRICE_ORE } from "../helpers/scenario.js";

const ORE: SplPool = { mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM };
const L = new Lifecycle({ week: 12, buyers: 5, sponsors: 2 });
let refs: PoolRefs;
let sponsors: Address[];
const paid: bigint[] = [];

describe("lifecycle 02: settled with two sponsors, ORE (Surfpool)", { timeout: 120_000 }, () => {
  beforeAll(async () => {
    await L.setup();
    // ORE for everyone through surfnet_setTokenAccount: exactly what each will spend.
    const holdings: [Address, bigint][] = [
      [L.creator.address, 5n * PRICE_ORE],
      ...L.buyers.map((b) => [b.address, 4n * PRICE_ORE] as [Address, bigint]),
      [L.sponsors[0]!.address, 3n * PRICE_ORE],
      [L.sponsors[1]!.address, PRICE_ORE],
    ];
    for (const [who, amount] of holdings) {
      await withRetry(() => L.localnet.setTokenAccount(who, ORE_MINT, { amount: Number(amount) }));
    }
    sponsors = L.sponsors.map((s) => s.address);
  }, 300_000);

  it("fills: an ORE pool, the creator's 5 and four from each of five buyers", async () => {
    refs = await fill(
      L.scenario(),
      { creatorAddonBps: 200 },
      L.buyers.map((b) => [b, 4] as const),
      ORE,
    );
    const p = await L.expectVaultInvariant(refs, [], ORE);
    expect(p.status).toBe(PoolStatus.Locked);
    expect(await tokenAmount(await vaultPda(refs.pool))).toBe(25n * PRICE_ORE);
    for (const who of [L.creator, ...L.buyers]) {
      expect(await tokenAmount(await ata(who.address, ORE_MINT))).toBe(0n);
    }
  });

  it("sponsors: A adds 1 then 2 ORE boxes' worth, B adds 1; sponsored_total 4 × price, two accounts", async () => {
    // PROGRAM §4.3 sponsor: amount ≥ price; Locked pools may be sponsored before kickoff.
    const path = async (who: Address) => ({
      mint: ORE_MINT,
      tokenAccount: await ata(who, ORE_MINT),
      tokenProgram: TOKEN_PROGRAM,
    });
    const [A, B] = L.sponsors as [typeof L.creator, typeof L.creator];
    await send(A, await sponsorInstruction(A, refs, PRICE_ORE, await path(A.address)));
    const sig = await send(
      A,
      await sponsorInstruction(A, refs, 2n * PRICE_ORE, await path(A.address)),
    );
    await L.measure("sponsor_top_up", sig);
    const ev = decodeEvent("Sponsored", (await emittedEvents(sig))[0]!, sponsoredDecoder);
    expect([ev.sponsor, ev.amount, ev.sponsoredTotal]).toEqual([
      A.address,
      2n * PRICE_ORE,
      3n * PRICE_ORE,
    ]);
    await send(B, await sponsorInstruction(B, refs, PRICE_ORE, await path(B.address)));
    const p = await L.expectVaultInvariant(refs, sponsors, ORE);
    expect(p.sponsoredTotal).toBe(4n * PRICE_ORE);
    expect(p.sponsorCount).toBe(2);
    expect(p.sponsorshipsOpen).toBe(2);
    const sA = decodeAccount(
      "Sponsorship",
      (await fetchAccountData(await sponsorshipPda(refs.pool, A.address)))!,
      sponsorshipDecoder,
    );
    expect([sA.wallet, sA.amount]).toEqual([A.address, 3n * PRICE_ORE]);
    expect(await tokenAmount(await vaultPda(refs.pool))).toBe(29n * PRICE_ORE);
  });

  it("draws", async () => {
    await drawLockedPool(L.scenario(), refs);
    expect((await L.expectVaultInvariant(refs, sponsors, ORE)).status).toBe(PoolStatus.Drawn);
  });

  it("Q1–Q4: prize pool = pot − fees + both sponsorships; the shares per the shared math", async () => {
    const p0 = await fetchPool(refs.pool);
    const expected = quarterPrizes(
      prizePool({
        pot: 25n * PRICE_ORE,
        fees: {
          platformFee: p0.platformFee,
          creatorFee: p0.creatorFee,
          integratorFee: p0.integratorFee,
        },
        sponsoredTotal: 4n * PRICE_ORE,
      }),
      PayoutPreset.Standard,
    );
    // 0.05 ORE boxes: pot 1.25, fees 0.0625 + 0.0875, prize pool 1.10 + 0.20 sponsored = 1.30.
    expect(expected.quarters).toEqual([
      26_000_000_000n,
      26_000_000_000n,
      26_000_000_000n,
      52_000_000_000n,
    ]);
    const feeAta = await ata(L.feeWallet, ORE_MINT);
    const feeBefore = (await tokenAmount(feeAta)) ?? 0n; // shared across the Surfpool session
    for (let q = 1; q <= 4; q++) {
      await L.postQuarter(q);
      const winner = await L.expectedWinner(refs.pool, q);
      const [w0] = await L.balances([winner], ORE);
      const { event } = await L.settle(refs, q, { spl: ORE });
      expect(event.amount).toBe(expected.quarters[q - 1]);
      expect(event.feesPaidNow).toBe(q === 1);
      const creatorIsWinner = winner === L.creator.address;
      expect((await L.balances([winner], ORE))[0]! - w0!).toBe(
        event.amount + (q === 1 && creatorIsWinner ? p0.creatorFee : 0n),
      );
      const p = await L.expectVaultInvariant(refs, sponsors, ORE);
      expect(p.quartersSettled).toBe(q);
      paid.push(event.amount);
    }
    const p = await fetchPool(refs.pool);
    expect(p.status).toBe(PoolStatus.Settled);
    expect(p.prizePool).toBe(130_000_000_000n);
    expect(p.unpaidPrizePool).toBe(expected.dust);
    expect(((await tokenAmount(feeAta)) ?? 0n) - feeBefore).toBe(p0.platformFee);
  });

  it("close_sponsorship × 2 by a stranger: the rent to each sponsor; sponsorships_open 0", async () => {
    for (const s of sponsors) await L.closeSponsorship(refs, s);
    const p = await L.expectVaultInvariant(refs, sponsors, ORE);
    expect(p.sponsorshipsOpen).toBe(0);
    expect(p.sponsorCount).toBe(2); // history, never decremented (§3.4)
  });

  it("close_pool: the vault token account is closed; money in = money out", async () => {
    const vault = await vaultPda(refs.pool);
    const feeAta = await ata(L.feeWallet, ORE_MINT);
    const before = (await tokenAmount(feeAta)) ?? 0n;
    const event = await L.closePool(refs, { spl: ORE, measure: "close_pool_ore" });
    expect(await fetchAccountData(vault)).toBeNull();
    expect(((await tokenAmount(feeAta)) ?? 0n) - before).toBe(event.dust);
    const p0Fees = 6_250_000_000n + 8_750_000_000n;
    const totalIn = 25n * PRICE_ORE + 4n * PRICE_ORE;
    const totalOut = paid.reduce((a, b) => a + b, 0n) + p0Fees + event.dust;
    expect(totalOut).toBe(totalIn);
  });

  it("compute units and the clock", () => {
    L.report("02-two-sponsors");
  });
});
