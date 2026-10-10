/** Add to a pool's prizes before kickoff. */
import { sponsorInstruction } from "@mybarpool/shared";
import { address } from "@solana/kit";

import { clientFromEnv, keypairFromArgv, sendAndPrint } from "./_common.js";

const client = clientFromEnv();
const sponsor = await keypairFromArgv();
const pool = address(process.argv[3] ?? "");
const amount = BigInt(process.argv[4] ?? "0"); // base units of the pool's token

const r = await sponsorInstruction(client, { sponsor, pool, amount });
console.log(`sponsorship ${r.sponsorship}`);
await sendAndPrint(client, sponsor, [r.instruction], r.createdAccountBytes);
