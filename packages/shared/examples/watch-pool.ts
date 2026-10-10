/**
 * Follow a pool: the account on every change, and every `QuarterSettled` as it confirms.
 * A box is "won" only from the settlement event; a leading box from live scores is provisional.
 */
import {
  displayStatus,
  getGame,
  getPool,
  leadingBox,
  toLabel,
  watchPool,
  watchSettlements,
  wonBoxes,
} from "@mybarpool/shared";
import { address } from "@solana/kit";

import { clientFromEnv } from "./_common.js";

const client = clientFromEnv();
const pool = address(process.argv[2] ?? "");

const first = await getPool(client, pool);
if (!first) throw new Error("no such pool");
const game = await getGame(client, first.data.game);
if (!game) throw new Error("no such game");

const now = () => BigInt(Math.floor(Date.now() / 1000)); // the app's clock, not the SDK's
console.log(displayStatus(first.data, game.data, now()));
// With live scores from your own feed, `leadingBox(pool, home, away)` names the box that would
// win right now — display only, never a result.
console.log("leading at 7–3:", leadingBox(first.data, 7, 3));

const stopPool = watchPool(client, pool, (p) => {
  if (!p) return console.log("pool closed");
  console.log("sold", p.sold, displayStatus(p, game.data, now()), "won", wonBoxes(p));
});
const stopSettlements = watchSettlements(client, pool, (s) => {
  console.log(
    `Q${s.event.quarter}: box ${toLabel(s.event.boxIndex)} to ${s.event.winner}, ${s.event.amount}`,
  );
});
process.on("SIGINT", () => {
  stopPool();
  stopSettlements();
  process.exit(0);
});
