/**
 * Every one of the 24 events round-trips through `decodeEvent`: encoded with its generated
 * encoder (discriminator included), identified and decoded to `{ name, data }`. Pins the
 * discriminator → decoder table in `events.ts`.
 */
import { describe, expect, it } from "vitest";

import * as g from "../../src/generated/index.js";
import { decodeEvent } from "../../src/index.js";
import { BUYER, CREATOR, GAME, POOL } from "./fixtures.js";

const T = 1_800_000_000n;
const b32 = (x: number) => new Uint8Array(32).fill(x);
const rule: g.TokenRuleArgs = {
  enabled: true,
  mint: POOL,
  tokenProgram: GAME,
  decimals: 9,
  minPrice: 1n,
  step: 1n,
  maxPrice: 2n,
  maxSponsorship: 3n,
};

/** `[name, encoded bytes, a field the decoded data must carry]`. */
const cases: Array<[string, ArrayLike<number>, Record<string, unknown>]> = [
  [
    "BoxesBought",
    g.getBoxesBoughtEventEncoder().encode({
      time: T,
      pool: POOL,
      buyer: BUYER,
      boxes: new Uint8Array([1, 2]),
      count: 2,
      soldAfter: 9,
    }),
    { count: 2 },
  ],
  [
    "BoxesReclaimed",
    g
      .getBoxesReclaimedEventEncoder()
      .encode({ time: T, pool: POOL, owner: BUYER, boxes: new Uint8Array([3]), amount: 50n }),
    { amount: 50n },
  ],
  [
    "BoxesReturned",
    g
      .getBoxesReturnedEventEncoder()
      .encode({ time: T, pool: POOL, owner: BUYER, boxes: new Uint8Array([4, 5]), amount: 100n }),
    { amount: 100n },
  ],
  [
    "BoxesSplit",
    g
      .getBoxesSplitEventEncoder()
      .encode({ time: T, pool: POOL, owner: BUYER, boxes: new Uint8Array([6]), amount: 7n }),
    { amount: 7n },
  ],
  [
    "ConfigUpdated",
    g.getConfigUpdatedEventEncoder().encode({
      time: T,
      admin: CREATOR,
      scoreAuthority: BUYER,
      entropyProvider: BUYER,
      feeWallet: GAME,
      platformBps: 500,
      creatorBps: 500,
      addonBudgetBps: 500,
      defaultPreset: 0,
      maxOpenPools: 3,
      maxOwnBoxes: 5,
      preseasonEnabled: false,
      paused: true,
      tokens: [rule, rule, rule],
    }),
    { paused: true },
  ],
  [
    "DigitsDrawn",
    g.getDigitsDrawnEventEncoder().encode({
      time: T,
      pool: POOL,
      var: GAME,
      value: b32(1),
      homeAxis: new Uint8Array(10),
      awayAxis: new Uint8Array(10),
    }),
    { var: GAME },
  ],
  [
    "GameCreated",
    g.getGameCreatedEventEncoder().encode({
      time: T,
      game: GAME,
      key: { season: 2026, week: 2, home: 15, away: 8 },
      scheduledKickoff: T,
    }),
    { scheduledKickoff: T },
  ],
  [
    "GameMarked",
    g.getGameMarkedEventEncoder().encode({ time: T, game: GAME, status: g.GameStatus.Postponed }),
    { status: g.GameStatus.Postponed },
  ],
  [
    "GateKeyRotated",
    g.getGateKeyRotatedEventEncoder().encode({ time: T, pool: POOL }),
    { pool: POOL },
  ],
  [
    "KickoffUpdated",
    g.getKickoffUpdatedEventEncoder().encode({ time: T, game: GAME, old: 1n, new: 2n }),
    { new: 2n },
  ],
  [
    "OverrideClosed",
    g
      .getOverrideClosedEventEncoder()
      .encode({ time: T, wallet: CREATOR, maxOpenPools: 4, maxOwnBoxes: 6 }),
    { maxOwnBoxes: 6 },
  ],
  [
    "OverrideSet",
    g
      .getOverrideSetEventEncoder()
      .encode({ time: T, wallet: CREATOR, maxOpenPools: 4, maxOwnBoxes: 6 }),
    { maxOpenPools: 4 },
  ],
  [
    "PoolCancelled",
    g.getPoolCancelledEventEncoder().encode({ time: T, pool: POOL }),
    { pool: POOL },
  ],
  [
    "PoolClosed",
    g.getPoolClosedEventEncoder().encode({ time: T, pool: POOL, destination: CREATOR, dust: 11n }),
    { dust: 11n },
  ],
  [
    "PoolCreated",
    g.getPoolCreatedEventEncoder().encode({
      time: T,
      pool: POOL,
      game: GAME,
      creator: CREATOR,
      token: 0,
      mint: GAME,
      price: 5n,
      preset: g.PayoutPreset.FinalOnly,
      accessType: g.AccessType.Link,
      creatorAddonBps: 200,
      integrator: GAME,
      integratorBps: 0,
      platformFee: 1n,
      creatorFee: 2n,
      integratorFee: 0n,
    }),
    { accessType: g.AccessType.Link },
  ],
  [
    "PoolLocked",
    g.getPoolLockedEventEncoder().encode({ time: T, pool: POOL, lockedAt: T }),
    { lockedAt: T },
  ],
  [
    "QuarterSettled",
    g.getQuarterSettledEventEncoder().encode({
      time: T,
      pool: POOL,
      quarter: 4,
      home: 24,
      away: 20,
      boxIndex: 24,
      winner: BUYER,
      amount: 440n,
      feesPaidNow: false,
      platformFee: 0n,
      creatorFee: 0n,
      integratorFee: 0n,
    }),
    { quarter: 4 },
  ],
  [
    "ScoresPosted",
    g.getScoresPostedEventEncoder().encode({
      time: T,
      game: GAME,
      quarter: 2,
      home: 14,
      away: 10,
      isFinal: false,
      hadOvertime: false,
    }),
    { home: 14 },
  ],
  [
    "Sponsored",
    g
      .getSponsoredEventEncoder()
      .encode({ time: T, pool: POOL, sponsor: BUYER, amount: 3n, sponsoredTotal: 9n }),
    { sponsoredTotal: 9n },
  ],
  [
    "SponsorshipClosed",
    g
      .getSponsorshipClosedEventEncoder()
      .encode({ time: T, pool: POOL, sponsor: BUYER, amount: 3n }),
    { sponsor: BUYER },
  ],
  [
    "SponsorshipReturned",
    g
      .getSponsorshipReturnedEventEncoder()
      .encode({ time: T, pool: POOL, sponsor: BUYER, amount: 3n }),
    { amount: 3n },
  ],
  [
    "VarReplaced",
    g.getVarReplacedEventEncoder().encode({
      time: T,
      pool: POOL,
      oldVar: GAME,
      newVar: CREATOR,
      endAt: 9n,
      replacements: 1,
      commit: b32(2),
    }),
    { replacements: 1 },
  ],
  [
    "VarSampled",
    g.getVarSampledEventEncoder().encode({
      time: T,
      pool: POOL,
      var: GAME,
      sampler: BUYER,
      slot: 5n,
      endAt: 4n,
      slotHash: b32(3),
    }),
    { slot: 5n },
  ],
  [
    "VarSet",
    g.getVarSetEventEncoder().encode({ time: T, pool: POOL, var: GAME, endAt: 4n, commit: b32(4) }),
    { endAt: 4n },
  ],
];

describe("every event round-trips through decodeEvent", () => {
  it("24 events, 24 names", () => {
    expect(cases).toHaveLength(24);
    expect(new Set(cases.map((c) => c[0])).size).toBe(24);
  });
  for (const [name, bytes, field] of cases) {
    it(name, () => {
      const e = decodeEvent(bytes as Uint8Array);
      expect(e?.name).toBe(name);
      expect(e?.data).toMatchObject(field);
      expect((e?.data as { time: bigint }).time).toBe(T);
    });
  }
});
