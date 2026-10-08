/**
 * Step 7 localnet suite: returns, splits, cancellations and sponsorship
 * closes against Surfpool forking mainnet (`anchor test`). Runs after
 * `pools.test.ts` and before `settlement.test.ts` by path; `config.test.ts`
 * initialised the config and preloaded the platform's Entropy deployment.
 * One `describe`, ordered. The 30-day reclaim paths live in
 * `unresolved.test.ts`, which runs last.
 *
 * Every number is the brief's: a return is always the full purchase price
 * (0.05 SOL / 0.05 ORE per box), a split after Q1 on the 1,000,000,013-sponsored
 * pool is 67,200,000 per box with 11 lamports of dust, the ORE split after Q1 is
 * 3,520,000,000 per box.
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
  type Instruction,
  type KeyPairSigner,
  type TransactionSigner,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { Localnet } from "../scripts/localnet.js";
import {
  ata,
  boxesReturnedDecoder,
  boxesSplitDecoder,
  buildTransaction,
  buyInstruction,
  cancelPoolInstruction,
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
  GameStatus,
  IDL,
  markGameInstruction,
  ORE_MINT,
  platformConfigDecoder,
  poolCancelledDecoder,
  poolClosedDecoder,
  poolParams,
  poolPda,
  PoolStatus,
  postScoresInstruction,
  quarterSettledDecoder,
  returnBoxesInstruction,
  returnSponsorshipInstruction,
  rpc,
  rpcSubscriptions,
  send,
  sendExpectingError,
  setComputeUnitLimit,
  settleInstruction,
  SLOT_HASHES_SYSVAR,
  splitInstruction,
  sponsorInstruction,
  sponsorshipPda,
  sponsorshipReturnedDecoder,
  sponsorshipClosedDecoder,
  SYSTEM_PROGRAM,
  TOKEN_PROGRAM,
  tokenAmount,
  updateConfigInstruction,
  vaultPda,
  wireTransaction,
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
  type Scenario,
} from "./helpers/scenario.js";

const SOL = 1_000_000_000n;
const HOUR = 3_600n;
const MINUTE = 60n;
/** PROGRAM §2 team table: KC hosting DAL in 2026; one week per game of this suite. */
const KEY: GameKey = { season: 2026, week: 10, home: 15, away: 8 };
/** The Step 3 scores: Q1 7–3, Q2 14–10, Q3 17–17, Q4 24–20 final with overtime. */
const SCORES: [number, number][] = [
  [7, 3],
  [14, 10],
  [17, 17],
  [24, 20],
];
/** Every batch transaction raises the limit well above the 25-owner bench row (135k). */
const BATCH_CU_LIMIT = 1_400_000;
const ORE: SplPool = { mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM };
const DUST_SPONSORSHIP = 1_000_000_013n;

const localnet = new Localnet();

