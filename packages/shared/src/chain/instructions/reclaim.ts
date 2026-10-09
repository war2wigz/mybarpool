import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import { getReclaimInstructionAsync, PoolStatus } from "../../generated/index.js";
import type { MyBarPoolClient } from "../client.js";
import { TOKEN_ACCOUNT_BYTES } from "../fees.js";
import { poolPdas, requirePool, splOf, tokenAccountToCredit } from "./common.js";

export interface ReclaimInput {
  owner: TransactionSigner;
  pool: Address;
}

export interface ReclaimResult {
  instruction: Instruction;
  game: Address;
  vault: Address;
  /** Passed only while the pool is `Open` (PROGRAM §4.6). */
  counter: Address | undefined;
  ownerTokenAccount: Address | undefined;
  /** 256 when the owner's ATA will be created, 0 otherwise. */
  createdAccountBytes: number;
}

/**
 * `reclaim` (PROGRAM §4.6): a box owner takes their own boxes back from a returnable pool.
 * Reads the pool; `counter` goes in exactly when the pool is still `Open`; the owner's ATA is
 * created by the program when missing.
 */
export async function reclaimInstruction(
  client: MyBarPoolClient,
  input: ReclaimInput,
): Promise<ReclaimResult> {
  const pool = await requirePool(client, input.pool);
  const { vault, counter } = await poolPdas(client, pool);
  const spl = splOf(pool.data);
  const ata = await tokenAccountToCredit(client, input.owner.address, spl);
  const withCounter = pool.data.status === PoolStatus.Open;
  const instruction = await getReclaimInstructionAsync(
    {
      boxOwner: input.owner,
      game: pool.data.game,
      pool: pool.address,
      vault,
      ...(withCounter ? { counter } : {}),
      ...(spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : {}),
      ...(ata.address ? { boxOwnerTokenAccount: ata.address } : {}),
    },
    { programAddress: client.programAddress },
  );
  return {
    instruction,
    game: pool.data.game,
    vault,
    counter: withCounter ? counter : undefined,
    ownerTokenAccount: ata.address,
    createdAccountBytes: ata.creates ? TOKEN_ACCOUNT_BYTES : 0,
  };
}
