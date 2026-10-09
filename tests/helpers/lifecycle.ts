/**
 * Shared machinery for the lifecycle scripts under `tests/lifecycle/` (build plan Step 8):
 * one cast per file, a game scheduled from the chain clock, the Step 3 scores, the settlement
 * and §4.6 instruction wrappers with their compute budgets, the PROGRAM §3.4 vault invariant,
 * and the compute-unit record every file prints in its last phase.
 *
 * Every script creates its own game from `chainNow()` (never `Date.now()`) and travels with
 * `surfnet_timeTravel` exactly as the Step 7 suites do (`travelTo(t)` lands at `t + 2`).
 */
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { winningBox, type Digits, type GameKey } from "@mybarpool/shared";
import {
  airdropFactory,
  createKeyPairSignerFromBytes,
  generateKeyPairSigner,
  lamports,
  type Address,
  type KeyPairSigner,
  type Signature,
} from "@solana/kit";
import { expect } from "vitest";

import { Localnet } from "../../scripts/localnet.js";
import {
  ata,
  cancelPoolInstruction,
  chainNow,
  closePoolInstruction,
  closeSponsorshipInstruction,
  computeUnitsConsumed,
  configPda,
  counterPda,
  createGameInstruction,
  creatorCounterDecoder,
  decodeAccount,
  decodeEvent,
  emittedEvents,
  fetchAccountData,
  fetchLamports,
  gamePda,
  markGameInstruction,
  platformConfigDecoder,
  poolClosedDecoder,
  PoolStatus,
  postScoresInstruction,
  quarterSettledDecoder,
  reclaimInstruction,
  reclaimSponsorshipInstruction,
  returnBoxesInstruction,
  returnSponsorshipInstruction,
  rpc,
  rpcSubscriptions,
  send,
  sendExpectingError,
  setComputeUnitLimit,
  settleInstruction,
  sponsorshipDecoder,
  sponsorshipPda,
  splitInstruction,
  tokenAmount,
  updateConfigInstruction,
  vaultPda,
  withRetry,
  type GameStatus,
  type Pool,
  type PoolRefs,
  type SplPool,
} from "./mybarpool.js";
import { CU_LIMIT, fetchPool, type Scenario } from "./scenario.js";

export const SOL = 1_000_000_000n;
export const HOUR = 3_600n;
export const MINUTE = 60n;
export const DAY = 86_400n;
/** PROGRAM §1 `RECLAIM_DELAY`: thirty days after the scheduled kickoff. */
export const RECLAIM_DELAY = 30n * DAY;
/** The SPL return and split pages carry up to seven inner instructions per owner (Step 7). */
export const BATCH_CU_LIMIT = 1_000_000;
/** The Step 3 scores: Q1 7–3, Q2 14–10, Q3 17–17, Q4 24–20 final with overtime. */
export const SCORES: readonly (readonly [number, number])[] = [
  [7, 3],
  [14, 10],
  [17, 17],
  [24, 20],
];

export interface Rent {
  vault: bigint;
  pool: bigint;
  tokenAccount: bigint;
  sponsorship: bigint;
  counter: bigint;
}

export interface CastOptions {
  /** PROGRAM §2 week (1–22); each script takes its own so game addresses never collide. */
  week: number;
  buyers: number;
  sponsors?: number;
  /** Hours from the chain clock to the game's kickoff (default 3). */
  kickoffHours?: bigint;
  /** Lamports airdropped to each wallet (default 100 SOL). */
  airdrop?: bigint;
}

/** Everything a lifecycle script needs; built once in its `beforeAll`. */
export class Lifecycle {
  readonly localnet = new Localnet();
  readonly measured: Record<string, bigint> = {};
  readonly travelled: string[] = [];
  admin!: KeyPairSigner;
  keeper!: KeyPairSigner;
  creator!: KeyPairSigner;
  stranger!: KeyPairSigner;
  buyers!: KeyPairSigner[];
  sponsors!: KeyPairSigner[];
  feeWallet!: Address;
  rent!: Rent;
  game!: Address;
  kickoff!: bigint;
  key!: GameKey;
  #nonce = 1n;
  #varId: bigint;

  constructor(readonly options: CastOptions) {
    // Var ids are per keeper key, which is fresh per file; the offset only keeps logs readable.
    this.#varId = 1_000n + BigInt(options.week) * 100n;
  }

  nextNonce = (): bigint => this.#nonce++;
  nextVarId = (): bigint => this.#varId++;

