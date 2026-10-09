import type { Address, Instruction } from "@solana/kit";

import { getCloseSponsorshipInstructionAsync } from "../../generated/index.js";
import type { MyBarPoolClient } from "../client.js";
import { sponsorshipPda } from "../pdas.js";

export interface CloseSponsorshipInput {
  pool: Address;
  /** The sponsor whose returned `Sponsorship` is closed; receives its rent. */
  sponsor: Address;
}

/**
 * `close_sponsorship` (PROGRAM §4.6): permissionless; no signer among the accounts, the fee
 * payer is whoever sends. No read: the PDA is derived from the inputs.
 */
export async function closeSponsorshipInstruction(
  client: MyBarPoolClient,
  input: CloseSponsorshipInput,
): Promise<{ instruction: Instruction; sponsorship: Address }> {
  const sponsorship = await sponsorshipPda(client, input.pool, input.sponsor);
  const instruction = await getCloseSponsorshipInstructionAsync(
    { pool: input.pool, sponsorship, sponsor: input.sponsor },
    { programAddress: client.programAddress },
  );
  return { instruction, sponsorship };
}
