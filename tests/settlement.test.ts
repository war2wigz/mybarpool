/**
 * Step 6 localnet suite: settlement against Surfpool forking mainnet
 * (`anchor test`). Runs after `pools.test.ts` by path; `config.test.ts`
 * initialised the config and preloaded the platform's Entropy deployment.
 * One `describe`, ordered.
 *
 * One game `G` carries every pool, so one set of four score posts serves them
 * all. Pools are drawn with nothing planted (`draw.test.ts` item 2's recipe):
 * `Open` → `set_var` → wait → `sample_var` → `Reveal` → `draw`. The winner is
 * never hard-coded: `expectedWinner` reads the pool's axes and owners and
 * computes the box with the shared `winningBox`.
 */
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import {
  drawAxes,
  entropyValue,
  INITIAL_LADDERS,
  keccak,
  winningBox,
  type Digits,
} from "@mybarpool/shared";
import {
  airdropFactory,
  createKeyPairSignerFromBytes,
  generateKeyPairSigner,
  lamports,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { Localnet } from "../scripts/localnet.js";
import {
  currentSlot,
  entropyVarPda,
  fetchVar,
  openInstruction,
  revealInstruction,
  waitForSlot,
} from "./helpers/entropy.js";
import {
  ata,
  buyInstruction,
  chainNow,
  closePoolInstruction,
  closeTokenAccountInstruction,
  COMPUTE_BUDGET_PROGRAM,
  computeUnitsConsumed,
  configPda,
  createGameInstruction,
  createPoolInstruction,
  decodeAccount,
  decodeEvent,
  drawInstruction,
  emittedEvents,
  errorCode,
  eventName,
  fetchAccountData,
  fetchLamports,
  gamePda,
  IDL,
  ORE_MINT,
  platformConfigDecoder,
  poolClosedDecoder,
  poolDecoder,
  poolParams,
  poolPda,
  PoolStatus,
  postScoresInstruction,
  quarterSettledDecoder,
  rpc,
  rpcSubscriptions,
  sampleVarInstruction,
  send,
  sendExpectingError,
  setComputeUnitLimit,
  settleInstruction,
  setVarInstruction,
  SLOT_HASHES_SYSVAR,
  sponsorInstruction,
  TOKEN_PROGRAM,
  tokenAmount,
  updateConfigInstruction,
  vaultPda,
  withRetry,
  type CreatePoolParams,
  type GameKey,
  type Pool,
  type PoolRefs,
  type SplPool,
} from "./helpers/mybarpool.js";

const SOL = 1_000_000_000n;
const HOUR = 3_600n;
const MINUTE = 60n;
/** ARCHITECTURE › Buying: the SOL minimum, 0.05 SOL; the ORE minimum, 0.05 ORE at 11 decimals. */
const PRICE = INITIAL_LADDERS.SOL.minPrice;
const PRICE_ORE = INITIAL_LADDERS.ORE.minPrice;
/** PROGRAM §2 team table: KC hosting DAL in week 4 of 2026. */
const KEY: GameKey = { season: 2026, week: 4, home: 15, away: 8 };
/** The Step 3 scores: Q1 7–3, Q2 14–10, Q3 17–17, Q4 24–20 final with overtime. */
const SCORES: [number, number][] = [
  [7, 3],
  [14, 10],
  [17, 17],
  [24, 20],
];
const CU_LIMIT = 400_000;
/** Slots ahead for `end_at` (6 s at 200 ms): Open and set_var both have to land inside it. */
const WINDOW_AHEAD = 30n;
const ORE: SplPool = { mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM };

const localnet = new Localnet();
const seedOf = (byte: number) => new Uint8Array(32).fill(byte);

async function fetchPool(pool: Address): Promise<Pool> {
  const data = await fetchAccountData(pool);
  expect(data).not.toBeNull();
  expect(data!.length).toBe(1442); // PROGRAM §3.3
  return decodeAccount("Pool", data!, poolDecoder);
}

describe("settlement (Surfpool, the platform's Entropy deployment)", () => {
  let admin: KeyPairSigner;
  let keeper: KeyPairSigner;
  let creator: KeyPairSigner;
  let buyerA: KeyPairSigner;
  let buyerB: KeyPairSigner;
  let stranger: KeyPairSigner;
  let integrator: KeyPairSigner;
  let feeWallet: Address;
  let game: Address;
  let kickoff: bigint;
  let rent: { vault: bigint; pool: bigint; tokenAccount: bigint };
  let nextNonce = 1n;
  let nextVarId = 500n;
  const measured: Record<string, bigint> = {};
  const travelled: string[] = [];

  let P1: PoolRefs; // SOL Standard, 2 % add-on: the worked example
  let P2: PoolRefs; // the same + 1 SOL sponsored
  let P3: PoolRefs; // SOL FinalOnly
  let P4: PoolRefs; // SOL Standard with an integrator
  let P5: PoolRefs; // ORE Standard

  /** PROGRAM §6.3 from the pool's stored axes and owners; the test never hard-codes a winner. */
  async function expectedWinner(pool: Address, quarter: number): Promise<Address> {
    const p = await fetchPool(pool);
    const [home, away] = SCORES[quarter - 1]!;
    const box = winningBox({
      home,
      away,
      homeAxis: p.homeAxis as unknown as Digits,
      awayAxis: p.awayAxis as unknown as Digits,
    });
    return p.owners[box]!;
  }

  async function settle(
    refs: PoolRefs,
    quarter: number,
    options: { integrator?: Address; spl?: SplPool; winner?: Address; signer?: KeyPairSigner } = {},
  ) {
    const winner = options.winner ?? (await expectedWinner(refs.pool, quarter));
    const signer = options.signer ?? keeper;
    return send(signer, [
      setComputeUnitLimit(CU_LIMIT),
      await settleInstruction(signer, refs, quarter, winner, feeWallet, options),
    ]);
  }

  async function settleExpectingError(
    refs: PoolRefs,
    quarter: number,
    options: { integrator?: Address; spl?: SplPool; winner?: Address; signer?: KeyPairSigner } = {},
  ) {
    const winner = options.winner ?? (await expectedWinner(refs.pool, quarter));
    const signer = options.signer ?? keeper;
    return sendExpectingError(signer, [
      setComputeUnitLimit(CU_LIMIT),
      await settleInstruction(signer, refs, quarter, winner, feeWallet, options),
    ]);
  }

  /** draw.test.ts item 2's recipe: a pool with 25 boxes sold, drawn with nothing planted. */
  async function drawnPool(params: Partial<CreatePoolParams>, spl?: SplPool): Promise<PoolRefs> {
    const nonce = nextNonce++;
    const tokenPath = spl
      ? {
          mint: spl.mint,
          tokenProgram: spl.tokenProgram,
        }
      : undefined;
    await send(
      creator,
      await createPoolInstruction(
        creator,
        game,
        poolParams({
          nonce,
          token: spl ? 2 : 0, // ORE is token index 2 (ARCHITECTURE › Buying table)
          price: spl ? PRICE_ORE : PRICE,
          initialBoxes: 5,
          ...params,
        }),
        tokenPath
          ? {
              ...tokenPath,
              tokenAccount: await ata(creator.address, spl!.mint, spl!.tokenProgram),
            }
          : {},
      ),
    );
    const refs: PoolRefs = {
      pool: await poolPda(game, creator.address, nonce),
      game,
      creator: creator.address,
    };
    for (const buyer of [buyerA, buyerB]) {
      await send(
        buyer,
        await buyInstruction(
          buyer,
          refs,
          10,
          tokenPath
            ? { ...tokenPath, tokenAccount: await ata(buyer.address, spl!.mint, spl!.tokenProgram) }
            : {},
        ),
      );
    }
    expect((await fetchPool(refs.pool)).status).toBe(PoolStatus.Locked);

    const id = nextVarId++;
    const seed = seedOf(Number(id % 200n) + 1);
    const varAddress = await entropyVarPda(keeper.address, id);
    // First touch of the new PDA: Surfpool asks mainnet whether it exists (Step 3 audit M1), and
    // a slow answer must not eat the window between Open and set_var.
    await withRetry(() => rpc.getAccountInfo(varAddress, { encoding: "base64" }).send());
    const endAt = (await currentSlot()) + WINDOW_AHEAD;
    await send(
      keeper,
      await openInstruction(keeper, keeper, id, keeper.address, keccak(seed), false, 1n, endAt),
    );
    await send(keeper, await setVarInstruction(keeper, refs.pool, varAddress));
    await waitForSlot(endAt + 1n);
    await send(stranger, [
      setComputeUnitLimit(CU_LIMIT),
      await sampleVarInstruction(stranger, refs.pool, varAddress),
    ]);
    await send(keeper, revealInstruction(keeper, varAddress, seed));
    const revealed = (await fetchVar(varAddress))!;
    expect(revealed.value).toEqual(entropyValue(revealed.slotHash, seed, 1n));
    await send(keeper, await drawInstruction(keeper, refs.pool, varAddress));
    const pool = await fetchPool(refs.pool);
    expect(pool.status).toBe(PoolStatus.Drawn);
    const axes = drawAxes(revealed.value);
    expect(pool.homeAxis).toEqual(Array.from(axes.home));
    return refs;
  }

  /** `games.test.ts`'s pattern: forward only, to `target + 2` seconds. */
  async function travelTo(target: bigint): Promise<void> {
    const now = await chainNow();
    if (now < target) {
      await localnet.timeTravel({ absoluteTimestamp: Number((target + 2n) * 1000n) });
    }
    const after = await chainNow();
    expect(after).toBeGreaterThanOrEqual(target);
    travelled.push(`${target} (kickoff + ${(target - kickoff) / MINUTE} min) → ${after}`);
  }

  async function postQuarter(quarter: number): Promise<void> {
    const [home, away] = SCORES[quarter - 1]!;
    // post_scores Q1 needs now ≥ kickoff + 900 and each later post ≥ 900 after the previous.
    await travelTo(kickoff + BigInt(quarter) * 16n * MINUTE);
    await send(
      keeper,
      await postScoresInstruction(keeper, game, {
        quarter,
        home,
        away,
        isFinal: quarter === 4,
        hadOvertime: quarter === 4,
      }),
    );
  }

  beforeAll(async () => {
    const walletPath = process.env["ANCHOR_WALLET"] ?? join(homedir(), ".config/solana/id.json");
    admin = await createKeyPairSignerFromBytes(
      Uint8Array.from(JSON.parse(readFileSync(walletPath, "utf8"))),
    );
    if ((await fetchAccountData(await configPda())) === null) {
      throw new Error(
        "config PDA missing: config.test.ts must run first (see tests/vitest.config.ts)",
      );
    }
    keeper = await generateKeyPairSigner();
    creator = await generateKeyPairSigner();
    buyerA = await generateKeyPairSigner();
    buyerB = await generateKeyPairSigner();
    stranger = await generateKeyPairSigner();
    integrator = await generateKeyPairSigner();
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    for (const who of [keeper, creator, buyerA, buyerB, stranger, integrator]) {
      await withRetry(() =>
        airdrop({
          recipientAddress: who.address,
          lamports: lamports(100n * SOL),
          commitment: "confirmed",
        }),
      );
    }
    // config.test.ts leaves platform_bps at 400 in this Surfpool session; the worked example is
    // computed at 500 / 500 (ARCHITECTURE › Fees), so set both. Fees are fixed at creation,
    // so a later change could not alter a payout anyway (the Mollusk suite proves that).
    await send(
      admin,
      await updateConfigInstruction(admin, {
        scoreAuthority: keeper.address,
        entropyProvider: keeper.address,
        platformBps: 500,
        creatorBps: 500,
      }),
    );
    const config = decodeAccount(
      "PlatformConfig",
      (await fetchAccountData(await configPda()))!,
      platformConfigDecoder,
    );
    feeWallet = config.feeWallet;
    expect(config.platformBps).toBe(500);
    expect(config.creatorBps).toBe(500);
    for (const addr of [COMPUTE_BUDGET_PROGRAM, SLOT_HASHES_SYSVAR, feeWallet]) {
      await withRetry(() => rpc.getAccountInfo(addr, { encoding: "base64" }).send());
    }
    const r = async (size: bigint) => rpc.getMinimumBalanceForRentExemption(size).send();
    rent = { vault: await r(0n), pool: await r(1442n), tokenAccount: await r(165n) };

    kickoff = (await chainNow()) + 3n * HOUR;
    game = await gamePda(KEY, kickoff);
    await send(keeper, await createGameInstruction(keeper, KEY, kickoff));

    // ORE for the creator (5 boxes) and the two buyers (10 each) so every ATA holds 0 after
    // buying (first use of setTokenAccount for an exact amount: shape recorded in NOTES).
    for (const [who, boxes] of [
      [creator, 5n],
      [buyerA, 10n],
      [buyerB, 10n],
    ] as const) {
      await withRetry(() =>
        localnet.setTokenAccount(who.address, ORE_MINT, { amount: Number(boxes * PRICE_ORE) }),
      );
    }

    P1 = await drawnPool({ creatorAddonBps: 200 });
    P2 = await drawnPool({ creatorAddonBps: 200 });
    await send(stranger, await sponsorInstruction(stranger, P2, SOL)); // Step 4's Drawn window
    P3 = await drawnPool({ creatorAddonBps: 200, preset: 2 /* FinalOnly */ });
    P4 = await drawnPool({
      creatorAddonBps: 200,
      integrator: integrator.address,
      integratorBps: 100,
    });
    P5 = await drawnPool({ creatorAddonBps: 200 }, ORE);
    for (const who of [creator, buyerA, buyerB]) {
      expect(await tokenAmount(await ata(who.address, ORE_MINT))).toBe(0n);
    }
  }, 300_000);

  it("1. the worked example on P1: Q1 moves 0.22 + 0.0625 + 0.0875, vault 1.25 → 0.88; then the negatives", async () => {
    await postQuarter(1);
    const winner = await expectedWinner(P1.pool, 1);
    const vault = await vaultPda(P1.pool);
    expect(await fetchLamports(vault)).toBe(rent.vault + 1_250_000_000n);
    const before = {
      winner: await fetchLamports(winner),
      fee: await fetchLamports(feeWallet),
      creator: await fetchLamports(creator.address),
    };
    const sig = await settle(P1, 1);
    measured["settle_q1_sol"] = await computeUnitsConsumed(sig);
    const events = await emittedEvents(sig);
    expect(events.map(eventName)).toEqual(["QuarterSettled"]);
    const ev = decodeEvent("QuarterSettled", events[0]!, quarterSettledDecoder);
    expect(ev.pool).toBe(P1.pool);
    expect([ev.quarter, ev.home, ev.away]).toEqual([1, 7, 3]);
    expect(ev.winner).toBe(winner);
    expect(ev.amount).toBe(220_000_000n); // ARCHITECTURE › Fees: 0.22
    expect(ev.feesPaidNow).toBe(true);
    expect([ev.platformFee, ev.creatorFee, ev.integratorFee]).toEqual([
      62_500_000n, // 0.0625
      87_500_000n, // 0.0875
      0n,
    ]);
    // Balances: the creator may also be the winner, so assert on the sums the test computed.
    const expectedCreator = 87_500_000n + (winner === creator.address ? 220_000_000n : 0n);
    expect((await fetchLamports(creator.address)) - before.creator).toBe(expectedCreator);
    if (winner !== creator.address) {
      expect((await fetchLamports(winner)) - before.winner).toBe(220_000_000n);
    }
    expect((await fetchLamports(feeWallet)) - before.fee).toBe(62_500_000n);
    expect(await fetchLamports(vault)).toBe(rent.vault + 880_000_000n);
    const pool = await fetchPool(P1.pool);
    expect(pool.prizePool).toBe(1_100_000_000n);
    expect(pool.quarterPrize).toEqual([220_000_000n, 220_000_000n, 220_000_000n, 440_000_000n]);
    expect(pool.unpaidPrizePool).toBe(880_000_000n);
    expect(pool.quartersSettled).toBe(1);
    expect(pool.feesPaid).toBe(true);
    expect(pool.winningBox[0]).toBe(ev.boxIndex);
    expect(pool.status).toBe(PoolStatus.Drawn);

    expect(await settleExpectingError(P1, 1, { winner })).toBe(errorCode("QuarterOutOfOrder")); // 6008
    expect(await settleExpectingError(P1, 3, { winner })).toBe(errorCode("QuarterOutOfOrder")); // 6008
    expect(await settleExpectingError(P1, 2, { winner })).toBe(errorCode("ScoresNotPosted")); // 6043
    expect(await settleExpectingError(P1, 2, { winner, signer: admin })).toBe(
      errorCode("Unauthorized"), // 6000
    );
    await postQuarter(2);
    const q2Winner = await expectedWinner(P1.pool, 2);
    const other = (await fetchPool(P1.pool)).owners.find((o) => o !== q2Winner)!;
    expect(await settleExpectingError(P1, 2, { winner: other })).toBe(errorCode("WinnerMismatch")); // 6044
  });

  it("2. sponsored P2: Q1 moves 0.42 + 0.0625 + 0.0875, vault 2.25 → 1.68", async () => {
    const winner = await expectedWinner(P2.pool, 1);
    const vault = await vaultPda(P2.pool);
    expect(await fetchLamports(vault)).toBe(rent.vault + 2_250_000_000n);
    const w0 = await fetchLamports(winner);
    const f0 = await fetchLamports(feeWallet);
    const c0 = await fetchLamports(creator.address);
    const sig = await settle(P2, 1);
    const ev = decodeEvent("QuarterSettled", (await emittedEvents(sig))[0]!, quarterSettledDecoder);
    expect(ev.amount).toBe(420_000_000n);
    expect([ev.platformFee, ev.creatorFee]).toEqual([62_500_000n, 87_500_000n]);
    const expectedCreator = 87_500_000n + (winner === creator.address ? 420_000_000n : 0n);
    expect((await fetchLamports(creator.address)) - c0).toBe(expectedCreator);
    if (winner !== creator.address) expect((await fetchLamports(winner)) - w0).toBe(420_000_000n);
    expect((await fetchLamports(feeWallet)) - f0).toBe(62_500_000n);
    expect(await fetchLamports(vault)).toBe(rent.vault + 1_680_000_000n);
    expect((await fetchPool(P2.pool)).prizePool).toBe(2_100_000_000n);
  });

  it("3. FinalOnly P3: Q1 records the box, moves nothing, pays no fee", async () => {
    const winner = await expectedWinner(P3.pool, 1);
    const vault = await vaultPda(P3.pool);
    const snapshot = async () => [
      await fetchLamports(winner),
      await fetchLamports(feeWallet),
      await fetchLamports(creator.address),
      await fetchLamports(vault),
    ];
    const before = await snapshot();
    const sig = await settle(P3, 1);
    const ev = decodeEvent("QuarterSettled", (await emittedEvents(sig))[0]!, quarterSettledDecoder);
    expect(ev.amount).toBe(0n);
    expect(ev.feesPaidNow).toBe(false);
    expect(await snapshot()).toEqual(before);
    const pool = await fetchPool(P3.pool);
    expect(pool.winningBox[0]).toBe(ev.boxIndex);
    expect(pool.feesPaid).toBe(false);
    expect(pool.quarterPrize).toEqual([0n, 0n, 0n, 1_100_000_000n]);
  });

  it("4. integrator P4: Q1 pays the integrator 0.0125 with the first prize; Q2 without the slot is 6045", async () => {
    const winner = await expectedWinner(P4.pool, 1);
    const i0 = await fetchLamports(integrator.address);
    const w0 = await fetchLamports(winner);
    const sig = await settle(P4, 1, { integrator: integrator.address });
    const ev = decodeEvent("QuarterSettled", (await emittedEvents(sig))[0]!, quarterSettledDecoder);
    expect(ev.amount).toBe(217_500_000n);
    expect([ev.platformFee, ev.creatorFee, ev.integratorFee]).toEqual([
      62_500_000n,
      87_500_000n,
      12_500_000n,
    ]);
    expect((await fetchLamports(integrator.address)) - i0).toBe(12_500_000n);
    if (winner !== creator.address) expect((await fetchLamports(winner)) - w0).toBe(217_500_000n);
    expect(await settleExpectingError(P4, 2)).toBe(errorCode("FeeAccountMismatch")); // 6045
  });

  it("5. ORE P5, the closed ATA: the Q1 winner's ATA is recreated and paid; the fee ATAs created", async () => {
    const winner = await expectedWinner(P5.pool, 1);
    const winnerSigner = [creator, buyerA, buyerB].find((k) => k.address === winner)!;
    const winnerAta = await ata(winner, ORE_MINT);
    expect(await tokenAmount(winnerAta)).toBe(0n);
    await send(
      winnerSigner,
      closeTokenAccountInstruction(winnerSigner, winnerAta, winnerSigner.address),
    );
    expect(await fetchAccountData(winnerAta)).toBeNull();
    const k0 = await fetchLamports(keeper.address);
    const sig = await settle(P5, 1, { spl: ORE });
    measured["settle_q1_ore"] = await computeUnitsConsumed(sig);
    // ORE quarters 22 / 22 / 22 / 44, fees 6.25 / 8.75 (§5.1, §5.2 at PRICE_ORE); the creator
    // may be the winner, in which case its ATA holds both.
    expect(await tokenAmount(winnerAta)).toBe(
      22_000_000_000n + (winner === creator.address ? 8_750_000_000n : 0n),
    );
    const winnerInfo = await rpc.getAccountInfo(winnerAta, { encoding: "base64" }).send();
    expect(winnerInfo.value?.owner).toBe(TOKEN_PROGRAM);
    expect(await tokenAmount(await ata(feeWallet, ORE_MINT))).toBe(6_250_000_000n);
    expect(await tokenAmount(await ata(creator.address, ORE_MINT))).toBe(
      8_750_000_000n + (winner === creator.address ? 22_000_000_000n : 0n),
    );
    // The keeper paid at least the winner's and the fee wallet's ATAs (the creator's existed).
    expect(k0 - (await fetchLamports(keeper.address))).toBeGreaterThanOrEqual(
      2n * rent.tokenAccount,
    );
    expect(await tokenAmount(await vaultPda(P5.pool))).toBe(88_000_000_000n);
  });

  it("6. Q2–Q4 on every pool: the shares, FinalOnly pays with the final, every pool Settled, money in = money out", async () => {
    const pools: [PoolRefs, { integrator?: Address; spl?: SplPool }][] = [
      [P1, {}],
      [P2, {}],
      [P3, {}],
      [P4, { integrator: integrator.address }],
      [P5, { spl: ORE }],
    ];
    // Q2 was posted in item 1.
    for (const [refs, o] of pools) {
      const sig = await settle(refs, 2, o);
      const ev = decodeEvent(
        "QuarterSettled",
        (await emittedEvents(sig))[0]!,
        quarterSettledDecoder,
      );
      expect(ev.feesPaidNow).toBe(false);
      if (refs === P3) expect(ev.amount).toBe(0n);
      if (refs === P1) measured["settle_q2_sol"] = await computeUnitsConsumed(sig);
    }
    await postQuarter(3);
    for (const [refs, o] of pools) await settle(refs, 3, o);
    await postQuarter(4);
    const totals: Record<string, bigint> = {};
    for (const [refs, o] of pools) {
      const winner = await expectedWinner(refs.pool, 4);
      const before = await fetchLamports(winner);
      const sig = await settle(refs, 4, o);
      const ev = decodeEvent(
        "QuarterSettled",
        (await emittedEvents(sig))[0]!,
        quarterSettledDecoder,
      );
      const pool = await fetchPool(refs.pool);
      if (refs === P3) {
        expect(ev.amount).toBe(1_100_000_000n);
        expect(ev.feesPaidNow).toBe(true);
        expect([ev.platformFee, ev.creatorFee]).toEqual([62_500_000n, 87_500_000n]);
        measured["settle_q4_final_only"] = await computeUnitsConsumed(sig);
      } else if (refs === P1) {
        expect(ev.amount).toBe(440_000_000n);
        expect((await fetchLamports(winner)) - before).toBe(440_000_000n);
      }
      expect(pool.status).toBe(PoolStatus.Settled);
      expect(pool.quartersSettled).toBe(4);
      expect(pool.unpaidPrizePool).toBe(0n);
      expect(pool.feesPaid).toBe(true);
      if (refs === P5) {
        expect(await tokenAmount(await vaultPda(refs.pool))).toBe(0n);
      } else {
        expect(await fetchLamports(await vaultPda(refs.pool))).toBe(rent.vault);
      }
      // Money in = money out: Σ quarter prizes + fees = 25 × price + sponsored.
      const paid =
        pool.quarterPrize.reduce((a, b) => a + b, 0n) +
        pool.platformFee +
        pool.creatorFee +
        pool.integratorFee;
      expect(paid).toBe(25n * pool.price + pool.sponsoredTotal);
      totals[refs.pool] = paid;
    }
    expect(await settleExpectingError(P1, 5, { winner: creator.address })).toBe(
      errorCode("PoolNotDrawn"), // 6028: the status is Settled
    );
  });

  it("7. close_pool: P1 by a stranger; P2 refused (open sponsorship); P5 ORE closes its vault; a closed pool; an Open pool", async () => {
    const vault1 = await vaultPda(P1.pool);
    const f0 = await fetchLamports(feeWallet);
    const sig = await send(stranger, [
      setComputeUnitLimit(CU_LIMIT),
      await closePoolInstruction(stranger, P1, feeWallet),
    ]);
    measured["close_pool_sol"] = await computeUnitsConsumed(sig);
    expect(await fetchAccountData(P1.pool)).toBeNull();
    expect(await fetchAccountData(vault1)).toBeNull();
    expect((await fetchLamports(feeWallet)) - f0).toBe(rent.vault + rent.pool);
    const events = await emittedEvents(sig);
    expect(events.map(eventName)).toEqual(["PoolClosed"]);
    const ev = decodeEvent("PoolClosed", events[0]!, poolClosedDecoder);
    expect(ev.destination).toBe(feeWallet);
    expect(ev.dust).toBe(0n);

    // P2 keeps its sponsorship account until Step 7's close_sponsorship: 6053, and it stays open.
    expect((await fetchPool(P2.pool)).sponsorshipsOpen).toBe(1);
    expect(
      await sendExpectingError(stranger, await closePoolInstruction(stranger, P2, feeWallet)),
    ).toBe(errorCode("SponsorshipsStillOpen")); // 6053

    const vault5 = await vaultPda(P5.pool);
    const f5 = await fetchLamports(feeWallet);
    const sig5 = await send(stranger, [
      setComputeUnitLimit(CU_LIMIT),
      await closePoolInstruction(stranger, P5, feeWallet, { spl: ORE }),
    ]);
    measured["close_pool_ore"] = await computeUnitsConsumed(sig5);
    expect(await fetchAccountData(vault5)).toBeNull();
    expect(await fetchAccountData(P5.pool)).toBeNull();
    expect((await fetchLamports(feeWallet)) - f5).toBe(rent.tokenAccount + rent.pool);
    const ev5 = decodeEvent("PoolClosed", (await emittedEvents(sig5))[0]!, poolClosedDecoder);
    expect(ev5.dust).toBe(0n);

    // P1 again: the account is gone (Anchor 3012 AccountNotInitialized).
    await expect(
      send(stranger, await closePoolInstruction(stranger, P1, feeWallet)),
    ).rejects.toBeTruthy();

    // A fresh Open pool (on a new game: G is Final after Q4, so create_pool on it is
    // GameNotScheduled): PoolNotTerminal.
    const laterKickoff = (await chainNow()) + 3n * HOUR;
    const game2 = await gamePda({ ...KEY, week: KEY.week + 1 }, laterKickoff);
    await send(
      keeper,
      await createGameInstruction(keeper, { ...KEY, week: KEY.week + 1 }, laterKickoff),
    );
    const nonce = nextNonce++;
    await send(
      creator,
      await createPoolInstruction(creator, game2, poolParams({ nonce, price: PRICE })),
    );
    const open: PoolRefs = {
      pool: await poolPda(game2, creator.address, nonce),
      game: game2,
      creator: creator.address,
    };
    expect(
      await sendExpectingError(stranger, await closePoolInstruction(stranger, open, feeWallet)),
    ).toBe(errorCode("PoolNotTerminal")); // 6055
  });

  it("8. compute units: ORE Q1 (three creates) under 250k; the SOL ones under 100k", () => {
    for (const [name, cu] of Object.entries(measured)) console.log(`${name} consumed ${cu} CU`);
    console.log(`time travel targets: ${travelled.join("; ")}`);
    expect(measured["settle_q1_ore"]!).toBeLessThan(250_000n);
    for (const name of [
      "settle_q1_sol",
      "settle_q2_sol",
      "settle_q4_final_only",
      "close_pool_sol",
    ]) {
      expect(measured[name], name).toBeDefined();
      expect(measured[name]!, name).toBeLessThan(100_000n);
    }
  });

  it("9. the IDL has 19 instructions, 6 accounts, 18 events, 65 errors, docs on each new item", () => {
    expect(IDL.instructions).toHaveLength(19);
    expect(IDL.accounts).toHaveLength(6);
    expect(IDL.events).toHaveLength(18);
    expect(IDL.errors).toHaveLength(65);
    const idl = IDL as unknown as {
      instructions: {
        name: string;
        docs?: string[];
        accounts: { name: string; docs?: string[] }[];
      }[];
      events: { name: string }[];
      types: {
        name: string;
        docs?: string[];
        type: { fields?: { name: string; docs?: string[] }[] };
      }[];
    };
    for (const name of ["settle", "close_pool"]) {
      const ix = idl.instructions.find((i) => i.name === name);
      expect(ix?.docs?.length ?? 0, name).toBeGreaterThan(0);
      for (const account of ix!.accounts.filter(
        (a) => a.name !== "event_authority" && a.name !== "program",
      )) {
        expect(account.docs?.length ?? 0, `${name}.${account.name}`).toBeGreaterThan(0);
      }
    }
    for (const name of ["QuarterSettled", "PoolClosed"]) {
      expect(
        idl.events.some((e) => e.name === name),
        name,
      ).toBe(true);
      const t = idl.types.find((x) => x.name === name);
      expect(t?.docs?.length ?? 0, name).toBeGreaterThan(0);
      for (const field of t!.type.fields ?? []) {
        expect(field.docs?.length ?? 0, `${name}.${field.name}`).toBeGreaterThan(0);
      }
    }
  });
});
