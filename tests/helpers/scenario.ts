/**
 * Localnet scenario helpers shared by the settlement, returns and unresolved
 * suites: a pool drawn with nothing planted (draw.test.ts item 2's recipe),
 * read back through the committed IDL.
 */
import { drawAxes, entropyValue, INITIAL_LADDERS, keccak } from "@mybarpool/shared";
import type { Address, KeyPairSigner } from "@solana/kit";
import { expect } from "vitest";

import {
  currentSlot,
  entropyVarPda,
  fetchVar,
  openInstruction,
  revealInstruction,
  waitForSlot,
} from "./entropy.js";
import {
  ata,
  buyInstruction,
  createPoolInstruction,
  decodeAccount,
  drawInstruction,
  fetchAccountData,
  poolDecoder,
  poolParams,
  poolPda,
  PoolStatus,
  rpc,
  sampleVarInstruction,
  send,
  setComputeUnitLimit,
  setVarInstruction,
  withRetry,
  type CreatePoolParams,
  type Pool,
  type PoolRefs,
  type SplPool,
} from "./mybarpool.js";

/** ARCHITECTURE › Buying: the SOL minimum, 0.05 SOL; the ORE minimum, 0.05 ORE at 11 decimals. */
export const PRICE = INITIAL_LADDERS.SOL.minPrice;
export const PRICE_ORE = INITIAL_LADDERS.ORE.minPrice;
/** Entropy's `Sample` alone is ~127k CU; every sample_var transaction raises the limit. */
export const CU_LIMIT = 400_000;
/** Slots ahead for `end_at` (6 s at 200 ms): Open and set_var both have to land inside it. */
export const WINDOW_AHEAD = 30n;

export async function fetchPool(pool: Address): Promise<Pool> {
  const data = await fetchAccountData(pool);
  expect(data).not.toBeNull();
  expect(data!.length).toBe(1442); // PROGRAM §3.3
  return decodeAccount("Pool", data!, poolDecoder);
}

/** Who plays which part in a drawn pool, and the counters the recipe consumes. */
export interface Scenario {
  creator: KeyPairSigner;
  /** Two buyers of ten boxes each (5–14, 15–24). */
  buyers: [KeyPairSigner, KeyPairSigner];
  /** The score authority and Entropy provider. */
  keeper: KeyPairSigner;
  /** Anyone: `sample_var` is permissionless. */
  sampler: KeyPairSigner;
  game: Address;
  nextNonce: () => bigint;
  nextVarId: () => bigint;
}

const seedOf = (byte: number) => new Uint8Array(32).fill(byte);

/**
 * A pool with 25 boxes sold (creator 5, each buyer 10) and drawn with nothing planted:
 * `Open` → `set_var` → wait → `sample_var` → `Reveal` → `draw`. ORE pools take `spl` and the
 * buyers' ATAs must already hold the price (`setTokenAccount` in the suite's `beforeAll`).
 */
export async function drawnPool(
  s: Scenario,
  params: Partial<CreatePoolParams>,
  spl?: SplPool,
): Promise<PoolRefs> {
  const nonce = s.nextNonce();
  const tokenPath = spl ? { mint: spl.mint, tokenProgram: spl.tokenProgram } : undefined;
  await send(
    s.creator,
    await createPoolInstruction(
      s.creator,
      s.game,
      poolParams({
        nonce,
        token: spl ? 2 : 0, // ORE is token index 2 (ARCHITECTURE › Buying table)
        price: spl ? PRICE_ORE : PRICE,
        initialBoxes: 5,
        ...params,
      }),
      tokenPath
        ? { ...tokenPath, tokenAccount: await ata(s.creator.address, spl!.mint, spl!.tokenProgram) }
        : {},
    ),
  );
  const refs: PoolRefs = {
    pool: await poolPda(s.game, s.creator.address, nonce),
    game: s.game,
    creator: s.creator.address,
  };
  for (const buyer of s.buyers) {
    await send(
      buyer,
      await buyInstruction(
        buyer,
        refs,
        10,
        tokenPath
          ? { ...tokenPath, tokenAccount: await ata(buyer.address, spl!.mint, spl!.tokenProgram) }
          : {},
      ),
    );
  }
  expect((await fetchPool(refs.pool)).status).toBe(PoolStatus.Locked);

  const id = s.nextVarId();
  const seed = seedOf(Number(id % 200n) + 1);
  const varAddress = await entropyVarPda(s.keeper.address, id);
  // First touch of the new PDA: Surfpool asks mainnet whether it exists (Step 3 audit M1), and
  // a slow answer must not eat the window between Open and set_var.
  await withRetry(() => rpc.getAccountInfo(varAddress, { encoding: "base64" }).send());
  const endAt = (await currentSlot()) + WINDOW_AHEAD;
  await send(
    s.keeper,
    await openInstruction(s.keeper, s.keeper, id, s.keeper.address, keccak(seed), false, 1n, endAt),
  );
  await send(s.keeper, await setVarInstruction(s.keeper, refs.pool, varAddress));
  await waitForSlot(endAt + 1n);
  await send(s.sampler, [
    setComputeUnitLimit(CU_LIMIT),
    await sampleVarInstruction(s.sampler, refs.pool, varAddress),
  ]);
  await send(s.keeper, revealInstruction(s.keeper, varAddress, seed));
  const revealed = (await fetchVar(varAddress))!;
  expect(revealed.value).toEqual(entropyValue(revealed.slotHash, seed, 1n));
  await send(s.keeper, await drawInstruction(s.keeper, refs.pool, varAddress));
  const pool = await fetchPool(refs.pool);
  expect(pool.status).toBe(PoolStatus.Drawn);
  const axes = drawAxes(revealed.value);
  expect(pool.homeAxis).toEqual(Array.from(axes.home));
  return refs;
}
