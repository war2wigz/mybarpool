import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import { getSponsorInstructionAsync } from "../../generated/index.js";
import { accountExists, SPONSORSHIP_SIZE } from "../accounts.js";
import type { MyBarPoolClient } from "../client.js";
import { sponsorshipPda, vaultPda } from "../pdas.js";
import { existingTokenAccount, requirePool, splOf } from "./common.js";

export interface SponsorInput {
  sponsor: TransactionSigner;
  pool: Address;
  /** Base units; the program enforces the token's floor and cap (PROGRAM §4.4). */
  amount: bigint;
}

export interface SponsorResult {
  instruction: Instruction;
  game: Address;
  vault: Address;
  sponsorship: Address;
  sponsorTokenAccount: Address | undefined;
  /** 81 for a first sponsorship, 0 for a top-up. */
  createdAccountBytes: number;
}

/**
 * `sponsor` (PROGRAM §4.4). Reads the pool; derives `game`, `vault`, `sponsorship` and the
 * sponsor's ATA on an SPL pool (which must exist); one `getAccountInfo` tells whether the
 * `Sponsorship` is new.
 */
export async function sponsorInstruction(
  client: MyBarPoolClient,
  input: SponsorInput,
): Promise<SponsorResult> {
  const sponsor = input.sponsor.address;
  const pool = await requirePool(client, input.pool);
  const vault = await vaultPda(client, pool.address);
  const sponsorship = await sponsorshipPda(client, pool.address, sponsor);
  const spl = splOf(pool.data);
  const sponsorTokenAccount = await existingTokenAccount(client, sponsor, spl);
  const instruction = await getSponsorInstructionAsync(
    {
      sponsor: input.sponsor,
      game: pool.data.game,
      pool: pool.address,
      vault,
      sponsorship,
      ...(spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : {}),
      ...(sponsorTokenAccount ? { sponsorTokenAccount } : {}),
      amount: input.amount,
    },
    { programAddress: client.programAddress },
  );
  const exists = await accountExists(client, sponsorship);
  return {
    instruction,
    game: pool.data.game,
    vault,
    sponsorship,
    sponsorTokenAccount,
    createdAccountBytes: exists ? 0 : SPONSORSHIP_SIZE,
  };
}
