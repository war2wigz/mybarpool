/**
 * Subscriptions (DESIGN §10.1): the pool account for state, a log subscription on an address
 * for the signatures of the transactions that touched it, each transaction's inner instructions
 * for the event bodies. All three need `client.rpcSubscriptions`, return a function that
 * aborts, and surface errors through `onError` — never an unhandled rejection.
 */
import { getBase64Encoder, type Address } from "@solana/kit";

import { getPoolDecoder, type Pool, type QuarterSettledEvent } from "../generated/index.js";
import type { MyBarPoolClient } from "./client.js";
import { SdkError } from "./errors.js";
import { eventsOf, type EmittedEvent } from "./events.js";

/** Options every `watch*` takes. */
export interface WatchOptions {
  /** Called with any error the subscription or a fetch raises; the watch keeps going. */
  onError?: (error: unknown) => void;
  /** An external signal; aborting it ends the watch too. */
  abortSignal?: AbortSignal;
}

/** The function a `watch*` returns: ends the subscription. */
export type Unsubscribe = () => void;

function requireSubscriptions(client: MyBarPoolClient) {
  if (!client.rpcSubscriptions) throw new SdkError("NoSubscriptions");
  return client.rpcSubscriptions;
}

function controller(options: WatchOptions): AbortController {
  const c = new AbortController();
  options.abortSignal?.addEventListener("abort", () => c.abort(), { once: true });
  return c;
}

function isAbort(e: unknown): boolean {
  return (e as { name?: string } | null)?.name === "AbortError";
}

/**
 * The `Pool` at `pool` on every change (`accountNotifications`, base64, confirmed); `null` once
 * the account is closed. The current value is not fetched first — read it with `getPool`.
 */
export function watchPool(
  client: MyBarPoolClient,
  pool: Address,
  onPool: (pool: Pool | null) => void,
  options: WatchOptions = {},
): Unsubscribe {
  const subs = requireSubscriptions(client);
  const abort = controller(options);
  const decoder = getPoolDecoder();
  const base64 = getBase64Encoder();
  void (async () => {
    try {
      const notifications = await subs
        .accountNotifications(pool, { encoding: "base64", commitment: "confirmed" })
        .subscribe({ abortSignal: abort.signal });
      for await (const n of notifications) {
        const data = n.value.data[0];
        if (n.value.lamports === 0n || data.length === 0) {
          onPool(null);
          continue;
        }
        try {
          onPool(decoder.decode(base64.encode(data)));
        } catch (e) {
          options.onError?.(e);
        }
      }
    } catch (e) {
      if (!isAbort(e)) options.onError?.(e);
    }
  })();
  return () => abort.abort();
}

/** Filter for {@link watchEvents}: the one address the log subscription mentions. */
export interface WatchEventsFilter {
  mentions: Address;
}

/**
 * Every event of every confirmed, successful transaction mentioning `mentions`
 * (`logsNotifications({ mentions })` for the signatures, `eventsOf` for the bodies).
 */
export function watchEvents(
  client: MyBarPoolClient,
  filter: WatchEventsFilter,
  onEvent: (event: EmittedEvent) => void,
  options: WatchOptions = {},
): Unsubscribe {
  const subs = requireSubscriptions(client);
  const abort = controller(options);
  void (async () => {
    try {
      const notifications = await subs
        .logsNotifications({ mentions: [filter.mentions] }, { commitment: "confirmed" })
        .subscribe({ abortSignal: abort.signal });
      for await (const n of notifications) {
        if (n.value.err !== null) continue;
        try {
          for (const e of await eventsOf(client, n.value.signature)) onEvent(e);
        } catch (e) {
          options.onError?.(e);
        }
      }
    } catch (e) {
      if (!isAbort(e)) options.onError?.(e);
    }
  })();
  return () => abort.abort();
}

/** A `QuarterSettled` with its transaction: the only proof of a result (PROGRAM §7). */
export interface EmittedSettlement extends Omit<EmittedEvent, "event"> {
  readonly event: QuarterSettledEvent;
}

/** {@link watchEvents} on `pool`, filtered to `QuarterSettled`. */
export function watchSettlements(
  client: MyBarPoolClient,
  pool: Address,
  onQuarterSettled: (settlement: EmittedSettlement) => void,
  options: WatchOptions = {},
): Unsubscribe {
  return watchEvents(
    client,
    { mentions: pool },
    (e) => {
      if (e.event.name === "QuarterSettled") {
        onQuarterSettled({ signature: e.signature, slot: e.slot, event: e.event.data });
      }
    },
    options,
  );
}
