/**
 * Lifecycle 06 — a Link pool, end to end (build plan Step 8; PROGRAM §4.3 gating,
 * `rotate_gate_key`; ARCHITECTURE › Private pools "otherwise identical"): 25 boxes bought by
 * eight buyers with the gate key co-signing, one rotation mid-way (the old key fails once),
 * then draw, four settlements, close.
 */
import { PayoutPreset, prizePool, quarterPrizes } from "@mybarpool/shared";
import { generateKeyPairSigner, type KeyPairSigner } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { distinctOwners, Lifecycle } from "../helpers/lifecycle.js";
import {
  AccessType,
  buyInstruction,
  createPoolInstruction,
  errorCode,
  gateKeyRotatedDecoder,
  decodeEvent,
  emittedEvents,
  poolParams,
  poolPda,
  PoolStatus,
  rotateGateKeyInstruction,
  send,
  sendExpectingError,
  type PoolRefs,
} from "../helpers/mybarpool.js";
import { drawLockedPool, fetchPool, PRICE } from "../helpers/scenario.js";

const L = new Lifecycle({ week: 16, buyers: 8 });
let refs: PoolRefs;
let gate: KeyPairSigner;
let gate2: KeyPairSigner;
const paid: bigint[] = [];

describe("lifecycle 06: a Link pool (Surfpool)", { timeout: 120_000 }, () => {
  beforeAll(async () => {
    await L.setup();
    gate = await generateKeyPairSigner();
    gate2 = await generateKeyPairSigner();
  }, 300_000);

  it("creates the Link pool with the creator's 1 box; four buyers take 3 each with the key", async () => {
    const nonce = L.nextNonce();
    await send(
      L.creator,
      await createPoolInstruction(
        L.creator,
        L.game,
        poolParams({
          nonce,
          price: PRICE,
          creatorAddonBps: 200,
          accessType: AccessType.Link,
          gateKey: gate.address,
          initialBoxes: 1,
        }),
      ),
    );
    refs = {
      pool: await poolPda(L.game, L.creator.address, nonce),
      game: L.game,
      creator: L.creator.address,
    };
    expect((await fetchPool(refs.pool)).gateKey).toBe(gate.address);
    for (const b of L.buyers.slice(0, 4)) {
      const sig = await send(b, await buyInstruction(b, refs, 3, {}, { gateKey: gate }));
      await L.measure("buy_link_3", sig);
    }
    const p = await L.expectVaultInvariant(refs);
    expect(p.sold).toBe(13);
    // Without the key: refused, for a buyer and for the creator alike (§4.3).
    expect(
      await sendExpectingError(L.buyers[4]!, await buyInstruction(L.buyers[4]!, refs, 3)),
    ).toBe(errorCode("GateKeyNotSigner"));
    expect(await sendExpectingError(L.creator, await buyInstruction(L.creator, refs, 1))).toBe(
      errorCode("GateKeyNotSigner"),
    );
  });

  it("rotate_gate_key mid-way: the old key fails once, the new key sells the rest; Locked", async () => {
    const sig = await send(
      L.creator,
      await rotateGateKeyInstruction(L.creator, refs.pool, gate2.address),
    );
    await L.measure("rotate_gate_key", sig);
    // PROGRAM §7 GateKeyRotated carries pool and time only (§4.3: only signatures matter).
    const ev = decodeEvent("GateKeyRotated", (await emittedEvents(sig))[0]!, gateKeyRotatedDecoder);
    expect(ev.pool).toBe(refs.pool);
    expect((await fetchPool(refs.pool)).gateKey).toBe(gate2.address);
    expect(
      await sendExpectingError(
        L.buyers[4]!,
        await buyInstruction(L.buyers[4]!, refs, 3, {}, { gateKey: gate }),
      ),
    ).toBe(errorCode("GateKeyNotSigner"));
    for (const b of L.buyers.slice(4)) {
      await send(b, await buyInstruction(b, refs, 3, {}, { gateKey: gate2 }));
    }
    const p = await L.expectVaultInvariant(refs);
    expect(p.status).toBe(PoolStatus.Locked);
    expect(p.sold).toBe(25);
    expect(distinctOwners(p)).toHaveLength(9);
    expect(await L.openCount(L.creator.address)).toBe(0);
  });

  it("draws like any pool", async () => {
    await drawLockedPool(L.scenario(), refs);
    expect((await L.expectVaultInvariant(refs)).status).toBe(PoolStatus.Drawn);
  });

  it("Q1–Q4 settle exactly as a public pool would (the worked example)", async () => {
    const p0 = await fetchPool(refs.pool);
    const expected = quarterPrizes(
      prizePool({
        pot: 25n * PRICE,
        fees: { platformFee: p0.platformFee, creatorFee: p0.creatorFee, integratorFee: 0n },
        sponsoredTotal: 0n,
      }),
      PayoutPreset.Standard,
    );
    expect(expected.quarters).toEqual([220_000_000n, 220_000_000n, 220_000_000n, 440_000_000n]);
    for (let q = 1; q <= 4; q++) {
      await L.postQuarter(q);
      const { event } = await L.settle(refs, q);
      expect(event.amount).toBe(expected.quarters[q - 1]);
      paid.push(event.amount);
      await L.expectVaultInvariant(refs);
    }
    expect((await fetchPool(refs.pool)).status).toBe(PoolStatus.Settled);
  });

  it("close_pool; money in = money out", async () => {
    const event = await L.closePool(refs, { measure: "close_pool" });
    expect(paid.reduce((a, b) => a + b, 0n) + 62_500_000n + 87_500_000n + event.dust).toBe(
      25n * PRICE,
    );
  });

  it("compute units and the clock", () => {
    L.report("06-link");
  });
});
