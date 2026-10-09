/**
 * Every `pdas.ts` function equals the generated finder for the same inputs, and the
 * hand-derived `gamePda` / `poolPda` / `associatedTokenAddress` equal the values the localnet
 * helper derived for the same fixture (frozen in `test/fixtures/pdas.json`).
 */
import { readFileSync } from "node:fs";

import { address, type Address } from "@solana/kit";
import { describe, expect, it } from "vitest";

import {
  findConfigPda,
  findCounterPda,
  findEventAuthorityPda,
  findSponsorshipPda,
  findVaultPda,
  findWalletOverridePda,
} from "../../src/generated/index.js";
import {
  associatedTokenAddress,
  configPda,
  counterPda,
  eventAuthorityPda,
  gamePda,
  poolPda,
  sponsorshipPda,
  vaultPda,
  walletOverridePda,
} from "../../src/index.js";
import { mockClient } from "./fixtures.js";

const F = JSON.parse(readFileSync(new URL("../fixtures/pdas.json", import.meta.url), "utf8")) as {
  programAddress: string;
  key: { season: number; week: number; home: number; away: number };
  scheduledKickoff: string;
  creator: string;
  buyer: string;
  nonce: string;
  game: string;
  pool: string;
  vault: string;
  counter: string;
  walletOverride: string;
  sponsorship: string;
  config: string;
  ata: { owner: string; mint: string; tokenProgram: string; address: string };
};
const a = (s: string) => address(s);
const { client } = mockClient({}, a(F.programAddress));

describe("PDAs (PROGRAM §3)", () => {
  it("gamePda and poolPda equal the localnet helper's values for the fixture", async () => {
    expect(await gamePda(client, F.key, BigInt(F.scheduledKickoff))).toBe(F.game);
    expect(await poolPda(client, a(F.game), a(F.creator), BigInt(F.nonce))).toBe(F.pool);
  });

  it("the generated-finder wrappers equal the finders and the helper's values", async () => {
    const pa = { programAddress: client.programAddress };
    expect(await configPda(client)).toBe((await findConfigPda(pa))[0]);
    expect(await configPda(client)).toBe(F.config);
    expect(await vaultPda(client, a(F.pool))).toBe(
      (await findVaultPda({ pool: a(F.pool) }, pa))[0],
    );
    expect(await vaultPda(client, a(F.pool))).toBe(F.vault);
    expect(await counterPda(client, a(F.creator), a(F.game))).toBe(
      (await findCounterPda({ creator: a(F.creator), game: a(F.game) }, pa))[0],
    );
    expect(await counterPda(client, a(F.creator), a(F.game))).toBe(F.counter);
    expect(await walletOverridePda(client, a(F.creator))).toBe(
      (await findWalletOverridePda({ wallet: a(F.creator) }, pa))[0],
    );
    expect(await walletOverridePda(client, a(F.creator))).toBe(F.walletOverride);
    expect(await sponsorshipPda(client, a(F.pool), a(F.buyer))).toBe(
      (await findSponsorshipPda({ pool: a(F.pool), sponsor: a(F.buyer) }, pa))[0],
    );
    expect(await sponsorshipPda(client, a(F.pool), a(F.buyer))).toBe(F.sponsorship);
    expect(await eventAuthorityPda(client)).toBe((await findEventAuthorityPda(pa))[0]);
  });

  it("another program address changes every PDA", async () => {
    const other = mockClient({}, a("ASo8r4EEFLPAMDk1w3XdKbEmq4c1GynbsHGa6RGG83fH")).client;
    expect(await configPda(other)).not.toBe(F.config);
    expect(await poolPda(other, a(F.game), a(F.creator), BigInt(F.nonce))).not.toBe(F.pool);
  });

  it("associatedTokenAddress equals the known ATA (owner, mint, Token program)", async () => {
    const got: Address = await associatedTokenAddress(
      a(F.ata.owner),
      a(F.ata.mint),
      a(F.ata.tokenProgram),
    );
    expect(got).toBe(F.ata.address);
    expect(await associatedTokenAddress(a(F.ata.owner), a(F.ata.mint))).toBe(F.ata.address);
  });
});
