/**
 * Offsets proven against the generated encoders (PROGRAM §3.2, §3.3, §3.7; `layout.rs` freezes
 * the same numbers), and the reads over a mock RPC.
 */
import { getAddressEncoder, getU64Encoder, getU16Encoder } from "@solana/kit";
import { describe, expect, it } from "vitest";

import {
  getCreatorCounterEncoder,
  getGameRecordEncoder,
  getPlatformConfigEncoder,
  getPoolDecoder,
  getPoolEncoder,
  getSponsorshipEncoder,
  getWalletOverrideEncoder,
  GameStatus,
  MYBARPOOL_PROGRAM_ADDRESS,
  PoolStatus,
  type PlatformConfigArgs,
} from "../../src/generated/index.js";
import {
  accountExists,
  createMyBarPoolClient,
  CREATOR_COUNTER_SIZE,
  GAME_RECORD_OFFSETS,
  GAME_RECORD_SIZE,
  generatedSizes,
  getConfig,
  getCreatorCounter,
  getGame,
  getPool,
  getSponsorship,
  getWalletOverride,
  listGames,
  listPools,
  listSponsors,
  PLATFORM_CONFIG_SIZE,
  POOL_OFFSETS,
  POOL_SIZE,
  poolsHoldingBoxesOf,
  SPONSORSHIP_OFFSETS,
  SPONSORSHIP_SIZE,
  WALLET_OVERRIDE_SIZE,
} from "../../src/index.js";
import {
  accountInfo,
  BUYER,
  CREATOR,
  DEFAULT,
  fixedAddress,
  GAME,
  gameRecordArgs,
  mockClient,
  mockRpc,
  POOL,
  poolArgs,
  PROGRAM,
  sponsorshipArgs,
} from "./fixtures.js";

const addr = getAddressEncoder();
const slice = (bytes: Uint8Array, offset: number, len: number) =>
  bytes.subarray(offset, offset + len);
const eq = (a: Uint8Array, b: ArrayLike<number>) =>
  a.length === b.length && a.every((x, i) => x === b[i]);

describe("account sizes and offsets (PROGRAM §3)", () => {
  it("the three sizes equal the generated codecs' and layout.rs", () => {
    const g = generatedSizes();
    expect(g).toEqual({
      pool: POOL_SIZE,
      sponsorship: SPONSORSHIP_SIZE,
      gameRecord: GAME_RECORD_SIZE,
    });
    expect([POOL_SIZE, SPONSORSHIP_SIZE, GAME_RECORD_SIZE]).toEqual([1442, 81, 153]);
    expect([PLATFORM_CONFIG_SIZE, CREATOR_COUNTER_SIZE, WALLET_OVERRIDE_SIZE]).toEqual([
      698, 74, 43,
    ]);
  });

  it("every POOL_OFFSETS constant locates its field in an encoded Pool", () => {
    const p = poolArgs({ status: PoolStatus.Drawn, sold: 8, token: 2, mint: fixedAddress(10) });
    const bytes = new Uint8Array(getPoolEncoder().encode(p));
    expect(bytes.length).toBe(POOL_SIZE);
    expect(eq(slice(bytes, POOL_OFFSETS.game, 32), addr.encode(GAME))).toBe(true);
    expect(eq(slice(bytes, POOL_OFFSETS.creator, 32), addr.encode(CREATOR))).toBe(true);
    expect(
      eq(slice(bytes, POOL_OFFSETS.nonce, 8), getU64Encoder().encode(0x1122334455667788n)),
    ).toBe(true);
    expect(bytes[POOL_OFFSETS.token]).toBe(2);
    expect(eq(slice(bytes, POOL_OFFSETS.mint, 32), addr.encode(fixedAddress(10)))).toBe(true);
    expect(bytes[POOL_OFFSETS.status]).toBe(PoolStatus.Drawn);
    expect(bytes[POOL_OFFSETS.sold]).toBe(8);
    for (let i = 0; i < 25; i++) {
      const expected = i < 3 ? CREATOR : i < 8 ? BUYER : DEFAULT;
      expect(
        eq(slice(bytes, POOL_OFFSETS.owners + 32 * i, 32), addr.encode(expected)),
        `owners[${i}]`,
      ).toBe(true);
    }
  });

  it("every SPONSORSHIP_OFFSETS constant locates its field", () => {
    const bytes = new Uint8Array(getSponsorshipEncoder().encode(sponsorshipArgs()));
    expect(bytes.length).toBe(SPONSORSHIP_SIZE);
    expect(eq(slice(bytes, SPONSORSHIP_OFFSETS.pool, 32), addr.encode(POOL))).toBe(true);
    expect(eq(slice(bytes, SPONSORSHIP_OFFSETS.wallet, 32), addr.encode(BUYER))).toBe(true);
    expect(
      eq(slice(bytes, SPONSORSHIP_OFFSETS.amount, 8), getU64Encoder().encode(0x0102030405060708n)),
    ).toBe(true);
  });

  it("every GAME_RECORD_OFFSETS constant locates its field", () => {
    const bytes = new Uint8Array(
      getGameRecordEncoder().encode(gameRecordArgs({ status: GameStatus.Suspended })),
    );
    expect(bytes.length).toBe(GAME_RECORD_SIZE);
    expect(eq(slice(bytes, GAME_RECORD_OFFSETS.key, 2), getU16Encoder().encode(2026))).toBe(true);
    expect([...slice(bytes, GAME_RECORD_OFFSETS.key + 2, 3)]).toEqual([2, 15, 8]);
    expect(
      eq(
        slice(bytes, GAME_RECORD_OFFSETS.scheduledKickoff, 8),
        getU64Encoder().encode(1_800_000_000n),
      ),
    ).toBe(true);
    expect(
      eq(
        slice(bytes, GAME_RECORD_OFFSETS.recordedKickoff, 8),
        getU64Encoder().encode(1_800_003_600n),
      ),
    ).toBe(true);
    expect(bytes[GAME_RECORD_OFFSETS.status]).toBe(GameStatus.Suspended);
  });
});

