/**
 * Step 3 localnet suite: the game instructions against Surfpool forking
 * mainnet (`anchor test`). Runs after `config.test.ts` (the sequencer in
 * `vitest.config.ts` orders files by path), which initialised the config.
 *
 * The clock is the chain's: `chainNow()` reads the Clock sysvar, and time is
 * moved with `surfnet_timeTravel`. In Surfpool 1.6.0 `absoluteTimestamp` is in
 * milliseconds, must not be in the past, sets `unix_timestamp` to
 * `target / 1000`, and the clock keeps running at the 200 ms slot time
 * afterwards, so every travel lands a little past the target and the timing
 * assertions stay clear of the exact boundary (boundaries are the Mollusk tests' job).
 */
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

import { gameRecordSeeds, TEAMS } from "@mybarpool/shared";
import {
  airdropFactory,
  createKeyPairSignerFromBytes,
  generateKeyPairSigner,
  getProgramDerivedAddress,
  lamports,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { Localnet } from "../scripts/localnet.js";
import {
  chainNow,
  configPda,
  createGameInstruction,
  createGameInstructionUnchecked,
  decodeAccount,
  decodeEvent,
  emittedEvents,
  errorCode,
  eventName,
  fetchAccountData,
  fetchLamports,
  gameCreatedDecoder,
  gameMarkedDecoder,
  gamePda,
  gameRecordDecoder,
  GameStatus,
  IDL,
  kickoffUpdatedDecoder,
  markGameInstruction,
  postScoresInstruction,
  PROGRAM_ID,
  rpc,
  rpcSubscriptions,
  scoresPostedDecoder,
  send,
  sendExpectingError,
  updateConfigInstruction,
  updateKickoffInstruction,
  withRetry,
  type GameKey,
} from "./helpers/mybarpool.js";

const SOL = 1_000_000_000n;
const HOUR = 3_600n;
const MINUTE = 60n;
/** PROGRAM §1 KICKOFF_UPDATE_BOUND: 72 hours. */
const KICKOFF_UPDATE_BOUND = 259_200n;

/** PROGRAM §2 team table: KC hosting DAL in week 1 of 2026. */
const KEY: GameKey = { season: 2026, week: 1, home: 15, away: 8 };

const localnet = new Localnet();

/**
 * Move the chain clock to just past `targetSeconds` (Surfpool takes milliseconds and refuses a
 * target behind its current time, so a target the clock has already passed is a no-op).
 */
async function travelTo(targetSeconds: bigint): Promise<bigint> {
  const current = await chainNow();
  if (current < targetSeconds) {
    await localnet.timeTravel({ absoluteTimestamp: Number((targetSeconds + 2n) * 1000n) });
  }
  const now = await chainNow();
  expect(now).toBeGreaterThanOrEqual(targetSeconds);
  return now;
}

async function fetchGame(game: Address) {
  const data = await fetchAccountData(game);
  expect(data).not.toBeNull();
  expect(data!.length).toBe(153); // PROGRAM §3.2
  return decodeAccount("GameRecord", data!, gameRecordDecoder);
}

describe("game instructions (Surfpool, mainnet fork)", () => {
  let admin: KeyPairSigner; // the provider wallet, config.admin since config.test.ts
  let keeper: KeyPairSigner;
  let kickoff: bigint;
  let game: Address;
  let secondGame: Address;

  beforeAll(async () => {
    const walletPath = process.env["ANCHOR_WALLET"] ?? join(homedir(), ".config/solana/id.json");
    admin = await createKeyPairSignerFromBytes(
      Uint8Array.from(JSON.parse(readFileSync(walletPath, "utf8"))),
    );
    keeper = await generateKeyPairSigner();
    // First touch of a fresh key: Surfpool asks mainnet whether it exists (Step 3 audit M1).
    const airdrop = airdropFactory({ rpc, rpcSubscriptions });
    await withRetry(() =>
      airdrop({
        recipientAddress: keeper.address,
        lamports: lamports(10n * SOL),
        commitment: "confirmed",
      }),
    );
    if ((await fetchAccountData(await configPda())) === null) {
      throw new Error(
        "config PDA missing: config.test.ts must run first (see tests/vitest.config.ts)",
      );
    }
    // This suite's keeper becomes the score authority.
    await send(admin, await updateConfigInstruction(admin, { scoreAuthority: keeper.address }));
    expect(TEAMS[KEY.home]!.abbreviation).toBe("KC");
    expect(TEAMS[KEY.away]!.abbreviation).toBe("DAL");
  });

  it("1. create_game refuses the admin (6000), a past kickoff (6005) and bad keys (6060)", async () => {
    const now = await chainNow();
    const future = now + HOUR;
    expect(await sendExpectingError(admin, await createGameInstruction(admin, KEY, future))).toBe(
      errorCode("Unauthorized"),
    );
    expect(
      await sendExpectingError(keeper, await createGameInstruction(keeper, KEY, now - MINUTE)),
    ).toBe(errorCode("KickoffInPast"));
    expect(errorCode("KickoffInPast")).toBe(6005);
    // `@mybarpool/shared` refuses to encode home == away or an out-of-range week, so a
    // well-behaved client cannot reach these on-chain checks; the instruction is built from
    // raw seeds here to prove the program enforces them anyway.
    for (const bad of [
      { ...KEY, away: KEY.home },
      { ...KEY, week: 101 }, // preseason off (PROGRAM §3.1 preseason_enabled = false)
    ]) {
      expect(
        await sendExpectingError(keeper, await createGameInstructionUnchecked(keeper, bad, future)),
      ).toBe(errorCode("InvalidGameKey"));
    }
    expect(errorCode("InvalidGameKey")).toBe(6060);
  });

  it("2. create_game by the keeper lands at the @mybarpool/shared address with the §3.2 record", async () => {
    kickoff = (await chainNow()) + HOUR;
    game = await gamePda(KEY, kickoff);
    // The same address from the shared package's seeds directly.
    const [fromShared] = await getProgramDerivedAddress({
      programAddress: PROGRAM_ID,
      seeds: gameRecordSeeds(KEY, kickoff),
    });
    expect(game).toBe(fromShared);

    const signature = await send(keeper, await createGameInstruction(keeper, KEY, kickoff));
    const stored = await fetchGame(game);
    expect(stored.key).toEqual(KEY);
    expect(stored.scheduledKickoff).toBe(kickoff);
    expect(stored.recordedKickoff).toBe(kickoff);
    expect(stored.status).toBe(GameStatus.Scheduled);
    expect(stored.quartersPosted).toBe(0);
    expect(stored.homeScore).toEqual([0, 0, 0, 0]);
    expect(stored.awayScore).toEqual([0, 0, 0, 0]);
    expect(stored.postedAt).toEqual([0n, 0n, 0n, 0n]);
    expect(stored.finalHadOvertime).toBe(false);
    expect(stored.markedAt).toBe(0n);
    expect([...stored.reserved].every((b) => b === 0)).toBe(true);

    const rent = await rpc.getMinimumBalanceForRentExemption(153n).send();
    expect(await fetchLamports(game)).toBe(rent);
    expect(rent).toBe(1_955_760n); // (128 + 153) × 6 960, recorded in NOTES

    const events = await emittedEvents(signature);
    expect(events.map(eventName)).toEqual(["GameCreated"]);
    const created = decodeEvent("GameCreated", events[0]!, gameCreatedDecoder);
    expect(created.game).toBe(game);
    expect(created.key).toEqual(KEY);
    expect(created.scheduledKickoff).toBe(kickoff);
    expect(created.time).toBeGreaterThan(0n);

    // Same key, later kickoff: a different record (PROGRAM §2 "gets a new record").
    secondGame = await gamePda(KEY, kickoff + HOUR);
    expect(secondGame).not.toBe(game);
    await send(keeper, await createGameInstruction(keeper, KEY, kickoff + HOUR));
    expect((await fetchGame(secondGame)).scheduledKickoff).toBe(kickoff + HOUR);
  });

  it("3. update_kickoff moves the recorded kickoff within the bounds and emits KickoffUpdated", async () => {
    const signature = await send(
      keeper,
      await updateKickoffInstruction(keeper, game, kickoff + 10n * MINUTE),
    );
    const stored = await fetchGame(game);
    expect(stored.recordedKickoff).toBe(kickoff + 10n * MINUTE);
    expect(stored.scheduledKickoff).toBe(kickoff);
    const events = await emittedEvents(signature);
    expect(events.map(eventName)).toEqual(["KickoffUpdated"]);
    const updated = decodeEvent("KickoffUpdated", events[0]!, kickoffUpdatedDecoder);
    expect([updated.game, updated.old, updated.new]).toEqual([
      game,
      kickoff,
      kickoff + 10n * MINUTE,
    ]);

    expect(
      await sendExpectingError(
        keeper,
        await updateKickoffInstruction(keeper, game, kickoff + KICKOFF_UPDATE_BOUND + 1n),
      ),
    ).toBe(errorCode("KickoffOutOfBounds"));
    expect(errorCode("KickoffOutOfBounds")).toBe(6006);
    expect(
      await sendExpectingError(
        keeper,
        await updateKickoffInstruction(keeper, game, (await chainNow()) - 1n),
      ),
    ).toBe(errorCode("KickoffInPast"));
    expect(
      await sendExpectingError(
        admin,
        await updateKickoffInstruction(admin, game, kickoff + 20n * MINUTE),
      ),
    ).toBe(errorCode("Unauthorized"));
    expect((await fetchGame(game)).recordedKickoff).toBe(kickoff + 10n * MINUTE);
  });

  it("4. post_scores honours the 15-minute floors, the order and the final flag through a full game", async () => {
    const recorded = kickoff + 10n * MINUTE;
    // Before kickoff: too soon (Q1 floor is recorded_kickoff + 15 min).
    expect(
      await sendExpectingError(
        keeper,
        await postScoresInstruction(keeper, game, {
          quarter: 1,
          home: 7,
          away: 3,
          isFinal: false,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("QuarterTooSoon"));
    expect(errorCode("QuarterTooSoon")).toBe(6009);

    // 14 minutes after the recorded kickoff: still too soon; 16 minutes: lands.
    await travelTo(recorded + 14n * MINUTE);
    expect(
      await sendExpectingError(
        keeper,
        await postScoresInstruction(keeper, game, {
          quarter: 1,
          home: 7,
          away: 3,
          isFinal: false,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("QuarterTooSoon"));
    // After the recorded kickoff, update_kickoff is too late (PROGRAM §4.2 now < recorded_kickoff).
    expect(
      await sendExpectingError(
        keeper,
        await updateKickoffInstruction(keeper, game, recorded + HOUR),
      ),
    ).toBe(errorCode("KickoffUpdateTooLate"));
    expect(errorCode("KickoffUpdateTooLate")).toBe(6007);

    await travelTo(recorded + 16n * MINUTE);
    const q1 = await send(
      keeper,
      await postScoresInstruction(keeper, game, {
        quarter: 1,
        home: 7,
        away: 3,
        isFinal: false,
        hadOvertime: false,
      }),
    );
    let stored = await fetchGame(game);
    expect(stored.quartersPosted).toBe(1);
    expect(stored.homeScore[0]).toBe(7);
    expect(stored.awayScore[0]).toBe(3);
    expect(stored.postedAt[0]).toBeGreaterThanOrEqual(recorded + 15n * MINUTE);
    const q1Events = await emittedEvents(q1);
    expect(q1Events.map(eventName)).toEqual(["ScoresPosted"]);
    const posted = decodeEvent("ScoresPosted", q1Events[0]!, scoresPostedDecoder);
    expect([posted.game, posted.quarter, posted.home, posted.away, posted.isFinal]).toEqual([
      game,
      1,
      7,
      3,
      false,
    ]);

    // Order, decrease, final flag, authority.
    // Q2 at once, right after the Q1 post: the 15-minute floor runs from posted_at[0]
    // (PROGRAM §4.2; Step 3 audit L4).
    expect(
      await sendExpectingError(
        keeper,
        await postScoresInstruction(keeper, game, {
          quarter: 2,
          home: 14,
          away: 10,
          isFinal: false,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("QuarterTooSoon"));

    const q1At = stored.postedAt[0]!;
    await travelTo(q1At + 16n * MINUTE);
    expect(
      await sendExpectingError(
        keeper,
        await postScoresInstruction(keeper, game, {
          quarter: 3,
          home: 14,
          away: 10,
          isFinal: false,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("QuarterOutOfOrder"));
    expect(
      await sendExpectingError(
        keeper,
        await postScoresInstruction(keeper, game, {
          quarter: 2,
          home: 6,
          away: 10,
          isFinal: false,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("ScoreDecreased"));
    expect(
      await sendExpectingError(
        keeper,
        await postScoresInstruction(keeper, game, {
          quarter: 2,
          home: 14,
          away: 10,
          isFinal: true,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("FinalFlagMismatch"));
    expect(
      await sendExpectingError(
        admin,
        await postScoresInstruction(admin, game, {
          quarter: 2,
          home: 14,
          away: 10,
          isFinal: false,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("Unauthorized"));

    // Q2, Q3, final with overtime, 16 minutes apart.
    const rest = [
      { quarter: 2, home: 14, away: 10, isFinal: false, hadOvertime: false },
      { quarter: 3, home: 17, away: 17, isFinal: false, hadOvertime: false },
      { quarter: 4, home: 24, away: 20, isFinal: true, hadOvertime: true },
    ];
    for (const post of rest) {
      const previous = (await fetchGame(game)).postedAt[post.quarter - 2]!;
      await travelTo(previous + 16n * MINUTE);
      if (post.quarter === 4) {
        // Q4 with is_final = false (PROGRAM §4.2; Step 3 audit L4).
        expect(
          await sendExpectingError(
            keeper,
            await postScoresInstruction(keeper, game, {
              ...post,
              isFinal: false,
              hadOvertime: false,
            }),
          ),
        ).toBe(errorCode("FinalFlagMismatch"));
        expect(errorCode("FinalFlagMismatch")).toBe(6011);
      }
      await send(keeper, await postScoresInstruction(keeper, game, post));
    }
    stored = await fetchGame(game);
    expect(stored.status).toBe(GameStatus.Final);
    expect(stored.quartersPosted).toBe(4);
    expect(stored.finalHadOvertime).toBe(true);
    expect(stored.homeScore).toEqual([7, 14, 17, 24]);
    expect(stored.awayScore).toEqual([3, 10, 17, 20]);
    expect(stored.postedAt.every((t) => t > 0n)).toBe(true);

    // Final is terminal.
    expect(
      await sendExpectingError(
        keeper,
        await postScoresInstruction(keeper, game, {
          quarter: 5,
          home: 30,
          away: 20,
          isFinal: true,
          hadOvertime: false,
        }),
      ),
    ).toBe(errorCode("GameNotScheduled"));
    expect(errorCode("GameNotScheduled")).toBe(6002);
    expect(
      await sendExpectingError(admin, await markGameInstruction(admin, game, GameStatus.Suspended)),
    ).toBe(errorCode("GameNotScheduled"));
  });

  it("5. mark_game is admin-only, irreversible, and only Suspended once scores are posted", async () => {
    expect(
      await sendExpectingError(
        keeper,
        await markGameInstruction(keeper, secondGame, GameStatus.Postponed),
      ),
    ).toBe(errorCode("Unauthorized"));
    const before = await chainNow();
    const signature = await send(
      admin,
      await markGameInstruction(admin, secondGame, GameStatus.Postponed),
    );
    const stored = await fetchGame(secondGame);
    expect(stored.status).toBe(GameStatus.Postponed);
    expect(stored.markedAt).toBeGreaterThanOrEqual(before);
    expect(stored.markedAt).toBeLessThanOrEqual(before + 10n);
    const events = await emittedEvents(signature);
    expect(events.map(eventName)).toEqual(["GameMarked"]);
    const marked = decodeEvent("GameMarked", events[0]!, gameMarkedDecoder);
    expect([marked.game, marked.status]).toEqual([secondGame, GameStatus.Postponed]);
    expect(
      await sendExpectingError(
        admin,
        await markGameInstruction(admin, secondGame, GameStatus.Cancelled),
      ),
    ).toBe(errorCode("GameAlreadyMarked"));
    expect(errorCode("GameAlreadyMarked")).toBe(6003);

    // A third record, kicked off and with Q1 posted: Postponed is refused, Suspended lands.
    const thirdKickoff = (await chainNow()) + 10n * MINUTE;
    const thirdGame = await gamePda(KEY, thirdKickoff);
    await send(keeper, await createGameInstruction(keeper, KEY, thirdKickoff));
    await travelTo(thirdKickoff + 16n * MINUTE);
    await send(
      keeper,
      await postScoresInstruction(keeper, thirdGame, {
        quarter: 1,
        home: 3,
        away: 0,
        isFinal: false,
        hadOvertime: false,
      }),
    );
    expect(
      await sendExpectingError(
        admin,
        await markGameInstruction(admin, thirdGame, GameStatus.Postponed),
      ),
    ).toBe(errorCode("InvalidGameStatus"));
    expect(errorCode("InvalidGameStatus")).toBe(6061);
    await send(admin, await markGameInstruction(admin, thirdGame, GameStatus.Suspended));
    const third = await fetchGame(thirdGame);
    expect(third.status).toBe(GameStatus.Suspended);
    expect(third.quartersPosted).toBe(1);
    expect(third.homeScore[0]).toBe(3);
  });

  it("6. the committed IDL freezes 64 errors from 6000 and carries the game types and events", () => {
    expect(IDL.errors).toHaveLength(64);
    IDL.errors.forEach((e, i) => expect(e.code).toBe(6000 + i));
    expect(IDL.errors[60]!.name).toBe("InvalidGameKey");
    expect(IDL.errors[61]!.name).toBe("InvalidGameStatus");
    const types = IDL.types.map((t) => t.name);
    for (const t of ["GameRecord", "GameKey", "GameStatus"]) expect(types).toContain(t);
    const accounts = IDL.accounts.map((a) => a.name);
    expect(accounts).toContain("GameRecord");
    const events = IDL.events.map((e) => e.name);
    for (const e of ["GameCreated", "KickoffUpdated", "ScoresPosted", "GameMarked"]) {
      expect(events).toContain(e);
    }
    const constants = IDL.constants.map((c) => c.name);
    expect(constants).toContain("GAME_SEED");
  });
});
