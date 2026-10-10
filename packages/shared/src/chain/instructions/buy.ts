import { AccountRole, type Address, type Instruction, type TransactionSigner } from "@solana/kit";

import { allowlistProof } from "../../allowlist.js";
import { AccessType, getBuyInstructionAsync } from "../../generated/index.js";
import type { MyBarPoolClient } from "../client.js";
import { SdkError } from "../errors.js";
import { walletOverridePda } from "../pdas.js";
import { addressBytes, existingTokenAccount, poolPdas, requirePool, splOf } from "./common.js";

/** PROGRAM §4.3 gating input: the gate key co-signs a `Link` pool; the list proves an `Allowlist` buyer. */
export type BuyGate = { gateKey: TransactionSigner } | { wallets: readonly Address[] };

/** Input to {@link buyInstruction}. */
export interface BuyInput {
  buyer: TransactionSigner;
  pool: Address;
  /** 1–25. */
  count: number;
  /** Required for `Link` and `Allowlist` pools; ignored for `Public`. */
  gate?: BuyGate;
}

/** Result of {@link buyInstruction}: the instruction and the addresses it derived. */
export interface BuyResult {
  instruction: Instruction;
  game: Address;
  vault: Address;
  counter: Address;
  /** The creator's override slot, as PROGRAM §4.3 seeds it. */
  walletOverride: Address;
  /** The buyer's ATA on an SPL pool (checked to exist); `undefined` on SOL. */
  buyerTokenAccount: Address | undefined;
  createdAccountBytes: 0;
}

/**
 * `buy` (PROGRAM §4.3). Reads the pool once; derives `game`, `vault`, `counter`, the
 * **creator's** `walletOverride` (the program's seeds are `["override", pool.creator]`: the
 * slot is read only when the buyer is the creator) and the buyer's ATA (which must exist:
 * `SdkError("TokenAccountMissing")` otherwise, so the wallet never opens). `gateKey` is passed only on a `Link` pool and the proof is built
 * only on an `Allowlist` pool; a buyer not in the list is `SdkError("NotAllowlisted")` before
 * any RPC.
 */
export async function buyInstruction(client: MyBarPoolClient, input: BuyInput): Promise<BuyResult> {
  const buyer = input.buyer.address;
  const proof =
    input.gate && "wallets" in input.gate ? proofFor(input.gate.wallets, buyer) : undefined;

  const pool = await requirePool(client, input.pool);
  const { vault, counter } = await poolPdas(client, pool);
  const walletOverride = await walletOverridePda(client, pool.data.creator);
  const spl = splOf(pool.data);
  const buyerTokenAccount = await existingTokenAccount(client, buyer, spl);
  const gateKey =
    pool.data.accessType === AccessType.Link && input.gate && "gateKey" in input.gate
      ? input.gate.gateKey
      : undefined;
  const allowlistProof_ = pool.data.accessType === AccessType.Allowlist ? (proof ?? []) : [];

  const instruction = await getBuyInstructionAsync(
    {
      buyer: input.buyer,
      game: pool.data.game,
      pool: pool.address,
      vault,
      counter,
      walletOverride,
      ...(spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : {}),
      ...(buyerTokenAccount ? { buyerTokenAccount } : {}),
      // The IDL types `gate_key` as an unchecked account (PROGRAM §4.3: a missing signature is
      // the program's 6017, not Anchor's), so the generated builder would drop the signer role; an
      // explicit meta keeps it.
      ...(gateKey
        ? {
            gateKey: {
              address: gateKey.address,
              role: AccountRole.READONLY_SIGNER,
              signer: gateKey,
            },
          }
        : {}),
      count: input.count,
      allowlistProof: allowlistProof_,
    },
    { programAddress: client.programAddress },
  );
  return {
    instruction,
    game: pool.data.game,
    vault,
    counter,
    walletOverride,
    buyerTokenAccount,
    createdAccountBytes: 0,
  };
}

function proofFor(wallets: readonly Address[], buyer: Address): Uint8Array[] {
  if (!wallets.includes(buyer)) throw new SdkError("NotAllowlisted", { address: buyer });
  return allowlistProof(wallets.map(addressBytes), addressBytes(buyer));
}
