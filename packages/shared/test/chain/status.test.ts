/**
 * DESIGN §10.2 derived states and the ARCHITECTURE "leading vs won" rule over the fixture pool.
 */
import { describe, expect, it } from "vitest";

import { GameStatus, PoolStatus, type Pool } from "../../src/generated/index.js";
import {
  displayStatus,
  leadingBox,
  poolView,
  RECLAIM_DELAY_SECONDS,
  returnReason,
  unreturnedBoxesOf,
  wonBoxes,
  toLabel,
  winningBox,
} from "../../src/index.js";
import { BUYER, CREATOR, DEFAULT, gameRecordArgs, poolArgs } from "./fixtures.js";

const pool = (o: Parameters<typeof poolArgs>[0] = {}) =>
  ({ ...poolArgs(o), discriminator: new Uint8Array(8) }) as unknown as Pool;
const game = (o: Parameters<typeof gameRecordArgs>[0] = {}) =>
  ({ ...gameRecordArgs(o), discriminator: new Uint8Array(8) }) as never;

const SCHEDULED = 1_800_000_000n;
const RECORDED = 1_800_003_600n;
const HOME = [3, 7, 0, 9, 1, 5, 8, 2, 6, 4] as const;
const AWAY = [9, 1, 4, 6, 0, 2, 7, 3, 5, 8] as const;
const drawn = (o: Parameters<typeof poolArgs>[0] = {}) =>
  pool({
    status: PoolStatus.Drawn,
    drawn: true,
    homeAxis: new Uint8Array(HOME),
    awayAxis: new Uint8Array(AWAY),
    ...o,
  });

describe("displayStatus (DESIGN §10.2, the chain-only rows)", () => {
  const g = game();
  it("walks the table", () => {
    expect(displayStatus(pool(), g, RECORDED - 1n)).toEqual({ kind: "OPEN" });
    expect(displayStatus(pool({ status: PoolStatus.Locked }), g, RECORDED - 1n)).toEqual({
      kind: "LOCKED_DRAWING",
    });
    expect(displayStatus(drawn(), g, RECORDED - 1n)).toEqual({ kind: "LOCKED_WAITING" });
    expect(displayStatus(drawn(), g, RECORDED)).toEqual({ kind: "LIVE" });
    expect(displayStatus(drawn({ quartersSettled: 2 }), g, RECORDED + 7_200n)).toEqual({
      kind: "LIVE",
    });
    expect(displayStatus(drawn(), game({ status: GameStatus.Final }), RECORDED + 9_000n)).toEqual({
      kind: "LIVE",
    }); // the keeper's settle is what moves it on
    expect(displayStatus(pool({ status: PoolStatus.Settled }), g, RECORDED + 9_000n)).toEqual({
      kind: "SETTLED",
    });
    expect(displayStatus(pool({ status: PoolStatus.Split }), g, RECORDED)).toEqual({
      kind: "SPLIT",
    });
  });

  it("RETURNED carries why", () => {
    const r = pool({ status: PoolStatus.Returned });
    expect(displayStatus(r, g, RECORDED)).toEqual({ kind: "RETURNED", reason: "unfilled" });
    expect(displayStatus(r, game({ status: GameStatus.Postponed }), RECORDED)).toEqual({
      kind: "RETURNED",
      reason: "postponed",
    });
    expect(displayStatus(r, game({ status: GameStatus.Cancelled }), RECORDED)).toEqual({
      kind: "RETURNED",
      reason: "cancelled",
    });
    expect(
      displayStatus(pool({ status: PoolStatus.Returned, cancelledByAdmin: true }), g, RECORDED),
    ).toEqual({ kind: "RETURNED", reason: "cancelledByAdmin" });
    // The game record wins over the admin flag, as PROGRAM §4.6 orders the reasons.
    expect(
      returnReason(
        pool({ status: PoolStatus.Returned, cancelledByAdmin: true }),
        game({ status: GameStatus.Postponed }),
      ),
    ).toBe("postponed");
  });

  it("ABANDONED: an unresolved pool 30 days after the scheduled kickoff, never a resolved one", () => {
    const late = SCHEDULED + RECLAIM_DELAY_SECONDS;
    expect(displayStatus(pool(), g, late - 1n)).toEqual({ kind: "OPEN" });
    expect(displayStatus(pool(), g, late)).toEqual({ kind: "ABANDONED" });
    expect(displayStatus(pool({ status: PoolStatus.Locked }), g, late)).toEqual({
      kind: "ABANDONED",
    });
    expect(displayStatus(drawn(), g, late)).toEqual({ kind: "ABANDONED" });
    // Measured from the scheduled kickoff even when the recorded one moved later.
    expect(displayStatus(pool(), game({ recordedKickoff: late + 10n }), late)).toEqual({
      kind: "ABANDONED",
    });
    expect(displayStatus(pool({ status: PoolStatus.Returned, abandoned: true }), g, late)).toEqual({
      kind: "RETURNED",
      reason: "unfilled",
    });
    expect(displayStatus(pool({ status: PoolStatus.Settled }), g, late)).toEqual({
      kind: "SETTLED",
    });
  });
});

