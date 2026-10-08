/**
 * Step 7 localnet suite, last by path: the 30-day reclaim paths against
 * Surfpool forking mainnet (`anchor test`). Its last act jumps the chain
 * clock thirty days ahead, which is why nothing may run after it.
 *
 * The game's recorded kickoff is moved an hour later than its scheduled one
 * so the two clocks differ: the reclaim window opens at
 * `scheduled + RECLAIM_DELAY`, an hour before `recorded + RECLAIM_DELAY`
 * (ARCHITECTURE › Trust model "pushing kickoff can't delay it"). Localnet is
 * approximate: two minutes short fails, two seconds past succeeds; the exact
 * boundary is Mollusk's (`programs/mybarpool/tests/reclaim.rs`).
 */
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { winningBox, type Digits } from "@mybarpool/shared";
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
  ata,
  boxesReclaimedDecoder,
  buyInstruction,
  chainNow,
  closeCounterInstruction,
  closePoolInstruction,
  closeSponsorshipInstruction,
  closeTokenAccountInstruction,
  COMPUTE_BUDGET_PROGRAM,
  computeUnitsConsumed,
  configPda,
  counterPda,
  createGameInstruction,
  createPoolInstruction,
  creatorCounterDecoder,
  decodeAccount,
  decodeEvent,
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
  poolParams,
  poolPda,
  PoolStatus,
  postScoresInstruction,
  reclaimInstruction,
  reclaimSponsorshipInstruction,
  returnBoxesInstruction,
  rpc,
  rpcSubscriptions,
  send,
  sendExpectingError,
  setComputeUnitLimit,
  settleInstruction,
  SLOT_HASHES_SYSVAR,
  sponsorInstruction,
  sponsorshipPda,
  sponsorshipReturnedDecoder,
  TOKEN_PROGRAM,
  tokenAmount,
  updateConfigInstruction,
  updateKickoffInstruction,
  vaultPda,
  withRetry,
  type GameKey,
  type PoolRefs,
  type SplPool,
} from "./helpers/mybarpool.js";
import {
  CU_LIMIT,
  drawnPool as drawnPoolScenario,
  fetchPool,
  PRICE,
  PRICE_ORE,
} from "./helpers/scenario.js";

const SOL = 1_000_000_000n;
const HOUR = 3_600n;
const MINUTE = 60n;
/** PROGRAM §1 `RECLAIM_DELAY`, read from the committed IDL (30 days). */
const RECLAIM_DELAY = BigInt(IDL.constants.find((c) => c.name === "RECLAIM_DELAY")!.value);
/** PROGRAM §2 team table: KC hosting DAL in 2026, a week of its own. */
const KEY: GameKey = { season: 2026, week: 17, home: 15, away: 8 };
const ORE: SplPool = { mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM };
const DUST_SPONSORSHIP = 1_000_000_013n;
/** A reclaiming signer also pays the transaction fee; its SOL delta is the amount minus that. */
const MAX_FEE = 20_000n;

const localnet = new Localnet();