  /** The `Scenario` view for `fill` / `drawLockedPool` (`sampler` is the stranger). */
  scenario(): Scenario {
    return {
      creator: this.creator,
      buyers: [this.buyers[0]!, this.buyers[1]!],
      keeper: this.keeper,
      sampler: this.stranger,
      game: this.game,
      nextNonce: this.nextNonce,
      nextVarId: this.nextVarId,
    };
  }

  async setup(): Promise<void> {
    const walletPath = process.env["ANCHOR_WALLET"] ?? join(homedir(), ".config/solana/id.json");
    this.admin = await createKeyPairSignerFromBytes(
      Uint8Array.from(JSON.parse(readFileSync(walletPath, "utf8"))),
    );
    if ((await fetchAccountData(await configPda())) === null) {
      throw new Error(
        "config PDA missing: config.test.ts must run first (see tests/vitest.config.ts)",
      );
    }
    this.keeper = await generateKeyPairSigner();
    this.creator = await generateKeyPairSigner();
    this.stranger = await generateKeyPairSigner();
    this.buyers = await Promise.all(
      Array.from({ length: this.options.buyers }, () => generateKeyPairSigner()),
    );
    this.sponsors = await Promise.all(
      Array.from({ length: this.options.sponsors ?? 0 }, () => generateKeyPairSigner()),
    );
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    for (const who of [
      this.keeper,
      this.creator,
      this.stranger,
      ...this.buyers,
      ...this.sponsors,
    ]) {
      // First touch of a fresh key: Surfpool asks mainnet whether it exists (Step 3 audit M1).
      await withRetry(() =>
        airdrop({
          recipientAddress: who.address,
          lamports: lamports(this.options.airdrop ?? 100n * SOL),
          commitment: "confirmed",
        }),
      );
    }
    // The worked numbers assume 500 / 500 (ARCHITECTURE › Fees); config.test.ts leaves
    // platform_bps at 400 in this Surfpool session. Fees are fixed at creation.
    await send(
      this.admin,
      await updateConfigInstruction(this.admin, {
        scoreAuthority: this.keeper.address,
        entropyProvider: this.keeper.address,
        platformBps: 500,
        creatorBps: 500,
      }),
    );
    const config = decodeAccount(
      "PlatformConfig",
      (await fetchAccountData(await configPda()))!,
      platformConfigDecoder,
    );
    this.feeWallet = config.feeWallet;
    await withRetry(() => rpc.getAccountInfo(this.feeWallet, { encoding: "base64" }).send());
    const r = async (size: bigint) => rpc.getMinimumBalanceForRentExemption(size).send();
    this.rent = {
      vault: await r(0n),
      pool: await r(1442n), // PROGRAM §3.3
      tokenAccount: await r(165n),
      sponsorship: await r(81n), // PROGRAM §3.7
      counter: await r(74n), // PROGRAM §3.5
    };
    // PROGRAM §2 team table: KC hosting DAL; the week is this script's own.
    this.key = { season: 2026, week: this.options.week, home: 15, away: 8 };
    this.kickoff = (await chainNow()) + (this.options.kickoffHours ?? 3n) * HOUR;
    this.game = await gamePda(this.key, this.kickoff);
    await send(this.keeper, await createGameInstruction(this.keeper, this.key, this.kickoff));
  }

  /** Forward only, to `target + 2` seconds (the Step 7 pattern). */
  async travelTo(target: bigint, label: string): Promise<bigint> {
    const now = await chainNow();
    if (now < target) {
      await this.localnet.timeTravel({ absoluteTimestamp: Number((target + 2n) * 1000n) });
    }
    const after = await chainNow();
    expect(after).toBeGreaterThanOrEqual(target);
    this.travelled.push(`${label}: ${target} → ${after}`);
    return after;
  }

  /** Post quarter `q` of the Step 3 scores at `kickoff + q × 16 min` (§4.2's 900 s spacing). */
  async postQuarter(q: number): Promise<void> {
    const [home, away] = SCORES[q - 1]!;
    await this.travelTo(this.kickoff + BigInt(q) * 16n * MINUTE, `Q${q} scores`);
    await send(
      this.keeper,
      await postScoresInstruction(this.keeper, this.game, {
        quarter: q,
        home,
        away,
        isFinal: q === 4,
        hadOvertime: q === 4,
      }),
    );
  }

  async markGame(status: GameStatus): Promise<void> {
    await send(this.admin, await markGameInstruction(this.admin, this.game, status));
  }

  /** PROGRAM §6.3 from the pool's stored axes and owners; never a hard-coded winner. */
  async expectedWinner(pool: Address, q: number): Promise<Address> {
    const p = await fetchPool(pool);
    const [home, away] = SCORES[q - 1]!;
    const box = winningBox({
      home,
      away,
      homeAxis: p.homeAxis as unknown as Digits,
      awayAxis: p.awayAxis as unknown as Digits,
    });
    return p.owners[box]!;
  }

