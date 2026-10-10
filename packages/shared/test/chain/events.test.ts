/**
 * Event decoding from inner instructions (PROGRAM §7) against a transaction captured from
 * localnet, plus the edge cases: another program's transaction, a truncated payload, an unknown
 * discriminator, `eventsOf` over a mock RPC, and `watch*` over mock subscriptions.
 */
import { readFileSync } from "node:fs";

import { address, getBase58Decoder, type Address, type Signature } from "@solana/kit";
import { describe, expect, it } from "vitest";

import {
  getPoolEncoder,
  getQuarterSettledEventEncoder,
  identifyMybarpoolEvent,
  MybarpoolEvent,
  QUARTER_SETTLED_EVENT_DISCRIMINATOR,
} from "../../src/generated/index.js";
import {
  accountKeysOf,
  createMyBarPoolClient,
  decodeEvent,
  decodeEvents,
  EVENT_IX_TAG,
  eventsOf,
  isSdkError,
  watchEvents,
  watchPool,
  watchSettlements,
  type EmittedEvent,
  type MyBarPoolClient,
} from "../../src/index.js";
import { accountInfo, mockRpc, POOL, poolArgs } from "./fixtures.js";

type Captured = {
  programAddress: string;
  pool: string;
  settle: Tx;
  buy: Tx;
};
type Tx = {
  slot: string;
  version: number | string;
  transaction: [string, "base64"];
  meta: {
    innerInstructions: Array<{
      index: number;
      instructions: Array<{ programIdIndex: number; data: string }>;
    }>;
    loadedAddresses?: { writable: string[]; readonly: string[] } | null;
    logMessages: string[];
  };
};
const F = JSON.parse(
  readFileSync(new URL("../fixtures/settle-transaction.json", import.meta.url), "utf8"),
) as Captured;
const PROGRAM = address(F.programAddress);

function keysOf(tx: Tx): Address[] {
  return accountKeysOf(tx.transaction[0]);
}

describe("events from inner instructions (PROGRAM §7)", () => {
  it("the settle transaction decodes to one QuarterSettled with the expected fields", () => {
    const events = decodeEvents(F.settle.meta as never, keysOf(F.settle), PROGRAM);
    expect(events.map((e) => e.name)).toEqual(["QuarterSettled"]);
    const e = events[0]!;
    if (e.name !== "QuarterSettled") throw new Error("narrowing");
    expect(e.data.pool).toBe(F.pool);
    expect(e.data.quarter).toBe(1);
    expect([e.data.home, e.data.away]).toEqual([7, 3]); // the Step 3 scores
    expect(e.data.boxIndex).toBeLessThan(25);
    expect(e.data.amount).toBe(220_000_000n); // ARCHITECTURE › Fees worked example, Q1
    expect(e.data.feesPaidNow).toBe(true);
    expect(e.data.platformFee).toBe(62_500_000n);
    expect(e.data.creatorFee).toBe(87_500_000n);
  });

  it("the locking buy decodes to BoxesBought then PoolLocked, in order", () => {
    const events = decodeEvents(F.buy.meta as never, keysOf(F.buy), PROGRAM);
    expect(events.map((e) => e.name)).toEqual(["BoxesBought", "PoolLocked"]);
    const bought = events[0]!;
    if (bought.name !== "BoxesBought") throw new Error("narrowing");
    expect(bought.data.pool).toBe(F.pool);
    expect(bought.data.count).toBe(4);
    expect(bought.data.soldAfter).toBe(25);
    expect(bought.data.boxes).toHaveLength(4);
    for (const b of bought.data.boxes) expect(b).toBeLessThan(25); // 0-based, the program's field
  });

  it("another program's inner instructions, a short payload and an unknown discriminator yield nothing", () => {
    const keys = keysOf(F.settle);
    const other = address("ASo8r4EEFLPAMDk1w3XdKbEmq4c1GynbsHGa6RGG83fH");
    expect(decodeEvents(F.settle.meta as never, keys, other)).toEqual([]);
    expect(decodeEvents(null, keys, PROGRAM)).toEqual([]);
    expect(decodeEvents({ innerInstructions: null }, keys, PROGRAM)).toEqual([]);

    const base58 = getBase58Decoder();
    const idx = keys.indexOf(PROGRAM);
    const tagged = (body: Uint8Array) => base58.decode(new Uint8Array([...EVENT_IX_TAG, ...body]));
    const synthetic = (data: string) => ({
      innerInstructions: [{ index: 0, instructions: [{ programIdIndex: idx, data }] }],
    });
    // Truncated: the discriminator and half a body.
    const truncated = new Uint8Array([...QUARTER_SETTLED_EVENT_DISCRIMINATOR, 1, 2, 3]);
    expect(decodeEvents(synthetic(tagged(truncated)), keys, PROGRAM)).toEqual([]);
    // Unknown discriminator with a long body.
    expect(decodeEvents(synthetic(tagged(new Uint8Array(80).fill(9))), keys, PROGRAM)).toEqual([]);
    // No tag at all (a CPI to our program that is not an event).
    expect(decodeEvents(synthetic(base58.decode(new Uint8Array(40))), keys, PROGRAM)).toEqual([]);
    // Not base58.
    expect(decodeEvents(synthetic("0OIl"), keys, PROGRAM)).toEqual([]);
    expect(decodeEvent(new Uint8Array(4))).toBeUndefined();
  });

  it("identifyMybarpoolEvent and the decoders expect the bytes after the tag, discriminator included", () => {
    const body = getQuarterSettledEventEncoder().encode({
      time: 1n,
      pool: POOL,
      quarter: 2,
      home: 14,
      away: 10,
      boxIndex: 7,
      winner: POOL,
      amount: 5n,
      feesPaidNow: false,
      platformFee: 0n,
      creatorFee: 0n,
      integratorFee: 0n,
    });
    expect(identifyMybarpoolEvent(body)).toBe(MybarpoolEvent.QuarterSettled);
    const e = decodeEvent(body);
    expect(e?.name).toBe("QuarterSettled");
    if (e?.name === "QuarterSettled") expect(e.data.boxIndex).toBe(7);
  });

  it("eventsOf fetches with maxSupportedTransactionVersion 1 and base64 and decodes", async () => {
    const { rpc, calls } = mockRpc({
      getTransaction: (sig: string) => (sig === "found" ? { ...F.settle, slot: 42n } : null),
    });
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    const got = await eventsOf(client, "found" as Signature);
    expect(got).toHaveLength(1);
    expect(got[0]!.slot).toBe(42n);
    expect(got[0]!.event.name).toBe("QuarterSettled");
    expect(calls[0]!.params[1]).toEqual({
      maxSupportedTransactionVersion: 1,
      encoding: "base64",
      commitment: "confirmed",
    });
    await expect(eventsOf(client, "missing" as Signature)).rejects.toSatisfy((e) =>
      isSdkError(e, "NotFound"),
    );
  });
});

