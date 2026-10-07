/**
 * Step 5 localnet suite: the draw against the real Regolith Entropy bytecode
 * forked from mainnet (`anchor test`). `config.test.ts` initialised the
 * config. One `describe`, ordered.
 *
 * The deployed Entropy program refuses `Open`, so every `Var` is planted with
 * `surfnet_setAccount` in the state `Open` would have left it in, and the real
 * `Sample`, `Reveal`, `Next` and `Close` run against it. Slots: `end_at` is a
 * few slots ahead so the happy path waits for the window; `timeTravel` is used
 * only to jump past one.
 */
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import {
  drawAxes,
  entropyValue,
  fallbackHash,
  INITIAL_LADDERS,
  keccak,
  type Var,
} from "@mybarpool/shared";
import {
  address,
  airdropFactory,
  appendTransactionMessageInstruction,
  createKeyPairSignerFromBytes,
  createTransactionMessage,
  generateKeyPairSigner,
  getAddressDecoder,
  getAddressEncoder,
  getBase64EncodedWireTransaction,
  getProgramDerivedAddress,
  lamports,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { Localnet } from "../scripts/localnet.js";
import {
  closeInstruction,
  currentSlot,
  entropyVarPda,
  fetchVar,
  hex,
  nextInstruction,
  openInstruction,
  plantVar,
  rentForVar,
  revealInstruction,
  sampleInstruction,
  slotHashFor,
  slotHashes,
  waitForSlot,
} from "./helpers/entropy.js";
import {
  BPF_LOADER_UPGRADEABLE,
  buyInstruction,
  chainNow,
  COMPUTE_BUDGET_PROGRAM,
  computeUnitsConsumed,
  configPda,
  createGameInstruction,
  createPoolInstruction,
  decodeAccount,
  decodeEvent,
  digitsDrawnDecoder,
  drawInstruction,
  emittedEvents,
  ENTROPY_PROGRAM_ADDRESS,
  errorCode,
  eventName,
  fetchAccountData,
  fetchLamports,
  gamePda,
  IDL,
  poolDecoder,
  poolParams,
  poolPda,
  PoolStatus,
  replaceVarInstruction,
  rpc,
  rpcSubscriptions,
  sampleVarInstruction,
  send,
  sendExpectingError,
  setComputeUnitLimit,
  setVarInstruction,
  SLOT_HASHES_SYSVAR,
  sponsorInstruction,
  SYSTEM_PROGRAM,
  updateConfigInstruction,
  varReplacedDecoder,
  varSampledDecoder,
  varSetDecoder,
  withRetry,
  type GameKey,
  type PoolRefs,
} from "./helpers/mybarpool.js";

const SOL = 1_000_000_000n;
const HOUR = 3_600n;
const PRICE = INITIAL_LADDERS.SOL.minPrice;
/** The Entropy provider on mainnet (the live ORE `Var`'s, the brief's `PROVIDER`). */
const PROVIDER = address("AKBXJ7jQ2DiqLQKzgPn791r1ZVNvLchTFH6kpesPAAWF");
/** The live ORE board `Var` and its authority (fetched 2026-10-07 at slot 454,331,258). */
const LIVE_VAR = address("BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E");
const LIVE_VAR_AUTHORITY = address("BrcSxdp1nXFzou1YyDnQJcPNBNHgoypZmTsyKBSLLXzi");
/** verify.osec.io `on_chain_hash` for the deployed program at `f26ae03`. */
const ENTROPY_ELF_SHA256 = "b64ffdfef7bb05839fbe0d24196697cc20de6bc72081031bc0523699181b18b1";
/** PROGRAM §2 team table: KC hosting DAL in week 3 of 2026. */
const KEY: GameKey = { season: 2026, week: 3, home: 15, away: 8 };
/** Slots ahead for a planted `end_at`: the happy path waits for it (≈ 3 s at 200 ms slots). */
const WINDOW_AHEAD = 15n;
const CU_LIMIT = 400_000;

const localnet = new Localnet();
const enc = getAddressEncoder();
const dec = getAddressDecoder();
const ZERO32 = new Uint8Array(32);
const seedOf = (byte: number) => new Uint8Array(32).fill(byte);

function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function stripTrailingZeros(bytes: Uint8Array): Uint8Array {
  let end = bytes.length;
  while (end > 0 && bytes[end - 1] === 0) end--;
  return bytes.subarray(0, end);
}

async function fetchPool(pool: Address) {
  const data = await fetchAccountData(pool);
  expect(data).not.toBeNull();
  expect(data!.length).toBe(1442); // PROGRAM §3.3
  return decodeAccount("Pool", data!, poolDecoder);
}

describe("draw (Surfpool, mainnet fork, real Entropy bytecode)", () => {
  let admin: KeyPairSigner;
  let keeper: KeyPairSigner;
  let creator: KeyPairSigner;
  let buyerA: KeyPairSigner;
  let buyerB: KeyPairSigner;
  let stranger: KeyPairSigner;
  let game: Address;
  let nextNonce = 1n;
  let nextVarId = 100n;
  let measuredSampleVarCu: bigint | undefined;

  /** A `Var` as `Open` would have left it: committed to `seed`, one manual sample. */
  function freshVar(
    id: bigint,
    seed: Uint8Array,
    endAt: bigint,
    overrides: Partial<Var> = {},
  ): Var {
    return {
      authority: Uint8Array.from(enc.encode(keeper.address)),
      id,
      provider: Uint8Array.from(enc.encode(PROVIDER)),
      commit: keccak(seed),
      seed: ZERO32,
      slotHash: ZERO32,
      value: ZERO32,
      samples: 1n,
      isAuto: 0n,
      startAt: 0n,
      endAt,
      ...overrides,
    };
  }

  /** Plant a fresh `Var` at the keeper's next id, `endAt` slots ahead unless given. */
  async function plantFresh(
    seed: Uint8Array,
    opts: { endAt?: bigint; ahead?: bigint; overrides?: Partial<Var> } = {},
  ): Promise<{ id: bigint; address: Address; endAt: bigint }> {
    const id = nextVarId++;
    const endAt = opts.endAt ?? (await currentSlot()) + (opts.ahead ?? WINDOW_AHEAD);
    const varAddress = await entropyVarPda(keeper.address, id);
    await plantVar(varAddress, freshVar(id, seed, endAt, opts.overrides));
    return { id, address: varAddress, endAt };
  }

  /** A SOL pool with all 25 boxes sold: creator 5 at creation, buyerA 10, buyerB 10. */
  async function lockedPool(): Promise<PoolRefs> {
    const nonce = nextNonce++;
    await send(
      creator,
      await createPoolInstruction(
        creator,
        game,
        poolParams({ nonce, price: PRICE, initialBoxes: 5 }),
      ),
    );
    const refs: PoolRefs = {
      pool: await poolPda(game, creator.address, nonce),
      game,
      creator: creator.address,
    };
    await send(buyerA, await buyInstruction(buyerA, refs, 10));
    await send(buyerB, await buyInstruction(buyerB, refs, 10));
    expect((await fetchPool(refs.pool)).status).toBe(PoolStatus.Locked);
    return refs;
  }

  async function sampleVar(sampler: KeyPairSigner, pool: Address, varAddress: Address) {
    return send(sampler, [
      setComputeUnitLimit(CU_LIMIT),
      await sampleVarInstruction(sampler, pool, varAddress),
    ]);
  }

  async function sampleVarExpectingError(
    sampler: KeyPairSigner,
    pool: Address,
    varAddress: Address,
  ) {
    return sendExpectingError(sampler, [
      setComputeUnitLimit(CU_LIMIT),
      await sampleVarInstruction(sampler, pool, varAddress),
    ]);
  }

  let p1: PoolRefs;
  let v1: { id: bigint; address: Address; endAt: bigint };
  let p2: PoolRefs;
  let v2: { id: bigint; address: Address; endAt: bigint };
  let p3: PoolRefs;

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
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    for (const who of [keeper, creator, buyerA, buyerB, stranger]) {
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
        entropyProvider: PROVIDER,
      }),
    );
    // First remote touches through withRetry (Step 3 audit M1): the ComputeBudget program
    // (every sample_var carries SetComputeUnitLimit) and the SlotHashes sysvar.
    for (const addr of [COMPUTE_BUDGET_PROGRAM, SLOT_HASHES_SYSVAR]) {
      await withRetry(() => rpc.getAccountInfo(addr, { encoding: "base64" }).send());
    }
    const kickoff = (await chainNow()) + 3n * HOUR;
    game = await gamePda(KEY, kickoff);
    await send(keeper, await createGameInstruction(keeper, KEY, kickoff));
  });

  it("1. the fork serves the verified Entropy bytecode and the live ORE Var decodes", async () => {
    const info = await withRetry(() =>
      rpc.getAccountInfo(ENTROPY_PROGRAM_ADDRESS, { encoding: "base64" }).send(),
    );
    expect(info.value?.executable).toBe(true);
    expect(info.value?.owner).toBe(BPF_LOADER_UPGRADEABLE);

    const [programData] = await getProgramDerivedAddress({
      programAddress: BPF_LOADER_UPGRADEABLE,
      seeds: [enc.encode(ENTROPY_PROGRAM_ADDRESS)],
    });
    const data = await withRetry(() => fetchAccountData(programData));
    expect(data).not.toBeNull();
    // UpgradeableLoaderState::ProgramData header is 45 bytes; the ELF follows, zero-padded.
    const elf = stripTrailingZeros(data!.subarray(45));
    expect(elf.length).toBe(98_929);
    expect(sha256Hex(elf)).toBe(ENTROPY_ELF_SHA256);

    const live = await withRetry(() => fetchVar(LIVE_VAR));
    expect(live).not.toBeNull();
    expect(dec.decode(live!.provider)).toBe(PROVIDER);
    expect(dec.decode(live!.authority)).toBe(LIVE_VAR_AUTHORITY);
    expect(live!.id).toBe(0n);
  });

  it("2. P1: plant → set_var → wait → sample_var (stranger) → Reveal → draw → Close; then draw again and sponsor", async () => {
    p1 = await lockedPool();
    const SEED1 = seedOf(0x11);
    v1 = await plantFresh(SEED1);
    const planted = await fetchVar(v1.address);
    expect(planted).not.toBeNull();
    expect(planted!.commit).toEqual(keccak(SEED1));
    expect(planted!.endAt).toBe(v1.endAt);

    // set_var by the keeper.
    const setSig = await send(keeper, await setVarInstruction(keeper, p1.pool, v1.address));
    let pool = await fetchPool(p1.pool);
    expect(pool.var).toBe(v1.address);
    expect(pool.varEndAt).toBe(v1.endAt);
    expect(pool.sampledSlot).toBe(0n);
    const setEvents = await emittedEvents(setSig);
    expect(setEvents.map(eventName)).toEqual(["VarSet"]);
    const varSet = decodeEvent("VarSet", setEvents[0]!, varSetDecoder);
    expect(varSet.pool).toBe(p1.pool);
    expect(varSet.var).toBe(v1.address);
    expect(varSet.endAt).toBe(v1.endAt);

    // Inside the window: sample_var sent by a stranger (permissionless), CPI into Entropy.
    await waitForSlot(v1.endAt + 1n);
    const sampleSig = await sampleVar(stranger, p1.pool, v1.address);
    measuredSampleVarCu = await computeUnitsConsumed(sampleSig);
    const sampled = (await fetchVar(v1.address))!;
    expect(sampled.slotHash).not.toEqual(ZERO32);
    expect(sampled.slotHash).not.toEqual(fallbackHash(v1.endAt));
    const sysvarHash = await slotHashFor(v1.endAt);
    expect(sysvarHash).toBeDefined();
    expect(sampled.slotHash).toEqual(sysvarHash);
    pool = await fetchPool(p1.pool);
    expect(pool.sampledSlot).toBeGreaterThanOrEqual(v1.endAt + 1n);
    expect(Uint8Array.from(pool.sampledHash)).toEqual(sampled.slotHash);
    expect(pool.status).toBe(PoolStatus.Locked);
    const sampleEvents = await emittedEvents(sampleSig);
    expect(sampleEvents.map(eventName)).toEqual(["VarSampled"]);
    const varSampled = decodeEvent("VarSampled", sampleEvents[0]!, varSampledDecoder);
    expect(varSampled.sampler).toBe(stranger.address);
    expect(varSampled.var).toBe(v1.address);
    expect(varSampled.slot).toBe(pool.sampledSlot);
    expect(varSampled.endAt).toBe(v1.endAt);
    expect(Uint8Array.from(varSampled.slotHash)).toEqual(sampled.slotHash);

    // Reveal by the stranger (permissionless): the real program computes the value.
    await send(stranger, revealInstruction(stranger, v1.address, SEED1));
    const revealed = (await fetchVar(v1.address))!;
    expect(revealed.seed).toEqual(SEED1);
    expect(revealed.value).toEqual(entropyValue(sampled.slotHash, SEED1, 1n));

    // draw by the keeper.
    const drawSig = await send(keeper, await drawInstruction(keeper, p1.pool, v1.address));
    pool = await fetchPool(p1.pool);
    expect(pool.status).toBe(PoolStatus.Drawn);
    expect(pool.drawn).toBe(true);
    const axes = drawAxes(revealed.value);
    expect(pool.homeAxis).toEqual(Array.from(axes.home));
    expect(pool.awayAxis).toEqual(Array.from(axes.away));
    expect(pool.var).toBe(v1.address);
    const drawEvents = await emittedEvents(drawSig);
    expect(drawEvents.map(eventName)).toEqual(["DigitsDrawn"]);
    const digits = decodeEvent("DigitsDrawn", drawEvents[0]!, digitsDrawnDecoder);
    expect(digits.var).toBe(v1.address);
    expect(Uint8Array.from(digits.value)).toEqual(revealed.value);
    expect(digits.homeAxis).toEqual(pool.homeAxis);
    expect(digits.awayAxis).toEqual(pool.awayAxis);

    // Second draw: the pool is Drawn, not Locked (6027).
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, p1.pool, v1.address)),
    ).toBe(errorCode("PoolNotLocked"));

    // Close by the keeper (authority), three accounts; the stranger pays the fee so the
    // keeper's balance moves by exactly the rent.
    const keeperBefore = await fetchLamports(keeper.address);
    await send(stranger, closeInstruction(keeper, v1.address));
    expect(await fetchAccountData(v1.address)).toBeNull();
    expect((await fetchLamports(keeper.address)) - keeperBefore).toBe(await rentForVar());
    // With the Var closed, `draw` fails at the account's owner constraint before the handler
    // sees the status: 6034, not 6027 (the brief's item 2 order gives this; recorded in NOTES).
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, p1.pool, v1.address)),
    ).toBe(errorCode("VarNotEntropy"));
    // Step 4's Drawn sponsorship window, now reachable.
    await send(stranger, await sponsorInstruction(stranger, p1, PRICE));
    expect((await fetchPool(p1.pool)).sponsorCount).toBe(1);
  });

  it("3. P2 negatives: authority, Var shape, freshness, binding, too-early, draw-before-sample", async () => {
    p2 = await lockedPool();
    // A long window: the negatives below take a few slots, and item 4 jumps past it anyway.
    v2 = await plantFresh(seedOf(0x22), { ahead: 300n });
    const setVarBy = async (who: KeyPairSigner, v: Address) =>
      sendExpectingError(who, await setVarInstruction(who, p2.pool, v));

    expect(await setVarBy(admin, v2.address)).toBe(errorCode("Unauthorized")); // 6000
    // The live ORE Var: right provider, but hundreds of thousands of samples.
    expect(await setVarBy(keeper, LIVE_VAR)).toBe(errorCode("VarNotFresh")); // 6036

    const wrongProvider = await plantFresh(seedOf(0x23), {
      ahead: 300n,
      overrides: { provider: Uint8Array.from(enc.encode(stranger.address)) },
    });
    expect(await setVarBy(keeper, wrongProvider.address)).toBe(errorCode("VarProviderMismatch")); // 6035
    const auto = await plantFresh(seedOf(0x24), { ahead: 300n, overrides: { isAuto: 1n } });
    expect(await setVarBy(keeper, auto.address)).toBe(errorCode("VarNotFresh")); // 6036
    const twoSamples = await plantFresh(seedOf(0x25), { ahead: 300n, overrides: { samples: 2n } });
    expect(await setVarBy(keeper, twoSamples.address)).toBe(errorCode("VarNotFresh")); // 6036
    const endsNow = await plantFresh(seedOf(0x26), { endAt: await currentSlot() });
    expect(await setVarBy(keeper, endsNow.address)).toBe(errorCode("VarNotFresh")); // 6036
    // A system-owned 240-byte account with a valid body.
    const systemOwned = await entropyVarPda(keeper.address, nextVarId++);
    await plantVar(systemOwned, freshVar(0n, seedOf(0x27), (await currentSlot()) + 300n));
    await localnet.setAccount(systemOwned, { owner: SYSTEM_PROGRAM });
    expect(await setVarBy(keeper, systemOwned)).toBe(errorCode("VarNotEntropy")); // 6034

    await send(keeper, await setVarInstruction(keeper, p2.pool, v2.address));
    expect((await fetchPool(p2.pool)).var).toBe(v2.address);
    expect(await setVarBy(keeper, v2.address)).toBe(errorCode("VarAlreadySet")); // 6031
    expect(await currentSlot()).toBeLessThan(v2.endAt);
    expect(await sampleVarExpectingError(keeper, p2.pool, v2.address)).toBe(
      errorCode("SampleTooEarly"), // 6062
    );
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, p2.pool, v2.address)),
    ).toBe(errorCode("VarNotSampledHere")); // 6038
    expect(await sampleVarExpectingError(keeper, p2.pool, auto.address)).toBe(
      errorCode("VarMismatch"), // 6033
    );
  });

  it("4. P2 window missed, replace_var, Reveal/Next on the replacement, the cap on P5", async () => {
    const before = await currentSlot();
    const target = v2.endAt + 600n;
    expect(target).toBeGreaterThan(before);
    await localnet.timeTravel({ absoluteSlot: Number(target) });
    // Right after the jump the confirmed slot reads target − 1 (the processed one is target).
    expect(await currentSlot()).toBeGreaterThanOrEqual(target - 1n);
    const after = await waitForSlot(target);
    expect(after).toBeGreaterThanOrEqual(target);
    // What the sysvar holds after the jump is recorded in NOTES; the test needs only that
    // `end_at` is gone from it (it is older than 512 slots).
    const entries = await slotHashes();
    expect(entries.find((e) => e.slot === v2.endAt)).toBeUndefined();

    expect(await sampleVarExpectingError(keeper, p2.pool, v2.address)).toBe(
      errorCode("SampleWindowMissed"), // 6039
    );
    // The CPI wrote the fallback; the failed transaction rolled it back.
    expect((await fetchVar(v2.address))!.slotHash).toEqual(ZERO32);
    expect((await fetchPool(p2.pool)).sampledSlot).toBe(0n);

    const replaceBy = async (who: KeyPairSigner, pool: Address, v: Address) =>
      sendExpectingError(who, await replaceVarInstruction(who, pool, v));
    const v3 = await plantFresh(seedOf(0x33));
    expect(await replaceBy(keeper, p2.pool, v3.address)).toBe(errorCode("Unauthorized")); // 6000
    expect(await replaceBy(admin, p1.pool, v3.address)).toBe(errorCode("PoolNotLocked")); // 6027

    const replaceSig = await send(admin, await replaceVarInstruction(admin, p2.pool, v3.address));
    let pool = await fetchPool(p2.pool);
    expect(pool.var).toBe(v3.address);
    expect(pool.varEndAt).toBe(v3.endAt);
    expect(pool.sampledSlot).toBe(0n);
    expect(pool.varReplacements).toBe(1);
    const replaceEvents = await emittedEvents(replaceSig);
    expect(replaceEvents.map(eventName)).toEqual(["VarReplaced"]);
    const replaced = decodeEvent("VarReplaced", replaceEvents[0]!, varReplacedDecoder);
    expect(replaced.oldVar).toBe(v2.address);
    expect(replaced.newVar).toBe(v3.address);
    expect(replaced.replacements).toBe(1);

    await waitForSlot(v3.endAt + 1n);
    await sampleVar(keeper, p2.pool, v3.address);
    pool = await fetchPool(p2.pool);
    expect(pool.sampledSlot).toBeGreaterThan(0n);
    const sampledHash = Uint8Array.from(pool.sampledHash);
    expect((await fetchVar(v3.address))!.slotHash).toEqual(sampledHash);

    // Reveal with the wrong seed: Entropy refuses (its own error, not one of ours).
    await expect(
      send(stranger, revealInstruction(stranger, v3.address, seedOf(0x34))),
    ).rejects.toBeTruthy();
    await send(stranger, revealInstruction(stranger, v3.address, seedOf(0x33)));
    expect((await fetchVar(v3.address))!.value).toEqual(
      entropyValue(sampledHash, seedOf(0x33), 1n),
    );

    // Next by the authority rolls the Var: commit = the revealed seed, zero hash/value, samples 0.
    await send(keeper, nextInstruction(keeper, v3.address, v3.endAt + 100n));
    const rolled = (await fetchVar(v3.address))!;
    expect(rolled.commit).toEqual(seedOf(0x33));
    expect(rolled.slotHash).toEqual(ZERO32);
    expect(rolled.value).toEqual(ZERO32);
    expect(rolled.samples).toBe(0n);
    // The pool keeps its verified sample; a rolled Var is an incident, not a re-roll.
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, p2.pool, v3.address)),
    ).toBe(errorCode("VarNotSampledHere")); // 6038
    const v4 = await plantFresh(seedOf(0x44));
    expect(await replaceBy(admin, p2.pool, v4.address)).toBe(errorCode("VarAlreadySampled")); // 6063

    // The cap on P5: Vars left unsampled, two replacements, a third refused.
    const p5 = await lockedPool();
    const v5a = await plantFresh(seedOf(0x51), { ahead: 300n });
    await send(keeper, await setVarInstruction(keeper, p5.pool, v5a.address));
    const v5b = await plantFresh(seedOf(0x52), { ahead: 300n });
    await send(admin, await replaceVarInstruction(admin, p5.pool, v5b.address));
    const v5c = await plantFresh(seedOf(0x53), { ahead: 300n });
    await send(admin, await replaceVarInstruction(admin, p5.pool, v5c.address));
    expect((await fetchPool(p5.pool)).varReplacements).toBe(2);
    const v5d = await plantFresh(seedOf(0x54), { ahead: 300n });
    expect(await replaceBy(admin, p5.pool, v5d.address)).toBe(errorCode("TooManyVarReplacements")); // 6041
  });

  it("5. P3 third-party Sample passes sample_var without a CPI; P4 forged fallback is refused twice", async () => {
    p3 = await lockedPool();
    const v6 = await plantFresh(seedOf(0x66));
    await send(keeper, await setVarInstruction(keeper, p3.pool, v6.address));
    await waitForSlot(v6.endAt + 1n);
    await send(stranger, sampleInstruction(stranger, v6.address));
    const thirdParty = (await fetchVar(v6.address))!;
    expect(thirdParty.slotHash).not.toEqual(ZERO32);
    const sig = await sampleVar(keeper, p3.pool, v6.address);
    expect((await fetchVar(v6.address))!.slotHash).toEqual(thirdParty.slotHash);
    const pool3 = await fetchPool(p3.pool);
    expect(Uint8Array.from(pool3.sampledHash)).toEqual(thirdParty.slotHash);
    // No Entropy CPI: the only inner instruction is the event CPI.
    const tx = await rpc
      .getTransaction(sig, { maxSupportedTransactionVersion: 0, encoding: "json" })
      .send();
    const inner = (tx?.meta?.innerInstructions ?? []).flatMap((g) => g.instructions);
    expect(inner).toHaveLength(1);
    // Carried from item 4: replace_var after a verified sample.
    const v6b = await plantFresh(seedOf(0x67));
    expect(
      await sendExpectingError(admin, await replaceVarInstruction(admin, p3.pool, v6b.address)),
    ).toBe(errorCode("VarAlreadySampled")); // 6063

    // P4: a Var carrying the fallback hash and a consistent reveal.
    const p4 = await lockedPool();
    const SEED7 = seedOf(0x77);
    const v7 = await plantFresh(SEED7);
    await send(keeper, await setVarInstruction(keeper, p4.pool, v7.address));
    await waitForSlot(v7.endAt + 1n);
    const fallback = fallbackHash(v7.endAt);
    await plantVar(
      v7.address,
      freshVar(v7.id, SEED7, v7.endAt, {
        slotHash: fallback,
        seed: SEED7,
        value: entropyValue(fallback, SEED7, 1n),
      }),
    );
    expect(await sampleVarExpectingError(keeper, p4.pool, v7.address)).toBe(
      errorCode("SampleWindowMissed"), // 6039
    );
    // Forge the pool's sample to match (raw patch at the §3.3 offsets 1166 / 1174).
    const raw = (await fetchAccountData(p4.pool))!;
    const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    view.setBigUint64(1166, 1n, true);
    raw.set(fallback, 1174);
    await localnet.setAccount(p4.pool, { data: hex(raw) });
    const forged = await fetchPool(p4.pool);
    expect(forged.sampledSlot).toBe(1n);
    expect(Uint8Array.from(forged.sampledHash)).toEqual(fallback);
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, p4.pool, v7.address)),
    ).toBe(errorCode("VarFallbackHash")); // 6040
  });

  it("6. Open canary: the deployed Entropy program refuses a correct Open", async () => {
    // If this passes, Regolith re-enabled `Open`; tell the auditor before doing anything else.
    const id = nextVarId++;
    const slot = await currentSlot();
    const open = await openInstruction(
      keeper,
      keeper,
      id,
      PROVIDER,
      keccak(seedOf(0x99)),
      false,
      1n,
      slot + 50n,
    );
    const { value: blockhash } = await rpc.getLatestBlockhash().send();
    const tx = await signTransactionMessageWithSigners(
      pipe(
        createTransactionMessage({ version: 0 }),
        (m) => setTransactionMessageFeePayerSigner(keeper, m),
        (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
        (m) => appendTransactionMessageInstruction(open, m),
      ),
    );
    const sim = await rpc
      .simulateTransaction(getBase64EncodedWireTransaction(tx), {
        encoding: "base64",
        commitment: "confirmed",
      })
      .send();
    expect(sim.value.err).toEqual({ InstructionError: [0n, "InvalidInstructionData"] });
    expect(sim.value.logs?.some((l) => l.includes("Invalid instruction"))).toBe(true);
    expect(await fetchVar(await entropyVarPda(keeper.address, id))).toBeNull();
  });

  it("7. sample_var with the Entropy CPI stays under 250k CU", () => {
    expect(measuredSampleVarCu).toBeDefined();
    console.log(`sample_var (CPI) consumed ${measuredSampleVarCu} CU`);
    expect(measuredSampleVarCu!).toBeLessThan(250_000n);
  });

  it("8. the IDL has 17 instructions, 6 accounts, 16 events, 64 errors, docs on every Step 5 item", () => {
    expect(IDL.instructions).toHaveLength(17);
    expect(IDL.accounts).toHaveLength(6);
    expect(IDL.events).toHaveLength(16);
    expect(IDL.errors).toHaveLength(64);
    const idl = IDL as unknown as {
      instructions: {
        name: string;
        docs?: string[];
        accounts: { name: string; docs?: string[] }[];
      }[];
      events: { name: string }[];
      types: { name: string; docs?: string[] }[];
      errors: { name: string; msg?: string }[];
    };
    for (const name of ["set_var", "sample_var", "draw", "replace_var"]) {
      const ix = idl.instructions.find((i) => i.name === name);
      expect(ix?.docs?.length ?? 0, name).toBeGreaterThan(0);
      // `event_authority` and `program` are `#[event_cpi]`'s own accounts, without docs by design.
      for (const account of ix!.accounts.filter(
        (a) => a.name !== "event_authority" && a.name !== "program",
      )) {
        expect(account.docs?.length ?? 0, `${name}.${account.name}`).toBeGreaterThan(0);
      }
    }
    for (const name of ["VarSet", "VarSampled", "VarReplaced", "DigitsDrawn"]) {
      expect(
        idl.events.some((e) => e.name === name),
        name,
      ).toBe(true);
      expect(idl.types.find((t) => t.name === name)?.docs?.length ?? 0, name).toBeGreaterThan(0);
    }
    for (const name of ["SampleTooEarly", "VarAlreadySampled"]) {
      expect(idl.errors.find((e) => e.name === name)?.msg ?? "", name).not.toBe("");
    }
    expect(IDL.errors.find((e) => e.name === "SampleTooEarly")?.code).toBe(6062);
    expect(IDL.errors.find((e) => e.name === "VarAlreadySampled")?.code).toBe(6063);
  });
});
