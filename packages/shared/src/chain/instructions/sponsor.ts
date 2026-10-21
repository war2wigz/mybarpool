import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import {
  getSponsorBoxInstructionAsync,
  getSponsorInstructionAsync,
} from "../../generated/index.js";
import { accountExists, SPONSORSHIP_SIZE } from "../accounts.js";
import type { MyBarPoolClient } from "../client.js";
import { sponsorshipPda, vaultPda } from "../pdas.js";
import { existingTokenAccount, requirePool, splOf } from "./common.js";

/** Input to {@link sponsorInstruction}. */
export interface SponsorInput {
  sponsor: TransactionSigner;
  pool: Address;
  /** Base units; the program enforces the token's floor and cap (PROGRAM §4.4). */
  amount: bigint;
}

/** Result of {@link sponsorInstruction}. */
export interface SponsorResult {
  instruction: Instruction;
  game: Address;
  vault: Address;
  sponsorship: Address;
  sponsorTokenAccount: Address | undefined;
  /** 85 for a first sponsorship, 0 for a top-up. */
  createdAccountBytes: number;
}

/** Input to {@link sponsorBoxInstruction}: a sponsorship plus the box the mark goes on. */
export interface SponsorBoxInput extends SponsorInput {
  /** 0–24; the program refuses a box that already carries a mark (PROGRAM §4.3). */
  boxIndex: number;
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
  return sponsorLike(client, input, undefined);
}

/**
 * `sponsor_box` (PROGRAM §4.3): the same sponsorship as {@link sponsorInstruction} plus the
 * sponsor's mark on `boxIndex`. The money is a sponsorship like any other; only the mark is
 * new, so the accounts and the result are {@link sponsorInstruction}'s.
 */
export async function sponsorBoxInstruction(
  client: MyBarPoolClient,
  input: SponsorBoxInput,
): Promise<SponsorResult> {
  if (!Number.isInteger(input.boxIndex) || input.boxIndex < 0 || input.boxIndex > 24) {
    throw new RangeError(`boxIndex must be 0–24, got ${input.boxIndex}`);
  }
  return sponsorLike(client, input, input.boxIndex);
}

async function sponsorLike(
  client: MyBarPoolClient,
  input: SponsorInput,
  boxIndex: number | undefined,
): Promise<SponsorResult> {
  const sponsor = input.sponsor.address;
  const pool = await requirePool(client, input.pool);
  const vault = await vaultPda(client, pool.address);
  const sponsorship = await sponsorshipPda(client, pool.address, sponsor);
  const spl = splOf(pool.data);
  const sponsorTokenAccount = await existingTokenAccount(client, sponsor, spl);
  const accounts = {
    sponsor: input.sponsor,
    game: pool.data.game,
    pool: pool.address,
    vault,
    sponsorship,
    ...(spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : {}),
    ...(sponsorTokenAccount ? { sponsorTokenAccount } : {}),
  };
  const options = { programAddress: client.programAddress };
  const instruction =
    boxIndex === undefined
      ? await getSponsorInstructionAsync({ ...accounts, amount: input.amount }, options)
      : await getSponsorBoxInstructionAsync(
          { ...accounts, boxIndex, amount: input.amount },
          options,
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
