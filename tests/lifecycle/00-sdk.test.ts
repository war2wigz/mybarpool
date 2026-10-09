/**
 * Lifecycle 00 — the SDK (build plan Step 9): every MyBarPool instruction goes through
 * `@mybarpool/shared`'s chain layer — the ten user helpers hand-written, the keeper's steps
 * through the generated builders — and every transaction through `prepareTransaction` +
 * `signAndSend`. Two blocks: v1 (a SOL pool from creation to `close_pool`, settlements watched
 * through `watchSettlements`) and legacy (a public pool, a `Link` pool with a rotated gate key,
 * an `Allowlist` pool; ComputeBudget instructions carried, every packet ≤ 1,232 bytes).
 *
 * From the old harness this file imports only plumbing: the RPC pair, the declared program id,
 * the chain clock, the airdrop retry, and Entropy's own instructions (`tests/helpers/entropy.ts`).
 */
import {
  buyInstruction,
  closeCounterInstruction,
  closePoolInstruction,
  closeSponsorshipInstruction,
  createMyBarPoolClient,
  createPoolInstruction,
  displayStatus,
  entropyValue,
  eventsOf,
  getConfig,
  getGame,
  getPool,
  isSdkError,
  refreshBlockhash,
  configPda,
  gamePda,
  keccak,
  listPools,
  listSponsors,
  networkFeeLamports,
  PayoutPreset,
  poolView,
  prepareTransaction,
  rotateGateKeyInstruction,
  sampleVarInstruction,
  signAndSend,
  sponsorInstruction,
  toLabel,
  winningBox,
  wonBoxes,
  type Digits,
  type EmittedSettlement,
  type MyBarPoolClient,
  type PreparedTransaction,
  type TransactionFormat,
  watchPool,
  watchSettlements,
} from "@mybarpool/shared";
import {
  AccessType,
  getBuyInstructionAsync,
  getCreateGameInstructionAsync,
  getDrawInstructionAsync,
  getPostScoresInstructionAsync,
  getSetVarInstructionAsync,
  getSettleInstructionAsync,
  getUpdateConfigInstructionAsync,
  PoolStatus,
  type Pool,
} from "@mybarpool/shared/generated";
import {
  airdropFactory,
  appendTransactionMessageInstructions,
  createKeyPairSignerFromBytes,
  createTransactionMessage,
  generateKeyPairSigner,
  getTransactionMessageSize,
  lamports,
  pipe,
  setTransactionMessageComputeUnitLimit,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  setTransactionMessageLoadedAccountsDataSizeLimit,
  type Address,
  type Instruction,
  type KeyPairSigner,
  type Signature,
} from "@solana/kit";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

import { Localnet } from "../../scripts/localnet.js";
import {
  currentSlot,
  entropyVarPda,
  fetchVar,
  openInstruction,
  revealInstruction,
  waitForSlot,
} from "../helpers/entropy.js";
import {
  chainNow,
  declaredProgramId,
  rpc,
  rpcSubscriptions,
  withRetry,
} from "../helpers/mybarpool.js";

const SOL = 1_000_000_000n;
const PRICE = 50_000_000n; // 0.05 SOL, on the SOL ladder
const MINUTE = 60n;
const HOUR = 3_600n;
const LEGACY_LIMIT = 1_232;
const COMPUTE_BUDGET = "ComputeBudget111111111111111111111111111111";
/** The Step 3 scores: Q1 7–3, Q2 14–10, Q3 17–17, final 24–20 (overtime). */
const SCORES: readonly (readonly [number, number])[] = [
  [7, 3],
  [14, 10],
  [17, 17],
  [24, 20],
];
/** Slots ahead for Entropy's `end_at`; `Open` and `set_var` land in one transaction. */
const WINDOW_AHEAD = 30n;

const localnet = new Localnet();
const client: MyBarPoolClient = createMyBarPoolClient({
  rpc,
  rpcSubscriptions,
  programAddress: declaredProgramId(),
});

