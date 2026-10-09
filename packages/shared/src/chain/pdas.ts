/**
 * Program-derived addresses, each `(client, …) → Promise<Address>`, wrapping the generated
 * finders with the client's program address. `gamePda` and `poolPda` are hand-derived: their
 * seeds include data (`GameKey`, `scheduled_kickoff`) or an argument (`nonce`), so the IDL
 * importer makes no PDA node for them. `associatedTokenAddress` is the SPL associated token
 * account by seeds (no `@solana-program/*` client; the program creates every token account it
 * needs itself).
 */
import {
  getAddressEncoder,
  getProgramDerivedAddress,
  type Address,
  type TransactionSigner,
} from "@solana/kit";

import { gameRecordSeeds, type GameKey } from "../gameKey.js";
import {
  findConfigPda,
  findCounterPda,
  findEventAuthorityPda,
  findSponsorshipPda,
  findVaultPda,
  findWalletOverridePda,
} from "../generated/index.js";
import { poolSeeds } from "../seeds.js";
import type { MyBarPoolClient } from "./client.js";

/** The SPL Token program. */
export const TOKEN_PROGRAM_ADDRESS = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" as Address;
/** The Token-2022 program. */
export const TOKEN_2022_PROGRAM_ADDRESS = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb" as Address;
/** The associated token account program. */
export const ASSOCIATED_TOKEN_PROGRAM_ADDRESS =
  "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL" as Address;
/** The system program. */
export const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111" as Address;
/** The SlotHashes sysvar (PROGRAM §6.1). */
export const SLOT_HASHES_SYSVAR_ADDRESS = "SysvarS1otHashes111111111111111111111111111" as Address;
/** `Pubkey::default()`: an unowned box, no integrator, no gate key. */
export const DEFAULT_ADDRESS = "11111111111111111111111111111111" as Address;

const bytes = (a: Address) => Uint8Array.from(getAddressEncoder().encode(a));

/** An `Address` or a signer; the latter's address is used. */
export type AddressLike = Address | TransactionSigner;
/** The address of an {@link AddressLike}. */
export function addressOf(a: AddressLike): Address {
  return typeof a === "string" ? a : a.address;
}

/** `["config"]` (PROGRAM §3.1). */
export async function configPda(client: MyBarPoolClient): Promise<Address> {
  return (await findConfigPda({ programAddress: client.programAddress }))[0];
}

/** `["game", key, scheduled_kickoff]` (PROGRAM §3.2). */
export async function gamePda(
  client: MyBarPoolClient,
  key: GameKey,
  scheduledKickoff: bigint,
): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: client.programAddress,
    seeds: gameRecordSeeds(key, scheduledKickoff),
  });
  return pda;
}

/** `["pool", game, creator, nonce]` (PROGRAM §3.3). */
export async function poolPda(
  client: MyBarPoolClient,
  game: Address,
  creator: Address,
  nonce: bigint,
): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: client.programAddress,
    seeds: poolSeeds(bytes(game), bytes(creator), nonce),
  });
  return pda;
}

/** `["vault", pool]` (PROGRAM §3.4). */
export async function vaultPda(client: MyBarPoolClient, pool: Address): Promise<Address> {
  return (await findVaultPda({ pool }, { programAddress: client.programAddress }))[0];
}

/** `["counter", creator, game]` (PROGRAM §3.5). */
export async function counterPda(
  client: MyBarPoolClient,
  creator: Address,
  game: Address,
): Promise<Address> {
  return (await findCounterPda({ creator, game }, { programAddress: client.programAddress }))[0];
}

/** `["override", wallet]` (PROGRAM §3.6). */
export async function walletOverridePda(
  client: MyBarPoolClient,
  wallet: Address,
): Promise<Address> {
  return (await findWalletOverridePda({ wallet }, { programAddress: client.programAddress }))[0];
}

/** `["sponsorship", pool, wallet]` (PROGRAM §3.7). */
export async function sponsorshipPda(
  client: MyBarPoolClient,
  pool: Address,
  wallet: Address,
): Promise<Address> {
  return (
    await findSponsorshipPda({ pool, sponsor: wallet }, { programAddress: client.programAddress })
  )[0];
}

/** Anchor's `["__event_authority"]`, the first account of every `emit_cpi!` pair (PROGRAM §7). */
export async function eventAuthorityPda(client: MyBarPoolClient): Promise<Address> {
  return (await findEventAuthorityPda({ programAddress: client.programAddress }))[0];
}

/** The associated token account of `owner` for `mint` under `tokenProgram` (PROGRAM §5.4). */
export async function associatedTokenAddress(
  owner: Address,
  mint: Address,
  tokenProgram: Address = TOKEN_PROGRAM_ADDRESS,
): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: ASSOCIATED_TOKEN_PROGRAM_ADDRESS,
    seeds: [bytes(owner), bytes(tokenProgram), bytes(mint)],
  });
  return pda;
}
