/**
 * The instruction helpers over a mock RPC: every helper's account list and roles equal the
 * generated builder's called with the addresses derived independently here, and the behaviour
 * the brief names (pre-simulation program errors, the gate slot, the proof, the counter rule,
 * token-account existence, `createdAccountBytes`) holds.
 */
import {
  AccountRole,
  generateKeyPairSigner,
  getAddressEncoder,
  type Address,
  type Instruction,
  type KeyPairSigner,
  type ReadonlyUint8Array,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import {
  AccessType,
  GameStatus,
  getBuyInstructionAsync,
  getCloseCounterInstructionAsync,
  getClosePoolInstructionAsync,
  getCloseSponsorshipInstructionAsync,
  getCreatePoolInstructionAsync,
  getGameRecordEncoder,
  getPlatformConfigEncoder,
  getPoolEncoder,
  getReclaimInstructionAsync,
  getReclaimSponsorshipInstructionAsync,
  getRotateGateKeyInstructionAsync,
  getSampleVarInstructionAsync,
  getSponsorInstructionAsync,
  PayoutPreset,
  PoolStatus,
  type PlatformConfigArgs,
} from "../../src/generated/index.js";
import {
  allowlistProof,
  allowlistRoot,
  associatedTokenAddress,
  buyInstruction,
  closeCounterInstruction,
  closePoolInstruction,
  closeSponsorshipInstruction,
  configPda,
  counterPda,
  CREATOR_COUNTER_SIZE,
  createPoolInstruction,
  DEFAULT_ADDRESS,
  ENTROPY_PROGRAM,
  isSdkError,
  POOL_SIZE,
  poolPda,
  reclaimInstruction,
  reclaimSponsorshipInstruction,
  rotateGateKeyInstruction,
  sampleVarInstruction,
  SPONSORSHIP_SIZE,
  sponsorInstruction,
  sponsorshipPda,
  TOKEN_ACCOUNT_BYTES,
  vaultPda,
  walletOverridePda,
  type SdkError,
} from "../../src/index.js";
import {
  accountInfo,
  BUYER,
  CREATOR,
  fixedAddress,
  GAME,
  gameRecordArgs,
  mockClient,
  ORE_MINT,
  POOL,
  poolArgs,
  TOKEN_PROGRAM,
} from "./fixtures.js";

const FEE_WALLET = fixedAddress(5);
const VAR = fixedAddress(6);
const GATE = fixedAddress(7);

function configArgs(overrides: Partial<PlatformConfigArgs> = {}): PlatformConfigArgs {
  const sol = {
    enabled: true,
    mint: DEFAULT_ADDRESS,
    tokenProgram: DEFAULT_ADDRESS,
    decimals: 9,
    minPrice: 50_000_000n,
    step: 50_000_000n,
    maxPrice: 1_000_000_000n,
    maxSponsorship: 10_000_000_000n,
  };
  return {
    admin: CREATOR,
    scoreAuthority: CREATOR,
    entropyProvider: CREATOR,
    feeWallet: FEE_WALLET,
    platformBps: 250,
    creatorBps: 350,
    addonBudgetBps: 500,
    defaultPreset: 0,
    maxOpenPools: 10,
    maxOwnBoxes: 10,
    preseasonEnabled: false,
    paused: false,
    tokens: [
      sol,
      { ...sol, enabled: false, mint: fixedAddress(9), tokenProgram: TOKEN_PROGRAM, decimals: 6 },
      {
        ...sol,
        mint: ORE_MINT,
        tokenProgram: TOKEN_PROGRAM,
        decimals: 11,
        minPrice: 100n,
        step: 100n,
        maxPrice: 2_000n,
      },
    ],
    bump: 255,
    reserved: new Uint8Array(256),
    ...overrides,
  };
}

/** A mock chain: `accounts` maps address → raw bytes; absent addresses answer `null`. */
function chain(accounts: Record<string, Uint8Array>) {
  return mockClient({
    getAccountInfo: (address: Address) => ({
      context: { slot: 1n },
      value: accounts[address] ? accountInfo(accounts[address]) : null,
    }),
  });
}

function metas(ix: Instruction) {
  return (ix.accounts ?? []).map((a) => ({ address: a.address, role: a.role }));
}

let creator: KeyPairSigner;
let buyer: KeyPairSigner;
let configAddress: Address;
beforeAll(async () => {
  creator = await generateKeyPairSigner();
  buyer = await generateKeyPairSigner();
  configAddress = await configPda(chain({}).client);
});

const u8 = (b: ReadonlyUint8Array) => Uint8Array.from(b);
const encodePool = (o: Parameters<typeof poolArgs>[0]) => u8(getPoolEncoder().encode(poolArgs(o)));
const encodeConfig = (o: Partial<PlatformConfigArgs> = {}) =>
  u8(getPlatformConfigEncoder().encode(configArgs(o)));
const encodeGame = (o: Parameters<typeof gameRecordArgs>[0] = {}) =>
  u8(getGameRecordEncoder().encode(gameRecordArgs(o)));

describe("createPoolInstruction", () => {
  const base = () => ({
    [configAddress]: encodeConfig(),
    [GAME]: encodeGame(),
  });

  it("SOL, public: the account list is the generated builder's; Pool + counter bytes", async () => {
    const { client, calls } = chain(base());
    const r = await createPoolInstruction(client, {
      creator,
      game: GAME,
      nonce: 7n,
      token: 0,
      price: 100_000_000n,
      preset: PayoutPreset.Standard,
      access: { type: "public" },
      initialBoxes: 2,
    });
    const pool = await poolPda(client, GAME, creator.address, 7n);
    expect(r.pool).toBe(pool);
    expect(r.vault).toBe(await vaultPda(client, pool));
    expect(r.counter).toBe(await counterPda(client, creator.address, GAME));
    expect(r.walletOverride).toBe(await walletOverridePda(client, creator.address));
    expect(r.nonce).toBe(7n);
    expect(u8(r.allowlistRoot)).toEqual(new Uint8Array(32));
    expect(r.createdAccountBytes).toBe(POOL_SIZE + CREATOR_COUNTER_SIZE);
    const expected = await getCreatePoolInstructionAsync(
      {
        creator,
        game: GAME,
        pool,
        nonce: 7n,
        token: 0,
        price: 100_000_000n,
        preset: PayoutPreset.Standard,
        accessType: AccessType.Public,
        gateKey: DEFAULT_ADDRESS,
        allowlistRoot: new Uint8Array(32),
        creatorAddonBps: 0,
        integrator: DEFAULT_ADDRESS,
        integratorBps: 0,
        initialBoxes: 2,
      },
      { programAddress: client.programAddress },
    );
    expect(metas(r.instruction)).toEqual(metas(expected));
    expect(u8(r.instruction.data!)).toEqual(u8(expected.data!));
    // The three optional token slots are the program id (Anchor's "absent").
    expect(r.instruction.accounts![7]!.address).toBe(client.programAddress);
    expect(r.instruction.accounts![8]!.address).toBe(client.programAddress);
    expect(r.instruction.accounts![9]!.address).toBe(client.programAddress);
    expect(calls.map((c) => c.method)).toEqual([
      "getAccountInfo",
      "getAccountInfo",
      "getAccountInfo",
    ]);
  });

  it("ORE, allowlist, creator buys: mint/ATA/token program filled, the root computed, vault bytes counted", async () => {
    const wallets = [buyer.address, CREATOR, BUYER];
    const counter = await counterPda(chain({}).client, creator.address, GAME);
    const { client } = chain({ ...base(), [counter]: new Uint8Array(74) });
    const r = await createPoolInstruction(client, {
      creator,
      game: GAME,
      nonce: 1n,
      token: 2,
      price: 300n,
      preset: PayoutPreset.Standard,
      access: { type: "allowlist", wallets },
      creatorAddonBps: 100,
      integrator: BUYER,
      integratorBps: 50,
      initialBoxes: 1,
    });
    expect(u8(r.allowlistRoot)).toEqual(allowlistRoot(wallets.map(addressBytes)));
    expect(r.createdAccountBytes).toBe(POOL_SIZE + TOKEN_ACCOUNT_BYTES);
    const ata = await associatedTokenAddress(creator.address, ORE_MINT, TOKEN_PROGRAM);
    expect(r.instruction.accounts![7]!.address).toBe(ORE_MINT);
    expect(r.instruction.accounts![8]).toMatchObject({ address: ata, role: AccountRole.WRITABLE });
    expect(r.instruction.accounts![9]!.address).toBe(TOKEN_PROGRAM);
  });

  it("ORE with no initial boxes: the creator's ATA slot stays absent", async () => {
    const { client } = chain(base());
    const r = await createPoolInstruction(client, {
      creator,
      game: GAME,
      token: 2,
      price: 100n,
      preset: PayoutPreset.Standard,
      access: { type: "link", gateKey: GATE },
      initialBoxes: 0,
    });
    expect(r.instruction.accounts![8]!.address).toBe(client.programAddress);
    expect(typeof r.nonce).toBe("bigint"); // random when omitted
  });

  it("fails with the program's error name before simulation", async () => {
    const attempt = (accounts: Record<string, Uint8Array>, over: Record<string, unknown> = {}) =>
      createPoolInstruction(chain(accounts).client, {
        creator,
        game: GAME,
        token: 0,
        price: 100_000_000n,
        preset: PayoutPreset.Standard,
        access: { type: "public" },
        initialBoxes: 0,
        ...over,
      }).catch((e: SdkError) => e);
    const name = (e: unknown) => (isSdkError(e) ? e.details.programError : undefined);
    expect(
      name(await attempt({ ...base(), [configAddress]: encodeConfig({ paused: true }) })),
    ).toBe("Paused");
    expect(
      name(await attempt({ ...base(), [GAME]: encodeGame({ status: GameStatus.Final }) })),
    ).toBe("GameNotScheduled");
    expect(name(await attempt(base(), { now: 1_800_003_600n }))).toBe("SalesClosed");
    expect(name(await attempt(base(), { now: 1_800_003_599n }))).toBeUndefined();
    expect(name(await attempt(base(), { token: 1, price: 1n }))).toBe("TokenDisabled");
    expect(name(await attempt(base(), { token: 5, price: 1n }))).toBe("TokenDisabled");
    const off = await attempt(base(), { price: 75_000_000n });
    expect(isSdkError(off, "ProgramError")).toBe(true);
    expect(name(off)).toBe("PriceOffLadder");
    expect((off as SdkError).details.programErrorCode).toBe(6013);
    expect(isSdkError(await attempt({ [GAME]: encodeGame() }), "NotFound")).toBe(true);
    expect(isSdkError(await attempt({ [configAddress]: encodeConfig() }), "NotFound")).toBe(true);
  });
});

function addressBytes(a: Address): Uint8Array {
  return Uint8Array.from(getAddressEncoder().encode(a));
}

describe("buyInstruction", () => {
  const solPool = () => encodePool({ accessType: AccessType.Public });
  const expectedBuy = async (
    client: ReturnType<typeof chain>["client"],
    extra: Record<string, unknown>,
    proof: Uint8Array[] = [],
  ) =>
    getBuyInstructionAsync(
      {
        buyer,
        game: GAME,
        pool: POOL,
        vault: await vaultPda(client, POOL),
        counter: await counterPda(client, CREATOR, GAME),
        walletOverride: await walletOverridePda(client, CREATOR), // the creator's, not the buyer's
        count: 3,
        allowlistProof: proof,
        ...extra,
      },
      { programAddress: client.programAddress },
    );

  it("public SOL pool: the generated list, no gate key, empty proof", async () => {
    const { client, calls } = chain({ [POOL]: solPool() });
    const r = await buyInstruction(client, { buyer, pool: POOL, count: 3 });
    expect(metas(r.instruction)).toEqual(metas(await expectedBuy(client, {})));
    expect(r.instruction.accounts![12]!.address).toBe(client.programAddress); // gate slot absent
    expect(r.game).toBe(GAME);
    expect(r.buyerTokenAccount).toBeUndefined();
    expect(calls).toHaveLength(1); // the pool read only
  });

  it("link pool: the gate key signs read-only; ignored on a public pool", async () => {
    const gate = await generateKeyPairSigner();
    const { client } = chain({
      [POOL]: encodePool({ accessType: AccessType.Link, gateKey: gate.address }),
    });
    const r = await buyInstruction(client, {
      buyer,
      pool: POOL,
      count: 3,
      gate: { gateKey: gate },
    });
    expect(metas(r.instruction)).toEqual(
      metas(
        await expectedBuy(client, {
          gateKey: { address: gate.address, role: AccountRole.READONLY_SIGNER, signer: gate },
        }),
      ),
    );
    // Through a bare signer the generated builder follows the IDL and marks the slot read-only.
    expect((await expectedBuy(client, { gateKey: gate })).accounts[12]!.role).toBe(
      AccountRole.READONLY,
    );
    expect(r.instruction.accounts![12]).toMatchObject({
      address: gate.address,
      role: AccountRole.READONLY_SIGNER,
    });
    const pub = await buyInstruction(chain({ [POOL]: solPool() }).client, {
      buyer,
      pool: POOL,
      count: 3,
      gate: { gateKey: gate },
    });
    expect(pub.instruction.accounts![12]!.address).toBe(client.programAddress);
  });

  it("allowlist pool: the proof is built; a buyer not listed fails before any RPC", async () => {
    const wallets = [CREATOR, buyer.address, BUYER, GATE];
    const { client, calls } = chain({ [POOL]: encodePool({ accessType: AccessType.Allowlist }) });
    const r = await buyInstruction(client, { buyer, pool: POOL, count: 3, gate: { wallets } });
    const proof = allowlistProof(wallets.map(addressBytes), addressBytes(buyer.address));
    expect(proof.length).toBeGreaterThan(0);
    expect(u8(r.instruction.data!)).toEqual(u8((await expectedBuy(client, {}, proof)).data!));
    const { client: c2, calls: calls2 } = chain({
      [POOL]: encodePool({ accessType: AccessType.Allowlist }),
    });
    const e = await buyInstruction(c2, {
      buyer,
      pool: POOL,
      count: 1,
      gate: { wallets: [CREATOR] },
    }).catch((x) => x);
    expect(isSdkError(e, "NotAllowlisted")).toBe(true);
    expect(calls2).toHaveLength(0);
    expect(calls.length).toBe(1);
  });

  it("ORE pool: the buyer's ATA must exist", async () => {
    const ata = await associatedTokenAddress(buyer.address, ORE_MINT, TOKEN_PROGRAM);
    const ore = encodePool({ token: 2, mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM });
    const missing = await buyInstruction(chain({ [POOL]: ore }).client, {
      buyer,
      pool: POOL,
      count: 1,
    }).catch((x) => x);
    expect(isSdkError(missing, "TokenAccountMissing")).toBe(true);
    expect((missing as SdkError).details.address).toBe(ata);
    const { client } = chain({ [POOL]: ore, [ata]: new Uint8Array(165) });
    const r = await buyInstruction(client, { buyer, pool: POOL, count: 3 });
    expect(r.buyerTokenAccount).toBe(ata);
    expect(metas(r.instruction)).toEqual(
      metas(
        await expectedBuy(client, {
          mint: ORE_MINT,
          tokenProgram: TOKEN_PROGRAM,
          buyerTokenAccount: ata,
        }),
      ),
    );
    const notFound = await buyInstruction(chain({}).client, { buyer, pool: POOL, count: 1 }).catch(
      (x) => x,
    );
    expect(isSdkError(notFound, "NotFound")).toBe(true);
  });
});

describe("sponsorInstruction", () => {
  it("derives the sponsorship, counts 81 bytes only for a first sponsorship", async () => {
    const sponsorship = await sponsorshipPda(chain({}).client, POOL, buyer.address);
    const { client } = chain({ [POOL]: encodePool({}) });
    const r = await sponsorInstruction(client, { sponsor: buyer, pool: POOL, amount: 5n });
    expect(r.sponsorship).toBe(sponsorship);
    expect(r.createdAccountBytes).toBe(SPONSORSHIP_SIZE);
    const expected = await getSponsorInstructionAsync(
      {
        sponsor: buyer,
        game: GAME,
        pool: POOL,
        vault: await vaultPda(client, POOL),
        sponsorship,
        amount: 5n,
      },
      { programAddress: client.programAddress },
    );
    expect(metas(r.instruction)).toEqual(metas(expected));
    const again = await sponsorInstruction(
      chain({ [POOL]: encodePool({}), [sponsorship]: new Uint8Array(81) }).client,
      { sponsor: buyer, pool: POOL, amount: 5n },
    );
    expect(again.createdAccountBytes).toBe(0);
  });

  it("ORE pool: ATA filled and required", async () => {
    const ata = await associatedTokenAddress(buyer.address, ORE_MINT, TOKEN_PROGRAM);
    const ore = encodePool({ token: 2, mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM });
    const { client } = chain({ [POOL]: ore, [ata]: new Uint8Array(165) });
    const r = await sponsorInstruction(client, { sponsor: buyer, pool: POOL, amount: 5n });
    expect(r.sponsorTokenAccount).toBe(ata);
    expect(r.instruction.accounts![6]!.address).toBe(ORE_MINT);
    expect(r.instruction.accounts![7]).toMatchObject({ address: ata, role: AccountRole.WRITABLE });
    const e = await sponsorInstruction(chain({ [POOL]: ore }).client, {
      sponsor: buyer,
      pool: POOL,
      amount: 5n,
    }).catch((x) => x);
    expect(isSdkError(e, "TokenAccountMissing")).toBe(true);
  });
});

describe("rotateGateKeyInstruction and closeSponsorshipInstruction (no reads)", () => {
  it("match the generated builders and make no RPC call", async () => {
    const { client, calls } = chain({});
    const rot = await rotateGateKeyInstruction(client, { creator, pool: POOL, newKey: GATE });
    expect(metas(rot.instruction)).toEqual(
      metas(
        await getRotateGateKeyInstructionAsync(
          { creator, pool: POOL, newKey: GATE },
          { programAddress: client.programAddress },
        ),
      ),
    );
    const sponsorship = await sponsorshipPda(client, POOL, BUYER);
    const cs = await closeSponsorshipInstruction(client, { pool: POOL, sponsor: BUYER });
    expect(cs.sponsorship).toBe(sponsorship);
    expect(metas(cs.instruction)).toEqual(
      metas(
        await getCloseSponsorshipInstructionAsync(
          { pool: POOL, sponsorship, sponsor: BUYER },
          { programAddress: client.programAddress },
        ),
      ),
    );
    expect(
      cs.instruction.accounts!.some(
        (a) => a.role === AccountRole.READONLY_SIGNER || a.role === AccountRole.WRITABLE_SIGNER,
      ),
    ).toBe(false);
    expect(calls).toHaveLength(0);
  });
});

describe("reclaimInstruction and reclaimSponsorshipInstruction (PROGRAM §4.6)", () => {
  it("passes the counter exactly while the pool is Open", async () => {
    const counter = await counterPda(chain({}).client, CREATOR, GAME);
    const open = await reclaimInstruction(
      chain({ [POOL]: encodePool({ status: PoolStatus.Open }) }).client,
      { owner: buyer, pool: POOL },
    );
    expect(open.counter).toBe(counter);
    expect(open.instruction.accounts![4]).toMatchObject({
      address: counter,
      role: AccountRole.WRITABLE,
    });
    const returned = await reclaimInstruction(
      chain({ [POOL]: encodePool({ status: PoolStatus.Returned }) }).client,
      { owner: buyer, pool: POOL },
    );
    expect(returned.counter).toBeUndefined();
    expect(returned.instruction.accounts![4]!.address).toBe(PROGRAM_OF(returned));
    const client = chain({ [POOL]: encodePool({ status: PoolStatus.Returned }) }).client;
    const expected = await getReclaimInstructionAsync(
      { boxOwner: buyer, game: GAME, pool: POOL, vault: await vaultPda(client, POOL) },
      { programAddress: client.programAddress },
    );
    expect(metas(returned.instruction)).toEqual(metas(expected));
    expect(returned.createdAccountBytes).toBe(0);
  });

  it("ORE: the ATA is passed whether or not it exists, and counted when the program will create it", async () => {
    const ata = await associatedTokenAddress(buyer.address, ORE_MINT, TOKEN_PROGRAM);
    const ore = encodePool({
      token: 2,
      mint: ORE_MINT,
      tokenProgram: TOKEN_PROGRAM,
      status: PoolStatus.Returned,
    });
    const missing = await reclaimInstruction(chain({ [POOL]: ore }).client, {
      owner: buyer,
      pool: POOL,
    });
    expect(missing.ownerTokenAccount).toBe(ata);
    expect(missing.createdAccountBytes).toBe(TOKEN_ACCOUNT_BYTES);
    const present = await reclaimInstruction(
      chain({ [POOL]: ore, [ata]: new Uint8Array(165) }).client,
      { owner: buyer, pool: POOL },
    );
    expect(present.createdAccountBytes).toBe(0);
    expect(present.instruction.accounts![6]).toMatchObject({
      address: ata,
      role: AccountRole.WRITABLE,
    });
  });

  it("reclaimSponsorship: sponsorship derived, counter rule, generated list", async () => {
    const { client } = chain({ [POOL]: encodePool({ status: PoolStatus.Open }) });
    const r = await reclaimSponsorshipInstruction(client, { sponsor: buyer, pool: POOL });
    const sponsorship = await sponsorshipPda(client, POOL, buyer.address);
    const counter = await counterPda(client, CREATOR, GAME);
    expect(r.sponsorship).toBe(sponsorship);
    expect(r.counter).toBe(counter);
    const expected = await getReclaimSponsorshipInstructionAsync(
      {
        sponsor: buyer,
        game: GAME,
        pool: POOL,
        vault: await vaultPda(client, POOL),
        sponsorship,
        counter,
      },
      { programAddress: client.programAddress },
    );
    expect(metas(r.instruction)).toEqual(metas(expected));
    const ore = encodePool({
      token: 2,
      mint: ORE_MINT,
      tokenProgram: TOKEN_PROGRAM,
      status: PoolStatus.Returned,
    });
    const r2 = await reclaimSponsorshipInstruction(chain({ [POOL]: ore }).client, {
      sponsor: buyer,
      pool: POOL,
    });
    expect(r2.counter).toBeUndefined();
    expect(r2.createdAccountBytes).toBe(TOKEN_ACCOUNT_BYTES);
  });
});

function PROGRAM_OF(r: { instruction: Instruction }): Address {
  return r.instruction.programAddress;
}

describe("sampleVarInstruction", () => {
  it("reads var from the pool and names the Entropy program", async () => {
    const { client } = chain({ [POOL]: encodePool({ var: VAR }) });
    const r = await sampleVarInstruction(client, { signer: buyer, pool: POOL });
    expect(r.var).toBe(VAR);
    const expected = await getSampleVarInstructionAsync(
      { sampler: buyer, pool: POOL, var: VAR },
      { programAddress: client.programAddress },
    );
    expect(metas(r.instruction)).toEqual(metas(expected));
    expect(r.instruction.accounts![4]!.address).toBe(ENTROPY_PROGRAM);
  });
});

describe("closePoolInstruction and closeCounterInstruction (PROGRAM §4.7)", () => {
  it("closePool: fee wallet from config, creator from the pool; SOL has no token slots", async () => {
    const { client } = chain({
      [POOL]: encodePool({ status: PoolStatus.Settled }),
      [configAddress]: encodeConfig(),
    });
    const r = await closePoolInstruction(client, { payer: buyer, pool: POOL });
    expect(r.feeWallet).toBe(FEE_WALLET);
    expect(r.creator).toBe(CREATOR);
    expect(r.destinationTokenAccount).toBeUndefined();
    expect(r.createdAccountBytes).toBe(0);
    const expected = await getClosePoolInstructionAsync(
      {
        payer: buyer,
        pool: POOL,
        vault: await vaultPda(client, POOL),
        feeWallet: FEE_WALLET,
        creator: CREATOR,
      },
      { programAddress: client.programAddress },
    );
    expect(metas(r.instruction)).toEqual(metas(expected));
  });

  it("closePool on ORE: the fee wallet's ATA, counted when missing", async () => {
    const ata = await associatedTokenAddress(FEE_WALLET, ORE_MINT, TOKEN_PROGRAM);
    const ore = encodePool({
      token: 2,
      mint: ORE_MINT,
      tokenProgram: TOKEN_PROGRAM,
      status: PoolStatus.Settled,
    });
    const r = await closePoolInstruction(
      chain({ [POOL]: ore, [configAddress]: encodeConfig() }).client,
      { payer: buyer, pool: POOL },
    );
    expect(r.destinationTokenAccount).toBe(ata);
    expect(r.createdAccountBytes).toBe(TOKEN_ACCOUNT_BYTES);
    expect(r.instruction.accounts![7]).toMatchObject({ address: ata, role: AccountRole.WRITABLE });
    const noConfig = await closePoolInstruction(chain({ [POOL]: ore }).client, {
      payer: buyer,
      pool: POOL,
    }).catch((x) => x);
    expect(isSdkError(noConfig, "NotFound")).toBe(true);
  });

  it("closeCounter: counter derived, fee wallet from config, no signer", async () => {
    const { client } = chain({ [configAddress]: encodeConfig() });
    const r = await closeCounterInstruction(client, { creator: CREATOR, game: GAME });
    const counter = await counterPda(client, CREATOR, GAME);
    expect(r.counter).toBe(counter);
    expect(r.feeWallet).toBe(FEE_WALLET);
    const expected = await getCloseCounterInstructionAsync(
      { counter, feeWallet: FEE_WALLET },
      { programAddress: client.programAddress },
    );
    expect(metas(r.instruction)).toEqual(metas(expected));
    expect(r.instruction.accounts).toHaveLength(3);
    const noConfig = await closeCounterInstruction(chain({}).client, {
      creator: CREATOR,
      game: GAME,
    }).catch((x) => x);
    expect(isSdkError(noConfig, "NotFound")).toBe(true);
  });
});