/** One row of the NOTES.md table: what `prepareTransaction` set, per transaction. */
interface Row {
  label: string;
  format: TransactionFormat;
  cuSimulated: number;
  cuLimit: number;
  dataSize: number | undefined;
  sizeBytes: number;
  feeLamports: bigint;
}
const rows: Row[] = [];

async function run(
  label: string,
  format: TransactionFormat,
  feePayer: KeyPairSigner,
  instructions: Instruction[],
  createdAccountBytes = 0,
): Promise<{ signature: Signature; prepared: PreparedTransaction }> {
  await touch(instructions);
  const prepared = await prepareTransaction(client, {
    feePayer,
    instructions,
    format,
    createdAccountBytes,
    // Surfpool answers `confirmed` with the hash of the slot it is still producing and then
    // asks mainnet about it at preflight ("Blockhash not found" after a 40 s stall); the
    // finalized hash is a few slots older and always known. Agave does not behave this way.
    blockhashCommitment: "finalized",
  });
  rows.push({
    label,
    format,
    cuSimulated: prepared.computeUnitsSimulated,
    cuLimit: prepared.computeUnitLimit,
    dataSize: prepared.loadedAccountsDataSizeLimit,
    sizeBytes: prepared.sizeBytes,
    feeLamports: networkFeeLamports(prepared).totalLamports,
  });
  if (format === "legacy") {
    expect(prepared.sizeBytes).toBeLessThanOrEqual(LEGACY_LIMIT);
    expect(
      prepared.message.instructions.filter((i) => i.programAddress === COMPUTE_BUDGET).length,
    ).toBeGreaterThanOrEqual(1);
  } else {
    expect(prepared.message.version).toBe(1);
    expect(prepared.message.instructions.some((i) => i.programAddress === COMPUTE_BUDGET)).toBe(
      false,
    );
  }
  // A remote-fetch stall at preflight (Step 3 audit M1) means nothing landed; the same prepared
  // transaction is sent again, as the harness's `send` does. Program errors are never retried.
  // `BlockhashExpired` gets the one re-stamp DESIGN §10.4 allows, as the app would do.
  const started = Date.now();
  try {
    const signature = await withRetry(() => sendOnce(prepared), { onlyRpcStalls: true });
    return { signature, prepared };
  } catch (error) {
    console.error(`${label}: failed after ${Date.now() - started} ms`, describeError(error));
    throw error;
  }
}

async function sendOnce(prepared: PreparedTransaction): Promise<Signature> {
  try {
    return await signAndSend(client, prepared);
  } catch (error) {
    if (isSdkError(error, "BlockhashExpired")) {
      console.log(
        `BlockhashExpired (lastValidBlockHeight ${prepared.lastValidBlockHeight}); re-stamping once`,
      );
      return signAndSend(client, await refreshBlockhash(client, prepared));
    }
    throw error;
  }
}

function describeError(error: unknown): string {
  const parts: string[] = [];
  let e: unknown = error;
  for (let depth = 0; depth < 6 && e; depth++) {
    const ctx = (e as { context?: unknown }).context;
    parts.push(
      `${(e as Error).name ?? "?"}: ${(e as Error).message ?? ""}${ctx ? ` ${JSON.stringify(ctx, (_, v) => (typeof v === "bigint" ? String(v) : v))}` : ""}`,
    );
    e = (e as { cause?: unknown }).cause;
  }
  return parts.join("\n  caused by ");
}

/**
 * Surfpool asks mainnet whether an account it has never seen exists, and a slow answer fails
 * the simulation or the send with "Failed to fetch accounts from remote" (Step 3 audit M1). The
 * harness's `withRetry` around a read of every account the transaction names warms the fork
 * before the SDK, which has no business knowing about forks, simulates.
 */
async function touch(instructions: Instruction[]): Promise<void> {
  const addresses = [
    ...new Set(instructions.flatMap((i) => (i.accounts ?? []).map((a) => a.address))),
  ];
  for (let i = 0; i < addresses.length; i += 50) {
    const chunk = addresses.slice(i, i + 50);
    await withRetry(() => rpc.getMultipleAccounts(chunk, { encoding: "base64" }).send());
  }
}

