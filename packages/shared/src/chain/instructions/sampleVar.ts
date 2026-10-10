import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import { ENTROPY_PROGRAM } from "../../entropy.js";
import { getSampleVarInstructionAsync } from "../../generated/index.js";
import type { MyBarPoolClient } from "../client.js";
import { requirePool } from "./common.js";

/** Input to {@link sampleVarInstruction}. */
export interface SampleVarInput {
  /** Anyone; the program checks the window, not the signer (PROGRAM §4.5). */
  signer: TransactionSigner;
  pool: Address;
}

/**
 * `sample_var` (PROGRAM §4.5): samples the pool's Entropy `Var` in its window. Reads the pool
 * for `var`; the Entropy program id is `ENTROPY_PROGRAM` from `src/entropy.ts`.
 */
export async function sampleVarInstruction(
  client: MyBarPoolClient,
  input: SampleVarInput,
): Promise<{ instruction: Instruction; var: Address }> {
  const pool = await requirePool(client, input.pool);
  const instruction = await getSampleVarInstructionAsync(
    {
      sampler: input.signer,
      pool: pool.address,
      var: pool.data.var,
      entropyProgram: ENTROPY_PROGRAM as Address,
    },
    { programAddress: client.programAddress },
  );
  return { instruction, var: pool.data.var };
}
