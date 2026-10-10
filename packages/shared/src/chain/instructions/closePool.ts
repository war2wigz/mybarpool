import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import { getClosePoolInstructionAsync } from "../../generated/index.js";
import { getConfig } from "../accounts.js";
import type { MyBarPoolClient } from "../client.js";
import { SdkError } from "../errors.js";
import { TOKEN_ACCOUNT_BYTES } from "../fees.js";
import { configPda, vaultPda } from "../pdas.js";
import { requirePool, splOf, tokenAccountToCredit } from "./common.js";

/** Input to {@link closePoolInstruction}. */
export interface ClosePoolInput {
  /** Whoever pays the fee; the rent goes to the creator and the dust to the fee wallet. */
  payer: TransactionSigner;
  pool: Address;
}

/** Result of {@link closePoolInstruction}. */
export interface ClosePoolResult {
  instruction: Instruction;
  vault: Address;
  creator: Address;
  feeWallet: Address;
  /** The fee wallet's ATA on an SPL pool, created by the program when missing. */
  destinationTokenAccount: Address | undefined;
  createdAccountBytes: number;
}

/**
 * `close_pool` (PROGRAM §4.7): permissionless close of a terminal pool. Reads the pool for
 * `creator`, `mint`, `token_program` and `config` for `fee_wallet`.
 */
export async function closePoolInstruction(
  client: MyBarPoolClient,
  input: ClosePoolInput,
): Promise<ClosePoolResult> {
  const pool = await requirePool(client, input.pool);
  const configAddress = await configPda(client);
  const config = await getConfig(client, configAddress);
  if (!config) throw new SdkError("NotFound", { address: configAddress });
  const feeWallet = config.data.feeWallet;
  const vault = await vaultPda(client, pool.address);
  const spl = splOf(pool.data);
  const ata = await tokenAccountToCredit(client, feeWallet, spl);
  const instruction = await getClosePoolInstructionAsync(
    {
      payer: input.payer,
      pool: pool.address,
      vault,
      feeWallet,
      creator: pool.data.creator,
      ...(spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : {}),
      ...(ata.address ? { destinationTokenAccount: ata.address } : {}),
    },
    { programAddress: client.programAddress },
  );
  return {
    instruction,
    vault,
    creator: pool.data.creator,
    feeWallet,
    destinationTokenAccount: ata.address,
    createdAccountBytes: ata.creates ? TOKEN_ACCOUNT_BYTES : 0,
  };
}
