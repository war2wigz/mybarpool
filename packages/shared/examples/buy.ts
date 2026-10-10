/** Buy boxes on a pool; a `Link` pool needs its gate key, an `Allowlist` pool the wallet list. */
import { buyInstruction, checkFunds, getPool, isSdkError } from "@mybarpool/shared";
import { address } from "@solana/kit";

import { clientFromEnv, keypairFromArgv, sendAndPrint } from "./_common.js";

const client = clientFromEnv();
const buyer = await keypairFromArgv();
const pool = address(process.argv[3] ?? "");
const count = Number(process.argv[4] ?? "1");

const current = await getPool(client, pool);
if (!current) throw new Error("no such pool");
const funds = await checkFunds(client, {
  wallet: buyer.address,
  token: current.data.token,
  amount: current.data.price * BigInt(count),
  feeLamports: 10_000n,
});
if (!funds.ok) throw new Error(`short ${funds.shortfall} (${funds.kind})`);

try {
  const r = await buyInstruction(client, { buyer, pool, count });
  await sendAndPrint(client, buyer, [r.instruction]);
} catch (e) {
  // The SDK names the error; the copy is the app's.
  if (isSdkError(e)) console.error(e.code, e.details);
  throw e;
}