/** `SystemInstruction::Transfer { lamports }`: `[2,0,0,0] ‖ u64 LE`. */
function transferInstruction(from: TransactionSigner, to: Address, amount: bigint): Instruction {
  const data = new Uint8Array(12);
  data[0] = 2;
  let v = amount;
  for (let i = 4; i < 12; i++) {
    data[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return {
    programAddress: SYSTEM_PROGRAM,
    accounts: [
      { address: from.address, role: 3 /* WRITABLE_SIGNER */, signer: from },
      { address: to, role: 1 /* WRITABLE */ },
    ],
    data,
  } as Instruction;
}

function popcount(bits: number): number {
  let n = 0;
  for (let v = bits >>> 0; v !== 0; v >>>= 1) n += v & 1;
  return n;
}

describe("returns, splits and cancellations (Surfpool, the platform's Entropy deployment)", () => {
  let admin: KeyPairSigner;
  let keeper: KeyPairSigner;
  let creator: KeyPairSigner;
  let creatorB: KeyPairSigner;
  let buyerA: KeyPairSigner;
  let buyerB: KeyPairSigner;
  let buyerC: KeyPairSigner;
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
  let nextNonce = 1n;
  let nextVarId = 700n;
  let week = KEY.week;
  const measured: Record<string, bigint> = {};
  const travelled: string[] = [];
  const profiles: Record<string, unknown> = {};
  const notes: string[] = [];

  function scenario(game: Address): Scenario {
    return {
      creator,
      buyers: [buyerA, buyerB],
      keeper,
      sampler: stranger,
      game,
      nextNonce: () => nextNonce++,
      nextVarId: () => nextVarId++,
    };
  }

  /** A fresh game `hours` ahead of the chain clock; returns its address and kickoff. */
  async function newGame(hours: bigint): Promise<{ game: Address; kickoff: bigint }> {
    const key = { ...KEY, week: week++ };
    const kickoff = (await chainNow()) + hours * HOUR;
    const game = await gamePda(key, kickoff);
    await send(keeper, await createGameInstruction(keeper, key, kickoff));
    return { game, kickoff };
  }

  /** `games.test.ts`'s pattern: forward only, to `target + 2` seconds. */
  async function travelTo(target: bigint, label: string): Promise<void> {
    const now = await chainNow();
    if (now < target) {
      await localnet.timeTravel({ absoluteTimestamp: Number((target + 2n) * 1000n) });
    }
    const after = await chainNow();
    expect(after).toBeGreaterThanOrEqual(target);
    travelled.push(`${label}: ${target} → ${after}`);
  }

  async function createPool(
    who: KeyPairSigner,
    game: Address,
    overrides: Parameters<typeof poolParams>[0] extends infer P ? Partial<P> : never,
    spl?: SplPool,
  ): Promise<PoolRefs> {
    const nonce = nextNonce++;
    await send(
      who,
      await createPoolInstruction(
        who,
        game,
        poolParams({ nonce, price: spl ? PRICE_ORE : PRICE, token: spl ? 2 : 0, ...overrides }),
        spl ? { ...spl, tokenAccount: await ata(who.address, spl.mint, spl.tokenProgram) } : {},
      ),
    );
    return { pool: await poolPda(game, who.address, nonce), game, creator: who.address };
  }

  async function buy(who: KeyPairSigner, refs: PoolRefs, count: number, spl?: SplPool) {
    await send(
      who,
      await buyInstruction(
        who,
        refs,
        count,
        spl ? { ...spl, tokenAccount: await ata(who.address, spl.mint, spl.tokenProgram) } : {},
      ),
    );
  }

  async function returnBoxes(
    refs: PoolRefs,
    owners: Address[],
    options: { counter?: boolean; spl?: SplPool; signer?: KeyPairSigner; measure?: string } = {},
  ) {
    const signer = options.signer ?? keeper;
    const sig = await send(signer, [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await returnBoxesInstruction(signer, refs, owners, options),
    ]);
    if (options.measure) measured[options.measure] = await computeUnitsConsumed(sig);
    return sig;
  }

  async function returnBoxesError(
    refs: PoolRefs,
    owners: Address[],
    options: { counter?: boolean; spl?: SplPool; signer?: KeyPairSigner } = {},
  ) {
    const signer = options.signer ?? keeper;
    return sendExpectingError(signer, [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await returnBoxesInstruction(signer, refs, owners, options),
    ]);
  }

  async function split(
    refs: PoolRefs,
    owners: Address[],
    options: { spl?: SplPool; signer?: KeyPairSigner; measure?: string } = {},
  ) {
    const signer = options.signer ?? keeper;
    const sig = await send(signer, [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await splitInstruction(signer, refs, owners, options),
    ]);
    if (options.measure) measured[options.measure] = await computeUnitsConsumed(sig);
    return sig;
  }

  async function splitError(
    refs: PoolRefs,
    owners: Address[],
    options: { spl?: SplPool; signer?: KeyPairSigner } = {},
  ) {
    const signer = options.signer ?? keeper;
    return sendExpectingError(signer, [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await splitInstruction(signer, refs, owners, options),
    ]);
  }

  async function closePool(refs: PoolRefs, options: { spl?: SplPool; measure?: string } = {}) {
    const sig = await send(stranger, [
      setComputeUnitLimit(CU_LIMIT),
      await closePoolInstruction(stranger, refs, feeWallet, options),
    ]);
    if (options.measure) measured[options.measure] = await computeUnitsConsumed(sig);
    return decodeEvent("PoolClosed", (await emittedEvents(sig))[0]!, poolClosedDecoder);
  }

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

  async function settle(refs: PoolRefs, quarter: number, spl?: SplPool) {
    const winner = await expectedWinner(refs.pool, quarter);
    const sig = await send(keeper, [
      setComputeUnitLimit(CU_LIMIT),
      await settleInstruction(keeper, refs, quarter, winner, feeWallet, spl ? { spl } : {}),
    ]);
    return {
      winner,
      event: decodeEvent("QuarterSettled", (await emittedEvents(sig))[0]!, quarterSettledDecoder),
    };
  }

  async function postScores(game: Address, quarter: number) {
    const [home, away] = SCORES[quarter - 1]!;
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

  async function openCount(who: Address, game: Address): Promise<number> {
    const data = await fetchAccountData(await counterPda(who, game));
    expect(data).not.toBeNull();
    return decodeAccount("CreatorCounter", data!, creatorCounterDecoder).openCount;
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
    creatorB = await generateKeyPairSigner();
    buyerA = await generateKeyPairSigner();
    buyerB = await generateKeyPairSigner();
    buyerC = await generateKeyPairSigner();
    sponsor = await generateKeyPairSigner();
    stranger = await generateKeyPairSigner();
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    for (const who of [keeper, creator, creatorB, buyerA, buyerB, buyerC, sponsor, stranger]) {
      await withRetry(() =>
        airdrop({
          recipientAddress: who.address,
          lamports: lamports(100n * SOL),
          commitment: "confirmed",
        }),
      );
    }
    // The worked numbers assume 500 / 500 (config.test.ts leaves platform_bps at 400).
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
    // ORE: the creator buys 5 on U2 and 5 on T2, buyerA 3 on U2 (so the ATA is empty and can
    // be closed) and later 10 on T2 (topped up with setTokenAccount again), buyerB 10 on T2.
    await withRetry(() =>
      localnet.setTokenAccount(creator.address, ORE_MINT, { amount: Number(10n * PRICE_ORE) }),
    );
    await withRetry(() =>
      localnet.setTokenAccount(buyerA.address, ORE_MINT, { amount: Number(3n * PRICE_ORE) }),
    );
    await withRetry(() =>
      localnet.setTokenAccount(buyerB.address, ORE_MINT, { amount: Number(10n * PRICE_ORE) }),
    );
  }, 300_000);

  it("1. unfilled at kickoff: U1 SOL sponsored, U2 ORE, U3 empty, W1 with 24 one-box buyers", async () => {
    const { game: GU, kickoff } = await newGame(1n);
    const U1 = await createPool(creator, GU, { initialBoxes: 5 });
    await buy(buyerA, U1, 10);
    await send(sponsor, await sponsorInstruction(sponsor, U1, 50_000_000n));
    const U2 = await createPool(creator, GU, { initialBoxes: 5 }, ORE);
    await buy(buyerA, U2, 3, ORE);
    const U3 = await createPool(creator, GU, { initialBoxes: 0 });
    expect(await openCount(creator.address, GU)).toBe(3);
    // W1: 24 distinct one-box buyers, funded in one transaction of 24 transfers.
    const W1 = await createPool(creatorB, GU, { initialBoxes: 0 });
    const buyers: KeyPairSigner[] = [];
    for (let i = 0; i < 24; i++) buyers.push(await generateKeyPairSigner());
    // Two transactions of twelve: twenty-four transfers are 1,344 bytes, over the 1,232-byte
    // packet (the RPC refuses it with -32602 "VersionedTransaction too large"; NOTES).
    for (const half of [buyers.slice(0, 12), buyers.slice(12)]) {
      await send(
        admin,
        half.map((b) => transferInstruction(admin, b.address, 200_000_000n)),
      );
    }
    for (const b of buyers) await buy(b, W1, 1);
    expect((await fetchPool(W1.pool)).sold).toBe(24);
    expect((await fetchPool(W1.pool)).status).toBe(PoolStatus.Open);

    // Before kickoff: nothing to return yet; the admin is not the keeper.
    expect(await returnBoxesError(U1, [creator.address, buyerA.address], { counter: true })).toBe(
      errorCode("NotReturnable"), // 6046
    );
    expect(
      await returnBoxesError(U1, [creator.address, buyerA.address], {
        counter: true,
        signer: admin,
      }),
    ).toBe(errorCode("Unauthorized")); // 6000

    await travelTo(kickoff + 2n, "GU kickoff");

    // U1: the keeper returns both owners; the counter drops 3 → 2.
    const c0 = await fetchLamports(creator.address);
    const a0 = await fetchLamports(buyerA.address);
    const sig = await returnBoxes(U1, [creator.address, buyerA.address], {
      counter: true,
      measure: "return_boxes_first_sol_2_owners",
    });
    expect((await fetchLamports(creator.address)) - c0).toBe(250_000_000n);
    expect((await fetchLamports(buyerA.address)) - a0).toBe(500_000_000n);
    let p = await fetchPool(U1.pool);
    expect(p.status).toBe(PoolStatus.Returned);
    // Box positions are assigned by the program (PROGRAM §6.1), so the bitmap is fifteen set
    // bits at whatever indices the creator's and buyerA's purchases landed on.
    expect(popcount(p.returned)).toBe(15);
    expect(await openCount(creator.address, GU)).toBe(2);
    const events = await emittedEvents(sig);
    expect(events.map(eventName)).toEqual(["BoxesReturned", "BoxesReturned"]);
    const ev = events.map((e) => decodeEvent("BoxesReturned", e, boxesReturnedDecoder));
    const pool1 = await fetchPool(U1.pool);
    const boxesOf = (o: Address) =>
      pool1.owners.flatMap((x, i) => (x === o ? [i] : [])) as number[];
    expect(ev[0]!.owner).toBe(creator.address);
    expect(ev[0]!.boxes).toEqual(boxesOf(creator.address));
    expect(ev[0]!.boxes).toHaveLength(5);
    expect(ev[0]!.amount).toBe(250_000_000n);
    expect(ev[1]!.owner).toBe(buyerA.address);
    expect(ev[1]!.boxes).toEqual(boxesOf(buyerA.address));
    expect(ev[1]!.boxes).toHaveLength(10);
    expect(ev[1]!.amount).toBe(500_000_000n);
    expect(await returnBoxesError(U1, [creator.address, buyerA.address])).toBe(
      errorCode("NothingToReturn"), // 6052
    );

    // The sponsorship: a substituted destination first (Anchor ConstraintAddress 2012).
    expect(
      await sendExpectingError(
        keeper,
        await returnSponsorshipInstruction(keeper, U1, sponsor.address, {
          destination: stranger.address,
        }),
      ),
    ).toBe(2012);
    const sp0 = await fetchLamports(sponsor.address);
    const sigS = await send(
      keeper,
      await returnSponsorshipInstruction(keeper, U1, sponsor.address),
    );
    measured["return_sponsorship_sol"] = await computeUnitsConsumed(sigS);
    expect((await fetchLamports(sponsor.address)) - sp0).toBe(50_000_000n + rent.sponsorship);
    expect(await fetchAccountData(await sponsorshipPda(U1.pool, sponsor.address))).toBeNull();
    p = await fetchPool(U1.pool);
    expect(p.sponsorshipsOpen).toBe(0);
    expect([p.sponsoredTotal, p.sponsorCount]).toEqual([50_000_000n, 1]);
    const evS = decodeEvent(
      "SponsorshipReturned",
      (await emittedEvents(sigS))[0]!,
      sponsorshipReturnedDecoder,
    );
    expect([evS.sponsor, evS.amount]).toEqual([sponsor.address, 50_000_000n]);
    // Again: the account is gone (Anchor 3012 AccountNotInitialized, surfaced as an error).
    await expect(
      send(keeper, await returnSponsorshipInstruction(keeper, U1, sponsor.address)),
    ).rejects.toBeTruthy();
    const f0 = await fetchLamports(feeWallet);
    const closed = await closePool(U1, { measure: "close_pool_returned_sol" });
    expect((await fetchLamports(feeWallet)) - f0).toBe(rent.vault + rent.pool);
    expect(closed.dust).toBe(0n);
    expect(closed.destination).toBe(feeWallet);

    // U2 (ORE): buyerA closes the empty ATA; the return recreates it at the keeper's expense.
    const buyerAta = await ata(buyerA.address, ORE_MINT);
    expect(await tokenAmount(buyerAta)).toBe(0n);
    await send(buyerA, closeTokenAccountInstruction(buyerA, buyerAta, buyerA.address));
    expect(await fetchAccountData(buyerAta)).toBeNull();
    const creatorAta = await ata(creator.address, ORE_MINT);
    const cOre0 = (await tokenAmount(creatorAta))!;
    const k0 = await fetchLamports(keeper.address);
    await returnBoxes(U2, [creator.address, buyerA.address], {
      counter: true,
      spl: ORE,
      measure: "return_boxes_ore_2_owners_one_create",
    });
    expect(await tokenAmount(buyerAta)).toBe(15_000_000_000n);
    expect((await tokenAmount(creatorAta))! - cOre0).toBe(25_000_000_000n);
    expect(k0 - (await fetchLamports(keeper.address))).toBeGreaterThanOrEqual(rent.tokenAccount);
    expect(await openCount(creator.address, GU)).toBe(1);
    const vault2 = await vaultPda(U2.pool);
    const f2 = await fetchLamports(feeWallet);
    const closed2 = await closePool(U2, { spl: ORE, measure: "close_pool_returned_ore" });
    expect(await fetchAccountData(vault2)).toBeNull();
    expect((await fetchLamports(feeWallet)) - f2).toBe(rent.tokenAccount + rent.pool);
    expect(closed2.dust).toBe(0n);

    // U3: nothing sold; the status change alone succeeds, no event, and the counter reaches 0.
    const sig3 = await returnBoxes(U3, [], { counter: true, measure: "return_boxes_empty" });
    expect((await fetchPool(U3.pool)).status).toBe(PoolStatus.Returned);
    expect(await emittedEvents(sig3)).toHaveLength(0);
    expect(await openCount(creator.address, GU)).toBe(0);
    await closePool(U3);
    const f3 = await fetchLamports(feeWallet);
    await send(stranger, await closeCounterInstruction(creator.address, GU, feeWallet));
    expect(await fetchAccountData(await counterPda(creator.address, GU))).toBeNull();
    expect((await fetchLamports(feeWallet)) - f3).toBe(rent.counter);

    // W1: one transaction for all 24 does not fit the 1,232-byte packet.
    const all = buyers.map((b) => b.address);
    let stage = "build";
    let detail = "";
    try {
      const tx = await buildTransaction(keeper, [
        setComputeUnitLimit(BATCH_CU_LIMIT),
        await returnBoxesInstruction(keeper, W1, all, { counter: true }),
      ]);
      const wire = wireTransaction(tx);
      stage = `send (${wire.bytes} bytes)`;
      await send(keeper, [
        setComputeUnitLimit(BATCH_CU_LIMIT),
        await returnBoxesInstruction(keeper, W1, all, { counter: true }),
      ]);
      stage = "sent";
    } catch (error) {
      detail = error instanceof Error ? error.message : String(error);
    }
    expect(stage).not.toBe("sent");
    notes.push(`W1 24-owner single transaction refused at ${stage}: ${detail}`);
    expect((await fetchPool(W1.pool)).status).toBe(PoolStatus.Open);

    // Two pages of twelve; both profiled.
    const balances0 = await Promise.all(all.map((a) => fetchLamports(a)));
    for (const [i, page] of [all.slice(0, 12), all.slice(12)].entries()) {
      const ixs = [
        setComputeUnitLimit(BATCH_CU_LIMIT),
        await returnBoxesInstruction(keeper, W1, page, { counter: i === 0 }),
      ];
      const tx = await buildTransaction(keeper, ixs);
      const wire = wireTransaction(tx);
      profiles[`W1 page ${i + 1} (${wire.bytes} bytes, ${ixs[1]!.accounts!.length} accounts)`] =
        await localnet.profileTransaction(wire.base64, `w1-page-${i + 1}`);
      await returnBoxes(W1, page, {
        counter: i === 0,
        measure: `return_boxes_sol_12_owners_page_${i + 1}`,
      });
    }
    const balances1 = await Promise.all(all.map((a) => fetchLamports(a)));
    for (let i = 0; i < 24; i++) expect(balances1[i]! - balances0[i]!).toBe(50_000_000n);
    p = await fetchPool(W1.pool);
    expect(p.status).toBe(PoolStatus.Returned);
    expect(popcount(p.returned)).toBe(24);
    expect(await openCount(creatorB.address, GU)).toBe(0);
    await closePool(W1);
  }, 600_000);

  it("2. marked Postponed: M1 drawn + sponsored, M2 Locked (cancelled), M3 Open (cancelled)", async () => {
    const { game: GM } = await newGame(2n);
    const M1 = await drawnPoolScenario(scenario(GM), { creatorAddonBps: 200 });
    await send(sponsor, await sponsorInstruction(sponsor, M1, SOL));
    const M2 = await createPool(creator, GM, { initialBoxes: 5 });
    await buy(buyerA, M2, 10);
    await buy(buyerB, M2, 10);
    expect((await fetchPool(M2.pool)).status).toBe(PoolStatus.Locked);
    const M3 = await createPool(creator, GM, { initialBoxes: 5 });
    expect(await openCount(creator.address, GM)).toBe(1);
    const owners = [creator.address, buyerA.address, buyerB.address];

    // Before any mark.
    expect(await returnBoxesError(M1, owners)).toBe(errorCode("NotReturnable")); // 6046
    expect(await splitError(M1, owners)).toBe(errorCode("NotSuspended")); // 6048
    expect(await sendExpectingError(keeper, await cancelPoolInstruction(keeper, M2))).toBe(
      errorCode("Unauthorized"), // 6000
    );
    const sigC = await send(admin, await cancelPoolInstruction(admin, M2));
    measured["cancel_pool_locked"] = await computeUnitsConsumed(sigC);
    const evC = decodeEvent("PoolCancelled", (await emittedEvents(sigC))[0]!, poolCancelledDecoder);
    expect(evC.pool).toBe(M2.pool);
    let p = await fetchPool(M2.pool);
    expect([p.status, p.cancelledByAdmin]).toEqual([PoolStatus.Returned, true]);
    const sigC3 = await send(admin, await cancelPoolInstruction(admin, M3, { counter: true }));
    measured["cancel_pool_open"] = await computeUnitsConsumed(sigC3);
    expect(await openCount(creator.address, GM)).toBe(0);
    expect(await sendExpectingError(admin, await cancelPoolInstruction(admin, M2))).toBe(
      errorCode("NotReturnable"), // 6046
    );

    await send(admin, await markGameInstruction(admin, GM, GameStatus.Postponed));

    // M1: every owner exactly their boxes at the price; the sponsorship stays until returned.
    const before = await Promise.all(owners.map((o) => fetchLamports(o)));
    const pool1 = await fetchPool(M1.pool);
    const boxesOf = (o: Address) => BigInt(pool1.owners.filter((x) => x === o).length);
    await returnBoxes(M1, owners, { measure: "return_boxes_drawn_sol_3_owners" });
    const after = await Promise.all(owners.map((o) => fetchLamports(o)));
    for (const [i, o] of owners.entries()) {
      expect(after[i]! - before[i]!, o).toBe(boxesOf(o) * 50_000_000n);
    }
    expect(await fetchLamports(await vaultPda(M1.pool))).toBe(rent.vault + SOL);
    expect(
      await sendExpectingError(stranger, await closePoolInstruction(stranger, M1, feeWallet)),
    ).toBe(errorCode("SponsorshipsStillOpen")); // 6053
    const sp0 = await fetchLamports(sponsor.address);
    await send(keeper, await returnSponsorshipInstruction(keeper, M1, sponsor.address));
    expect((await fetchLamports(sponsor.address)) - sp0).toBe(SOL + rent.sponsorship);
    expect((await closePool(M1)).dust).toBe(0n);

    // M2 (cancelled, Locked) and M3 (cancelled, Open): the cancelled branch pays everyone.
    await returnBoxes(M2, owners, { measure: "return_boxes_cancelled_locked" });
    await returnBoxes(M3, [creator.address], { measure: "return_boxes_cancelled_open" });
    expect((await fetchPool(M2.pool)).returned).toBe(0x1ff_ffff);
    expect(popcount((await fetchPool(M3.pool)).returned)).toBe(5);
    await closePool(M2);
    await closePool(M3);

    // A marked game accepts no new pool (Step 4's check, now reached through a mark).
    expect(
      await sendExpectingError(
        creator,
        await createPoolInstruction(creator, GM, poolParams({ nonce: nextNonce++, price: PRICE })),
      ),
    ).toBe(errorCode("GameNotScheduled")); // 6002
  }, 600_000);

  it("3. suspended before any payout: S1 returns in full, split refused", async () => {
    const { game: GS1 } = await newGame(2n);
    const S1 = await drawnPoolScenario(scenario(GS1), { creatorAddonBps: 200 });
    await send(sponsor, await sponsorInstruction(sponsor, S1, SOL));
    await send(admin, await markGameInstruction(admin, GS1, GameStatus.Suspended));
    const owners = [creator.address, buyerA.address, buyerB.address];
    expect(await splitError(S1, owners)).toBe(errorCode("NotSplittable")); // 6049
    const before = await Promise.all(owners.map((o) => fetchLamports(o)));
    await returnBoxes(S1, owners, { measure: "return_boxes_suspended_drawn" });
    const after = await Promise.all(owners.map((o) => fetchLamports(o)));
    expect(after.map((a, i) => a - before[i]!)).toEqual([250_000_000n, 500_000_000n, 500_000_000n]);
    const sp0 = await fetchLamports(sponsor.address);
    await send(keeper, await returnSponsorshipInstruction(keeper, S1, sponsor.address));
    expect((await fetchLamports(sponsor.address)) - sp0).toBe(SOL + rent.sponsorship);
    expect((await closePool(S1)).dust).toBe(0n);
  }, 600_000);

  it("4. suspended after Q1: T1 splits 67,200,000 per box with 11 dust, T2 ORE splits, T3 FinalOnly returns", async () => {
    const { game: GS2, kickoff } = await newGame(2n);
    const s = scenario(GS2);
    const T1 = await drawnPoolScenario(s, { creatorAddonBps: 200 });
    await send(sponsor, await sponsorInstruction(sponsor, T1, DUST_SPONSORSHIP));
    // ORE: the creator spent 5 on U2 (5 left), buyerA holds the 15 returned by U2 → set to 10.
    await withRetry(() =>
      localnet.setTokenAccount(buyerA.address, ORE_MINT, { amount: Number(10n * PRICE_ORE) }),
    );
    const T2 = await drawnPoolScenario(s, { creatorAddonBps: 200 }, ORE);
    const T3 = await drawnPoolScenario(s, { creatorAddonBps: 200, preset: 2 /* FinalOnly */ });
    const owners = [creator.address, buyerA.address, buyerB.address];

    await travelTo(kickoff + 16n * MINUTE, "GS2 Q1");
    await postScores(GS2, 1);
    const q1 = await settle(T1, 1);
    expect(q1.event.amount).toBe(420_000_002n); // 20 % of 2,100,000,013
    const q1Ore = await settle(T2, 1, ORE);
    expect(q1Ore.event.amount).toBe(22_000_000_000n);
    const q1Final = await settle(T3, 1);
    expect(q1Final.event.amount).toBe(0n);
    await send(admin, await markGameInstruction(admin, GS2, GameStatus.Suspended));

    // T1: the fees are paid, so no cancel, no return, no sponsorship return — a split.
    expect(await sendExpectingError(admin, await cancelPoolInstruction(admin, T1))).toBe(
      errorCode("FeesAlreadyPaid"), // 6047
    );
    expect(await returnBoxesError(T1, owners)).toBe(errorCode("NotReturnable")); // 6046
    expect(
      await sendExpectingError(
        keeper,
        await returnSponsorshipInstruction(keeper, T1, sponsor.address),
      ),
    ).toBe(errorCode("FeesAlreadyPaid")); // 6047
    const c0 = await fetchLamports(creator.address);
    const a0 = await fetchLamports(buyerA.address);
    const sig = await split(T1, [creator.address, buyerA.address], {
      measure: "split_first_sol_2_owners",
    });
    let p = await fetchPool(T1.pool);
    expect(p.splitAmount).toBe(67_200_000n);
    expect(p.status).toBe(PoolStatus.Split);
    expect((await fetchLamports(creator.address)) - c0).toBe(5n * 67_200_000n);
    expect((await fetchLamports(buyerA.address)) - a0).toBe(10n * 67_200_000n);
    const evs = (await emittedEvents(sig)).map((e) =>
      decodeEvent("BoxesSplit", e, boxesSplitDecoder),
    );
    expect(evs.map((e) => e.amount)).toEqual([5n * 67_200_000n, 10n * 67_200_000n]);
    const b0 = await fetchLamports(buyerB.address);
    await split(T1, [buyerB.address], { measure: "split_continuation_sol_1_owner" });
    expect((await fetchLamports(buyerB.address)) - b0).toBe(10n * 67_200_000n);
    expect(await splitError(T1, [creator.address, buyerA.address])).toBe(
      errorCode("NothingToReturn"), // 6052
    );
    p = await fetchPool(T1.pool);
    expect(p.unpaidPrizePool).toBe(11n);
    expect(await fetchLamports(await vaultPda(T1.pool))).toBe(rent.vault + 11n);
    expect(
      await sendExpectingError(keeper, [
        setComputeUnitLimit(CU_LIMIT),
        await settleInstruction(keeper, T1, 2, q1.winner, feeWallet),
      ]),
    ).toBe(errorCode("PoolNotDrawn")); // 6028
    expect(
      await sendExpectingError(stranger, await closePoolInstruction(stranger, T1, feeWallet)),
    ).toBe(errorCode("SponsorshipsStillOpen")); // 6053
    const sp0 = await fetchLamports(sponsor.address);
    const sigS = await send(stranger, await closeSponsorshipInstruction(T1, sponsor.address));
    measured["close_sponsorship"] = await computeUnitsConsumed(sigS);
    expect((await fetchLamports(sponsor.address)) - sp0).toBe(rent.sponsorship);
    expect((await fetchPool(T1.pool)).sponsorshipsOpen).toBe(0);
    const evS = decodeEvent(
      "SponsorshipClosed",
      (await emittedEvents(sigS))[0]!,
      sponsorshipClosedDecoder,
    );
    expect([evS.sponsor, evS.amount]).toEqual([sponsor.address, DUST_SPONSORSHIP]);
    const f0 = await fetchLamports(feeWallet);
    const closed = await closePool(T1, { measure: "close_pool_split_sol" });
    expect(closed.dust).toBe(11n);
    expect((await fetchLamports(feeWallet)) - f0).toBe(rent.vault + 11n + rent.pool);

    // T2 (ORE): the Q1 winner empties and closes their ATA; the split recreates it.
    const winnerAta = await ata(q1Ore.winner, ORE_MINT);
    await withRetry(() => localnet.setTokenAccount(q1Ore.winner, ORE_MINT, { amount: 0 }));
    const winnerSigner = [creator, buyerA, buyerB].find((w) => w.address === q1Ore.winner)!;
    await send(winnerSigner, closeTokenAccountInstruction(winnerSigner, winnerAta, q1Ore.winner));
    expect(await fetchAccountData(winnerAta)).toBeNull();
    const oreBefore = await Promise.all(
      owners.map(async (o) => (await tokenAmount(await ata(o, ORE_MINT))) ?? 0n),
    );
    const splitIxs = [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await splitInstruction(keeper, T2, owners, { spl: ORE }),
    ];
    const splitWire = wireTransaction(await buildTransaction(keeper, splitIxs));
    profiles[`T2 ORE split (${splitWire.bytes} bytes)`] = await localnet.profileTransaction(
      splitWire.base64,
      "t2-split",
    );
    await split(T2, owners, { spl: ORE, measure: "split_ore_3_owners_one_create" });
    p = await fetchPool(T2.pool);
    expect(p.splitAmount).toBe(3_520_000_000n);
    const pool2 = p;
    for (const [i, o] of owners.entries()) {
      const boxes = BigInt(pool2.owners.filter((x) => x === o).length);
      expect((await tokenAmount(await ata(o, ORE_MINT)))! - oreBefore[i]!, o).toBe(
        boxes * 3_520_000_000n,
      );
    }
    expect((await closePool(T2, { spl: ORE })).dust).toBe(0n);

    // T3 (FinalOnly, fees not paid after Q1): a split is refused, a return pays the price.
    expect(await splitError(T3, owners)).toBe(errorCode("NotSplittable")); // 6049
    const before = await Promise.all(owners.map((o) => fetchLamports(o)));
    await returnBoxes(T3, owners, { measure: "return_boxes_final_only_after_q1" });
    const after = await Promise.all(owners.map((o) => fetchLamports(o)));
    expect(after.map((a, i) => a - before[i]!)).toEqual([250_000_000n, 500_000_000n, 500_000_000n]);
    await closePool(T3);
  }, 900_000);

  it("5. dust after a full settlement: D1 closes its sponsorship, then the pool sweeps 2 lamports", async () => {
    const { game: GD, kickoff } = await newGame(2n);
    const D1 = await drawnPoolScenario(scenario(GD), { creatorAddonBps: 200 });
    await send(sponsor, await sponsorInstruction(sponsor, D1, DUST_SPONSORSHIP));
    for (let q = 1; q <= 4; q++) {
      await travelTo(kickoff + BigInt(q) * 16n * MINUTE, `GD Q${q}`);
      await postScores(GD, q);
      await settle(D1, q);
    }
    const p = await fetchPool(D1.pool);
    expect(p.quarterPrize).toEqual([420_000_002n, 420_000_002n, 420_000_002n, 840_000_005n]);
    expect(p.status).toBe(PoolStatus.Settled);
    expect(p.unpaidPrizePool).toBe(2n);
    expect(
      await sendExpectingError(stranger, await closePoolInstruction(stranger, D1, feeWallet)),
    ).toBe(errorCode("SponsorshipsStillOpen")); // 6053
    const sp0 = await fetchLamports(sponsor.address);
    await send(stranger, await closeSponsorshipInstruction(D1, sponsor.address));
    expect((await fetchLamports(sponsor.address)) - sp0).toBe(rent.sponsorship);
    const f0 = await fetchLamports(feeWallet);
    const closed = await closePool(D1);
    expect(closed.dust).toBe(2n);
    expect((await fetchLamports(feeWallet)) - f0).toBe(rent.vault + 2n + rent.pool);
  }, 600_000);

  it("6. compute units and profiles: the 12-owner SOL page under 200k, the ORE split under 150k", () => {
    for (const [name, cu] of Object.entries(measured)) console.log(`${name} consumed ${cu} CU`);
    console.log(`time travel targets: ${travelled.join("; ")}`);
    for (const n of notes) console.log(n);
    for (const [name, profile] of Object.entries(profiles)) {
      console.log(`profile ${name}: ${JSON.stringify(profile)}`);
    }
    expect(measured["return_boxes_sol_12_owners_page_1"]!).toBeLessThan(200_000n);
    expect(measured["return_boxes_sol_12_owners_page_2"]!).toBeLessThan(200_000n);
    expect(measured["split_ore_3_owners_one_create"]!).toBeLessThan(150_000n);
  });

  it("7. the IDL has 26 instructions, 6 accounts, 24 events, 65 errors, docs on each new item", () => {
    expect(IDL.instructions).toHaveLength(26);
    expect(IDL.accounts).toHaveLength(6);
    expect(IDL.events).toHaveLength(24);
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
    for (const name of [
      "return_boxes",
      "return_sponsorship",
      "cancel_pool",
      "split",
      "reclaim",
      "reclaim_sponsorship",
      "close_sponsorship",
    ]) {
      const ix = idl.instructions.find((i) => i.name === name);
      expect(ix, name).toBeDefined();
      expect(ix?.docs?.length ?? 0, name).toBeGreaterThan(0);
      for (const account of ix!.accounts.filter(
        (a) => a.name !== "event_authority" && a.name !== "program",
      )) {
        expect(account.docs?.length ?? 0, `${name}.${account.name}`).toBeGreaterThan(0);
      }
    }
    for (const name of [
      "BoxesReturned",
      "BoxesSplit",
      "BoxesReclaimed",
      "SponsorshipReturned",
      "SponsorshipClosed",
      "PoolCancelled",
    ]) {
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
