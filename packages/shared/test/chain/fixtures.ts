/**
 * Shared fixtures for the chain-layer unit tests: a `Pool`, a `Sponsorship` and a `GameRecord`
 * with distinct sentinel values (the `layout.rs` pattern), a mock RPC that records every call,
 * and a client over it. No network anywhere in `test/chain`.
 */
import { address, type Address, type Rpc } from "@solana/kit";

import {
  AccessType,
  GameStatus,
  PayoutPreset,
  PoolStatus,
  type GameRecordArgs,
  type PoolArgs,
  type SponsorshipArgs,
} from "../../src/generated/index.js";
import {
  createMyBarPoolClient,
  type MyBarPoolClient,
  type MyBarPoolRpcApi,
} from "../../src/index.js";

export const fixedAddress = (byte: number): Address =>
  address(
    // base58 of a 32-byte array filled with `byte` is not needed; distinct valid addresses are.
    [
      "TJWdV8jWMDDmbx7McMTVQsRGQ87pG8MVFXd4bfhM7dm",
      "Fq3rxgkHE5hjaoKWWZqWQo8hjEP9TGjd4qPXa7VpkRbw",
      "5hhyQJ2SpeEvC4q5rDDuu1bFLdzZfJGXkN61PL2EK7a3",
      "CNKBgcCvxy5WRuScuzJBeg2dMPw7dn5tBtA2bd2g4K2N",
      "5s5xfrJrMXCMAKFcKf6FWiVb1SgUtnjTwxPnxakGHtM5",
      "ELYx3R1x18T1Sp1p1D1FBkxk3DMUHxTpuY1mciNPctXd",
      "BznSYup22WP5NZtjiv5ZUwzeEsU4wHkQXdQsaBhU1AMZ",
      "5mhHbjCc8chp1Suj3EMTgfy3dLmF1hezwfRz4n6k1ewA",
      "FvzYvTNMUZty8tZ3WLRW4yibizzR2HE3ff11fT3BoXvX",
      "E9Vmosqg6AtuvviST4YdX3RchMuQerLyb4URW3rNfPU5",
      "oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp",
      "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
    ][byte % 12]!,
  );

export const DEFAULT = address("11111111111111111111111111111111");
export const PROGRAM = address("3jk4YM9xnpoMYK3nuaUHZ59SCDxwExfT9EHxwP2wRMBw");

export const GAME = fixedAddress(2);
export const CREATOR = fixedAddress(0);
export const BUYER = fixedAddress(1);
export const POOL = fixedAddress(3);
export const VAULT = fixedAddress(4);
export const ORE_MINT = fixedAddress(10);
export const TOKEN_PROGRAM = fixedAddress(11);

/** A pool with every field a distinct sentinel; `owners` 0–2 to the creator, 3–7 to the buyer. */
export function poolArgs(overrides: Partial<PoolArgs> = {}): PoolArgs {
  const owners = Array.from({ length: 25 }, (_, i) => (i < 3 ? CREATOR : i < 8 ? BUYER : DEFAULT));
  return {
    game: GAME,
    creator: CREATOR,
    nonce: 0x1122334455667788n,
    token: 0,
    mint: DEFAULT,
    tokenProgram: DEFAULT,
    vault: VAULT,
    price: 50_000_000n,
    preset: PayoutPreset.Standard,
    accessType: AccessType.Public,
    gateKey: DEFAULT,
    allowlistRoot: new Uint8Array(32),
    creatorAddonBps: 200,
    integrator: DEFAULT,
    integratorBps: 0,
    platformFee: 62_500_000n,
    creatorFee: 87_500_000n,
    integratorFee: 0n,
    status: PoolStatus.Open,
    sold: 8,
    owners,
    creatorBoxes: 3,
    sponsoredTotal: 0n,
    sponsorCount: 0,
    sponsorshipsOpen: 0,
    var: DEFAULT,
    varEndAt: 0n,
    sampledSlot: 0n,
    sampledHash: new Uint8Array(32),
    varReplacements: 0,
    drawn: false,
    homeAxis: new Uint8Array(10),
    awayAxis: new Uint8Array(10),
    prizePool: 0n,
    quarterPrize: [0n, 0n, 0n, 0n],
    quartersSettled: 0,
    winningBox: new Uint8Array([255, 255, 255, 255]),
    feesPaid: false,
    unpaidPrizePool: 0n,
    returned: 0,
    splitAmount: 0n,
    cancelledByAdmin: false,
    abandoned: false,
    createdAt: 1_800_000_000n,
    lockedAt: 0n,
    bump: 254,
    vaultBump: 253,
    varCommit: new Uint8Array(32),
    reserved: new Uint8Array(96),
    ...overrides,
  };
}

export function sponsorshipArgs(overrides: Partial<SponsorshipArgs> = {}): SponsorshipArgs {
  return { pool: POOL, wallet: BUYER, amount: 0x0102030405060708n, bump: 251, ...overrides };
}

export function gameRecordArgs(overrides: Partial<GameRecordArgs> = {}): GameRecordArgs {
  return {
    key: { season: 2026, week: 2, home: 15, away: 8 },
    scheduledKickoff: 1_800_000_000n,
    recordedKickoff: 1_800_003_600n,
    status: GameStatus.Scheduled,
    quartersPosted: 0,
    homeScore: [0, 0, 0, 0],
    awayScore: [0, 0, 0, 0],
    postedAt: [0n, 0n, 0n, 0n],
    finalHadOvertime: false,
    markedAt: 0n,
    bump: 250,
    reserved: new Uint8Array(96),
    ...overrides,
  };
}

/** One recorded RPC call: the method and its params. */
export interface RpcCall {
  method: string;
  params: unknown[];
}

/**
 * A mock `Rpc`: `handlers[method](...params)` returns the `send()` value; every call is recorded
 * in order so tests can assert what was called and in what order.
 */
export function mockRpc(handlers: Record<string, (...params: never[]) => unknown>): {
  rpc: Rpc<MyBarPoolRpcApi>;
  calls: RpcCall[];
} {
  const calls: RpcCall[] = [];
  const rpc = new Proxy(
    {},
    {
      get(_t, method: string) {
        return (...params: unknown[]) => ({
          send: async () => {
            calls.push({ method, params });
            const h = handlers[method];
            if (!h) throw new Error(`mock RPC: no handler for ${method}`);
            return (h as (...p: unknown[]) => unknown)(...params);
          },
        });
      },
    },
  ) as Rpc<MyBarPoolRpcApi>;
  return { rpc, calls };
}

export function mockClient(
  handlers: Record<string, (...params: never[]) => unknown>,
  programAddress: Address = PROGRAM,
): { client: MyBarPoolClient; calls: RpcCall[] } {
  const { rpc, calls } = mockRpc(handlers);
  return { client: createMyBarPoolClient({ rpc, programAddress }), calls };
}

/** An `getAccountInfo` value for raw bytes under `owner`. */
export function accountInfo(data: Uint8Array, owner: Address = PROGRAM) {
  return {
    data: [Buffer.from(data).toString("base64"), "base64"] as const,
    executable: false,
    lamports: 1_000_000n,
    owner,
    rentEpoch: 0n,
    space: BigInt(data.length),
  };
}
