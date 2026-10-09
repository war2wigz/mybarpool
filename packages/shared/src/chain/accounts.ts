/**
 * Reads: one account by address (the decoded account or `null`, never a throw on absence),
 * program-account listings by `dataSize` + `memcmp` on proven offsets, and the pure wallet view
 * over a list of pools. Offsets are declared here as constants and proven against the
 * generated encoders in `test/chain/accounts.test.ts`, so a filter can never drift from the
 * codec silently (PROGRAM §3.3, §3.7, §3.2; `layout.rs` freezes the same numbers).
 */
import {
  getAddressEncoder,
  getBase64Decoder,
  getBase64Encoder,
  getU8Encoder,
  type Address,
  type Base64EncodedBytes,
  type GetProgramAccountsMemcmpFilter,
  type ReadonlyUint8Array,
} from "@solana/kit";

import {
  fetchMaybeCreatorCounter,
  fetchMaybeGameRecord,
  fetchMaybePlatformConfig,
  fetchMaybePool,
  fetchMaybeSponsorship,
  fetchMaybeWalletOverride,
  getGameRecordDecoder,
  getGameRecordSize,
  getPoolDecoder,
  getPoolSize,
  getSponsorshipDecoder,
  getSponsorshipSize,
  type CreatorCounter,
  type GameRecord,
  type GameStatus,
  type PlatformConfig,
  type Pool,
  type PoolStatus,
  type Sponsorship,
  type WalletOverride,
} from "../generated/index.js";
import type { MyBarPoolClient } from "./client.js";
import { DEFAULT_ADDRESS } from "./pdas.js";

/** An account read from the chain: its address and decoded data. */
export interface Fetched<T> {
  readonly address: Address;
  readonly data: T;
}

/** `Pool` is 1,442 bytes (PROGRAM §3.3). */
export const POOL_SIZE = 1_442;
/** Byte offsets into a `Pool` account (discriminator at 0–8). */
export const POOL_OFFSETS = Object.freeze({
  game: 8,
  creator: 40,
  nonce: 72,
  token: 80,
  mint: 81,
  /** `PoolStatus` discriminant, one byte. */
  status: 311,
  sold: 312,
  /** `owners[i]` is at `owners + 32 i` for `i` in 0–24. */
  owners: 313,
});
/** `Sponsorship` is 81 bytes (PROGRAM §3.7). */
export const SPONSORSHIP_SIZE = 81;
/** Byte offsets into a `Sponsorship` account. */
export const SPONSORSHIP_OFFSETS = Object.freeze({ pool: 8, wallet: 40, amount: 72 });
/** `GameRecord` is 153 bytes (PROGRAM §3.2). */
export const GAME_RECORD_SIZE = 153;
/** Byte offsets into a `GameRecord` account. */
export const GAME_RECORD_OFFSETS = Object.freeze({
  /** `GameKey`: `season` u16 LE, `week`, `home`, `away`. */
  key: 8,
  scheduledKickoff: 13,
  recordedKickoff: 21,
  /** `GameStatus` discriminant, one byte. */
  status: 29,
});
/** `PlatformConfig` is 698 bytes (PROGRAM §3.1). */
export const PLATFORM_CONFIG_SIZE = 698;
/** `CreatorCounter` is 74 bytes (PROGRAM §3.5). */
export const CREATOR_COUNTER_SIZE = 74;
/** `WalletOverride` is 43 bytes (PROGRAM §3.6). */
export const WALLET_OVERRIDE_SIZE = 43;

const COMMITMENT = "confirmed" as const;

function orNull<T>(
  m: { exists: false } | { exists: true; address: Address; data: T },
): Fetched<T> | null {
  return m.exists ? { address: m.address, data: m.data } : null;
}

/** The `PlatformConfig` at `address`, or `null`. */
export async function getConfig(client: MyBarPoolClient, address: Address) {
  return orNull<PlatformConfig>(
    await fetchMaybePlatformConfig(client.rpc, address, { commitment: COMMITMENT }),
  );
}
/** The `GameRecord` at `address`, or `null`. */
export async function getGame(client: MyBarPoolClient, address: Address) {
  return orNull<GameRecord>(
    await fetchMaybeGameRecord(client.rpc, address, { commitment: COMMITMENT }),
  );
}
/** The `Pool` at `address`, or `null` (after `close_pool`, say). */
export async function getPool(client: MyBarPoolClient, address: Address) {
  return orNull<Pool>(await fetchMaybePool(client.rpc, address, { commitment: COMMITMENT }));
}
/** The `Sponsorship` at `address`, or `null`. */
export async function getSponsorship(client: MyBarPoolClient, address: Address) {
  return orNull<Sponsorship>(
    await fetchMaybeSponsorship(client.rpc, address, { commitment: COMMITMENT }),
  );
}
/** The `WalletOverride` at `address`, or `null` (most wallets have none). */
export async function getWalletOverride(client: MyBarPoolClient, address: Address) {
  return orNull<WalletOverride>(
    await fetchMaybeWalletOverride(client.rpc, address, { commitment: COMMITMENT }),
  );
}
/** The `CreatorCounter` at `address`, or `null` (closed, or the creator has no pool on the game). */
export async function getCreatorCounter(client: MyBarPoolClient, address: Address) {
  return orNull<CreatorCounter>(
    await fetchMaybeCreatorCounter(client.rpc, address, { commitment: COMMITMENT }),
  );
}

