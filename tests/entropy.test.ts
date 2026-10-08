/**
 * Step 0 localnet test: `anchor test` starts Surfpool as a mainnet fork,
 * deploys the empty program, and this test proves the fork works by reading
 * Regolith's Entropy program account, which Surfpool fetches from mainnet on
 * first touch (ARCHITECTURE › Environments: no clone script). Since Step 5b
 * `ENTROPY_PROGRAM` is the platform's own deployment, preloaded from the fork
 * fixture by `config.test.ts` (first in path order), so a third assertion
 * checks that it is there and is the fixture's bytecode.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { REGOLITH_ENTROPY_PROGRAM } from "@mybarpool/shared";
import { address, createSolanaRpc } from "@solana/kit";
import { describe, expect, it } from "vitest";

import { ENTROPY_PROGRAM, LOCALNET_URL, Localnet } from "../scripts/localnet.js";
import {
  deployedElf,
  ENTROPY_FORK_EXECUTABLE_HASH,
  REGOLITH_ENTROPY_EXECUTABLE_HASH,
  sha256Hex,
} from "./helpers/entropy.js";
import { withRetry } from "./helpers/mybarpool.js";

const rpc = createSolanaRpc(LOCALNET_URL);

/** The id `anchor test` deployed, from Anchor.toml (CI syncs it to a fresh keypair). */
function declaredProgramId(): string {
  const toml = readFileSync(fileURLToPath(new URL("../Anchor.toml", import.meta.url)), "utf8");
  const match = /^mybarpool\s*=\s*"([1-9A-HJ-NP-Za-km-z]{32,44})"/m.exec(toml);
  if (!match?.[1]) throw new Error("programs.localnet.mybarpool not found in Anchor.toml");
  return match[1];
}

describe("localnet (Surfpool, mainnet fork)", () => {
  it("serves Regolith's Entropy program cloned from mainnet", async () => {
    // Regolith's 3jSk…, a mainnet account Surfpool fetches on first touch (Step 3 audit M1);
    // not ENTROPY_PROGRAM, which since Step 5b is the platform's own deployment and lives
    // nowhere on mainnet yet. Its bytecode is also where the Mollusk Open canary comes from.
    const info = await withRetry(() =>
      rpc.getAccountInfo(address(REGOLITH_ENTROPY_PROGRAM), { encoding: "base64" }).send(),
    );
    expect(info.value).not.toBeNull();
    expect(info.value?.executable).toBe(true);
    const elf = await withRetry(() => deployedElf(address(REGOLITH_ENTROPY_PROGRAM)));
    expect(sha256Hex(elf)).toBe(REGOLITH_ENTROPY_EXECUTABLE_HASH);
  });

  it("has the platform's Entropy deployment preloaded from the fork fixture", async () => {
    const info = await rpc.getAccountInfo(address(ENTROPY_PROGRAM), { encoding: "base64" }).send();
    expect(info.value?.executable).toBe(true);
    expect(sha256Hex(await deployedElf(address(ENTROPY_PROGRAM)))).toBe(
      ENTROPY_FORK_EXECUTABLE_HASH,
    );
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
