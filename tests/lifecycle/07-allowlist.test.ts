/**
 * Lifecycle 07 — an Allowlist pool with the FinalOnly preset (build plan Step 8; PROGRAM §4.3
 * gating, §6.4, §4.5 FinalOnly): a 12-wallet list the creator is not on; `initial_boxes = 5`
 * ungated; the twelve fill the rest with proofs; a thirteenth fails; draw; Q1–Q3 move nothing
 * and Q4 pays everything (and the fees); close.
 */
import {
  allowlistProof,
  allowlistRoot,
  PayoutPreset,
  prizePool,
  quarterPrizes,
} from "@mybarpool/shared";
import { getAddressEncoder } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { distinctOwners, Lifecycle } from "../helpers/lifecycle.js";
import {
  AccessType,
  buyInstruction,
  createPoolInstruction,
  errorCode,
  fetchLamports,
  poolParams,
  poolPda,
  PoolStatus,
  send,
  sendExpectingError,
  vaultPda,
  type PoolRefs,
} from "../helpers/mybarpool.js";
import { drawLockedPool, fetchPool, PRICE } from "../helpers/scenario.js";

const L = new Lifecycle({ week: 17, buyers: 12 });
const enc = getAddressEncoder();
let refs: PoolRefs;
let wallets: Uint8Array[];

describe("lifecycle 07: an Allowlist pool, FinalOnly (Surfpool)", { timeout: 120_000 }, () => {
  beforeAll(() => L.setup(), 300_000);

  it("creates the pool from a 12-wallet root with 5 ungated creator boxes", async () => {
    wallets = L.buyers.map((b) => Uint8Array.from(enc.encode(b.address)));
    const root = allowlistRoot(wallets);
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
          preset: PayoutPreset.FinalOnly,
          accessType: AccessType.Allowlist,
          allowlistRoot: root,
          initialBoxes: 5,
        }),
      ),
    );
    refs = {
      pool: await poolPda(L.game, L.creator.address, nonce),
      game: L.game,
      creator: L.creator.address,
    };
    const p = await L.expectVaultInvariant(refs);
    expect(p.accessType).toBe(AccessType.Allowlist);
    expect([...p.allowlistRoot]).toEqual([...root]);
    expect(p.sold).toBe(5);
    expect(p.creatorBoxes).toBe(5);
    expect(p.preset).toBe(PayoutPreset.FinalOnly);
  });

  it("the twelve buy with proofs (eight take 2, four take 1); a thirteenth is AllowlistProofInvalid; Locked", async () => {
    for (const [i, b] of L.buyers.entries()) {
      const proof = allowlistProof(wallets, wallets[i]!);
      const sig = await send(b, await buyInstruction(b, refs, i < 8 ? 2 : 1, {}, { proof }));
      if (i === 0) await L.measure(`buy_allowlist_${proof.length}_entry_proof`, sig);
      if (i === 10) {
        // The stranger with a listed buyer's proof, one box from the end.
        expect(
          await sendExpectingError(
            L.stranger,
            await buyInstruction(L.stranger, refs, 1, {}, { proof }),
          ),
        ).toBe(errorCode("AllowlistProofInvalid"));
      }
    }
    const p = await L.expectVaultInvariant(refs);
    expect(p.status).toBe(PoolStatus.Locked);
    expect(p.sold).toBe(25);
    expect(distinctOwners(p)).toHaveLength(13);
  });

  it("draws", async () => {
    await drawLockedPool(L.scenario(), refs);
    expect((await L.expectVaultInvariant(refs)).status).toBe(PoolStatus.Drawn);
  });

  it("FinalOnly: Q1–Q3 record the box and move nothing; the fees stay in the vault", async () => {
    // PROGRAM §4.5: fees move with the first non-zero prize; §3.4: through Q3 the vault still
    // holds unpaid_prize_pool + the three fees.
    for (const q of [1, 2, 3]) {
      await L.postQuarter(q);
      const f0 = await fetchLamports(L.feeWallet);
      const { event } = await L.settle(refs, q);
      expect(event.amount).toBe(0n);
      expect(event.feesPaidNow).toBe(false);
      expect(await fetchLamports(L.feeWallet)).toBe(f0);
      const p = await L.expectVaultInvariant(refs);
      expect(p.feesPaid).toBe(false);
      expect(p.winningBox[q - 1]).toBeLessThan(25);
    }
    expect(await fetchLamports(await vaultPda(refs.pool))).toBe(L.rent.vault + 25n * PRICE);
  });

  it("Q4 pays the whole prize pool and the fees; Settled", async () => {
    const p0 = await fetchPool(refs.pool);
    const expected = quarterPrizes(
      prizePool({
        pot: 25n * PRICE,
        fees: { platformFee: p0.platformFee, creatorFee: p0.creatorFee, integratorFee: 0n },
        sponsoredTotal: 0n,
      }),
      PayoutPreset.FinalOnly,
    );
    expect(expected.quarters).toEqual([0n, 0n, 0n, 1_100_000_000n]);
    await L.postQuarter(4);
    const winner = await L.expectedWinner(refs.pool, 4);
    const [w0, f0] = await L.balances([winner, L.feeWallet]);
    const { event } = await L.settle(refs, 4);
    expect(event.amount).toBe(1_100_000_000n);
    expect(event.feesPaidNow).toBe(true);
    const [w1, f1] = await L.balances([winner, L.feeWallet]);
    expect(w1! - w0!).toBe(1_100_000_000n + (winner === L.creator.address ? p0.creatorFee : 0n));
    expect(f1! - f0!).toBe(p0.platformFee);
    const p = await L.expectVaultInvariant(refs);
    expect(p.status).toBe(PoolStatus.Settled);
    expect(p.feesPaid).toBe(true);
    expect(p.unpaidPrizePool).toBe(0n);
  });

  it("close_pool", async () => {
    const event = await L.closePool(refs, { measure: "close_pool" });
    expect(event.dust).toBe(0n);
  });

  it("compute units and the clock", () => {
    L.report("07-allowlist");
  });
});