/** Whether an account exists at `address` (any owner, any data). */
export async function accountExists(client: MyBarPoolClient, address: Address): Promise<boolean> {
  const { value } = await client.rpc
    .getAccountInfo(address, { commitment: COMMITMENT, encoding: "base64" })
    .send();
  return value !== null;
}

function memcmp(offset: number, bytes: ReadonlyUint8Array): GetProgramAccountsMemcmpFilter {
  return {
    memcmp: {
      offset: BigInt(offset),
      bytes: getBase64Decoder().decode(bytes) as Base64EncodedBytes,
      encoding: "base64",
    },
  };
}

async function programAccounts(
  client: MyBarPoolClient,
  dataSize: number,
  filters: GetProgramAccountsMemcmpFilter[],
): Promise<Array<{ address: Address; bytes: Uint8Array }>> {
  const result = await client.rpc
    .getProgramAccounts(client.programAddress, {
      commitment: COMMITMENT,
      encoding: "base64",
      filters: [{ dataSize: BigInt(dataSize) }, ...filters],
    })
    .send();
  const base64 = getBase64Encoder();
  return result.map((a) => ({
    address: a.pubkey,
    bytes: new Uint8Array(base64.encode(a.account.data[0])),
  }));
}

/** Filters for {@link listPools}; each present key is one `memcmp`, ANDed. */
export interface PoolFilters {
  game?: Address;
  creator?: Address;
  status?: PoolStatus;
}

/**
 * Pools on the program matching every given filter (`dataSize` 1,442 plus one `memcmp` per
 * key). "Pools where this wallet owns a box" is not one query — 25 offsets, OR — so wallet
 * views are {@link poolsHoldingBoxesOf} over the pools of the games in view.
 */
export async function listPools(
  client: MyBarPoolClient,
  filters: PoolFilters = {},
): Promise<Fetched<Pool>[]> {
  const address = getAddressEncoder();
  const f: GetProgramAccountsMemcmpFilter[] = [];
  if (filters.game !== undefined) f.push(memcmp(POOL_OFFSETS.game, address.encode(filters.game)));
  if (filters.creator !== undefined) {
    f.push(memcmp(POOL_OFFSETS.creator, address.encode(filters.creator)));
  }
  if (filters.status !== undefined) {
    f.push(memcmp(POOL_OFFSETS.status, getU8Encoder().encode(filters.status)));
  }
  const decoder = getPoolDecoder();
  const raw = await programAccounts(client, POOL_SIZE, f);
  return raw.map(({ address: a, bytes }) => ({ address: a, data: decoder.decode(bytes) }));
}

/** The open `Sponsorship` accounts of `pool` (`dataSize` 81, `memcmp` on `pool`). */
export async function listSponsors(
  client: MyBarPoolClient,
  pool: Address,
): Promise<Fetched<Sponsorship>[]> {
  const decoder = getSponsorshipDecoder();
  const raw = await programAccounts(client, SPONSORSHIP_SIZE, [
    memcmp(SPONSORSHIP_OFFSETS.pool, getAddressEncoder().encode(pool)),
  ]);
  return raw.map(({ address: a, bytes }) => ({ address: a, data: decoder.decode(bytes) }));
}

/** Filters for {@link listGames}. */
export interface GameFilters {
  status?: GameStatus;
}

/** The `GameRecord`s on the program (`dataSize` 153, optionally `memcmp` on `status`). */
export async function listGames(
  client: MyBarPoolClient,
  filters: GameFilters = {},
): Promise<Fetched<GameRecord>[]> {
  const f: GetProgramAccountsMemcmpFilter[] = [];
  if (filters.status !== undefined) {
    f.push(memcmp(GAME_RECORD_OFFSETS.status, getU8Encoder().encode(filters.status)));
  }
  const decoder = getGameRecordDecoder();
  const raw = await programAccounts(client, GAME_RECORD_SIZE, f);
  return raw.map(({ address: a, bytes }) => ({ address: a, data: decoder.decode(bytes) }));
}

/** The pools among `pools` in which `wallet` owns at least one box (pure; the wallet view). */
export function poolsHoldingBoxesOf<T extends { data: Pool }>(
  pools: readonly T[],
  wallet: Address,
): T[] {
  return pools.filter((p) => p.data.owners.some((o) => o === wallet && o !== DEFAULT_ADDRESS));
}

/** The generated sizes, for the proof test and for callers that want them in one place. */
export function generatedSizes(): { pool: number; sponsorship: number; gameRecord: number } {
  return {
    pool: getPoolSize(),
    sponsorship: getSponsorshipSize(),
    gameRecord: getGameRecordSize(),
  };
}
