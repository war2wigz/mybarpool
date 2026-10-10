/** Create a public SOL pool on a game and buy the creator's first boxes. */
import { createPoolInstruction, PayoutPreset, TokenIndex } from "@mybarpool/shared";
import { address } from "@solana/kit";

import { clientFromEnv, keypairFromArgv, sendAndPrint } from "./_common.js";

const client = clientFromEnv();
const creator = await keypairFromArgv();
const game = address(process.argv[3] ?? "");
const price = BigInt(process.argv[4] ?? "0"); // base units, on the token's ladder

const r = await createPoolInstruction(client, {
  creator,
  game,
  token: TokenIndex.SOL,
  price,
  preset: PayoutPreset.Standard,
  access: { type: "public" },
  initialBoxes: 1,
});
console.log(`pool ${r.pool} (nonce ${r.nonce})`);
await sendAndPrint(client, creator, [r.instruction], r.createdAccountBytes);
