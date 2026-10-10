import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import { getReclaimSponsorshipInstructionAsync, PoolStatus } from "../../generated/index.js";
import type { MyBarPoolClient } from "../client.js";
import { TOKEN_ACCOUNT_BYTES } from "../fees.js";
import { sponsorshipPda } from "../pdas.js";
import { poolPdas, requirePool, splOf, tokenAccountToCredit } from "./common.js";

/** Input to {@link reclaimSponsorshipInstruction}. */
export interface ReclaimSponsorshipInput {
  sponsor: TransactionSigner;
  pool: Address;
}

/** Result of {@link reclaimSponsorshipInstruction}. */
export interface ReclaimSponsorshipResult {
  instruction: Instruction;
  game: Address;
  vault: Address;
  sponsorship: Address;
  /** Passed only while the pool is `Open` (PROGRAM §4.6). */
  counter: Address | undefined;
  sponsorTokenAccount: Address | undefined;
  createdAccountBytes: number;
}

/**
 * `reclaim_sponsorship` (PROGRAM §4.6): a sponsor takes their sponsorship back from a
 * returnable pool. Same shape as {@link reclaimInstruction} plus the `Sponsorship`.
 */
export async function reclaimSponsorshipInstruction(
  client: MyBarPoolClient,
  input: ReclaimSponsorshipInput,
): Promise<ReclaimSponsorshipResult> {
  const pool = await requirePool(client, input.pool);
  const { vault, counter } = await poolPdas(client, pool);
  const sponsorship = await sponsorshipPda(client, pool.address, input.sponsor.address);
  const spl = splOf(pool.data);
  const ata = await tokenAccountToCredit(client, input.sponsor.address, spl);
  const withCounter = pool.data.status === PoolStatus.Open;
  const instruction = await getReclaimSponsorshipInstructionAsync(
    {
      sponsor: input.sponsor,
      game: pool.data.game,
      pool: pool.address,
      vault,
      sponsorship,
      ...(withCounter ? { counter } : {}),
      ...(spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : {}),
      ...(ata.address ? { sponsorTokenAccount: ata.address } : {}),
    },
    { programAddress: client.programAddress },
  );
  return {
    instruction,
    game: pool.data.game,
    vault,
    sponsorship,
    counter: withCounter ? counter : undefined,
    sponsorTokenAccount: ata.address,
    createdAccountBytes: ata.creates ? TOKEN_ACCOUNT_BYTES : 0,
  };
}