async function travelTo(target: bigint): Promise<bigint> {
  const now = await chainNow();
  if (now < target) {
    await localnet.timeTravel({ absoluteTimestamp: Number((target + 2n) * 1000n) });
  }
  const after = await chainNow();
  expect(after).toBeGreaterThanOrEqual(target);
  return after;
}

async function fund(...who: KeyPairSigner[]): Promise<void> {
  const airdrop = airdropFactory({ rpc, rpcSubscriptions });
  for (const w of who) {
    // First touch of a fresh key: Surfpool asks mainnet whether it exists (Step 3 audit M1).
    await withRetry(() =>
      airdrop({
        recipientAddress: w.address,
        lamports: lamports(100n * SOL),
        commitment: "confirmed",
      }),
    );
  }
}

async function pool(address: Address): Promise<Pool> {
  const p = await getPool(client, address);
  if (!p) throw new Error(`pool ${address} not found`);
  return p.data;
}

/** The keeper's draw: Entropy `Open` + `set_var` in one transaction, sample in the window, `Reveal`, `draw`. */
async function drawThroughSdk(
  keeper: KeyPairSigner,
  sampler: KeyPairSigner,
  poolAddress: Address,
  format: TransactionFormat,
  varId: bigint,
): Promise<Address> {
  const seed = new Uint8Array(32).fill(Number(varId % 200n) + 1);
  const varAddress = await entropyVarPda(keeper.address, varId);
  await withRetry(() => rpc.getAccountInfo(varAddress, { encoding: "base64" }).send());
  const endAt = (await currentSlot()) + WINDOW_AHEAD;
  const open = await openInstruction(
    keeper,
    keeper,
    varId,
    keeper.address,
    keccak(seed),
    false,
    1n,
    endAt,
  );
  const setVar = await getSetVarInstructionAsync(
    { scoreAuthority: keeper, pool: poolAddress, var: varAddress },
    { programAddress: client.programAddress },
  );
  await run(`open+set_var (${format})`, format, keeper, [open, setVar]);
  await waitForSlot(endAt + 1n);
  const sample = await sampleVarInstruction(client, { signer: sampler, pool: poolAddress });
  expect(sample.var).toBe(varAddress);
  await run(`sample_var (${format})`, format, sampler, [sample.instruction]);
  await run(`reveal (${format})`, format, keeper, [revealInstruction(keeper, varAddress, seed)]);
  const revealed = (await fetchVar(varAddress))!;
  expect(revealed.value).toEqual(entropyValue(revealed.slotHash, seed, 1n));
  const draw = await getDrawInstructionAsync(
    { scoreAuthority: keeper, pool: poolAddress, var: varAddress },
    { programAddress: client.programAddress },
  );
  await run(`draw (${format})`, format, keeper, [draw]);
  const p = await pool(poolAddress);
  expect(p.status).toBe(PoolStatus.Drawn);
  expect(p.drawn).toBe(true);
  return varAddress;
}

let admin: KeyPairSigner;
let keeper: KeyPairSigner;
let creator: KeyPairSigner;
let creator2: KeyPairSigner;
let stranger: KeyPairSigner;
let buyers: KeyPairSigner[];
let sponsors: KeyPairSigner[];
let feeWallet: Address;
let game: Address;
let kickoff: bigint;
let varId = 900n;

