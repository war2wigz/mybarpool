/**
 * DESIGN §10.4: the balance check the app runs before the wallet opens, so an
 * `InsufficientFunds` is named here rather than by a failed simulation.
 */
import { getBase64Encoder, type Address } from "@solana/kit";

import { getConfig } from "./accounts.js";
import type { MyBarPoolClient } from "./client.js";
import { SdkError } from "./errors.js";
import { associatedTokenAddress, configPda } from "./pdas.js";

/** Input to {@link checkFunds}. */
export interface CheckFundsInput {
  wallet: Address;
  /** PROGRAM §2 token index: 0 SOL, else an SPL token. */
  token: number;
  /** The SPL mint and its program; read from `config` when omitted on an SPL token. */
  mint?: Address;
  tokenProgram?: Address;
  /** What the instruction moves out of the wallet, base units of `token`. */
  amount: bigint;
  /** `networkFeeLamports(prepared).totalLamports`, plus any rent the instruction charges. */
  feeLamports: bigint;
}

/** Result of {@link checkFunds}. */
export type FundsCheck =
  | { ok: true }
  | {
      ok: false;
      /** `lamports`: SOL for the amount and/or the fee; `token`: the SPL balance. */
      kind: "lamports" | "token";
      /** Base units of what is short. */
      shortfall: bigint;
    };

/** Lamports at `address`, 0 when it does not exist. */
async function lamportsOf(client: MyBarPoolClient, address: Address): Promise<bigint> {
  const { value } = await client.rpc
    .getAccountInfo(address, { commitment: "confirmed", encoding: "base64" })
    .send();
  return value?.lamports ?? 0n;
}

/** The SPL token account's `amount` (u64 LE at offset 64), 0 when it does not exist. */
async function tokenBalanceOf(client: MyBarPoolClient, address: Address): Promise<bigint> {
  const { value } = await client.rpc
    .getAccountInfo(address, { commitment: "confirmed", encoding: "base64" })
    .send();
  if (!value) return 0n;
  // Kit's codec, never a Node-only global: the SDK runs in the browser and on the phone too.
  const bytes = Uint8Array.from(getBase64Encoder().encode(value.data[0]));
  if (bytes.length < 72) return 0n;
  return new DataView(bytes.buffer, bytes.byteOffset).getBigUint64(64, true);
}

/**
 * Whether `wallet` can cover `amount` in `token` plus `feeLamports` in SOL. Two reads at most;
 * the SOL check comes first so a wallet short of both reports the lamports.
 */
export async function checkFunds(
  client: MyBarPoolClient,
  input: CheckFundsInput,
): Promise<FundsCheck> {
  const lamports = await lamportsOf(client, input.wallet);
  const solNeeded = input.feeLamports + (input.token === 0 ? input.amount : 0n);
  if (lamports < solNeeded) {
    return { ok: false, kind: "lamports", shortfall: solNeeded - lamports };
  }
  if (input.token === 0) return { ok: true };

  let { mint, tokenProgram } = input;
  if (!mint || !tokenProgram) {
    const configAddress = await configPda(client);
    const config = await getConfig(client, configAddress);
    if (!config) throw new SdkError("NotFound", { address: configAddress });
    const rule = config.data.tokens[input.token];
    if (!rule) throw new SdkError("NotFound", { address: `token ${input.token}` });
    mint ??= rule.mint;
    tokenProgram ??= rule.tokenProgram;
  }
  const balance = await tokenBalanceOf(
    client,
    await associatedTokenAddress(input.wallet, mint, tokenProgram),
  );
  if (balance < input.amount) {
    return { ok: false, kind: "token", shortfall: input.amount - balance };
  }
  return { ok: true };
}