  async measure(name: string, sig: Signature): Promise<bigint> {
    const cu = await computeUnitsConsumed(sig);
    this.measured[name] = cu;
    return cu;
  }

  async settle(refs: PoolRefs, q: number, options: { spl?: SplPool; integrator?: Address } = {}) {
    const winner = await this.expectedWinner(refs.pool, q);
    const sig = await send(this.keeper, [
      setComputeUnitLimit(CU_LIMIT),
      await settleInstruction(this.keeper, refs, q, winner, this.feeWallet, options),
    ]);
    await this.measure(`settle_q${q}`, sig);
    const event = decodeEvent(
      "QuarterSettled",
      (await emittedEvents(sig))[0]!,
      quarterSettledDecoder,
    );
    expect(event.winner).toBe(winner);
    return { winner, event, sig };
  }

  async returnBoxes(
    refs: PoolRefs,
    owners: Address[],
    options: { counter?: boolean; spl?: SplPool; measure?: string } = {},
  ): Promise<Signature> {
    const sig = await send(this.keeper, [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await returnBoxesInstruction(this.keeper, refs, owners, options),
    ]);
    if (options.measure) await this.measure(options.measure, sig);
    return sig;
  }

  async split(
    refs: PoolRefs,
    owners: Address[],
    options: { spl?: SplPool; measure?: string } = {},
  ): Promise<Signature> {
    const sig = await send(this.keeper, [
      setComputeUnitLimit(BATCH_CU_LIMIT),
      await splitInstruction(this.keeper, refs, owners, options),
    ]);
    if (options.measure) await this.measure(options.measure, sig);
    return sig;
  }

  async returnSponsorship(
    refs: PoolRefs,
    sponsor: Address,
    options: { spl?: SplPool } = {},
  ): Promise<Signature> {
    const sig = await send(this.keeper, [
      setComputeUnitLimit(CU_LIMIT),
      await returnSponsorshipInstruction(this.keeper, refs, sponsor, options),
    ]);
    await this.measure("return_sponsorship", sig);
    return sig;
  }

  async returnSponsorshipError(refs: PoolRefs, sponsor: Address): Promise<number | null> {
    return sendExpectingError(this.keeper, [
      setComputeUnitLimit(CU_LIMIT),
      await returnSponsorshipInstruction(this.keeper, refs, sponsor),
    ]);
  }

  async cancelPool(refs: PoolRefs, options: { counter?: boolean } = {}): Promise<Signature> {
    const sig = await send(this.admin, await cancelPoolInstruction(this.admin, refs, options));
    await this.measure("cancel_pool", sig);
    return sig;
  }

  async reclaim(
    owner: KeyPairSigner,
    refs: PoolRefs,
    options: { counter?: boolean; spl?: SplPool; measure?: string } = {},
  ): Promise<Signature> {
    const sig = await send(owner, [
      setComputeUnitLimit(CU_LIMIT),
      await reclaimInstruction(owner, refs, options),
    ]);
    if (options.measure) await this.measure(options.measure, sig);
    return sig;
  }

  async reclaimSponsorship(
    sponsor: KeyPairSigner,
    refs: PoolRefs,
    options: { counter?: boolean; spl?: SplPool } = {},
  ): Promise<Signature> {
    const sig = await send(sponsor, [
      setComputeUnitLimit(CU_LIMIT),
      await reclaimSponsorshipInstruction(sponsor, refs, options),
    ]);
    await this.measure("reclaim_sponsorship", sig);
    return sig;
  }

  /** `close_sponsorship` by the stranger: the rent goes to the sponsor (§4.6). */
  async closeSponsorship(refs: PoolRefs, sponsor: Address): Promise<Signature> {
    const before = await fetchLamports(sponsor);
    const sig = await send(this.stranger, await closeSponsorshipInstruction(refs, sponsor));
    await this.measure("close_sponsorship", sig);
    expect((await fetchLamports(sponsor)) - before).toBe(this.rent.sponsorship);
    expect(await fetchAccountData(await sponsorshipPda(refs.pool, sponsor))).toBeNull();
    return sig;
  }