describe("lifecycle 00: the SDK, v1 (Surfpool)", { timeout: 120_000 }, () => {
  let poolAddress: Address;
  let counter: Address;
  const settlements: EmittedSettlement[] = [];
  let unwatchSettlements: () => void;

  beforeAll(async () => {
    const walletPath = process.env["ANCHOR_WALLET"] ?? join(homedir(), ".config/solana/id.json");
    admin = await createKeyPairSignerFromBytes(
      Uint8Array.from(JSON.parse(readFileSync(walletPath, "utf8"))),
    );
    keeper = await generateKeyPairSigner();
    creator = await generateKeyPairSigner();
    creator2 = await generateKeyPairSigner();
    stranger = await generateKeyPairSigner();
    buyers = await Promise.all(Array.from({ length: 8 }, () => generateKeyPairSigner()));
    sponsors = await Promise.all(Array.from({ length: 2 }, () => generateKeyPairSigner()));
    await fund(keeper, creator, creator2, stranger, ...buyers, ...sponsors);

    // The keeper of this file becomes score authority and entropy provider (admin, generated).
    const nulls = { admin: null, feeWallet: null, addonBudgetBps: null, defaultPreset: null };
    const update = await getUpdateConfigInstructionAsync(
      {
        admin,
        adminArg: nulls.admin,
        scoreAuthority: keeper.address,
        entropyProvider: keeper.address,
        feeWallet: nulls.feeWallet,
        platformBps: 500,
        creatorBps: 500,
        addonBudgetBps: nulls.addonBudgetBps,
        defaultPreset: nulls.defaultPreset,
        maxOpenPools: null,
        maxOwnBoxes: null,
        preseasonEnabled: null,
        paused: null,
        tokens: [null, null, null],
      },
      { programAddress: client.programAddress },
    );
    await run("update_config (v1)", "v1", admin, [update]);
    const config = (await getConfig(client, await configPda(client)))!;
    expect(config.data.scoreAuthority).toBe(keeper.address);
    feeWallet = config.data.feeWallet;
    await withRetry(() => rpc.getAccountInfo(feeWallet, { encoding: "base64" }).send());

    // A game three hours out, from the chain clock (PROGRAM §2: KC hosting DAL, week 9).
    const key = { season: 2026, week: 9, home: 15, away: 8 };
    kickoff = (await chainNow()) + 3n * HOUR;
    game = await gamePda(client, key, kickoff);
    const createGame = await getCreateGameInstructionAsync(
      { scoreAuthority: keeper, game, key, scheduledKickoff: kickoff },
      { programAddress: client.programAddress },
    );
    const { signature } = await run("create_game (v1)", "v1", keeper, [createGame]);
    const events = await eventsOf(client, signature);
    expect(events.map((e) => e.event.name)).toEqual(["GameCreated"]);
    expect((await getGame(client, game))!.data.scheduledKickoff).toBe(kickoff);
  }, 300_000);

  it("creates a SOL pool with 3 initial boxes; PoolCreated and BoxesBought decode", async () => {
    const r = await createPoolInstruction(client, {
      creator,
      game,
      token: 0,
      price: PRICE,
      preset: PayoutPreset.Standard,
      access: { type: "public" },
      creatorAddonBps: 200,
      initialBoxes: 3,
      now: await chainNow(),
    });
    poolAddress = r.pool;
    counter = r.counter;
    expect(r.createdAccountBytes).toBe(1_442 + 74);
    const { signature, prepared } = await run(
      "create_pool (v1)",
      "v1",
      creator,
      [r.instruction],
      r.createdAccountBytes,
    );
    expect(prepared.loadedAccountsDataSizeLimit).toBeDefined();
    const names = (await eventsOf(client, signature)).map((e) => e.event.name);
    expect(names).toEqual(["PoolCreated", "BoxesBought"]);
    const p = await pool(poolAddress);
    expect(p.sold).toBe(3);
    expect(p.creatorBoxes).toBe(3);
    const listed = await listPools(client, { game });
    expect(listed.map((x) => x.address)).toContain(poolAddress);
    expect(displayStatus(p, (await getGame(client, game))!.data, await chainNow())).toEqual({
      kind: "OPEN",
    });
  });

  it("five buyers fill it 5/5/5/5/2; watchPool sees Locked at the 25th box", async () => {
    const seen: (PoolStatus | null)[] = [];
    let resolveLocked!: () => void;
    const locked = new Promise<void>((res) => (resolveLocked = res));
    const unwatch = watchPool(client, poolAddress, (p) => {
      seen.push(p?.status ?? null);
      if (p?.status === PoolStatus.Locked) resolveLocked();
    });
    const counts = [5, 5, 5, 5, 2];
    for (const [i, count] of counts.entries()) {
      const b = await buyInstruction(client, { buyer: buyers[i]!, pool: poolAddress, count });
      expect(b.counter).toBe(counter);
      const { signature } = await run(`buy ×${count} (v1)`, "v1", buyers[i]!, [b.instruction]);
      const events = await eventsOf(client, signature);
      expect(events[0]!.event.name).toBe("BoxesBought");
      if (i === counts.length - 1) expect(events.map((e) => e.event.name)).toContain("PoolLocked");
    }
    await locked;
    unwatch();
    expect(seen).toContain(PoolStatus.Open);
    expect(seen.at(-1)).toBe(PoolStatus.Locked);
    const p = await pool(poolAddress);
    expect(p.sold).toBe(25);
    expect(p.status).toBe(PoolStatus.Locked);
  });

  it("two sponsors, the second twice; listSponsors returns two with the summed amount", async () => {
    const amounts = [
      [sponsors[0]!, 100_000_000n],
      [sponsors[1]!, 50_000_000n],
      [sponsors[1]!, 70_000_000n],
    ] as const;
    for (const [who, amount] of amounts) {
      const s = await sponsorInstruction(client, { sponsor: who, pool: poolAddress, amount });
      const { signature } = await run(
        `sponsor ${amount} (v1)`,
        "v1",
        who,
        [s.instruction],
        s.createdAccountBytes,
      );
      expect((await eventsOf(client, signature)).map((e) => e.event.name)).toEqual(["Sponsored"]);
    }
    const list = await listSponsors(client, poolAddress);
    expect(list).toHaveLength(2);
    const byWallet = new Map(list.map((s) => [s.data.wallet, s.data.amount]));
    expect(byWallet.get(sponsors[0]!.address)).toBe(100_000_000n);
    expect(byWallet.get(sponsors[1]!.address)).toBe(120_000_000n);
    expect((await pool(poolAddress)).sponsoredTotal).toBe(220_000_000n);
  });

  it("the keeper draws: Entropy Open + set_var, sample_var through the SDK, Reveal, draw", async () => {
    await drawThroughSdk(keeper, stranger, poolAddress, "v1", varId++);
    const now = await chainNow();
    const g = (await getGame(client, game))!.data;
    expect(displayStatus(await pool(poolAddress), g, now)).toEqual({ kind: "LOCKED_WAITING" });
  });

  it("watchSettlements sees four QuarterSettled in order; labels agree with wonBoxes", async () => {
    unwatchSettlements = watchSettlements(client, poolAddress, (s) => settlements.push(s), {
      onError: (e) => console.error("watchSettlements", e),
    });
    await travelTo(kickoff);
    const g = (await getGame(client, game))!.data;
    expect(displayStatus(await pool(poolAddress), g, await chainNow())).toEqual({ kind: "LIVE" });
    for (let q = 1; q <= 4; q++) {
      const [home, away] = SCORES[q - 1]!;
      await travelTo(kickoff + BigInt(q) * 16n * MINUTE);
      const post = await getPostScoresInstructionAsync(
        {
          scoreAuthority: keeper,
          game,
          quarter: q,
          home,
          away,
          isFinal: q === 4,
          hadOvertime: q === 4,
        },
        { programAddress: client.programAddress },
      );
      await run(`post_scores q${q} (v1)`, "v1", keeper, [post]);
      const p = await pool(poolAddress);
      const box = winningBox({
        home,
        away,
        homeAxis: p.homeAxis as unknown as Digits,
        awayAxis: p.awayAxis as unknown as Digits,
      });
      const winner = p.owners[box]!;
      const settle = await getSettleInstructionAsync(
        {
          scoreAuthority: keeper,
          game,
          pool: poolAddress,
          winner,
          feeWallet,
          creator: creator.address,
          quarter: q,
        },
        { programAddress: client.programAddress },
      );
      const { signature } = await run(`settle q${q} (v1)`, "v1", keeper, [settle]);
      const ev = (await eventsOf(client, signature)).find(
        (e) => e.event.name === "QuarterSettled",
      )!;
      expect(ev.event.name).toBe("QuarterSettled");
      if (ev.event.name === "QuarterSettled") {
        expect(ev.event.data.boxIndex).toBe(box);
        expect(ev.event.data.winner).toBe(winner);
      }
    }
    // The subscription delivers after confirmation; give it a moment.
    for (let i = 0; i < 100 && settlements.length < 4; i++)
      await new Promise((r) => setTimeout(r, 100));
    unwatchSettlements();
    expect(settlements.map((s) => s.event.quarter)).toEqual([1, 2, 3, 4]);
    const p = await pool(poolAddress);
    const won = wonBoxes(p);
    expect(won).toHaveLength(4);
    settlements.forEach((s, q) => {
      expect(s.event.boxIndex).toBe(p.winningBox[q]);
      expect(toLabel(s.event.boxIndex)).toBe(won[q]!.box);
      expect(s.event.winner).toBe(won[q]!.winner);
    });
    const g2 = (await getGame(client, game))!.data;
    expect(displayStatus(p, g2, await chainNow())).toEqual({ kind: "SETTLED" });
    const view = poolView(p, g2, await chainNow());
    expect(view.boxes.map((b) => b.label)).toEqual(Array.from({ length: 25 }, (_, i) => i + 1));
    expect(view.winners).toEqual(won);
  });

  it("a stranger closes both sponsorships and the pool; getPool is null afterwards", async () => {
    for (const s of sponsors) {
      const cs = await closeSponsorshipInstruction(client, {
        pool: poolAddress,
        sponsor: s.address,
      });
      const { signature } = await run("close_sponsorship (v1)", "v1", stranger, [cs.instruction]);
      expect((await eventsOf(client, signature)).map((e) => e.event.name)).toEqual([
        "SponsorshipClosed",
      ]);
    }
    expect(await listSponsors(client, poolAddress)).toHaveLength(0);
    const cp = await closePoolInstruction(client, { payer: stranger, pool: poolAddress });
    expect(cp.feeWallet).toBe(feeWallet);
    expect(cp.creator).toBe(creator.address);
    const { signature } = await run("close_pool (v1)", "v1", stranger, [cp.instruction]);
    expect((await eventsOf(client, signature)).map((e) => e.event.name)).toEqual(["PoolClosed"]);
    expect(await getPool(client, poolAddress)).toBeNull();
    expect((await listPools(client, { game })).map((x) => x.address)).not.toContain(poolAddress);
    const cc = await closeCounterInstruction(client, { creator: creator.address, game });
    expect(cc.counter).toBe(counter);
    await run("close_counter (v1)", "v1", stranger, [cc.instruction]);
    expect(
      await rpc
        .getAccountInfo(counter, { encoding: "base64" })
        .send()
        .then((r) => r.value),
    ).toBeNull();
  });
});