describe("reads over a mock RPC", () => {
  it("getPool returns the decoded account, or null when absent", async () => {
    const bytes = new Uint8Array(getPoolEncoder().encode(poolArgs()));
    const { client, calls } = mockClient({
      getAccountInfo: (a: string) => ({
        context: { slot: 1n },
        value: a === POOL ? accountInfo(bytes) : null,
      }),
    });
    const pool = await getPool(client, POOL);
    expect(pool?.address).toBe(POOL);
    expect(pool?.data.creator).toBe(CREATOR);
    expect(pool?.data.sold).toBe(8);
    expect(await getPool(client, GAME)).toBeNull();
    expect(calls.map((c) => c.method)).toEqual(["getAccountInfo", "getAccountInfo"]);
  });

  it("listPools sends dataSize 1442 plus one base64 memcmp per filter at the proven offsets", async () => {
    const bytes = new Uint8Array(getPoolEncoder().encode(poolArgs()));
    const { client, calls } = mockClient({
      getProgramAccounts: () => [{ pubkey: POOL, account: accountInfo(bytes) }],
    });
    const pools = await listPools(client, { game: GAME, status: PoolStatus.Open });
    expect(pools).toHaveLength(1);
    expect(pools[0]!.address).toBe(POOL);
    expect(pools[0]!.data.game).toBe(GAME);
    const [program, config] = calls[0]!.params as [
      string,
      { filters: unknown[]; encoding: string },
    ];
    expect(program).toBe(PROGRAM);
    expect(config.encoding).toBe("base64");
    expect(config.filters).toEqual([
      { dataSize: 1442n },
      {
        memcmp: {
          offset: 8n,
          bytes: Buffer.from(addr.encode(GAME)).toString("base64"),
          encoding: "base64",
        },
      },
      { memcmp: { offset: 311n, bytes: Buffer.from([0]).toString("base64"), encoding: "base64" } },
    ]);
  });

  it("listSponsors filters on the pool at offset 40 and decodes", async () => {
    const bytes = new Uint8Array(getSponsorshipEncoder().encode(sponsorshipArgs()));
    const { client, calls } = mockClient({
      getProgramAccounts: () => [{ pubkey: fixedAddress(7), account: accountInfo(bytes) }],
    });
    const s = await listSponsors(client, POOL);
    expect(s[0]!.data.wallet).toBe(BUYER);
    expect(s[0]!.data.amount).toBe(0x0102030405060708n);
    const [, config] = calls[0]!.params as [string, { filters: unknown[] }];
    expect(config.filters).toEqual([
      { dataSize: 81n },
      {
        memcmp: {
          offset: 8n,
          bytes: Buffer.from(addr.encode(POOL)).toString("base64"),
          encoding: "base64",
        },
      },
    ]);
  });

  it("poolsHoldingBoxesOf is the pure wallet view; the default address owns nothing", () => {
    const decoder = getPoolDecoder();
    const a = { address: POOL, data: decoder.decode(getPoolEncoder().encode(poolArgs())) };
    const b = {
      address: GAME,
      data: decoder.decode(getPoolEncoder().encode(poolArgs({ owners: Array(25).fill(DEFAULT) }))),
    };
    expect(poolsHoldingBoxesOf([a, b], BUYER).map((p) => p.address)).toEqual([POOL]);
    expect(poolsHoldingBoxesOf([a, b], CREATOR).map((p) => p.address)).toEqual([POOL]);
    expect(poolsHoldingBoxesOf([a, b], DEFAULT)).toEqual([]);
  });

  it("the other reads decode their account types and return null when absent", async () => {
    const game = new Uint8Array(getGameRecordEncoder().encode(gameRecordArgs()));
    const sponsorship = new Uint8Array(getSponsorshipEncoder().encode(sponsorshipArgs()));
    const override = new Uint8Array(
      getWalletOverrideEncoder().encode({
        wallet: CREATOR,
        maxOpenPools: 9,
        maxOwnBoxes: 7,
        bump: 1,
      }),
    );
    const counter = new Uint8Array(
      getCreatorCounterEncoder().encode({ creator: CREATOR, game: GAME, openCount: 2, bump: 1 }),
    );
    const config: PlatformConfigArgs = {
      admin: CREATOR,
      scoreAuthority: BUYER,
      entropyProvider: BUYER,
      feeWallet: fixedAddress(9),
      platformBps: 500,
      creatorBps: 500,
      addonBudgetBps: 500,
      defaultPreset: 0,
      maxOpenPools: 3,
      maxOwnBoxes: 5,
      preseasonEnabled: false,
      paused: false,
      tokens: [0, 1, 2].map((i) => ({
        enabled: i !== 1,
        mint: i === 0 ? DEFAULT : fixedAddress(10),
        tokenProgram: i === 0 ? DEFAULT : fixedAddress(11),
        decimals: 9,
        minPrice: 1n,
        step: 1n,
        maxPrice: 10n,
        maxSponsorship: 100n,
      })),
      bump: 255,
      reserved: new Uint8Array(256),
    };
    const configBytes = new Uint8Array(getPlatformConfigEncoder().encode(config));
    expect(configBytes.length).toBe(PLATFORM_CONFIG_SIZE);
    const table: Record<string, Uint8Array> = {
      [GAME]: game,
      [fixedAddress(7)]: sponsorship,
      [fixedAddress(6)]: override,
      [fixedAddress(5)]: counter,
      [fixedAddress(8)]: configBytes,
    };
    const { client, calls } = mockClient({
      getAccountInfo: (a: string) => ({
        context: { slot: 1n },
        value: table[a] ? accountInfo(table[a]) : null,
      }),
    });
    expect((await getGame(client, GAME))?.data.key.week).toBe(2);
    expect((await getSponsorship(client, fixedAddress(7)))?.data.amount).toBe(0x0102030405060708n);
    expect((await getWalletOverride(client, fixedAddress(6)))?.data.maxOwnBoxes).toBe(7);
    expect((await getCreatorCounter(client, fixedAddress(5)))?.data.openCount).toBe(2);
    expect((await getConfig(client, fixedAddress(8)))?.data.tokens[1]?.enabled).toBe(false);
    for (const read of [getGame, getSponsorship, getWalletOverride, getCreatorCounter, getConfig]) {
      expect(await read(client, POOL)).toBeNull();
    }
    expect(await accountExists(client, GAME)).toBe(true);
    expect(await accountExists(client, POOL)).toBe(false);
    expect(calls.every((c) => c.method === "getAccountInfo")).toBe(true);
  });

  it("listGames filters on status at offset 29 when given", async () => {
    const bytes = new Uint8Array(
      getGameRecordEncoder().encode(gameRecordArgs({ status: GameStatus.Final })),
    );
    const { client, calls } = mockClient({
      getProgramAccounts: () => [{ pubkey: GAME, account: accountInfo(bytes) }],
    });
    expect((await listGames(client))[0]!.data.status).toBe(GameStatus.Final);
    expect((calls[0]!.params[1] as { filters: unknown[] }).filters).toEqual([{ dataSize: 153n }]);
    await listGames(client, { status: GameStatus.Final });
    expect((calls[1]!.params[1] as { filters: unknown[] }).filters).toEqual([
      { dataSize: 153n },
      { memcmp: { offset: 29n, bytes: Buffer.from([4]).toString("base64"), encoding: "base64" } },
    ]);
  });

  it("createMyBarPoolClient defaults the program address and keeps subscriptions when given", () => {
    const { rpc } = mockRpc({});
    const bare = createMyBarPoolClient({ rpc });
    expect(bare.programAddress).toBe(MYBARPOOL_PROGRAM_ADDRESS);
    expect("rpcSubscriptions" in bare).toBe(false);
    const subs = {} as never;
    const withSubs = createMyBarPoolClient({ rpc, rpcSubscriptions: subs, programAddress: GAME });
    expect(withSubs.rpcSubscriptions).toBe(subs);
    expect(withSubs.programAddress).toBe(GAME);
  });
});
