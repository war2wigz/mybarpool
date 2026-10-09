/**
 * Lifecycle 03 — unfilled (build plan Step 8; PROGRAM §4.6 `return_boxes`,
 * `return_sponsorship`, §4.5 `close_pool`): a SOL pool with 15 boxes sold and one sponsor,
 * the clock past kickoff, `return_boxes` in two batches of three owners, `return_sponsorship`,
 * `close_pool`. Every return is exactly the price.
 */
import type { Address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { boxesOf, Lifecycle, popcount } from "../helpers/lifecycle.js";
import {
  boxesReturnedDecoder,
  buyInstruction,
  createPoolInstruction,
  decodeEvent,
  emittedEvents,
  errorCode,
  eventName,
  fetchAccountData,
  fetchLamports,
  poolParams,
  poolPda,
  PoolStatus,
  returnBoxesInstruction,
  send,
  sendExpectingError,
  sponsorInstruction,
  sponsorshipPda,
  sponsorshipReturnedDecoder,
  vaultPda,
  type PoolRefs,
} from "../helpers/mybarpool.js";
import { fetchPool, PRICE } from "../helpers/scenario.js";

const L = new Lifecycle({ week: 13, buyers: 5, sponsors: 1 });
let refs: PoolRefs;
let sponsor: Address;
let owners: Address[];

describe("lifecycle 03: unfilled at kickoff (Surfpool)", { timeout: 120_000 }, () => {
  beforeAll(() => L.setup(), 300_000);

  it("sells 15 of 25: the creator 5, five buyers 2 each; one sponsor", async () => {
    const nonce = L.nextNonce();
    await send(
      L.creator,
      await createPoolInstruction(
        L.creator,
        L.game,
        poolParams({ nonce, price: PRICE, creatorAddonBps: 200, initialBoxes: 5 }),
      ),
    );
    refs = {
      pool: await poolPda(L.game, L.creator.address, nonce),
      game: L.game,
      creator: L.creator.address,
    };
    for (const b of L.buyers) await send(b, await buyInstruction(b, refs, 2));
    sponsor = L.sponsors[0]!.address;
    await send(L.sponsors[0]!, await sponsorInstruction(L.sponsors[0]!, refs, PRICE));
    const p = await L.expectVaultInvariant(refs, [sponsor]);
    expect(p.status).toBe(PoolStatus.Open);
    expect(p.sold).toBe(15);
    expect(p.sponsoredTotal).toBe(PRICE);
    expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault + 16n * PRICE);
    expect(await L.openCount(L.creator.address)).toBe(1);
    owners = [L.creator.address, ...L.buyers.map((b) => b.address)];
  });

  it("before kickoff a return is NotReturnable; past kickoff buy and sponsor are SalesClosed", async () => {
    // PROGRAM §10 Timing: no return_boxes (unfilled) before recorded_kickoff.
    expect(
      await sendExpectingError(
        L.keeper,
        await returnBoxesInstruction(L.keeper, refs, owners.slice(0, 3), { counter: true }),
      ),
    ).toBe(errorCode("NotReturnable"));
    await L.travelTo(L.kickoff, "kickoff");
    expect(
      await sendExpectingError(L.buyers[0]!, await buyInstruction(L.buyers[0]!, refs, 1)),
    ).toBe(errorCode("SalesClosed"));
    expect(
      await sendExpectingError(
        L.sponsors[0]!,
        await sponsorInstruction(L.sponsors[0]!, refs, PRICE),
      ),
    ).toBe(errorCode("SalesClosed"));
  });

  it("return_boxes batch 1 (creator + two buyers, with the counter): Returned, counter 0, price per box", async () => {
    const batch = owners.slice(0, 3);
    const before = await L.balances(batch);
    const p0 = await fetchPool(refs.pool);
    const sig = await L.returnBoxes(refs, batch, {
      counter: true,
      measure: "return_boxes_first_3_owners",
    });
    const after = await L.balances(batch);
    const events = await emittedEvents(sig);
    expect(events.map(eventName)).toEqual(["BoxesReturned", "BoxesReturned", "BoxesReturned"]);
    for (const [i, owner] of batch.entries()) {
      const boxes = boxesOf(p0, owner);
      expect(after[i]! - before[i]!, owner).toBe(BigInt(boxes.length) * PRICE);
      const ev = decodeEvent("BoxesReturned", events[i]!, boxesReturnedDecoder);
      expect([ev.owner, ev.amount]).toEqual([owner, BigInt(boxes.length) * PRICE]);
      expect(ev.boxes).toEqual(boxes);
    }
    const p = await L.expectVaultInvariant(refs, [sponsor]);
    expect(p.status).toBe(PoolStatus.Returned);
    expect(popcount(p.returned)).toBe(9); // 5 + 2 + 2
    expect(await L.openCount(L.creator.address)).toBe(0);
  });

  it("return_boxes batch 2 (three buyers): every sold box returned; a third call is NothingToReturn", async () => {
    const batch = owners.slice(3);
    const before = await L.balances(batch);
    await L.returnBoxes(refs, batch, { measure: "return_boxes_second_3_owners" });
    const after = await L.balances(batch);
    for (const [i] of batch.entries()) expect(after[i]! - before[i]!).toBe(2n * PRICE);
    const p = await L.expectVaultInvariant(refs, [sponsor]);
    expect(popcount(p.returned)).toBe(15);
    expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault + PRICE);
    expect(
      await sendExpectingError(L.keeper, await returnBoxesInstruction(L.keeper, refs, batch)),
    ).toBe(errorCode("NothingToReturn"));
  });

  it("return_sponsorship: the sponsor's amount back, the account closed", async () => {
    const s0 = await fetchLamports(sponsor);
    const sig = await L.returnSponsorship(refs, sponsor);
    expect((await fetchLamports(sponsor)) - s0).toBe(PRICE + L.rent.sponsorship);
    const ev = decodeEvent(
      "SponsorshipReturned",
      (await emittedEvents(sig))[0]!,
      sponsorshipReturnedDecoder,
    );
    expect([ev.sponsor, ev.amount]).toEqual([sponsor, PRICE]);
    expect(await fetchAccountData(await sponsorshipPda(refs.pool, sponsor))).toBeNull();
    const p = await L.expectVaultInvariant(refs, [sponsor]);
    expect(p.sponsorshipsOpen).toBe(0);
    expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault);
  });

  it("close_pool: nothing but the rents left; both to the fee wallet", async () => {
    // PROGRAM §4.5: the destination is fee_wallet unless the pool was abandoned (08 covers that).
    const event = await L.closePool(refs, { measure: "close_pool_returned" });
    expect(event.dust).toBe(0n);
    expect(event.destination).toBe(L.feeWallet);
  });

  it("compute units and the clock", () => {
    L.report("03-unfilled");
  });
});
