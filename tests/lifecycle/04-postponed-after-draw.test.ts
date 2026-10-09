/**
 * Lifecycle 04 — postponed after the draw, with a sponsor (build plan Step 8; PROGRAM §4.2
 * `mark_game`, §4.6 `return_boxes` on an SPL pool, `return_sponsorship`, §4.5 `close_pool`):
 * an ORE pool with 25 distinct owners, drawn, one sponsor; `mark_game(Postponed)`;
 * `return_boxes` in SPL pages with every ATA created on the way — the nine-owner page the
 * Step 7 bench measured does not fit a real transaction once the compute-budget instruction
 * is counted, so the pages are eight, eight, eight and one — `return_sponsorship`,
 * `close_pool` closes the vault token account.
 */
import type { Address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { BATCH_CU_LIMIT, boxesOf, Lifecycle, popcount } from "../helpers/lifecycle.js";
import {
  ata,
  boxesReturnedDecoder,
  closeTokenAccountInstruction,
  decodeEvent,
  emittedEvents,
  errorCode,
  fetchAccountData,
  GameStatus,
  ORE_MINT,
  PoolStatus,
  returnBoxesInstruction,
  send,
  sendExpectingError,
  settleInstruction,
  setComputeUnitLimit,
  sponsorInstruction,
  sponsorshipPda,
  TOKEN_PROGRAM,
  tokenAmount,
  vaultPda,
  withRetry,
  type PoolRefs,
  type SplPool,
} from "../helpers/mybarpool.js";
import { CU_LIMIT, drawLockedPool, fetchPool, fill, PRICE_ORE } from "../helpers/scenario.js";

const ORE: SplPool = { mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM };
const L = new Lifecycle({ week: 14, buyers: 24, sponsors: 1, airdrop: 5_000_000_000n });
let refs: PoolRefs;
let sponsor: Address;
let owners: Address[];

describe("lifecycle 04: postponed after the draw, ORE (Surfpool)", { timeout: 120_000 }, () => {
  beforeAll(async () => {
    await L.setup();
    const holdings: [Address, bigint][] = [
      [L.creator.address, PRICE_ORE],
      ...L.buyers.map((b) => [b.address, PRICE_ORE] as [Address, bigint]),
      [L.sponsors[0]!.address, 2n * PRICE_ORE],
    ];
    for (const [who, amount] of holdings) {
      await withRetry(() => L.localnet.setTokenAccount(who, ORE_MINT, { amount: Number(amount) }));
    }
    sponsor = L.sponsors[0]!.address;
  }, 600_000);

  it("fills with 25 distinct owners (the creator 1, 24 buyers 1 each) and takes a sponsor", async () => {
    refs = await fill(
      L.scenario(),
      { creatorAddonBps: 200, initialBoxes: 1 },
      L.buyers.map((b) => [b, 1] as const),
      ORE,
    );
    await send(
      L.sponsors[0]!,
      await sponsorInstruction(L.sponsors[0]!, refs, 2n * PRICE_ORE, {
        mint: ORE_MINT,
        tokenAccount: await ata(sponsor, ORE_MINT),
        tokenProgram: TOKEN_PROGRAM,
      }),
    );
    const p = await L.expectVaultInvariant(refs, [sponsor], ORE);
    expect(p.status).toBe(PoolStatus.Locked);
    expect(new Set(p.owners).size).toBe(25);
    owners = [...p.owners]; // box order: one box each, so this is the owner order too
    expect(await tokenAmount(await vaultPda(refs.pool))).toBe(27n * PRICE_ORE);
  });

  it("draws, then the game is marked Postponed; settle is GameNotScheduled", async () => {
    await drawLockedPool(L.scenario(), refs);
    await L.markGame(GameStatus.Postponed);
    // PROGRAM §4.5: settle needs a Scheduled game (the scores are never posted anyway).
    expect(
      await sendExpectingError(L.keeper, [
        setComputeUnitLimit(CU_LIMIT),
        await settleInstruction(L.keeper, refs, 1, owners[0]!, L.feeWallet, { spl: ORE }),
      ]),
    ).not.toBeNull();
    const p = await L.expectVaultInvariant(refs, [sponsor], ORE);
    expect(p.status).toBe(PoolStatus.Drawn);
  });

  it("every owner and the sponsor close their (empty) ATAs, so the returns must create them", async () => {
    const signers = [L.creator, ...L.buyers, L.sponsors[0]!];
    for (const who of signers) {
      const account = await ata(who.address, ORE_MINT);
      expect(await tokenAmount(account)).toBe(0n); // each spent exactly what it was given
      await send(who, closeTokenAccountInstruction(who, account, who.address));
      expect(await fetchAccountData(account)).toBeNull();
    }
  });

  it("a nine-owner page with the ATAs missing does not fit a transaction: 9 × 7 inner + the instruction + the compute budget = 65 > 64", async () => {
    // Mollusk measures the nine-owner page at ~280k CU as a single instruction (64 in the
    // trace); a real transaction needs SetComputeUnitLimit as well (the default 200k is not
    // enough), and that one instruction takes the trace to 65. Eight owners is the page.
    const error = await sendExpectingError(L.keeper, [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await returnBoxesInstruction(L.keeper, refs, owners.slice(0, 9), { spl: ORE }),
    ]).catch((e: unknown) => e);
    // The runtime's message is on the cause chain, under the preflight error and its logs.
    const messages: string[] = [];
    for (let e = error as { message?: string; cause?: unknown } | undefined; e;) {
      if (typeof e.message === "string") messages.push(e.message);
      e = e.cause as typeof e;
    }
    expect(messages.join("\n")).toMatch(/instruction trace length/i);
    const p = await L.expectVaultInvariant(refs, [sponsor], ORE);
    expect(p.status).toBe(PoolStatus.Drawn); // nothing moved
  });

  it("return_boxes page 1: eight owners, eight ATAs created, price each", async () => {
    const page = owners.slice(0, 8);
    const p0 = await fetchPool(refs.pool);
    const sig = await L.returnBoxes(refs, page, {
      spl: ORE,
      measure: "return_boxes_ore_8_owners_atas_missing",
    });
    const events = await emittedEvents(sig);
    for (const [i, owner] of page.entries()) {
      expect(await tokenAmount(await ata(owner, ORE_MINT))).toBe(PRICE_ORE);
      const ev = decodeEvent("BoxesReturned", events[i]!, boxesReturnedDecoder);
      expect([ev.owner, ev.amount, ev.boxes]).toEqual([owner, PRICE_ORE, boxesOf(p0, owner)]);
    }
    const p = await L.expectVaultInvariant(refs, [sponsor], ORE);
    expect(p.status).toBe(PoolStatus.Returned);
    expect(popcount(p.returned)).toBe(8);
  });

  it("return_boxes pages 2–4: eight, eight, then the last owner; all 25 returned", async () => {
    await L.returnBoxes(refs, owners.slice(8, 16), {
      spl: ORE,
      measure: "return_boxes_ore_8_owners_page_2",
    });
    expect(popcount((await fetchPool(refs.pool)).returned)).toBe(16);
    await L.returnBoxes(refs, owners.slice(16, 24), {
      spl: ORE,
      measure: "return_boxes_ore_8_owners_page_3",
    });
    expect(popcount((await fetchPool(refs.pool)).returned)).toBe(24);
    await L.returnBoxes(refs, owners.slice(24), {
      spl: ORE,
      measure: "return_boxes_ore_1_owner_page_4",
    });
    const p = await L.expectVaultInvariant(refs, [sponsor], ORE);
    expect(popcount(p.returned)).toBe(25);
    for (const owner of owners)
      expect(await tokenAmount(await ata(owner, ORE_MINT))).toBe(PRICE_ORE);
    expect(await tokenAmount(await vaultPda(refs.pool))).toBe(2n * PRICE_ORE);
  });

  it("return_sponsorship creates the sponsor's ATA and pays 2 × price; the account closes", async () => {
    await L.returnSponsorship(refs, sponsor, { spl: ORE });
    expect(await tokenAmount(await ata(sponsor, ORE_MINT))).toBe(2n * PRICE_ORE);
    expect(await fetchAccountData(await sponsorshipPda(refs.pool, sponsor))).toBeNull();
    const p = await L.expectVaultInvariant(refs, [sponsor], ORE);
    expect(p.sponsorshipsOpen).toBe(0);
    expect(await tokenAmount(await vaultPda(refs.pool))).toBe(0n);
  });

  it("close_pool closes the vault token account; the fee wallet's ATA is created for a zero sweep", async () => {
    const vault = await vaultPda(refs.pool);
    const event = await L.closePool(refs, { spl: ORE, measure: "close_pool_ore_returned" });
    expect(event.dust).toBe(0n);
    expect(await fetchAccountData(vault)).toBeNull();
    expect(errorCode("NotReturnable")).toBe(6046);
  });

  it("compute units and the clock", () => {
    L.report("04-postponed-after-draw");
  });
});
