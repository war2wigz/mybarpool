import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import { getRotateGateKeyInstructionAsync } from "../../generated/index.js";
import type { MyBarPoolClient } from "../client.js";

export interface RotateGateKeyInput {
  creator: TransactionSigner;
  pool: Address;
  newKey: Address;
}

/** `rotate_gate_key` (PROGRAM §4.3): the creator replaces a `Link` pool's gate key. No read. */
export async function rotateGateKeyInstruction(
  client: MyBarPoolClient,
  input: RotateGateKeyInput,
): Promise<{ instruction: Instruction; pool: Address }> {
  const instruction = await getRotateGateKeyInstructionAsync(
    { creator: input.creator, pool: input.pool, newKey: input.newKey },
    { programAddress: client.programAddress },
  );
  return { instruction, pool: input.pool };
}
