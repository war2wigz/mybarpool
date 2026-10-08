/**
 * Step 5 / 5b localnet suite: the draw against the platform's own Entropy
 * deployment — the fork fixture preloaded at `ENTROPY_PROGRAM` by
 * `config.test.ts` — on Surfpool forking mainnet (`anchor test`). One
 * `describe`, ordered.
 *
 * Since Step 5b `Open` works, so the real flow runs with nothing planted:
 * `Open` → `set_var` → `sample_var` → `Reveal` → `draw` → `Close`. `Var`s are
 * still planted with `surfnet_setAccount` for the adversarial states (a seed
 * that matches a different commit, an edited `samples`, the forged fallback,
 * the shape negatives). Slots: `end_at` is a few slots ahead so the happy
 * paths wait for the window; `timeTravel` is used only to jump past one.
 */
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
  createKeyPairSignerFromBytes,
  generateKeyPairSigner,
  getAddressDecoder,
  getAddressEncoder,
  lamports,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { Localnet } from "../scripts/localnet.js";
import {
  closeInstruction,
  currentSlot,
  deployedElf,
  ENTROPY_FORK_EXECUTABLE_HASH,
  entropyVarPda,
  fetchVar,
  hex,
  openInstruction,
  plantVar,
  REGOLITH_ENTROPY_EXECUTABLE_HASH,
  rentForVar,
  revealInstruction,
  sha256Hex,
  slotHashes,
  slotHashFor,
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
  REGOLITH_ENTROPY_PROGRAM_ADDRESS,
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
/** The live ORE board `Var` under Regolith's program, its authority and provider (Step 5). */
const LIVE_VAR = address("BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E");
const LIVE_VAR_AUTHORITY = address("BrcSxdp1nXFzou1YyDnQJcPNBNHgoypZmTsyKBSLLXzi");
const LIVE_VAR_PROVIDER = address("AKBXJ7jQ2DiqLQKzgPn791r1ZVNvLchTFH6kpesPAAWF");
/** PROGRAM §2 team table: KC hosting DAL in week 3 of 2026. */
const KEY: GameKey = { season: 2026, week: 3, home: 15, away: 8 };
/** Slots ahead for an `end_at`: the happy paths wait for it (≈ 3 s at 200 ms slots). */
const WINDOW_AHEAD = 15n;
const CU_LIMIT = 400_000;

const localnet = new Localnet();
const enc = getAddressEncoder();
const dec = getAddressDecoder();
const ZERO32 = new Uint8Array(32);
const seedOf = (byte: number) => new Uint8Array(32).fill(byte);

async function fetchPool(pool: Address) {
  const data = await fetchAccountData(pool);
  expect(data).not.toBeNull();
  expect(data!.length).toBe(1442); // PROGRAM §3.3
  return decodeAccount("Pool", data!, poolDecoder);
}

describe("draw (Surfpool, the platform's Entropy deployment preloaded from the fork)", () => {
  let admin: KeyPairSigner;
  let keeper: KeyPairSigner;
  let creator: KeyPairSigner;
  let buyerA: KeyPairSigner;
  let buyerB: KeyPairSigner;
  let stranger: KeyPairSigner;
  let game: Address;
  let nextNonce = 1n;
  let nextVarId = 100n;
  const measured: Record<string, bigint> = {};

  type Opened = { id: bigint; address: Address; endAt: bigint; seed: Uint8Array };

  /** A `Var` as `Open` leaves it: committed to `seed`, one manual sample, provider the keeper. */
  function freshVar(
    id: bigint,
    seed: Uint8Array,
    endAt: bigint,
    overrides: Partial<Var> = {},
  ): Var {
    return {
      authority: Uint8Array.from(enc.encode(keeper.address)),
      id,
      provider: Uint8Array.from(enc.encode(keeper.address)),
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

  /** Plant a fresh `Var` at the keeper's next id (the adversarial-state path). */
  async function plantFresh(
    seed: Uint8Array,
    opts: { endAt?: bigint; ahead?: bigint; overrides?: Partial<Var> } = {},
  ): Promise<Opened> {
    const id = nextVarId++;
    const endAt = opts.endAt ?? (await currentSlot()) + (opts.ahead ?? WINDOW_AHEAD);
    const varAddress = await entropyVarPda(keeper.address, id);
    await plantVar(varAddress, freshVar(id, seed, endAt, opts.overrides));
    return { id, address: varAddress, endAt, seed };
  }

  /**
   * The real thing: Entropy `Open` on the platform's deployment, by `authority` (also the
   * payer), provider `provider`, `commit = keccak(seed)`, one manual sample, `end_at` ahead.
   */
  async function openFresh(
    authority: KeyPairSigner,
    provider: Address,
    seed: Uint8Array,
    opts: { ahead?: bigint } = {},
  ): Promise<Opened> {
    const id = nextVarId++;
    const endAt = (await currentSlot()) + (opts.ahead ?? WINDOW_AHEAD);
    await send(
      authority,
      await openInstruction(authority, authority, id, provider, keccak(seed), false, 1n, endAt),
    );
    const varAddress = await entropyVarPda(authority.address, id);
    const opened = await fetchVar(varAddress);
    expect(opened).not.toBeNull();
    expect(opened!.commit).toEqual(keccak(seed));
    expect(opened!.endAt).toBe(endAt);
    return { id, address: varAddress, endAt, seed };
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

  /** `Open` → `set_var` → wait → `sample_var` by the stranger on a fresh locked pool. */
  async function openBindAndSample(seed: Uint8Array): Promise<{ refs: PoolRefs; v: Opened }> {
    const refs = await lockedPool();
    const v = await openFresh(keeper, keeper.address, seed);
    await send(keeper, await setVarInstruction(keeper, refs.pool, v.address));
    expect(Uint8Array.from((await fetchPool(refs.pool)).varCommit)).toEqual(keccak(seed));
    await waitForSlot(v.endAt + 1n);
    await sampleVar(stranger, refs.pool, v.address);
    const pool = await fetchPool(refs.pool);
    expect(pool.sampledSlot).toBeGreaterThan(0n);
    expect(Uint8Array.from(pool.sampledHash)).toEqual((await fetchVar(v.address))!.slotHash);
    return { refs, v };
  }

  let p1: PoolRefs;

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
    // The keeper is the provider (ARCHITECTURE › Randomness, Step 5b).
    await send(
      admin,
      await updateConfigInstruction(admin, {
        scoreAuthority: keeper.address,
        entropyProvider: keeper.address,
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

  it("1. ENTROPY_PROGRAM is the fork's bytecode; Regolith's deployment and the live ORE Var are still there", async () => {
    const info = await rpc.getAccountInfo(ENTROPY_PROGRAM_ADDRESS, { encoding: "base64" }).send();
    expect(info.value?.executable).toBe(true);
    expect(info.value?.owner).toBe(BPF_LOADER_UPGRADEABLE);
    expect(sha256Hex(await deployedElf(ENTROPY_PROGRAM_ADDRESS))).toBe(
      ENTROPY_FORK_EXECUTABLE_HASH,
    );
    // The mainnet fork still works: Regolith's program, fetched on first touch.
    const regolith = await withRetry(() => deployedElf(REGOLITH_ENTROPY_PROGRAM_ADDRESS));
    expect(regolith.length).toBe(98_929);
    expect(sha256Hex(regolith)).toBe(REGOLITH_ENTROPY_EXECUTABLE_HASH);
    // The live ORE Var lives under Regolith's program and is untouched.
    const live = await withRetry(() => fetchVar(LIVE_VAR));
    expect(live).not.toBeNull();
    expect(dec.decode(live!.provider)).toBe(LIVE_VAR_PROVIDER);
    expect(dec.decode(live!.authority)).toBe(LIVE_VAR_AUTHORITY);
    expect(live!.id).toBe(0n);
  });

  it("2. P1, nothing planted: Open → set_var (commit recorded) → sample_var (stranger) → Reveal → draw → Close", async () => {
    p1 = await lockedPool();
    const SEED1 = seedOf(0x11);
    const v1 = await openFresh(keeper, keeper.address, SEED1);
    const opened = (await fetchVar(v1.address))!;
    expect(dec.decode(opened.provider)).toBe(keeper.address);
    expect(dec.decode(opened.authority)).toBe(keeper.address);
    expect(opened.samples).toBe(1n);
    expect(opened.isAuto).toBe(0n);
    const varInfo = await rpc.getAccountInfo(v1.address, { encoding: "base64" }).send();
    expect(varInfo.value?.owner).toBe(ENTROPY_PROGRAM_ADDRESS);
    expect(varInfo.value?.lamports).toBe(await rentForVar());

    // set_var by the keeper: the pool records var, end_at and the commit.
    const setSig = await send(keeper, await setVarInstruction(keeper, p1.pool, v1.address));
    measured["set_var"] = await computeUnitsConsumed(setSig);
    let pool = await fetchPool(p1.pool);
    expect(pool.var).toBe(v1.address);
    expect(pool.varEndAt).toBe(v1.endAt);
    expect(Uint8Array.from(pool.varCommit)).toEqual(keccak(SEED1));
    expect(pool.sampledSlot).toBe(0n);
    const setEvents = await emittedEvents(setSig);
    expect(setEvents.map(eventName)).toEqual(["VarSet"]);
    const varSet = decodeEvent("VarSet", setEvents[0]!, varSetDecoder);
    expect(varSet.pool).toBe(p1.pool);
    expect(varSet.var).toBe(v1.address);
    expect(varSet.endAt).toBe(v1.endAt);
    expect(Uint8Array.from(varSet.commit)).toEqual(keccak(SEED1));

    // Inside the window: sample_var sent by a stranger (permissionless), CPI into Entropy.
    await waitForSlot(v1.endAt + 1n);
    const sampleSig = await sampleVar(stranger, p1.pool, v1.address);
    measured["sample_var"] = await computeUnitsConsumed(sampleSig);
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

    // Reveal by the keeper (the provider): the real program computes the value.
    await send(keeper, revealInstruction(keeper, v1.address, SEED1));
    const revealed = (await fetchVar(v1.address))!;
    expect(revealed.seed).toEqual(SEED1);
    expect(revealed.value).toEqual(entropyValue(sampled.slotHash, SEED1, 1n));

    // draw by the keeper.
    const drawSig = await send(keeper, await drawInstruction(keeper, p1.pool, v1.address));
    measured["draw"] = await computeUnitsConsumed(drawSig);
    pool = await fetchPool(p1.pool);
    expect(pool.status).toBe(PoolStatus.Drawn);
    expect(pool.drawn).toBe(true);
    const axes = drawAxes(revealed.value);
    expect(pool.homeAxis).toEqual(Array.from(axes.home));
    expect(pool.awayAxis).toEqual(Array.from(axes.away));
    expect(pool.var).toBe(v1.address);
    expect(Uint8Array.from(pool.varCommit)).toEqual(keccak(SEED1));
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
    // With the Var closed, `draw` fails at the owner constraint before any status check:
    // 6034 (PROGRAM §4.4 draw, Step 5 decision 19).
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, p1.pool, v1.address)),
    ).toBe(errorCode("VarNotEntropy"));
    // Step 4's Drawn sponsorship window, now reachable.
    await send(stranger, await sponsorInstruction(stranger, p1, PRICE));
    expect((await fetchPool(p1.pool)).sponsorCount).toBe(1);
  });

  it("3. the provider label: a Var opened with another provider, signed or not, is VarProviderMismatch", async () => {
    const refs = await lockedPool();
    // The stranger opens its own Var naming itself provider: Open succeeds on the fork.
    const own = await openFresh(stranger, stranger.address, seedOf(0x31), { ahead: 300n });
    expect(
      await sendExpectingError(keeper, await setVarInstruction(keeper, refs.pool, own.address)),
    ).toBe(errorCode("VarProviderMismatch")); // 6035
    // The keeper opens a Var naming the stranger as provider, who does not sign: Open succeeds
    // too (the fork keeps Regolith's commented provider check — the unsigned-provider fact).
    const unsigned = await openFresh(keeper, stranger.address, seedOf(0x32), { ahead: 300n });
    expect(dec.decode((await fetchVar(unsigned.address))!.provider)).toBe(stranger.address);
    expect(
      await sendExpectingError(
        keeper,
        await setVarInstruction(keeper, refs.pool, unsigned.address),
      ),
    ).toBe(errorCode("VarProviderMismatch")); // 6035
  });

  it("4. P2: a wrong-seed Reveal fails inside Entropy, draw is VarNotRevealed, the right Reveal then draws", async () => {
    const SEED2 = seedOf(0x22);
    const { refs, v } = await openBindAndSample(SEED2);
    await expect(
      send(stranger, revealInstruction(stranger, v.address, seedOf(0x23))),
    ).rejects.toBeTruthy();
    expect((await fetchVar(v.address))!.seed).toEqual(ZERO32);
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, refs.pool, v.address)),
    ).toBe(errorCode("VarNotRevealed")); // 6037
    await send(keeper, revealInstruction(keeper, v.address, SEED2));
    await send(keeper, await drawInstruction(keeper, refs.pool, v.address));
    const pool = await fetchPool(refs.pool);
    expect(pool.status).toBe(PoolStatus.Drawn);
    const axes = drawAxes((await fetchVar(v.address))!.value);
    expect(pool.homeAxis).toEqual(Array.from(axes.home));
  });

  it("5. P3, the commit-binding test on the real program: a self-consistent Var for another seed is VarCommitMismatch", async () => {
    // PROGRAM §4.4 "Why the two Step 5b checks": the Var is exactly as Reveal would leave it for
    // SEED4 — commit, seed, the verified slot hash, samples 1, a value that recomputes — and the
    // draw refuses because the pool recorded keccak(SEED3) before the end slot.
    const SEED3 = seedOf(0x33);
    const SEED4 = seedOf(0x34);
    const { refs, v } = await openBindAndSample(SEED3);
    const sampledHash = (await fetchVar(v.address))!.slotHash;
    const forSeed4 = freshVar(v.id, SEED4, v.endAt, {
      seed: SEED4,
      slotHash: sampledHash,
      value: entropyValue(sampledHash, SEED4, 1n),
    });
    await plantVar(v.address, forSeed4);
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, refs.pool, v.address)),
    ).toBe(errorCode("VarCommitMismatch")); // 6064
    expect((await fetchPool(refs.pool)).status).toBe(PoolStatus.Locked);
    // Step 5b audit L1: the Var's `commit` equal to the pool's, the seed SEED4 (which does not
    // hash to it), value consistent for SEED4 — a bad Reveal's shape; refused by
    // keccak(seed) == var_commit and only by that check.
    await plantVar(
      v.address,
      freshVar(v.id, SEED3, v.endAt, {
        seed: SEED4,
        slotHash: sampledHash,
        value: entropyValue(sampledHash, SEED4, 1n),
      }),
    );
    expect(Uint8Array.from((await fetchVar(v.address))!.commit)).toEqual(keccak(SEED3));
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, refs.pool, v.address)),
    ).toBe(errorCode("VarCommitMismatch")); // 6064
    // Re-plant the fields Reveal(SEED3) would have written: the draw goes through.
    await plantVar(
      v.address,
      freshVar(v.id, SEED3, v.endAt, {
        seed: SEED3,
        slotHash: sampledHash,
        value: entropyValue(sampledHash, SEED3, 1n),
      }),
    );
    await send(keeper, await drawInstruction(keeper, refs.pool, v.address));
    expect((await fetchPool(refs.pool)).status).toBe(PoolStatus.Drawn);
  });

  it("6. P4: samples edited to 2 after a correct reveal is VarNotFresh", async () => {
    const SEED6 = seedOf(0x66);
    const { refs, v } = await openBindAndSample(SEED6);
    await send(keeper, revealInstruction(keeper, v.address, SEED6));
    const revealed = (await fetchVar(v.address))!;
    await plantVar(v.address, {
      ...revealed,
      samples: 2n,
      value: entropyValue(revealed.slotHash, SEED6, 2n),
    });
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, refs.pool, v.address)),
    ).toBe(errorCode("VarNotFresh")); // 6036
  });

  it("7. P5: a missed window, then replace_var records the new commit; a never-bound P6 is VarNotSet; B samples and draws", async () => {
    const SEED_A = seedOf(0x51);
    const SEED_B = seedOf(0x52);
    const p5 = await lockedPool();
    const a = await openFresh(keeper, keeper.address, SEED_A);
    await send(keeper, await setVarInstruction(keeper, p5.pool, a.address));
    expect(Uint8Array.from((await fetchPool(p5.pool)).varCommit)).toEqual(keccak(SEED_A));

    // Past end_at + 512: the sysvar no longer holds end_at.
    const target = a.endAt + 600n;
    await localnet.timeTravel({ absoluteSlot: Number(target) });
    await waitForSlot(target);
    expect((await slotHashes()).find((e) => e.slot === a.endAt)).toBeUndefined();
    expect(await sampleVarExpectingError(keeper, p5.pool, a.address)).toBe(
      errorCode("SampleWindowMissed"), // 6039
    );
    expect((await fetchVar(a.address))!.slotHash).toEqual(ZERO32); // the CPI rolled back

    // Open B, replace: the pool's recorded commit moves with it.
    const b = await openFresh(keeper, keeper.address, SEED_B);
    const replaceSig = await send(admin, await replaceVarInstruction(admin, p5.pool, b.address));
    let pool = await fetchPool(p5.pool);
    expect(pool.var).toBe(b.address);
    expect(pool.varEndAt).toBe(b.endAt);
    expect(Uint8Array.from(pool.varCommit)).toEqual(keccak(SEED_B));
    expect(pool.sampledSlot).toBe(0n);
    expect(pool.varReplacements).toBe(1);
    const events = await emittedEvents(replaceSig);
    expect(events.map(eventName)).toEqual(["VarReplaced"]);
    const replaced = decodeEvent("VarReplaced", events[0]!, varReplacedDecoder);
    expect(replaced.oldVar).toBe(a.address);
    expect(replaced.newVar).toBe(b.address);
    expect(replaced.replacements).toBe(1);
    expect(Uint8Array.from(replaced.commit)).toEqual(keccak(SEED_B));

    // Step 5 audit L1: the admin never binds the first Var.
    const p6 = await lockedPool();
    expect(
      await sendExpectingError(admin, await replaceVarInstruction(admin, p6.pool, b.address)),
    ).toBe(errorCode("VarNotSet")); // 6032
    expect((await fetchPool(p6.pool)).varReplacements).toBe(0);

    // B's window: sample, reveal, draw; the axes come from B's value.
    await waitForSlot(b.endAt + 1n);
    await sampleVar(stranger, p5.pool, b.address);
    await send(keeper, revealInstruction(keeper, b.address, SEED_B));
    await send(keeper, await drawInstruction(keeper, p5.pool, b.address));
    pool = await fetchPool(p5.pool);
    expect(pool.status).toBe(PoolStatus.Drawn);
    expect(pool.homeAxis).toEqual(Array.from(drawAxes((await fetchVar(b.address))!.value).home));
    // A pool with a verified sample (now drawn) is never replaced: PoolNotLocked first.
    expect(
      await sendExpectingError(admin, await replaceVarInstruction(admin, p5.pool, a.address)),
    ).toBe(errorCode("PoolNotLocked")); // 6027
  });

  it("8. the Step 5 negatives hold against the fork: shape, too early, missed window with rollback, forged fallback", async () => {
    const refs = await lockedPool();
    const setVarBy = async (who: KeyPairSigner, v: Address) =>
      sendExpectingError(who, await setVarInstruction(who, refs.pool, v));
    expect(await setVarBy(admin, (await plantFresh(seedOf(0x81), { ahead: 300n })).address)).toBe(
      errorCode("Unauthorized"), // 6000
    );
    const auto = await plantFresh(seedOf(0x82), { ahead: 300n, overrides: { isAuto: 1n } });
    expect(await setVarBy(keeper, auto.address)).toBe(errorCode("VarNotFresh")); // 6036
    const two = await plantFresh(seedOf(0x83), { ahead: 300n, overrides: { samples: 2n } });
    expect(await setVarBy(keeper, two.address)).toBe(errorCode("VarNotFresh")); // 6036
    const now = await plantFresh(seedOf(0x84), { endAt: await currentSlot() });
    expect(await setVarBy(keeper, now.address)).toBe(errorCode("VarNotFresh")); // 6036
    const systemOwned = await entropyVarPda(keeper.address, nextVarId++);
    await plantVar(systemOwned, freshVar(0n, seedOf(0x85), (await currentSlot()) + 300n));
    await localnet.setAccount(systemOwned, { owner: SYSTEM_PROGRAM });
    expect(await setVarBy(keeper, systemOwned)).toBe(errorCode("VarNotEntropy")); // 6034
    // The live ORE Var under Regolith's program: wrong owner for ENTROPY_PROGRAM now.
    expect(await setVarBy(keeper, LIVE_VAR)).toBe(errorCode("VarNotEntropy")); // 6034

    // Bind a planted Var and the window negatives.
    const v = await plantFresh(seedOf(0x86), { ahead: 300n });
    await send(keeper, await setVarInstruction(keeper, refs.pool, v.address));
    expect(await setVarBy(keeper, v.address)).toBe(errorCode("VarAlreadySet")); // 6031
    expect(await sampleVarExpectingError(keeper, refs.pool, v.address)).toBe(
      errorCode("SampleTooEarly"), // 6062
    );
    expect(await sampleVarExpectingError(keeper, refs.pool, auto.address)).toBe(
      errorCode("VarMismatch"), // 6033
    );
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, refs.pool, v.address)),
    ).toBe(errorCode("VarNotSampledHere")); // 6038

    // Window missed: the CPI writes the fallback, sample_var refuses, the write rolls back.
    const target = v.endAt + 600n;
    await localnet.timeTravel({ absoluteSlot: Number(target) });
    await waitForSlot(target);
    expect(await sampleVarExpectingError(keeper, refs.pool, v.address)).toBe(
      errorCode("SampleWindowMissed"), // 6039
    );
    expect((await fetchVar(v.address))!.slotHash).toEqual(ZERO32);
    expect((await fetchPool(refs.pool)).sampledSlot).toBe(0n);

    // Forged fallback: a Var carrying keccak(end_at) with a consistent reveal.
    const fallback = fallbackHash(v.endAt);
    await plantVar(
      v.address,
      freshVar(v.id, v.seed, v.endAt, {
        slotHash: fallback,
        seed: v.seed,
        value: entropyValue(fallback, v.seed, 1n),
      }),
    );
    expect(await sampleVarExpectingError(keeper, refs.pool, v.address)).toBe(
      errorCode("SampleWindowMissed"), // 6039
    );
    // Forge the pool's sample to match (raw patch at the §3.3 offsets 1166 / 1174); var_commit
    // at 1314 is untouched by it.
    const raw = (await fetchAccountData(refs.pool))!;
    new DataView(raw.buffer, raw.byteOffset, raw.byteLength).setBigUint64(1166, 1n, true);
    raw.set(fallback, 1174);
    await localnet.setAccount(refs.pool, { data: hex(raw) });
    const forged = await fetchPool(refs.pool);
    expect(forged.sampledSlot).toBe(1n);
    expect(Uint8Array.from(forged.sampledHash)).toEqual(fallback);
    expect(Uint8Array.from(forged.varCommit)).toEqual(keccak(v.seed));
    expect(
      await sendExpectingError(keeper, await drawInstruction(keeper, refs.pool, v.address)),
    ).toBe(errorCode("VarFallbackHash")); // 6040
  });

  it("9. compute units: sample_var (CPI) under 250k; set_var and draw recorded", () => {
    for (const name of ["set_var", "sample_var", "draw"]) {
      expect(measured[name], name).toBeDefined();
      console.log(`${name} consumed ${measured[name]} CU`);
    }
    expect(measured["sample_var"]!).toBeLessThan(250_000n);
  });

  it("10. the IDL has 26 instructions, 6 accounts, 24 events, 65 errors, the Step 5b fields with docs", () => {
    expect(IDL.instructions).toHaveLength(26); // 19 after Step 6, + the seven §4.6 instructions
    expect(IDL.accounts).toHaveLength(6);
    expect(IDL.events).toHaveLength(24); // 18 after Step 6, + the six §4.6 events
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
    const field = (type: string, name: string) =>
      idl.types.find((t) => t.name === type)?.type.fields?.find((f) => f.name === name);
    expect(field("Pool", "var_commit")?.docs?.length ?? 0).toBeGreaterThan(0);
    expect(field("VarSet", "commit")?.docs?.length ?? 0).toBeGreaterThan(0);
    expect(field("VarReplaced", "commit")?.docs?.length ?? 0).toBeGreaterThan(0);
    const e = IDL.errors.find((x) => x.name === "VarCommitMismatch");
    expect(e?.code).toBe(6064);
    expect(idl.errors.find((x) => x.name === "VarCommitMismatch")?.msg ?? "").not.toBe("");
    expect(IDL.errors.find((x) => x.name === "SampleTooEarly")?.code).toBe(6062);
    expect(IDL.errors.find((x) => x.name === "VarAlreadySampled")?.code).toBe(6063);
  });
});
