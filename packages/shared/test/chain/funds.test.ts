import { describe, expect, it } from "vitest";
import type { Address } from "@solana/kit";

import { getPlatformConfigEncoder, type PlatformConfigArgs } from "../../src/generated/index.js";
import { associatedTokenAddress, checkFunds, configPda, isSdkError } from "../../src/index.js";
import {
  accountInfo,
  BUYER,
  CREATOR,
  DEFAULT,
  mockClient,
  ORE_MINT,
  TOKEN_PROGRAM,
} from "./fixtures.js";

function tokenAccount(amount: bigint): Uint8Array {
  const b = new Uint8Array(165);
  new DataView(b.buffer).setBigUint64(64, amount, true);
  return b;
}

function chain(accounts: Record<string, { lamports?: bigint; data?: Uint8Array }>) {
  return mockClient({
    getAccountInfo: (address: Address) => {
      const a = accounts[address];
      return {
        context: { slot: 1n },
        value: a
          ? { ...accountInfo(a.data ?? new Uint8Array(0)), lamports: a.lamports ?? 1n }
          : null,
      };
    },
  });
}

const rule = {
  enabled: true,
  mint: DEFAULT,
  tokenProgram: DEFAULT,
  decimals: 9,
  minPrice: 1n,
  step: 1n,
  maxPrice: 2n,
  maxSponsorship: 3n,
};
const config: PlatformConfigArgs = {
  admin: CREATOR,
  scoreAuthority: CREATOR,
  entropyProvider: CREATOR,
  feeWallet: CREATOR,
  platformBps: 0,
  creatorBps: 0,
  addonBudgetBps: 0,
  defaultPreset: 0,
  maxOpenPools: 1,
  maxOwnBoxes: 1,
  preseasonEnabled: false,
  paused: false,
  tokens: [rule, rule, { ...rule, mint: ORE_MINT, tokenProgram: TOKEN_PROGRAM }],
  bump: 0,
  reserved: new Uint8Array(256),
};

describe("checkFunds (DESIGN §10.4)", () => {
  it("SOL: amount + fee against lamports", async () => {
    const { client, calls } = chain({ [BUYER]: { lamports: 1_000_000n } });
    expect(
      await checkFunds(client, {
        wallet: BUYER,
        token: 0,
        amount: 900_000n,
        feeLamports: 100_000n,
      }),
    ).toEqual({ ok: true });
    expect(
      await checkFunds(client, {
        wallet: BUYER,
        token: 0,
        amount: 900_001n,
        feeLamports: 100_000n,
      }),
    ).toEqual({
      ok: false,
      kind: "lamports",
      shortfall: 1n,
    });
    expect(calls).toHaveLength(2);
    expect(
      await checkFunds(chain({}).client, {
        wallet: BUYER,
        token: 0,
        amount: 0n,
        feeLamports: 5_000n,
      }),
    ).toEqual({
      ok: false,
      kind: "lamports",
      shortfall: 5_000n,
    });
  });

  it("SPL: the fee against lamports first, then the ATA balance; mint from config when omitted", async () => {
    const ata = await associatedTokenAddress(BUYER, ORE_MINT, TOKEN_PROGRAM);
    const cfg = await configPda(chain({}).client);
    const accounts = {
      [BUYER]: { lamports: 10_000n },
      [ata]: { data: tokenAccount(500n) },
      [cfg]: { data: Uint8Array.from(getPlatformConfigEncoder().encode(config)) },
    };
    const { client, calls } = chain(accounts);
    expect(
      await checkFunds(client, { wallet: BUYER, token: 2, amount: 500n, feeLamports: 10_000n }),
    ).toEqual({ ok: true });
    expect(calls).toHaveLength(3); // wallet, config, ATA
    expect(
      await checkFunds(client, {
        wallet: BUYER,
        token: 2,
        mint: ORE_MINT,
        tokenProgram: TOKEN_PROGRAM,
        amount: 501n,
        feeLamports: 1n,
      }),
    ).toEqual({ ok: false, kind: "token", shortfall: 1n });
    expect(calls).toHaveLength(5); // no config read with the mint given
    expect(
      await checkFunds(client, { wallet: BUYER, token: 2, amount: 1n, feeLamports: 10_001n }),
    ).toEqual({
      ok: false,
      kind: "lamports",
      shortfall: 1n,
    });
    const noAta = chain({ [BUYER]: { lamports: 10_000n }, [cfg]: accounts[cfg]! }).client;
    expect(
      await checkFunds(noAta, { wallet: BUYER, token: 2, amount: 7n, feeLamports: 0n }),
    ).toEqual({ ok: false, kind: "token", shortfall: 7n });
    const noConfig = await checkFunds(chain({ [BUYER]: { lamports: 10_000n } }).client, {
      wallet: BUYER,
      token: 2,
      amount: 1n,
      feeLamports: 0n,
    }).catch((e) => e);
    expect(isSdkError(noConfig, "NotFound")).toBe(true);
    const badIndex = await checkFunds(client, {
      wallet: BUYER,
      token: 7,
      amount: 1n,
      feeLamports: 0n,
    }).catch((e) => e);
    expect(isSdkError(badIndex, "NotFound")).toBe(true);
  });
});