describe("lifecycle 00: the SDK, legacy (Surfpool)", { timeout: 120_000 }, () => {
  let publicPool: Address;
  let linkPool: Address;
  let allowlistPool: Address;
  let gateKey: KeyPairSigner;
  let newGate: KeyPairSigner;
  let allowlist: Address[];

  beforeAll(async () => {
    gateKey = await generateKeyPairSigner();
    newGate = await generateKeyPairSigner();
    allowlist = [...buyers.map((b) => b.address)]; // eight wallets
    expect(allowlist).toHaveLength(8);
    // The v1 block settled its game, so sales on it are closed; the legacy pools get a second
    // game (same teams, week 19), three hours out from the clock the v1 block left behind.
    const key = { season: 2026, week: 19, home: 15, away: 8 };
    kickoff = (await chainNow()) + 3n * HOUR;
    game = await gamePda(client, key, kickoff);
    const createGame = await getCreateGameInstructionAsync(
      { scoreAuthority: keeper, game, key, scheduledKickoff: kickoff },
      { programAddress: client.programAddress },
    );
    await run("create_game (legacy)", "legacy", keeper, [createGame]);
  });

  it("a public pool: create, two buys, one sponsor — ComputeBudget carried, ≤ 1,232 bytes", async () => {
    const r = await createPoolInstruction(client, {
      creator,
      game,
      token: 0,
      price: PRICE,
      preset: PayoutPreset.Standard,
      access: { type: "public" },
      initialBoxes: 0,
      now: await chainNow(),
    });
    publicPool = r.pool;
    const { signature } = await run(
      "create_pool (legacy)",
      "legacy",
      creator,
      [r.instruction],
      r.createdAccountBytes,
    );
    expect((await eventsOf(client, signature)).map((e) => e.event.name)).toEqual(["PoolCreated"]);
    for (const [i, count] of [2, 3].entries()) {
      const b = await buyInstruction(client, { buyer: buyers[i]!, pool: publicPool, count });
      const { signature: sig } = await run(`buy ×${count} (legacy)`, "legacy", buyers[i]!, [
        b.instruction,
      ]);
      expect((await eventsOf(client, sig))[0]!.event.name).toBe("BoxesBought");
    }
    const s = await sponsorInstruction(client, {
      sponsor: sponsors[0]!,
      pool: publicPool,
      amount: 60_000_000n,
    });
    const { signature: sig } = await run(
      "sponsor (legacy)",
      "legacy",
      sponsors[0]!,
      [s.instruction],
      s.createdAccountBytes,
    );
    expect((await eventsOf(client, sig)).map((e) => e.event.name)).toEqual(["Sponsored"]);
    expect((await pool(publicPool)).sold).toBe(5);
  });

  it("a Link pool by a second creator: rotate_gate_key, then a gated buy with the new key co-signing", async () => {
    const r = await createPoolInstruction(client, {
      creator: creator2,
      game,
      token: 0,
      price: PRICE,
      preset: PayoutPreset.Standard,
      access: { type: "link", gateKey: gateKey.address },
      initialBoxes: 1,
      now: await chainNow(),
    });
    linkPool = r.pool;
    await run(
      "create_pool link (legacy)",
      "legacy",
      creator2,
      [r.instruction],
      r.createdAccountBytes,
    );
    expect((await pool(linkPool)).accessType).toBe(AccessType.Link);
    const rot = await rotateGateKeyInstruction(client, {
      creator: creator2,
      pool: linkPool,
      newKey: newGate.address,
    });
    const { signature } = await run("rotate_gate_key (legacy)", "legacy", creator2, [
      rot.instruction,
    ]);
    expect((await eventsOf(client, signature)).map((e) => e.event.name)).toEqual([
      "GateKeyRotated",
    ]);
    expect((await pool(linkPool)).gateKey).toBe(newGate.address);
    const b = await buyInstruction(client, {
      buyer: buyers[2]!,
      pool: linkPool,
      count: 2,
      gate: { gateKey: newGate },
    });
    const { signature: sig, prepared } = await run("buy link ×2 (legacy)", "legacy", buyers[2]!, [
      b.instruction,
    ]);
    expect(prepared.signatureCount).toBe(2);
    console.log(
      `Link buy (legacy): ${prepared.sizeBytes} bytes, ${prepared.signatureCount} signatures`,
    );
    expect((await eventsOf(client, sig))[0]!.event.name).toBe("BoxesBought");
    expect((await pool(linkPool)).sold).toBe(3);
  });

  it("an Allowlist pool with eight wallets: a gated buy with the proof", async () => {
    const r = await createPoolInstruction(client, {
      creator: creator2,
      game,
      token: 0,
      price: PRICE,
      preset: PayoutPreset.Standard,
      access: { type: "allowlist", wallets: allowlist },
      initialBoxes: 0,
      now: await chainNow(),
    });
    allowlistPool = r.pool;
    expect(r.allowlistRoot.some((b) => b !== 0)).toBe(true);
    await run(
      "create_pool allowlist (legacy)",
      "legacy",
      creator2,
      [r.instruction],
      r.createdAccountBytes,
    );
    const b = await buyInstruction(client, {
      buyer: buyers[5]!,
      pool: allowlistPool,
      count: 4,
      gate: { wallets: allowlist },
    });
    const { signature, prepared } = await run("buy allowlist ×4 (legacy)", "legacy", buyers[5]!, [
      b.instruction,
    ]);
    console.log(`Allowlist buy, 8 wallets (legacy): ${prepared.sizeBytes} bytes`);
    expect((await eventsOf(client, signature))[0]!.event.name).toBe("BoxesBought");
    expect((await pool(allowlistPool)).sold).toBe(4);
    expect(
      (await listPools(client, { game, creator: creator2.address })).map((x) => x.address).sort(),
    ).toEqual([linkPool, allowlistPool].sort());
    expect(await listPools(client, { game, status: PoolStatus.Open })).toHaveLength(3);
  });

  it("measures the deepest allowlist proof that fits a legacy packet", async () => {
    // The same message prepareTransaction builds (limit, data size, blockhash), the proof grown
    // until the size assertion would fail; the depth is what NOTES.md records.
    const p = await pool(allowlistPool);
    const { value: blockhash } = await rpc.getLatestBlockhash().send();
    const real = await buyInstruction(client, {
      buyer: buyers[6]!,
      pool: allowlistPool,
      count: 1,
      gate: { wallets: allowlist },
    });
    const sizeFor = async (depth: number) => {
      const ix = await getBuyInstructionAsync(
        {
          buyer: buyers[6]!,
          game,
          pool: allowlistPool,
          counter: real.counter,
          walletOverride: real.walletOverride,
          count: 1,
          allowlistProof: Array.from({ length: depth }, (_, i) => new Uint8Array(32).fill(i + 1)),
        },
        { programAddress: client.programAddress },
      );
      const message = pipe(
        createTransactionMessage({ version: "legacy" }),
        (m) => setTransactionMessageFeePayerSigner(buyers[6]!, m),
        (m) => appendTransactionMessageInstructions([ix], m),
        (m) => setTransactionMessageComputeUnitLimit(200_000, m),
        (m) => setTransactionMessageLoadedAccountsDataSizeLimit(32_768, m),
        (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
      );
      return getTransactionMessageSize(message);
    };
    let depth = 0;
    let size = await sizeFor(0);
    while (true) {
      const next = await sizeFor(depth + 1);
      if (next > LEGACY_LIMIT) break;
      depth++;
      size = next;
    }
    console.log(
      `Deepest legacy allowlist proof: ${depth} hashes (${size} bytes; ${await sizeFor(depth + 1)} with one more)`,
    );
    expect(depth).toBeGreaterThanOrEqual(20); // 2^20 wallets, far above any real list
    expect(p.accessType).toBe(AccessType.Allowlist);
  });

  it("prints the per-transaction table", () => {
    const header =
      "| transaction | format | CU simulated | CU limit | data-size limit | bytes | fee (lamports) |";
    console.log(header);
    console.log("|---|---|---:|---:|---:|---:|---:|");
    for (const r of rows) {
      console.log(
        `| ${r.label} | ${r.format} | ${r.cuSimulated.toLocaleString("en-US")} | ${r.cuLimit.toLocaleString("en-US")} | ${
          r.dataSize === undefined ? "—" : r.dataSize.toLocaleString("en-US")
        } | ${r.sizeBytes} | ${r.feeLamports} |`,
      );
    }
    expect(rows.length).toBeGreaterThan(20);
    expect(rows.every((r) => r.cuLimit <= 1_000_000)).toBe(true); // nothing near the readiness bar
  });
});