/** A mock subscriptions object: `method(...)` → `{ subscribe }` yielding the given notifications. */
function mockSubscriptions(notifications: Record<string, unknown[]>, recorded: unknown[][] = []) {
  return new Proxy(
    {},
    {
      get(_t, method: string) {
        return (...params: unknown[]) => ({
          subscribe: async ({ abortSignal }: { abortSignal: AbortSignal }) => {
            recorded.push([method, ...params]);
            const items = notifications[method] ?? [];
            return (async function* () {
              for (const n of items) {
                if (abortSignal.aborted) return;
                yield n;
                await new Promise((r) => setTimeout(r, 0));
              }
              // Stay open until aborted, as a real subscription would.
              await new Promise<void>((resolve) =>
                abortSignal.addEventListener("abort", () => resolve(), { once: true }),
              );
            })();
          },
        });
      },
    },
  );
}

const tick = () => new Promise((r) => setTimeout(r, 10));

describe("watch* (DESIGN §10.1)", () => {
  it("every watch needs rpcSubscriptions", () => {
    const { rpc } = mockRpc({});
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    expect(() => watchPool(client, POOL, () => {})).toThrow(/rpcSubscriptions/);
    expect(() => watchEvents(client, { mentions: POOL }, () => {})).toThrow(/rpcSubscriptions/);
    expect(() => watchSettlements(client, POOL, () => {})).toThrow(/rpcSubscriptions/);
  });

  it("watchPool decodes each account notification and reports a closed account as null", async () => {
    const bytes = new Uint8Array(getPoolEncoder().encode(poolArgs({ sold: 12 })));
    const recorded: unknown[][] = [];
    const subs = mockSubscriptions(
      {
        accountNotifications: [
          { context: { slot: 1n }, value: accountInfo(bytes) },
          { context: { slot: 2n }, value: { ...accountInfo(new Uint8Array()), lamports: 0n } },
          {
            context: { slot: 3n },
            value: { ...accountInfo(new Uint8Array([1, 2, 3])), lamports: 5n },
          },
        ],
      },
      recorded,
    );
    const { rpc } = mockRpc({});
    const client: MyBarPoolClient = createMyBarPoolClient({
      rpc,
      rpcSubscriptions: subs as never,
      programAddress: PROGRAM,
    });
    const seen: Array<number | null> = [];
    const errors: unknown[] = [];
    const stop = watchPool(client, POOL, (p) => seen.push(p ? p.sold : null), {
      onError: (e) => errors.push(e),
    });
    await tick();
    await tick();
    stop();
    expect(seen).toEqual([12, null]);
    expect(errors).toHaveLength(1); // the 3-byte garbage: reported, the watch kept going
    expect(recorded[0]).toEqual([
      "accountNotifications",
      POOL,
      { encoding: "base64", commitment: "confirmed" },
    ]);
  });

  it("watchEvents skips failed transactions and emits every event of the successful ones; watchSettlements filters", async () => {
    const recorded: unknown[][] = [];
    const subs = mockSubscriptions(
      {
        logsNotifications: [
          {
            context: { slot: 7n },
            value: { err: { InstructionError: [0, "Custom"] }, logs: [], signature: "bad" },
          },
          { context: { slot: 8n }, value: { err: null, logs: [], signature: "buy" } },
          { context: { slot: 9n }, value: { err: null, logs: [], signature: "settle" } },
          { context: { slot: 10n }, value: { err: null, logs: [], signature: "gone" } },
        ],
      },
      recorded,
    );
    const { rpc } = mockRpc({
      getTransaction: (sig: string) =>
        sig === "buy"
          ? { ...F.buy, slot: 8n }
          : sig === "settle"
            ? { ...F.settle, slot: 9n }
            : null,
    });
    const client = createMyBarPoolClient({
      rpc,
      rpcSubscriptions: subs as never,
      programAddress: PROGRAM,
    });
    const events: EmittedEvent[] = [];
    const errors: unknown[] = [];
    const stop = watchEvents(client, { mentions: POOL }, (e) => events.push(e), {
      onError: (e) => errors.push(e),
      fetchRetries: 0,
    });
    for (let i = 0; i < 6; i++) await tick();
    stop();
    expect(events.map((e) => [e.event.name, e.signature, e.slot])).toEqual([
      ["BoxesBought", "buy", 8n],
      ["PoolLocked", "buy", 8n],
      ["QuarterSettled", "settle", 9n],
    ]);
    expect(errors).toHaveLength(1); // "gone": NotFound reported (asked once here), not thrown
    expect(isSdkError(errors[0], "NotFound")).toBe(true);
    expect(recorded[0]).toEqual([
      "logsNotifications",
      { mentions: [POOL] },
      { commitment: "confirmed" },
    ]);

    const settled: number[] = [];
    const stop2 = watchSettlements(client, POOL, (s) => settled.push(s.event.quarter));
    for (let i = 0; i < 6; i++) await tick();
    stop2();
    expect(settled).toEqual([1]);
  });

  it("watchEvents asks getTransaction again when the node has not indexed an announced signature yet", async () => {
    // The log notification can arrive a moment before `getTransaction` knows the signature;
    // a settlement must not be dropped for that. Two nulls, then the transaction.
    const subs = mockSubscriptions({
      logsNotifications: [
        { context: { slot: 9n }, value: { err: null, logs: [], signature: "settle" } },
      ],
    });
    let asked = 0;
    const { rpc, calls } = mockRpc({
      getTransaction: () => (++asked <= 2 ? null : { ...F.settle, slot: 9n }),
    });
    const client = createMyBarPoolClient({
      rpc,
      rpcSubscriptions: subs as never,
      programAddress: PROGRAM,
    });
    const settled: number[] = [];
    const errors: unknown[] = [];
    const stop = watchSettlements(client, POOL, (s) => settled.push(s.event.quarter), {
      onError: (e) => errors.push(e),
      fetchRetryDelayMs: 0,
    });
    for (let i = 0; i < 6; i++) await tick();
    stop();
    expect(settled).toEqual([1]);
    expect(errors).toEqual([]);
    expect(calls.filter((c) => c.method === "getTransaction")).toHaveLength(3);

    // Past the retry budget it is reported, once.
    asked = 0;
    const never = mockRpc({ getTransaction: () => null });
    const client2 = createMyBarPoolClient({
      rpc: never.rpc,
      rpcSubscriptions: mockSubscriptions({
        logsNotifications: [
          { context: { slot: 9n }, value: { err: null, logs: [], signature: "settle" } },
        ],
      }) as never,
      programAddress: PROGRAM,
    });
    const errors2: unknown[] = [];
    const stop2 = watchSettlements(client2, POOL, () => {}, {
      onError: (e) => errors2.push(e),
      fetchRetries: 2,
      fetchRetryDelayMs: 0,
    });
    for (let i = 0; i < 6; i++) await tick();
    stop2();
    expect(errors2).toHaveLength(1);
    expect(isSdkError(errors2[0], "NotFound")).toBe(true);
    expect(never.calls.filter((c) => c.method === "getTransaction")).toHaveLength(3);
  });

  it("an external abort signal ends the watch; a failing subscription reaches onError", async () => {
    const failing = new Proxy(
      {},
      {
        get: () => () => ({
          subscribe: async () => {
            throw new Error("socket down");
          },
        }),
      },
    );
    const { rpc } = mockRpc({});
    const client = createMyBarPoolClient({
      rpc,
      rpcSubscriptions: failing as never,
      programAddress: PROGRAM,
    });
    const errors: unknown[] = [];
    const ac = new AbortController();
    watchPool(client, POOL, () => {}, { onError: (e) => errors.push(e), abortSignal: ac.signal });
    await tick();
    expect(String(errors[0])).toContain("socket down");
    ac.abort();
  });
});
