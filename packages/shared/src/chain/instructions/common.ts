/**
 * What the instruction helpers share: the pool read that most of them start with, the SPL
 * plumbing a pool implies, the token-account existence check DESIGN §10.4 asks for before the
 * wallet opens, and a program error raised by name before simulation.
 */
import { getAddressEncoder, type Address, type TransactionSigner } from "@solana/kit";

import { MYBARPOOL_ERROR_NAMES, type Pool } from "../../generated/index.js";
import { accountExists, getPool, type Fetched } from "../accounts.js";
import type { MyBarPoolClient } from "../client.js";
import { SdkError } from "../errors.js";
import { associatedTokenAddress, counterPda, DEFAULT_ADDRESS, vaultPda } from "../pdas.js";

/** The pool at `address`, or `SdkError("NotFound")`. */
export async function requirePool(
  client: MyBarPoolClient,
  address: Address,
): Promise<Fetched<Pool>> {
  const pool = await getPool(client, address);
  if (!pool) throw new SdkError("NotFound", { address });
  return pool;
}

/** `mint` and `tokenProgram` for an SPL pool; `undefined` for SOL (PROGRAM §2: token index 0). */
export function splOf(pool: Pool): { mint: Address; tokenProgram: Address } | undefined {
  return pool.token === 0 ? undefined : { mint: pool.mint, tokenProgram: pool.tokenProgram };
}

/**
 * The wallet's ATA for an SPL pool, which must already exist (`SdkError("TokenAccountMissing")`:
 * the program never creates a payer's token account); `undefined` for SOL.
 */
export async function existingTokenAccount(
  client: MyBarPoolClient,
  wallet: Address,
  spl: { mint: Address; tokenProgram: Address } | undefined,
): Promise<Address | undefined> {
  if (!spl) return undefined;
  const ata = await associatedTokenAddress(wallet, spl.mint, spl.tokenProgram);
  if (!(await accountExists(client, ata))) {
    throw new SdkError("TokenAccountMissing", { address: ata });
  }
  return ata;
}

/**
 * The wallet's ATA for an SPL pool whether or not it exists, and whether the program will have
 * to create it (`reclaim`, `reclaim_sponsorship`, `close_pool` carry the ATA program for that).
 */
export async function tokenAccountToCredit(
  client: MyBarPoolClient,
  wallet: Address,
  spl: { mint: Address; tokenProgram: Address } | undefined,
): Promise<{ address: Address | undefined; creates: boolean }> {
  if (!spl) return { address: undefined, creates: false };
  const ata = await associatedTokenAddress(wallet, spl.mint, spl.tokenProgram);
  return { address: ata, creates: !(await accountExists(client, ata)) };
}

/** `vault` and `counter` for a pool: from its address and its `creator`/`game`. */
export async function poolPdas(
  client: MyBarPoolClient,
  pool: Fetched<Pool>,
): Promise<{ vault: Address; counter: Address }> {
  return {
    vault: await vaultPda(client, pool.address),
    counter: await counterPda(client, pool.data.creator, pool.data.game),
  };
}

const CODE_BY_NAME: ReadonlyMap<string, number> = new Map(
  Object.entries(MYBARPOOL_ERROR_NAMES).map(([code, name]) => [name, Number(code)]),
);

/** The program's error `name`, raised before any simulation; the code is the IDL's. */
export function programError(name: string): SdkError {
  const code = CODE_BY_NAME.get(name);
  return new SdkError("ProgramError", {
    programError: name,
    ...(code !== undefined ? { programErrorCode: code } : {}),
  });
}

/** The address of a signer or an address. */
export function addr(a: Address | TransactionSigner): Address {
  return typeof a === "string" ? a : a.address;
}

/** The 32 bytes of an address. */
export function addressBytes(a: Address): Uint8Array {
  return Uint8Array.from(getAddressEncoder().encode(a));
}

export { DEFAULT_ADDRESS };