describe("leadingBox vs wonBoxes (ARCHITECTURE › Live scores vs. results)", () => {
  it("leading is undefined until drawn, then the label the scores point at", () => {
    expect(leadingBox(pool(), 14, 7)).toBeUndefined();
    const index = winningBox({ home: 14, away: 7, homeAxis: HOME, awayAxis: AWAY });
    expect(leadingBox(drawn(), 14, 7)).toBe(toLabel(index));
    expect(leadingBox(drawn(), 14, 7)).not.toBe(leadingBox(drawn(), 15, 7)); // moves with the score
  });

  it("won lists only the quarters below quarters_settled", () => {
    const p = drawn({
      quartersSettled: 2,
      winningBox: new Uint8Array([3, 7, 255, 255]),
      quarterPrize: [100n, 200n, 300n, 400n],
    });
    expect(wonBoxes(p)).toEqual([
      { quarter: 1, box: toLabel(3), winner: BUYER, amount: 100n },
      { quarter: 2, box: toLabel(7), winner: BUYER, amount: 200n },
    ]);
    expect(wonBoxes(drawn())).toEqual([]);
    expect(
      wonBoxes(drawn({ quartersSettled: 1, winningBox: new Uint8Array([255, 255, 255, 255]) })),
    ).toEqual([]);
    expect(
      wonBoxes(
        drawn({
          quartersSettled: 4,
          winningBox: new Uint8Array([0, 1, 2, 24]),
          quarterPrize: [1n, 2n, 3n, 4n],
        }),
      ),
    ).toHaveLength(4);
  });

  it("unreturnedBoxesOf reads the bitmap", () => {
    const p = pool({ returned: 0b0000_0011 }); // boxes 0 and 1 (creator's) paid
    expect(unreturnedBoxesOf(p, CREATOR)).toEqual([toLabel(2)]);
    expect(unreturnedBoxesOf(p, BUYER)).toEqual([3, 4, 5, 6, 7].map(toLabel));
    expect(unreturnedBoxesOf(p, DEFAULT)).toHaveLength(17); // the unsold boxes, owner default
  });
});

describe("poolView (DESIGN §10.1)", () => {
  it("names every box by label, in order, with owner, returned bit and won quarters", () => {
    const p = drawn({
      quartersSettled: 3,
      winningBox: new Uint8Array([3, 3, 0, 255]),
      quarterPrize: [100n, 200n, 300n, 400n],
      prizePool: 1_000n,
      returned: 1 << 4,
      sponsoredTotal: 50n,
      feesPaid: true,
    });
    const v = poolView(p, game(), RECORDED + 1n);
    expect(v.status).toEqual({ kind: "LIVE" });
    expect(v.boxes).toHaveLength(25);
    expect(v.boxes.map((b) => b.label)).toEqual(Array.from({ length: 25 }, (_, i) => i + 1));
    expect(v.boxes[0]).toEqual({ label: 1, owner: CREATOR, returned: false, wonQuarters: [3] });
    expect(v.boxes[3]).toEqual({ label: 4, owner: BUYER, returned: false, wonQuarters: [1, 2] });
    expect(v.boxes[4]).toEqual({ label: 5, owner: BUYER, returned: true, wonQuarters: [] });
    expect(v.boxes[24]).toEqual({ label: 25, owner: null, returned: false, wonQuarters: [] });
    expect(v.winners).toHaveLength(3);
    expect(v.sold).toBe(8);
    expect(v.prizePool).toBe(1_000n);
    expect(v.quarterPrizes).toEqual([100n, 200n, 300n, 400n]);
    expect(v.fees).toEqual({
      platform: 62_500_000n,
      creator: 87_500_000n,
      integrator: 0n,
      paid: true,
    });
    expect(v.sponsoredTotal).toBe(50n);
    expect(v.axes).toEqual({ home: [...HOME], away: [...AWAY] });
    expect(poolView(pool(), game(), RECORDED - 1n).axes).toBeUndefined();
  });
});
