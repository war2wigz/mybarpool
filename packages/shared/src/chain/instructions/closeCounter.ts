import type { Address, Instruction } from "@solana/kit";

import { getCloseCounterInstructionAsync } from "../../generated/index.js";
import { getConfig } from "../accounts.js";
import type { MyBarPoolClient } from "../client.js";
import { SdkError } from "../errors.js";
import { configPda, counterPda } from "../pdas.js";

/** Input to {@link closeCounterInstruction}. */
export interface CloseCounterInput {
  creator: Address;
  game: Address;
}

/**
 * `close_counter` (PROGRAM §4.7): permissionless close of an empty `CreatorCounter`; its rent
 * goes to the fee wallet, read from `config`.
 */
export async function closeCounterInstruction(
  client: MyBarPoolClient,
  input: CloseCounterInput,
): Promise<{ instruction: Instruction; counter: Address; feeWallet: Address }> {
  const configAddress = await configPda(client);
  const config = await getConfig(client, configAddress);
  if (!config) throw new SdkError("NotFound", { address: configAddress });
  const counter = await counterPda(client, input.creator, input.game);
  const instruction = await getCloseCounterInstructionAsync(
    { counter, feeWallet: config.data.feeWallet },
    { programAddress: client.programAddress },
  );
  return { instruction, counter, feeWallet: config.data.feeWallet };
}
