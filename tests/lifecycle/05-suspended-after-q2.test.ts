/**
 * Lifecycle 05 — suspended after Q2 (build plan Step 8; PROGRAM §4.2 `mark_game`, §4.6
 * `split`, `close_sponsorship`, §4.5 `close_pool`): a SOL pool with a sponsor, 25 boxes across
 * six buyers, drawn, Q1 and Q2 settled (fees paid at Q1), `mark_game(Suspended)`, `split` over
 * the six owners in one call with `split_amount` and the dust asserted against the shared
 * math, `return_sponsorship` → 6047, `close_sponsorship`, `close_pool` sweeps the dust.
 */
import { PayoutPreset, prizePool, quarterPrizes } from "@mybarpool/shared";
import type { Address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { boxesOf, distinctOwners, Lifecycle, popcount } from "../helpers/lifecycle.js";
import {
  boxesSplitDecoder,
  closePoolInstruction,
  decodeEvent,
  emittedEvents,
  errorCode,
  fetchLamports,
  GameStatus,
  PoolStatus,
  send,
  sendExpectingError,
  setComputeUnitLimit,
  settleInstruction,
  splitInstruction,
  sponsorInstruction,
  vaultPda,
  type PoolRefs,
} from "../helpers/mybarpool.js";
import { CU_LIMIT, drawLockedPool, fetchPool, fill, PRICE } from "../helpers/scenario.js";

/** ARCHITECTURE › Sponsorship: the minimum is one box; 13 lamports more makes dust. */
const SPONSORSHIP = PRICE + 13n;
const L = new Lifecycle({ week: 15, buyers: 5, sponsors: 1 });
let refs: PoolRefs;
let sponsor: Address;
let owners: Address[];
let expected: { quarters: readonly bigint[]; splitAmount: bigint; dust: bigint };

describe("lifecycle 05: suspended after Q2 (Surfpool)", { timeout: 120_000 }, () => {
  beforeAll(() => L.setup(), 300_000);

  it("fills (the creator 5, five buyers 4 each), takes the sponsor, draws", async () => {
    refs = await fill(
      L.scenario(),
      { creatorAddonBps: 200 },
      L.buyers.map((b) => [b, 4] as const),
    );
    sponsor = L.sponsors[0]!.address;
    await send(L.sponsors[0]!, await sponsorInstruction(L.sponsors[0]!, refs, SPONSORSHIP));
    await drawLockedPool(L.scenario(), refs);
    const p = await L.expectVaultInvariant(refs, [sponsor]);
    expect(p.status).toBe(PoolStatus.Drawn);
    owners = distinctOwners(p);
    expect(owners).toHaveLength(6);
    const prizes = quarterPrizes(
      prizePool({
        pot: 25n * PRICE,
        fees: { platformFee: p.platformFee, creatorFee: p.creatorFee, integratorFee: 0n },
        sponsoredTotal: SPONSORSHIP,
      }),
      PayoutPreset.Standard,
    );
    // prize pool 1,150,000,013: Q1 and Q2 230,000,002 each; after them 690,000,009 unpaid,
    // split_amount = floor(690,000,009 / 25) = 27,600,000 and 9 lamports of dust (§4.6).
    const unpaidAfterQ2 = prizes.quarters[0]! + prizes.quarters[1]!;
    const remaining = 1_150_000_013n - unpaidAfterQ2;
    expected = {
      quarters: prizes.quarters,
      splitAmount: remaining / 25n,
      dust: remaining - (remaining / 25n) * 25n,
    };
    expect(expected.quarters.slice(0, 2)).toEqual([230_000_002n, 230_000_002n]);
    expect(expected.splitAmount).toBe(27_600_000n);
    expect(expected.dust).toBe(9n);
  });

  it("Q1 pays the fees and the first prize; Q2 the second", async () => {
    for (const q of [1, 2]) {
      await L.postQuarter(q);
      const { event } = await L.settle(refs, q);
      expect(event.amount).toBe(expected.quarters[q - 1]);
      expect(event.feesPaidNow).toBe(q === 1);
    }
    const p = await L.expectVaultInvariant(refs, [sponsor]);
    expect(p.feesPaid).toBe(true);
    expect(p.unpaidPrizePool).toBe(690_000_009n);
  });

  it("the game is marked Suspended: Q3 cannot settle; return_sponsorship is FeesAlreadyPaid", async () => {
    await L.markGame(GameStatus.Suspended);
    // No Q3 score exists (a suspended game gets no further posts, §4.2), so settle stops at
    // ScoresNotPosted 6043 before anything else.
    const winner = await L.expectedWinner(refs.pool, 3);
    expect(
      await sendExpectingError(L.keeper, [
        setComputeUnitLimit(CU_LIMIT),
        await settleInstruction(L.keeper, refs, 3, winner, L.feeWallet),
      ]),
    ).toBe(errorCode("ScoresNotPosted"));
    expect(errorCode("ScoresNotPosted")).toBe(6043);
    expect(await L.returnSponsorshipError(refs, sponsor)).toBe(errorCode("FeesAlreadyPaid"));
    expect(errorCode("FeesAlreadyPaid")).toBe(6047);
  });

  it("split over the six owners in one call: split_amount × boxes to each, 9 lamports left", async () => {
    const p0 = await fetchPool(refs.pool);
    const before = await L.balances(owners);
    const sig = await L.split(refs, owners, { measure: "split_sol_6_owners" });
    const after = await L.balances(owners);
    const events = await emittedEvents(sig);
    expect(events).toHaveLength(6);
    for (const [i, owner] of owners.entries()) {
      const boxes = boxesOf(p0, owner);
      expect(after[i]! - before[i]!, owner).toBe(BigInt(boxes.length) * expected.splitAmount);
      const ev = decodeEvent("BoxesSplit", events[i]!, boxesSplitDecoder);
      expect([ev.owner, ev.boxes, ev.amount]).toEqual([
        owner,
        boxes,
        BigInt(boxes.length) * expected.splitAmount,
      ]);
    }
    const p = await L.expectVaultInvariant(refs, [sponsor]);
    expect(p.status).toBe(PoolStatus.Split);
    expect(p.splitAmount).toBe(expected.splitAmount);
    expect(p.unpaidPrizePool).toBe(expected.dust);
    expect(popcount(p.returned)).toBe(25);
    expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault + expected.dust);
    expect(await sendExpectingError(L.keeper, await splitInstruction(L.keeper, refs, owners))).toBe(
      errorCode("NothingToReturn"),
    );
  });

  it("close_sponsorship by a stranger (the sponsorship is committed); then close_pool sweeps the dust", async () => {
    expect(
      await sendExpectingError(
        L.stranger,
        await closePoolInstruction(L.stranger, refs, L.feeWallet),
      ),
    ).toBe(errorCode("SponsorshipsStillOpen"));
    await L.closeSponsorship(refs, sponsor);
    const event = await L.closePool(refs, { measure: "close_pool_split" });
    expect(event.dust).toBe(expected.dust);
    // PROGRAM §10 Money over the pool's life: in = 25 × price + sponsorship; out = two
    // prizes + fees + 25 × split_amount + dust.
    const p0Fees = 62_500_000n + 87_500_000n;
    const totalIn = 25n * PRICE + SPONSORSHIP;
    const totalOut =
      expected.quarters[0]! +
      expected.quarters[1]! +
      p0Fees +
      25n * expected.splitAmount +
      event.dust;
    expect(totalOut).toBe(totalIn);
  });

  it("compute units and the clock", () => {
    L.report("05-suspended-after-q2");
  });
});