describe("unresolved pools: reclaim at thirty days (Surfpool, runs last)", () => {
  let admin: KeyPairSigner;
  let keeper: KeyPairSigner;
  let creator: KeyPairSigner;
  let buyerA: KeyPairSigner;
  let buyerB: KeyPairSigner;
  let sponsor: KeyPairSigner;
  let stranger: KeyPairSigner;
  let feeWallet: Address;
  let rent: {
    vault: bigint;
    pool: bigint;
    tokenAccount: bigint;
    sponsorship: bigint;
    counter: bigint;
  };
  let GR: Address;
  let scheduled: bigint;
  let recorded: bigint;
  let R1: PoolRefs;
  let R2: PoolRefs;
  let R3: PoolRefs;
  let q1Winner: Address;
  let nextNonce = 1n;
  let nextVarId = 900n;
  const measured: Record<string, bigint> = {};
  const travelled: string[] = [];

  async function travelTo(target: bigint, label: string): Promise<void> {
    const now = await chainNow();
    let info: unknown = "no travel needed";
    if (now < target) {
      info = await localnet.timeTravel({ absoluteTimestamp: Number((target + 2n) * 1000n) });
    }
    const after = await chainNow();
    expect(after).toBeGreaterThanOrEqual(target);
    travelled.push(
      `${label}: target ${target} (scheduled ${(target - scheduled) / MINUTE} min) → ${after}; EpochInfo ${JSON.stringify(info)}`,
    );
  }

  async function openCount(): Promise<number> {
    const data = await fetchAccountData(await counterPda(creator.address, GR));
    expect(data).not.toBeNull();
    return decodeAccount("CreatorCounter", data!, creatorCounterDecoder).openCount;
  }

  async function reclaim(
    who: KeyPairSigner,
    refs: PoolRefs,
    options: { counter?: boolean; spl?: SplPool; measure?: string } = {},
  ) {
    const sig = await send(who, [
      setComputeUnitLimit(CU_LIMIT),
      await reclaimInstruction(who, refs, options),
    ]);
    if (options.measure) measured[options.measure] = await computeUnitsConsumed(sig);
    return sig;
  }

  async function reclaimError(
    who: KeyPairSigner,
    refs: PoolRefs,
    options: { counter?: boolean; spl?: SplPool } = {},
  ) {
    return sendExpectingError(who, [
      setComputeUnitLimit(CU_LIMIT),
      await reclaimInstruction(who, refs, options),
    ]);
  }

  async function closePool(
    refs: PoolRefs,
    options: { spl?: SplPool; destination?: Address; measure?: string } = {},
  ) {
    const sig = await send(stranger, [
      setComputeUnitLimit(CU_LIMIT),
      await closePoolInstruction(stranger, refs, feeWallet, options),
    ]);
    if (options.measure) measured[options.measure] = await computeUnitsConsumed(sig);
    return decodeEvent("PoolClosed", (await emittedEvents(sig))[0]!, poolClosedDecoder);
  }

  /** The signer's SOL went up by `amount` less the fee it paid. */
  function expectReceived(delta: bigint, amount: bigint, what: string) {
    expect(delta, what).toBeLessThanOrEqual(amount);
    expect(delta, what).toBeGreaterThan(amount - MAX_FEE);
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
    sponsor = await generateKeyPairSigner();
    stranger = await generateKeyPairSigner();
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    for (const who of [keeper, creator, buyerA, buyerB, sponsor, stranger]) {
      await withRetry(() =>
        airdrop({
          recipientAddress: who.address,
          lamports: lamports(100n * SOL),
          commitment: "confirmed",
        }),
      );
    }
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
    for (const addr of [COMPUTE_BUDGET_PROGRAM, SLOT_HASHES_SYSVAR, feeWallet]) {
      await withRetry(() => rpc.getAccountInfo(addr, { encoding: "base64" }).send());
    }
    const r = async (size: bigint) => rpc.getMinimumBalanceForRentExemption(size).send();
    rent = {
      vault: await r(0n),
      pool: await r(1442n),
      tokenAccount: await r(165n),
      sponsorship: await r(81n),
      counter: await r(74n),
    };
    // ORE for R3 exactly: every ATA holds 0 after buying, so buyerA can close theirs.
    for (const [who, boxes] of [
      [creator, 5n],
      [buyerA, 10n],
      [buyerB, 10n],
    ] as const) {
      await withRetry(() =>
        localnet.setTokenAccount(who.address, ORE_MINT, { amount: Number(boxes * PRICE_ORE) }),
      );
    }

    // GR: scheduled two hours out, then the keeper moves the recorded kickoff an hour later.
    scheduled = (await chainNow()) + 2n * HOUR;
    recorded = scheduled + HOUR;
    GR = await gamePda(KEY, scheduled);
    await send(keeper, await createGameInstruction(keeper, KEY, scheduled));
    await send(keeper, await updateKickoffInstruction(keeper, GR, recorded));

    const pool = async (overrides: Record<string, unknown>, spl?: SplPool): Promise<PoolRefs> => {
      const nonce = nextNonce++;
      await send(
        creator,
        await createPoolInstruction(
          creator,
          GR,
          poolParams({ nonce, price: spl ? PRICE_ORE : PRICE, token: spl ? 2 : 0, ...overrides }),
          spl
            ? { ...spl, tokenAccount: await ata(creator.address, spl.mint, spl.tokenProgram) }
            : {},
        ),
      );
      return {
        pool: await poolPda(GR, creator.address, nonce),
        game: GR,
        creator: creator.address,
      };
    };
    const buy = async (who: KeyPairSigner, refs: PoolRefs, count: number, spl?: SplPool) =>
      send(
        who,
        await buyInstruction(
          who,
          refs,
          count,
          spl ? { ...spl, tokenAccount: await ata(who.address, spl.mint, spl.tokenProgram) } : {},
        ),
      );

    R1 = await pool({ initialBoxes: 5 });
    await buy(buyerA, R1, 10);
    await send(sponsor, await sponsorInstruction(sponsor, R1, 50_000_000n));
    R2 = await drawnPoolScenario(
      {
        creator,
        buyers: [buyerA, buyerB],
        keeper,
        sampler: stranger,
        game: GR,
        nextNonce: () => nextNonce++,
        nextVarId: () => nextVarId++,
      },
      { creatorAddonBps: 200 },
    );
    await send(sponsor, await sponsorInstruction(sponsor, R2, DUST_SPONSORSHIP));
    R3 = await pool({ initialBoxes: 5 }, ORE);
    await buy(buyerA, R3, 10, ORE);
    await buy(buyerB, R3, 10, ORE);
    expect((await fetchPool(R3.pool)).status).toBe(PoolStatus.Locked);
    expect(await openCount()).toBe(1); // R1 only; R2 and R3 locked

    // Q1 on R2 at the recorded kickoff + 16 min.
    await travelTo(recorded + 16n * MINUTE, "GR recorded kickoff + 16 min");
    await send(
      keeper,
      await postScoresInstruction(keeper, GR, {
        quarter: 1,
        home: 7,
        away: 3,
        isFinal: false,
        hadOvertime: false,
      }),
    );
    // The winner from the stored axes (PROGRAM §6.3), as settlement.test.ts does.
    const p2 = await fetchPool(R2.pool);
    const idx = winningBox({
      home: 7,
      away: 3,
      homeAxis: p2.homeAxis as unknown as Digits,
      awayAxis: p2.awayAxis as unknown as Digits,
    });
    q1Winner = p2.owners[idx]!;
    await send(keeper, [
      setComputeUnitLimit(CU_LIMIT),
      await settleInstruction(keeper, R2, 1, q1Winner, feeWallet),
    ]);
    expect((await fetchPool(R2.pool)).feesPaid).toBe(true);
  }, 600_000);

  it("1. before the window: ReclaimTooEarly right after kickoff", async () => {
    expect(await reclaimError(buyerA, R1, { counter: true })).toBe(errorCode("ReclaimTooEarly")); // 6050
  });

  it("2. the clock: two minutes short fails, two seconds past succeeds — an hour before recorded + 30 d", async () => {
    await travelTo(scheduled + RECLAIM_DELAY - 122n, "scheduled + RECLAIM_DELAY − 120 s");
    expect(await chainNow()).toBeLessThan(scheduled + RECLAIM_DELAY);
    expect(await reclaimError(buyerA, R1, { counter: true })).toBe(errorCode("ReclaimTooEarly")); // 6050
    expect(
      await sendExpectingError(sponsor, [
        setComputeUnitLimit(CU_LIMIT),
        await reclaimSponsorshipInstruction(sponsor, R1, { counter: true }),
      ]),
    ).toBe(errorCode("ReclaimTooEarly")); // 6050
    await travelTo(scheduled + RECLAIM_DELAY, "scheduled + RECLAIM_DELAY + 2 s");
    const now = await chainNow();
    expect(now).toBeGreaterThanOrEqual(scheduled + RECLAIM_DELAY);
    expect(now).toBeLessThan(recorded + RECLAIM_DELAY - 30n * MINUTE);
  }, 300_000);

  it("3. R1 (never paid, Open): the owners and the sponsor reclaim; close_pool goes to the creator", async () => {
    const a0 = await fetchLamports(buyerA.address);
    const sig = await reclaim(buyerA, R1, { counter: true, measure: "reclaim_sol_open" });
    expectReceived((await fetchLamports(buyerA.address)) - a0, 500_000_000n, "buyerA");
    let p = await fetchPool(R1.pool);
    expect([p.status, p.abandoned]).toEqual([PoolStatus.Returned, true]);
    expect(await openCount()).toBe(0);
    const events = await emittedEvents(sig);
    expect(events.map(eventName)).toEqual(["BoxesReclaimed"]);
    const ev = decodeEvent("BoxesReclaimed", events[0]!, boxesReclaimedDecoder);
    expect([ev.owner, ev.amount]).toEqual([buyerA.address, 500_000_000n]);
    expect(ev.boxes).toHaveLength(10); // positions are the program's (PROGRAM §6.1)
    expect(await reclaimError(buyerA, R1)).toBe(errorCode("NothingToReturn")); // 6052
    expect(await reclaimError(buyerB, R1)).toBe(errorCode("NotOwner")); // 6051
    const c0 = await fetchLamports(creator.address);
    await reclaim(creator, R1, { measure: "reclaim_sol_returned" });
    expectReceived((await fetchLamports(creator.address)) - c0, 250_000_000n, "creator");

    const s0 = await fetchLamports(sponsor.address);
    const sigS = await send(sponsor, [
      setComputeUnitLimit(CU_LIMIT),
      await reclaimSponsorshipInstruction(sponsor, R1),
    ]);
    measured["reclaim_sponsorship_sol"] = await computeUnitsConsumed(sigS);
    expectReceived(
      (await fetchLamports(sponsor.address)) - s0,
      50_000_000n + rent.sponsorship,
      "sponsor",
    );
    expect(await fetchAccountData(await sponsorshipPda(R1.pool, sponsor.address))).toBeNull();
    const evS = decodeEvent(
      "SponsorshipReturned",
      (await emittedEvents(sigS))[0]!,
      sponsorshipReturnedDecoder,
    );
    expect([evS.sponsor, evS.amount]).toEqual([sponsor.address, 50_000_000n]);
    // A stranger has no Sponsorship at their seeds (Anchor 3012 AccountNotInitialized).
    await expect(
      send(stranger, await reclaimSponsorshipInstruction(stranger, R1)),
    ).rejects.toBeTruthy();
    p = await fetchPool(R1.pool);
    expect(p.sponsorshipsOpen).toBe(0);
    expect(await fetchLamports(await vaultPda(R1.pool))).toBe(rent.vault);

    const c1 = await fetchLamports(creator.address);
    const closed = await closePool(R1, { measure: "close_pool_abandoned_sol" });
    expect([closed.destination, closed.dust]).toEqual([creator.address, 0n]);
    expect((await fetchLamports(creator.address)) - c1).toBe(rent.vault + rent.pool);
    const f0 = await fetchLamports(feeWallet);
    await send(stranger, await closeCounterInstruction(creator.address, GR, feeWallet));
    expect((await fetchLamports(feeWallet)) - f0).toBe(rent.counter);
  }, 300_000);

  it("4. R2 (paid Q1, Drawn): the first reclaim splits; the sponsorship can only be closed", async () => {
    const pool = await fetchPool(R2.pool);
    const boxesOf = (o: Address) => BigInt(pool.owners.filter((x) => x === o).length);
    const a0 = await fetchLamports(buyerA.address);
    await reclaim(buyerA, R2, { measure: "reclaim_sol_after_q1" });
    let p = await fetchPool(R2.pool);
    expect([p.status, p.abandoned, p.splitAmount]).toEqual([PoolStatus.Split, true, 67_200_000n]);
    expectReceived(
      (await fetchLamports(buyerA.address)) - a0,
      boxesOf(buyerA.address) * 67_200_000n,
      "buyerA",
    );
    expect(
      await sendExpectingError(sponsor, [
        setComputeUnitLimit(CU_LIMIT),
        await reclaimSponsorshipInstruction(sponsor, R2),
      ]),
    ).toBe(errorCode("FeesAlreadyPaid")); // 6047
    const s0 = await fetchLamports(sponsor.address);
    await send(stranger, await closeSponsorshipInstruction(R2, sponsor.address));
    expect((await fetchLamports(sponsor.address)) - s0).toBe(rent.sponsorship);
    for (const who of [creator, buyerB]) {
      const b0 = await fetchLamports(who.address);
      await reclaim(who, R2);
      expectReceived(
        (await fetchLamports(who.address)) - b0,
        boxesOf(who.address) * 67_200_000n,
        who.address,
      );
    }
    p = await fetchPool(R2.pool);
    expect(p.unpaidPrizePool).toBe(11n);
    expect(p.returned).toBe(0x1ff_ffff);
    const c0 = await fetchLamports(creator.address);
    const closed = await closePool(R2);
    expect([closed.destination, closed.dust]).toEqual([creator.address, 11n]);
    expect((await fetchLamports(creator.address)) - c0).toBe(rent.vault + 11n + rent.pool);
  }, 300_000);

  it("5. R3 (ORE, Locked): the buyer reclaims and pays their own ATA; the keeper finishes; close to the creator", async () => {
    const buyerAta = await ata(buyerA.address, ORE_MINT);
    expect(await tokenAmount(buyerAta)).toBe(0n);
    await send(buyerA, closeTokenAccountInstruction(buyerA, buyerAta, buyerA.address));
    expect(await fetchAccountData(buyerAta)).toBeNull();
    const a0 = await fetchLamports(buyerA.address);
    await reclaim(buyerA, R3, { spl: ORE, measure: "reclaim_ore_locked_ata_missing" });
    expect(await tokenAmount(buyerAta)).toBe(50_000_000_000n);
    expect(a0 - (await fetchLamports(buyerA.address))).toBeGreaterThanOrEqual(rent.tokenAccount);
    let p = await fetchPool(R3.pool);
    expect([p.status, p.abandoned]).toEqual([PoolStatus.Returned, true]);

    // The keeper finishes an abandoned pool (§4.6 "already returning").
    const sig = await send(keeper, [
      setComputeUnitLimit(CU_LIMIT),
      await returnBoxesInstruction(keeper, R3, [creator.address, buyerB.address], { spl: ORE }),
    ]);
    measured["return_boxes_ore_finish_abandoned"] = await computeUnitsConsumed(sig);
    p = await fetchPool(R3.pool);
    expect(p.returned).toBe(0x1ff_ffff);
    expect(await tokenAmount(await ata(creator.address, ORE_MINT))).toBe(25_000_000_000n);
    expect(await tokenAmount(await ata(buyerB.address, ORE_MINT))).toBe(50_000_000_000n);
    const vault = await vaultPda(R3.pool);
    expect(await tokenAmount(vault)).toBe(0n);

    const c0 = await fetchLamports(creator.address);
    const closed = await closePool(R3, {
      spl: ORE,
      destination: creator.address,
      measure: "close_pool_abandoned_ore",
    });
    expect([closed.destination, closed.dust]).toEqual([creator.address, 0n]);
    expect(await fetchAccountData(vault)).toBeNull();
    expect(await fetchAccountData(R3.pool)).toBeNull();
    expect((await fetchLamports(creator.address)) - c0).toBe(rent.tokenAccount + rent.pool);
  }, 300_000);

  it("6. compute units and the clock targets", () => {
    for (const [name, cu] of Object.entries(measured)) console.log(`${name} consumed ${cu} CU`);
    for (const t of travelled) console.log(`time travel ${t}`);
    for (const name of [
      "reclaim_sol_open",
      "reclaim_sponsorship_sol",
      "close_pool_abandoned_sol",
    ]) {
      expect(measured[name], name).toBeDefined();
      expect(measured[name]!, name).toBeLessThan(100_000n);
    }
  });
});
