/**
 * Shared plumbing for the examples: the client from the environment, a keypair from argv, and
 * a send-and-print loop. The RPC endpoints are yours — the SDK ships with none.
 *
 *   RPC_URL=… WS_URL=… node --import tsx examples/buy.ts ./keypair.json <pool>
 */
import {
  createMyBarPoolClient,
  eventsOf,
  networkFeeLamports,
  prepareTransaction,
  signAndSend,
  suggestPriorityFee,
  transactionFormatFor,
  type MyBarPoolClient,
} from "@mybarpool/shared";
import {
  createKeyPairSignerFromBytes,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  type Instruction,
  type KeyPairSigner,
} from "@solana/kit";
import { readFileSync } from "node:fs";

export function clientFromEnv(): MyBarPoolClient {
  const rpcUrl = process.env["RPC_URL"];
  const wsUrl = process.env["WS_URL"];
  if (!rpcUrl || !wsUrl) throw new Error("set RPC_URL and WS_URL");
  return createMyBarPoolClient({
    rpc: createSolanaRpc(rpcUrl),
    rpcSubscriptions: createSolanaRpcSubscriptions(wsUrl),
    // programAddress defaults to the declared program id; pass your own for a fork or localnet.
  });
}

export async function keypairFromArgv(index = 2): Promise<KeyPairSigner> {
  const path = process.argv[index];
  if (!path) throw new Error("usage: <keypair.json> …");
  return createKeyPairSignerFromBytes(Uint8Array.from(JSON.parse(readFileSync(path, "utf8"))));
}

/**
 * Prepare, sign, send, print. A wallet's `supportedTransactionVersions` (from its Wallet
 * Standard `solana:signAndSendTransaction` feature) decides the format; a script with its own
 * keypair may build v1 outright, so `[0, 1]` stands in for the wallet's list here.
 */
export async function sendAndPrint(
  client: MyBarPoolClient,
  feePayer: KeyPairSigner,
  instructions: Instruction[],
  createdAccountBytes = 0,
): Promise<void> {
  const format = transactionFormatFor([0, 1]);
  const prepared = await prepareTransaction(client, {
    feePayer,
    instructions,
    format,
    createdAccountBytes,
    priorityFee: await suggestPriorityFee(
      client,
      instructions.flatMap((i) => (i.accounts ?? []).map((a) => a.address)),
    ),
  });
  const fee = networkFeeLamports(prepared);
  console.log(
    `${format}: ${prepared.sizeBytes} bytes, ${prepared.computeUnitLimit} CU, fee ${fee.totalLamports} lamports`,
  );
  const signature = await signAndSend(client, prepared);
  console.log(`confirmed ${signature}`);
  for (const e of await eventsOf(client, signature)) {
    console.log(e.event.name, e.event.data);
  }
}