  /**
   * `close_pool` by the stranger; asserts the pool account is gone, the SPL vault closed, and
   * the rent destination received vault rent + dust + pool rent (SOL) or the two rents (SPL).
   */
  async closePool(
    refs: PoolRefs,
    options: { spl?: SplPool; destination?: Address; measure?: string } = {},
  ) {
    const destination = options.destination ?? this.feeWallet;
    const vault = await vaultPda(refs.pool);
    const before = await fetchLamports(destination);
    const vaultLamports = await fetchLamports(vault);
    const sig = await send(this.stranger, [
      setComputeUnitLimit(CU_LIMIT),
      await closePoolInstruction(this.stranger, refs, this.feeWallet, options),
    ]);
    await this.measure(options.measure ?? "close_pool", sig);
    const event = decodeEvent("PoolClosed", (await emittedEvents(sig))[0]!, poolClosedDecoder);
    expect(await fetchAccountData(refs.pool)).toBeNull();
    if (options.spl) {
      expect(await fetchAccountData(vault)).toBeNull();
      expect((await fetchLamports(destination)) - before).toBe(this.rent.pool + vaultLamports);
    } else {
      expect(await fetchLamports(vault)).toBe(0n);
      expect((await fetchLamports(destination)) - before).toBe(vaultLamports + this.rent.pool);
      expect(vaultLamports).toBe(this.rent.vault + event.dust);
    }
    return event;
  }

  async openCount(creator: Address): Promise<number> {
    const data = await fetchAccountData(await counterPda(creator, this.game));
    expect(data).not.toBeNull();
    return decodeAccount("CreatorCounter", data!, creatorCounterDecoder).openCount;
  }

  /** The sum of `amount` over the sponsorship accounts of `sponsors` that still exist. */
  async openSponsorshipAmount(refs: PoolRefs, sponsors: readonly Address[]): Promise<bigint> {
    let total = 0n;
    for (const s of sponsors) {
      const data = await fetchAccountData(await sponsorshipPda(refs.pool, s));
      if (data) total += decodeAccount("Sponsorship", data, sponsorshipDecoder).amount;
    }
    return total;
  }

  /**
   * PROGRAM §3.4: the vault holds exactly what the pool's state says (SOL: above its own rent).
   * `sponsors` are the wallets whose `Sponsorship` accounts may still be open.
   */
  async expectVaultInvariant(
    refs: PoolRefs,
    sponsors: readonly Address[] = [],
    spl?: SplPool,
  ): Promise<Pool> {
    const p = await fetchPool(refs.pool);
    const vault = await vaultPda(refs.pool);
    const balance = spl
      ? ((await tokenAmount(vault)) ?? 0n)
      : (await fetchLamports(vault)) - this.rent.vault;
    const fees = p.feesPaid ? 0n : p.platformFee + p.creatorFee + p.integratorFee;
    let expected: bigint;
    switch (p.status) {
      case PoolStatus.Open:
      case PoolStatus.Locked:
      case PoolStatus.Drawn:
        expected =
          p.quartersSettled === 0
            ? BigInt(p.sold) * p.price + p.sponsoredTotal
            : p.unpaidPrizePool + fees;
        break;
      case PoolStatus.Settled:
        expected = p.unpaidPrizePool + fees;
        break;
      case PoolStatus.Returned:
        expected =
          p.price * BigInt(p.sold - popcount(p.returned)) +
          (await this.openSponsorshipAmount(refs, sponsors));
        break;
      case PoolStatus.Split:
        expected = p.unpaidPrizePool;
        break;
      default:
        throw new Error(`unknown status ${p.status}`);
    }
    expect(balance, `vault invariant in status ${PoolStatus[p.status]}`).toBe(expected);
    return p;
  }

  /** Lamport (or token) balances of `wallets`, for before/after deltas. */
  async balances(wallets: readonly Address[], spl?: SplPool): Promise<bigint[]> {
    return Promise.all(
      wallets.map(async (w) =>
        spl
          ? ((await tokenAmount(await ata(w, spl.mint, spl.tokenProgram))) ?? 0n)
          : fetchLamports(w),
      ),
    );
  }

  /** The last phase of every script: the measured compute of each phase and the clock targets. */
  report(file: string): void {
    const rows = Object.entries(this.measured).sort((a, b) => (a[1] < b[1] ? 1 : -1));
    for (const [name, cu] of rows) console.log(`${file} ${name}: ${cu} CU`);
    if (rows[0]) console.log(`${file} largest: ${rows[0][0]} ${rows[0][1]} CU`);
    console.log(`${file} time travel: ${this.travelled.join("; ")}`);
  }
}

export function popcount(bits: number): number {
  let n = bits >>> 0;
  let c = 0;
  while (n) {
    c += n & 1;
    n >>>= 1;
  }
  return c;
}

/** Boxes of `owner` in the pool's `owners` array. */
export function boxesOf(p: Pool, owner: Address): number[] {
  return p.owners.flatMap((o, i) => (o === owner ? [i] : []));
}

/** The distinct owners of a pool in box order. */
export function distinctOwners(p: Pool): Address[] {
  return [...new Set(p.owners)];
}
