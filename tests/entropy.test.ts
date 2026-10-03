/**
 * Step 0 localnet test: `anchor test` starts Surfpool as a mainnet fork,
 * deploys the empty program, and this test proves the fork works by reading
 * the real Entropy program account, which Surfpool fetches from mainnet on
 * first touch (ARCHITECTURE › Environments: no clone script).
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { address, createSolanaRpc } from "@solana/kit";
import { describe, expect, it } from "vitest";

import { ENTROPY_PROGRAM, LOCALNET_URL, Localnet } from "../scripts/localnet.js";

const rpc = createSolanaRpc(LOCALNET_URL);

/** The id `anchor test` deployed, from Anchor.toml (CI syncs it to a fresh keypair). */
function declaredProgramId(): string {
  const toml = readFileSync(fileURLToPath(new URL("../Anchor.toml", import.meta.url)), "utf8");
  const match = /^mybarpool\s*=\s*"([1-9A-HJ-NP-Za-km-z]{32,44})"/m.exec(toml);
  if (!match?.[1]) throw new Error("programs.localnet.mybarpool not found in Anchor.toml");
  return match[1];
}

describe("localnet (Surfpool, mainnet fork)", () => {
  it("serves the Entropy program cloned from mainnet", async () => {
    const info = await rpc.getAccountInfo(address(ENTROPY_PROGRAM), { encoding: "base64" }).send();
    expect(info.value).not.toBeNull();
    expect(info.value?.executable).toBe(true);
  });

  it("has the MyBarPool program deployed", async () => {
    const info = await rpc
      .getAccountInfo(address(declaredProgramId()), { encoding: "base64" })
      .send();
    expect(info.value?.executable).toBe(true);
  });

  it("answers Surfpool cheatcodes", async () => {
    const localnet = new Localnet();
    const before = await localnet.pauseClock();
    const after = await localnet.resumeClock();
    expect(after.absoluteSlot).toBeGreaterThanOrEqual(before.absoluteSlot);
  });
});
