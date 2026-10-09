import type { Address, Instruction, TransactionSigner } from "@solana/kit";

import { allowlistRoot } from "../../allowlist.js";
import { isValidPrice } from "../../price.js";
import {
  AccessType,
  getCreatePoolInstructionAsync,
  GameStatus,
  type PayoutPreset,
} from "../../generated/index.js";
import { accountExists, CREATOR_COUNTER_SIZE, getConfig, getGame, POOL_SIZE } from "../accounts.js";
import type { MyBarPoolClient } from "../client.js";
import { SdkError } from "../errors.js";
import { TOKEN_ACCOUNT_BYTES } from "../fees.js";
import {
  associatedTokenAddress,
  configPda,
  counterPda,
  poolPda,
  vaultPda,
  walletOverridePda,
} from "../pdas.js";
import { addressBytes, DEFAULT_ADDRESS, programError } from "./common.js";

/** PROGRAM §3.3 access, as the creator chooses it; the helper fills the iff fields. */
export type PoolAccess =
  | { type: "public" }
  | { type: "link"; gateKey: Address }
  | { type: "allowlist"; wallets: readonly Address[] };

export interface CreatePoolInput {
  creator: TransactionSigner;
  game: Address;
  /** Part of the pool's seeds; a random u64 when omitted. */
  nonce?: bigint;
  /** PROGRAM §2 token index (0 SOL, 1 SKR, 2 ORE). */
  token: number;
  /** Per box, base units; must be on the token's ladder. */
  price: bigint;
  preset: PayoutPreset;
  access: PoolAccess;
  creatorAddonBps?: number;
  integrator?: Address;
  integratorBps?: number;
  /** Boxes the creator buys in the same instruction. */
  initialBoxes: number;
  /**
   * The chain's clock in seconds (`chainNow` or the app's synced clock) to fail a closed game
   * as `SalesClosed` before simulation; the check is skipped when omitted.
   */
  now?: bigint;
}

export interface CreatePoolResult {
  instruction: Instruction;
  pool: Address;
  vault: Address;
  counter: Address;
  walletOverride: Address;
  nonce: bigint;
  /** The Merkle root sent for an `allowlist` pool; zero otherwise. */
  allowlistRoot: Uint8Array;
  /** Bytes the instruction may create, for `prepareTransaction`. */
  createdAccountBytes: number;
}

function randomNonce(): bigint {
  const b = new Uint8Array(8);
  crypto.getRandomValues(b);
  return new DataView(b.buffer).getBigUint64(0, true);
}

/**
 * `create_pool` (PROGRAM §4.3). Reads `config` and `game` once, so a paused platform, a
 * disabled token, an off-ladder price or a game that is not `Scheduled` fail with the program's
 * error name before simulation; derives `pool`, `vault`, `counter`, the creator's
 * `walletOverride` and, for an SPL pool with `initialBoxes > 0`, the creator's ATA.
 */
export async function createPoolInstruction(
  client: MyBarPoolClient,
  input: CreatePoolInput,
): Promise<CreatePoolResult> {
  const creator = input.creator.address;
  const config = await getConfig(client, await configPda(client));
  if (!config) throw new SdkError("NotFound", { address: await configPda(client) });
  if (config.data.paused) throw programError("Paused");
  const game = await getGame(client, input.game);
  if (!game) throw new SdkError("NotFound", { address: input.game });
  if (game.data.status !== GameStatus.Scheduled) throw programError("GameNotScheduled");
  if (input.now !== undefined && input.now >= game.data.recordedKickoff) {
    throw programError("SalesClosed");
  }
  const rule = config.data.tokens[input.token];
  if (!rule || !rule.enabled) throw programError("TokenDisabled");
  const ladder = {
    decimals: rule.decimals,
    minPrice: rule.minPrice,
    step: rule.step,
    maxPrice: rule.maxPrice,
  };
  if (!isValidPrice(ladder, input.price)) throw programError("PriceOffLadder");

  const nonce = input.nonce ?? randomNonce();
  const pool = await poolPda(client, input.game, creator, nonce);
  const vault = await vaultPda(client, pool);
  const counter = await counterPda(client, creator, input.game);
  const walletOverride = await walletOverridePda(client, creator);
  const spl = input.token === 0 ? undefined : { mint: rule.mint, tokenProgram: rule.tokenProgram };
  const creatorTokenAccount =
    spl && input.initialBoxes > 0
      ? await associatedTokenAddress(creator, spl.mint, spl.tokenProgram)
      : undefined;

  const root =
    input.access.type === "allowlist"
      ? allowlistRoot(input.access.wallets.map(addressBytes))
      : new Uint8Array(32);
  const accessType =
    input.access.type === "public"
      ? AccessType.Public
      : input.access.type === "link"
        ? AccessType.Link
        : AccessType.Allowlist;

  const instruction = await getCreatePoolInstructionAsync(
    {
      creator: input.creator,
      game: input.game,
      pool,
      vault,
      counter,
      walletOverride,
      ...(spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : {}),
      ...(creatorTokenAccount ? { creatorTokenAccount } : {}),
      nonce,
      token: input.token,
      price: input.price,
      preset: input.preset,
      accessType,
      gateKey: input.access.type === "link" ? input.access.gateKey : DEFAULT_ADDRESS,
      allowlistRoot: root,
      creatorAddonBps: input.creatorAddonBps ?? 0,
      integrator: input.integrator ?? DEFAULT_ADDRESS,
      integratorBps: input.integratorBps ?? 0,
      initialBoxes: input.initialBoxes,
    },
    { programAddress: client.programAddress },
  );

  const counterExists = await accountExists(client, counter);
  const createdAccountBytes =
    POOL_SIZE + (counterExists ? 0 : CREATOR_COUNTER_SIZE) + (spl ? TOKEN_ACCOUNT_BYTES : 0);

  return {
    instruction,
    pool,
    vault,
    counter,
    walletOverride,
    nonce,
    allowlistRoot: root,
    createdAccountBytes,
  };
}
