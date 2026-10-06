/**
 * Step 2 localnet suite: the admin instructions against Surfpool forking
 * mainnet (`anchor test`). One `describe`, ordered, because the config PDA
 * can only be initialised once per deployment. ORE's rule points at the real
 * mainnet mint, which Surfpool fetches on first touch (no clone script).
 */
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { INITIAL_LADDERS } from "@mybarpool/shared";
import {
  address,
  airdropFactory,
  createKeyPairSignerFromBytes,
  generateKeyPairSigner,
  lamports,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import {
  closeWalletOverrideInstruction,
  configPda,
  configUpdatedDecoder,
  decodeAccount,
  decodeEvent,
  DEFAULT_ADDRESS,
  emittedEvents,
  errorCode,
  eventName,
  fetchAccountData,
  fetchLamports,
  IDL,
  initializeInstruction,
  ORE_MINT,
  overrideEventDecoder,
  overridePda,
  platformConfigDecoder,
  rpc,
  rpcSubscriptions,
  send,
  sendExpectingError,
  setWalletOverrideInstruction,
  TOKEN_PROGRAM,
  updateConfigInstruction,
  walletOverrideDecoder,
  type InitializeParams,
  type TokenRule,
} from "./helpers/mybarpool.js";

const SOL = 1_000_000_000n;

/** PROGRAM §3.1 "initial" column with ARCHITECTURE › Buying ladders; SKR disabled until its mint is known. */
function initialParams(
  keeper: Address,
  entropyProvider: Address,
  feeWallet: Address,
): InitializeParams {
  const sol: TokenRule = {
    enabled: true,
    mint: DEFAULT_ADDRESS,
    tokenProgram: DEFAULT_ADDRESS,
    decimals: INITIAL_LADDERS.SOL.decimals,
    minPrice: INITIAL_LADDERS.SOL.minPrice,
    step: INITIAL_LADDERS.SOL.step,
    maxPrice: INITIAL_LADDERS.SOL.maxPrice,
    maxSponsorship: INITIAL_LADDERS.SOL.maxPrice * 25n,
  };
  const skr: TokenRule = {
    enabled: false,
    mint: DEFAULT_ADDRESS,
    tokenProgram: DEFAULT_ADDRESS,
    decimals: 0,
    minPrice: 1n,
    step: 1n,
    maxPrice: 1n,
    maxSponsorship: 1n,
  };
  const ore: TokenRule = {
    enabled: true,
    mint: ORE_MINT,
    tokenProgram: TOKEN_PROGRAM,
    decimals: INITIAL_LADDERS.ORE.decimals,
    minPrice: INITIAL_LADDERS.ORE.minPrice,
    step: INITIAL_LADDERS.ORE.step,
    maxPrice: INITIAL_LADDERS.ORE.maxPrice,
    maxSponsorship: INITIAL_LADDERS.ORE.maxPrice * 25n,
  };
  return {
    scoreAuthority: keeper,
    entropyProvider,
    feeWallet,
    platformBps: 500,
    creatorBps: 500,
    addonBudgetBps: 500,
    defaultPreset: 0,
    maxOpenPools: 3,
    maxOwnBoxes: 5,
    preseasonEnabled: false,
    paused: false,
    tokens: [sol, skr, ore],
  };
}

describe("admin instructions (Surfpool, mainnet fork)", () => {
  let admin: KeyPairSigner; // the provider wallet: `anchor deploy` made it the upgrade authority
  let stranger: KeyPairSigner;
  let keeper: KeyPairSigner;
  let params: InitializeParams;
  let config: Address;

  beforeAll(async () => {
    const walletPath = process.env["ANCHOR_WALLET"] ?? join(homedir(), ".config/solana/id.json");
    admin = await createKeyPairSignerFromBytes(
      Uint8Array.from(JSON.parse(readFileSync(walletPath, "utf8"))),
    );
    stranger = await generateKeyPairSigner();
    keeper = await generateKeyPairSigner();
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    for (const who of [stranger, keeper]) {
      await airdrop({
        recipientAddress: who.address,
        lamports: lamports(10n * SOL),
        commitment: "confirmed",
      });
    }
    params = initialParams(
      keeper.address,
      (await generateKeyPairSigner()).address,
      (await generateKeyPairSigner()).address,
    );
    config = await configPda();
  });

  it("1. refuses initialize from a funded key that is not the upgrade authority (6000)", async () => {
    const ix = await initializeInstruction(stranger, params, { mint2: ORE_MINT });
    expect(await sendExpectingError(stranger, ix)).toBe(errorCode("Unauthorized"));
    expect(errorCode("Unauthorized")).toBe(6000);
    expect(await fetchAccountData(config)).toBeNull();
  });

  it("2. initialize by the upgrade authority writes the §3.1 config and emits ConfigUpdated", async () => {
    const signature = await send(
      admin,
      await initializeInstruction(admin, params, { mint2: ORE_MINT }),
    );

    const data = await fetchAccountData(config);
    expect(data).not.toBeNull();
    expect(data!.length).toBe(698); // PROGRAM §3.1
    const stored = decodeAccount("PlatformConfig", data!, platformConfigDecoder);
    expect(stored.admin).toBe(admin.address);
    expect(stored.scoreAuthority).toBe(keeper.address);
    expect(stored.platformBps).toBe(500);
    expect(stored.creatorBps).toBe(500);
    expect(stored.addonBudgetBps).toBe(500);
    expect(stored.maxOpenPools).toBe(3);
    expect(stored.maxOwnBoxes).toBe(5);
    expect(stored.paused).toBe(false);
    expect(stored.tokens).toEqual(params.tokens);
    expect([...stored.reserved].every((b) => b === 0)).toBe(true);
    // @mybarpool/shared's ladders are what the chain holds.
    const [sol, , ore] = stored.tokens;
    expect({
      decimals: sol.decimals,
      minPrice: sol.minPrice,
      step: sol.step,
      maxPrice: sol.maxPrice,
    }).toEqual(INITIAL_LADDERS.SOL);
    expect({
      decimals: ore.decimals,
      minPrice: ore.minPrice,
      step: ore.step,
      maxPrice: ore.maxPrice,
    }).toEqual(INITIAL_LADDERS.ORE);

    const events = await emittedEvents(signature);
    expect(events).toHaveLength(1);
    expect(eventName(events[0]!)).toBe("ConfigUpdated");
    const event = decodeEvent("ConfigUpdated", events[0]!, configUpdatedDecoder);
    expect(event.admin).toBe(admin.address);
    expect(event.tokens).toEqual(params.tokens);
    expect(event.time).toBeGreaterThan(0n);

    const rent = await rpc.getMinimumBalanceForRentExemption(698n).send();
    expect(await fetchLamports(config)).toBe(rent);
  });

  it("3. update_config lowers platform_bps to 400, rejects 501 (6059) and the keeper (6000)", async () => {
    await send(admin, await updateConfigInstruction(admin, { platformBps: 400 }));
    let stored = decodeAccount(
      "PlatformConfig",
      (await fetchAccountData(config))!,
      platformConfigDecoder,
    );
    expect(stored.platformBps).toBe(400);

    expect(
      await sendExpectingError(admin, await updateConfigInstruction(admin, { platformBps: 501 })),
    ).toBe(errorCode("InvalidConfig"));
    expect(errorCode("InvalidConfig")).toBe(6059);
    expect(
      await sendExpectingError(keeper, await updateConfigInstruction(keeper, { platformBps: 300 })),
    ).toBe(errorCode("Unauthorized"));
    stored = decodeAccount(
      "PlatformConfig",
      (await fetchAccountData(config))!,
      platformConfigDecoder,
    );
    expect(stored.platformBps).toBe(400);
  });

  it("4. set_wallet_override creates the PDA and close_wallet_override returns its rent", async () => {
    const wallet = (await generateKeyPairSigner()).address;
    const pda = await overridePda(wallet);

    const setSig = await send(admin, await setWalletOverrideInstruction(admin, wallet, 10, 5));
    const data = await fetchAccountData(pda);
    expect(data!.length).toBe(43); // PROGRAM §3.6
    const stored = decodeAccount("WalletOverride", data!, walletOverrideDecoder);
    expect(stored.wallet).toBe(wallet);
    expect(stored.maxOpenPools).toBe(10);
    expect(stored.maxOwnBoxes).toBe(5);
    const setEvents = await emittedEvents(setSig);
    expect(setEvents.map(eventName)).toEqual(["OverrideSet"]);
    const set = decodeEvent("OverrideSet", setEvents[0]!, overrideEventDecoder);
    expect([set.wallet, set.maxOpenPools, set.maxOwnBoxes]).toEqual([wallet, 10, 5]);

    const rent = await fetchLamports(pda);
    expect(rent).toBe(await rpc.getMinimumBalanceForRentExemption(43n).send());
    const before = await fetchLamports(admin.address);
    const closeSig = await send(admin, await closeWalletOverrideInstruction(admin, wallet));
    const tx = await rpc
      .getTransaction(closeSig, {
        maxSupportedTransactionVersion: 0,
        commitment: "confirmed",
        encoding: "json",
      })
      .send();
    const fee = tx!.meta!.fee;
    expect(await fetchAccountData(pda)).toBeNull();
    expect(await fetchLamports(admin.address)).toBe(before + rent - fee);
    const closeEvents = await emittedEvents(closeSig);
    expect(closeEvents.map(eventName)).toEqual(["OverrideClosed"]);
    const closed = decodeEvent("OverrideClosed", closeEvents[0]!, overrideEventDecoder);
    expect([closed.wallet, closed.maxOpenPools, closed.maxOwnBoxes]).toEqual([wallet, 10, 5]);
  });

  it("5. the committed IDL freezes 62 errors from 6000 and carries every §1 constant", () => {
    expect(IDL.errors).toHaveLength(62); // 60 after Step 2, + InvalidGameKey, InvalidGameStatus (Step 3)
    IDL.errors.forEach((e, i) => expect(e.code).toBe(6000 + i));
    const names = IDL.constants.map((c) => c.name);
    for (const c of [
      "BOXES",
      "LANES",
      "QUARTERS",
      "PLATFORM_BPS_MAX",
      "TOTAL_BPS_MAX",
      "ENTROPY_PROGRAM",
      "RECLAIM_DELAY",
    ]) {
      expect(names).toContain(c);
    }
    expect(address(IDL.address)).toBeTruthy();
  });
});
