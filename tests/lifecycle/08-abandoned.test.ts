/**
 * Lifecycle 08 — abandoned, reclaimed at thirty days (build plan Step 8; PROGRAM §4.6
 * `reclaim`, `reclaim_sponsorship`, `return_boxes` in `Returned`; §4.5 `close_pool` to the
 * creator): a SOL pool, 25 boxes, Locked, never drawn (the keeper "vanished"); a sponsor;
 * the clock at `scheduled_kickoff + 30 d + 2 s`; three owners reclaim (the first flips the
 * pool to `Returned`, `abandoned = true`), the sponsor reclaims, the keeper finishes the rest
 * with `return_boxes`, `close_pool` pays the creator. Runs last: nothing follows its jump.
 */
import type { Address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import {
  boxesOf,
  distinctOwners,
  Lifecycle,
  popcount,
  RECLAIM_DELAY,
} from "../helpers/lifecycle.js";
import {
  boxesReclaimedDecoder,
  chainNow,
  decodeEvent,
  emittedEvents,
  errorCode,
  fetchAccountData,
  fetchLamports,
  PoolStatus,
  reclaimInstruction,
  send,
  sendExpectingError,
  sponsorInstruction,
  sponsorshipPda,
  vaultPda,
  type PoolRefs,
} from "../helpers/mybarpool.js";
import { fetchPool, fill, PRICE } from "../helpers/scenario.js";

const L = new Lifecycle({ week: 18, buyers: 5, sponsors: 1 });
let refs: PoolRefs;
let sponsor: Address;
let owners: Address[];

describe(
  "lifecycle 08: abandoned, reclaimed at thirty days (Surfpool, last)",
  { timeout: 120_000 },
  () => {
    beforeAll(() => L.setup(), 300_000);

    it("fills and takes a sponsor; nobody draws", async () => {
      refs = await fill(
        L.scenario(),
        { creatorAddonBps: 200 },
        L.buyers.map((b) => [b, 4] as const),
      );
      sponsor = L.sponsors[0]!.address;
      await send(L.sponsors[0]!, await sponsorInstruction(L.sponsors[0]!, refs, PRICE));
      const p = await L.expectVaultInvariant(refs, [sponsor]);
      expect(p.status).toBe(PoolStatus.Locked);
      expect(p.drawn).toBe(false);
      owners = distinctOwners(p);
      expect(owners).toHaveLength(6);
      // Locked already decremented the counter (PROGRAM §3.5); the reclaim will not touch it.
      expect(await L.openCount(L.creator.address)).toBe(0);
    });

    it("before the window a reclaim is ReclaimTooEarly", async () => {
      // PROGRAM §10 Timing: no reclaim before scheduled_kickoff + 30 d.
      await L.travelTo(L.kickoff + 3_600n, "an hour after kickoff");
      expect(
        await sendExpectingError(L.buyers[0]!, await reclaimInstruction(L.buyers[0]!, refs)),
      ).toBe(errorCode("ReclaimTooEarly"));
      expect(errorCode("ReclaimTooEarly")).toBe(6050);
    });

    it("at scheduled_kickoff + 30 d + 2 s: the first reclaim flips the pool to Returned and abandoned", async () => {
      const target = L.kickoff + RECLAIM_DELAY;
      const now = await L.travelTo(target, "kickoff + 30 d");
      expect(now - target).toBeGreaterThanOrEqual(2n);
      expect(now - target).toBeLessThan(60n);
      const p0 = await fetchPool(refs.pool);
      const first = L.buyers[0]!;
      const b0 = await fetchLamports(first.address);
      const sig = await L.reclaim(first, refs, { measure: "reclaim_first_locked" });
      const boxes = boxesOf(p0, first.address);
      const ev = decodeEvent(
        "BoxesReclaimed",
        (await emittedEvents(sig))[0]!,
        boxesReclaimedDecoder,
      );
      expect([ev.owner, ev.boxes, ev.amount]).toEqual([first.address, boxes, 4n * PRICE]);
      // The reclaimer pays its own transaction fee, so the delta is the return less that fee.
      const delta = (await fetchLamports(first.address)) - b0;
      expect(delta).toBeGreaterThan(4n * PRICE - 100_000n);
      expect(delta).toBeLessThanOrEqual(4n * PRICE);
      const p = await L.expectVaultInvariant(refs, [sponsor]);
      expect(p.status).toBe(PoolStatus.Returned);
      expect(p.abandoned).toBe(true);
      expect(popcount(p.returned)).toBe(4);
      expect(await L.openCount(L.creator.address)).toBe(0);
    });

    it("two more owners reclaim; a second reclaim by the same wallet is NothingToReturn; a non-owner is NotOwner", async () => {
      for (const who of [L.buyers[1]!, L.creator]) {
        const before = await fetchLamports(who.address);
        await L.reclaim(who, refs, {
          measure: `reclaim_${who === L.creator ? "creator" : "buyer"}`,
        });
        const boxes = who === L.creator ? 5n : 4n;
        expect((await fetchLamports(who.address)) - before).toBeGreaterThan(
          boxes * PRICE - 100_000n,
        );
      }
      expect(
        await sendExpectingError(L.buyers[1]!, await reclaimInstruction(L.buyers[1]!, refs)),
      ).toBe(errorCode("NothingToReturn"));
      expect(await sendExpectingError(L.stranger, await reclaimInstruction(L.stranger, refs))).toBe(
        errorCode("NotOwner"),
      );
      const p = await L.expectVaultInvariant(refs, [sponsor]);
      expect(popcount(p.returned)).toBe(13);
    });

    it("the sponsor reclaims its own sponsorship", async () => {
      const s0 = await fetchLamports(sponsor);
      await L.reclaimSponsorship(L.sponsors[0]!, refs);
      const delta = (await fetchLamports(sponsor)) - s0;
      expect(delta).toBeGreaterThan(PRICE + L.rent.sponsorship - 100_000n);
      expect(await fetchAccountData(await sponsorshipPda(refs.pool, sponsor))).toBeNull();
      const p = await L.expectVaultInvariant(refs, [sponsor]);
      expect(p.sponsorshipsOpen).toBe(0);
    });

    it("the keeper finishes the other three owners with return_boxes (allowed in Returned)", async () => {
      const rest = owners.filter(
        (o) => ![L.buyers[0]!.address, L.buyers[1]!.address, L.creator.address].includes(o),
      );
      expect(rest).toHaveLength(3);
      const before = await L.balances(rest);
      await L.returnBoxes(refs, rest, { measure: "return_boxes_after_reclaims" });
      const after = await L.balances(rest);
      for (const [i] of rest.entries()) expect(after[i]! - before[i]!).toBe(4n * PRICE);
      const p = await L.expectVaultInvariant(refs, [sponsor]);
      expect(popcount(p.returned)).toBe(25);
      expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault);
    });

    it("close_pool pays the creator (abandoned); the clock stays where the jump left it", async () => {
      const event = await L.closePool(refs, {
        destination: L.creator.address,
        measure: "close_pool_abandoned",
      });
      expect(event.destination).toBe(L.creator.address);
      expect(event.dust).toBe(0n);
      console.log(`08-abandoned: chain clock after the jump ${await chainNow()}`);
    });

    it("compute units and the clock", () => {
      L.report("08-abandoned");
    });
  },
);
