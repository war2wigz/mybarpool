/**
 * Program events (PROGRAM §7). Each emitting instruction self-CPIs with data =
 * `EVENT_IX_TAG` ‖ 8-byte event discriminator ‖ Borsh body; in a fetched transaction they are
 * the inner instructions whose program is ours and whose data starts with the tag. Program
 * logs are never parsed for events (they truncate; `QuarterSettled` is the only proof of a
 * result and must never be lost).
 */
import {
  getBase58Encoder,
  getBase64Encoder,
  getCompiledTransactionMessageDecoder,
  getTransactionDecoder,
  type Address,
  type ReadonlyUint8Array,
  type Signature,
} from "@solana/kit";

import {
  getBoxesBoughtEventDecoder,
  type BoxesBoughtEvent,
  getBoxesReclaimedEventDecoder,
  type BoxesReclaimedEvent,
  getBoxesReturnedEventDecoder,
  type BoxesReturnedEvent,
  getBoxesSplitEventDecoder,
  type BoxesSplitEvent,
  getConfigUpdatedEventDecoder,
  type ConfigUpdatedEvent,
  getDigitsDrawnEventDecoder,
  type DigitsDrawnEvent,
  getGameCreatedEventDecoder,
  type GameCreatedEvent,
  getGameMarkedEventDecoder,
  type GameMarkedEvent,
  getGateKeyRotatedEventDecoder,
  type GateKeyRotatedEvent,
  getKickoffUpdatedEventDecoder,
  type KickoffUpdatedEvent,
  getOverrideClosedEventDecoder,
  type OverrideClosedEvent,
  getOverrideSetEventDecoder,
  type OverrideSetEvent,
  getPoolCancelledEventDecoder,
  type PoolCancelledEvent,
  getPoolClosedEventDecoder,
  type PoolClosedEvent,
  getPoolCreatedEventDecoder,
  type PoolCreatedEvent,
  getPoolLockedEventDecoder,
  type PoolLockedEvent,
  getQuarterSettledEventDecoder,
  type QuarterSettledEvent,
  getScoresPostedEventDecoder,
  type ScoresPostedEvent,
  getSponsoredEventDecoder,
  type SponsoredEvent,
  getSponsorshipClosedEventDecoder,
  type SponsorshipClosedEvent,
  getSponsorshipReturnedEventDecoder,
  type SponsorshipReturnedEvent,
  getVarReplacedEventDecoder,
  type VarReplacedEvent,
  getVarSampledEventDecoder,
  type VarSampledEvent,
  getVarSetEventDecoder,
  type VarSetEvent,
  identifyMybarpoolEvent,
  MybarpoolEvent,
} from "../generated/index.js";
import type { MyBarPoolClient } from "./client.js";
import { SdkError } from "./errors.js";

/** `anchor_lang::event::EVENT_IX_TAG` (0x1d9acb512ea545e4) little-endian. */
export const EVENT_IX_TAG: ReadonlyUint8Array = new Uint8Array([
  0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d,
]);

/** One decoded event: its PROGRAM §7 name and typed body. */
export type MyBarPoolEvent =
  | { name: "BoxesBought"; data: BoxesBoughtEvent }
  | { name: "BoxesReclaimed"; data: BoxesReclaimedEvent }
  | { name: "BoxesReturned"; data: BoxesReturnedEvent }
  | { name: "BoxesSplit"; data: BoxesSplitEvent }
  | { name: "ConfigUpdated"; data: ConfigUpdatedEvent }
  | { name: "DigitsDrawn"; data: DigitsDrawnEvent }
  | { name: "GameCreated"; data: GameCreatedEvent }
  | { name: "GameMarked"; data: GameMarkedEvent }
  | { name: "GateKeyRotated"; data: GateKeyRotatedEvent }
  | { name: "KickoffUpdated"; data: KickoffUpdatedEvent }
  | { name: "OverrideClosed"; data: OverrideClosedEvent }
  | { name: "OverrideSet"; data: OverrideSetEvent }
  | { name: "PoolCancelled"; data: PoolCancelledEvent }
  | { name: "PoolClosed"; data: PoolClosedEvent }
  | { name: "PoolCreated"; data: PoolCreatedEvent }
  | { name: "PoolLocked"; data: PoolLockedEvent }
  | { name: "QuarterSettled"; data: QuarterSettledEvent }
  | { name: "ScoresPosted"; data: ScoresPostedEvent }
  | { name: "Sponsored"; data: SponsoredEvent }
  | { name: "SponsorshipClosed"; data: SponsorshipClosedEvent }
  | { name: "SponsorshipReturned"; data: SponsorshipReturnedEvent }
  | { name: "VarReplaced"; data: VarReplacedEvent }
  | { name: "VarSampled"; data: VarSampledEvent }
  | { name: "VarSet"; data: VarSetEvent };

/** The name of a {@link MyBarPoolEvent}. */
export type MyBarPoolEventName = MyBarPoolEvent["name"];

const decoders: Record<MybarpoolEvent, (b: ReadonlyUint8Array) => MyBarPoolEvent> = {
  [MybarpoolEvent.BoxesBought]: (b) => ({
    name: "BoxesBought",
    data: getBoxesBoughtEventDecoder().decode(b),
  }),
  [MybarpoolEvent.BoxesReclaimed]: (b) => ({
    name: "BoxesReclaimed",
    data: getBoxesReclaimedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.BoxesReturned]: (b) => ({
    name: "BoxesReturned",
    data: getBoxesReturnedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.BoxesSplit]: (b) => ({
    name: "BoxesSplit",
    data: getBoxesSplitEventDecoder().decode(b),
  }),
  [MybarpoolEvent.ConfigUpdated]: (b) => ({
    name: "ConfigUpdated",
    data: getConfigUpdatedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.DigitsDrawn]: (b) => ({
    name: "DigitsDrawn",
    data: getDigitsDrawnEventDecoder().decode(b),
  }),
  [MybarpoolEvent.GameCreated]: (b) => ({
    name: "GameCreated",
    data: getGameCreatedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.GameMarked]: (b) => ({
    name: "GameMarked",
    data: getGameMarkedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.GateKeyRotated]: (b) => ({
    name: "GateKeyRotated",
    data: getGateKeyRotatedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.KickoffUpdated]: (b) => ({
    name: "KickoffUpdated",
    data: getKickoffUpdatedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.OverrideClosed]: (b) => ({
    name: "OverrideClosed",
    data: getOverrideClosedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.OverrideSet]: (b) => ({
    name: "OverrideSet",
    data: getOverrideSetEventDecoder().decode(b),
  }),
  [MybarpoolEvent.PoolCancelled]: (b) => ({
    name: "PoolCancelled",
    data: getPoolCancelledEventDecoder().decode(b),
  }),
  [MybarpoolEvent.PoolClosed]: (b) => ({
    name: "PoolClosed",
    data: getPoolClosedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.PoolCreated]: (b) => ({
    name: "PoolCreated",
    data: getPoolCreatedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.PoolLocked]: (b) => ({
    name: "PoolLocked",
    data: getPoolLockedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.QuarterSettled]: (b) => ({
    name: "QuarterSettled",
    data: getQuarterSettledEventDecoder().decode(b),
  }),
  [MybarpoolEvent.ScoresPosted]: (b) => ({
    name: "ScoresPosted",
    data: getScoresPostedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.Sponsored]: (b) => ({
    name: "Sponsored",
    data: getSponsoredEventDecoder().decode(b),
  }),
  [MybarpoolEvent.SponsorshipClosed]: (b) => ({
    name: "SponsorshipClosed",
    data: getSponsorshipClosedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.SponsorshipReturned]: (b) => ({
    name: "SponsorshipReturned",
    data: getSponsorshipReturnedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.VarReplaced]: (b) => ({
    name: "VarReplaced",
    data: getVarReplacedEventDecoder().decode(b),
  }),
  [MybarpoolEvent.VarSampled]: (b) => ({
    name: "VarSampled",
    data: getVarSampledEventDecoder().decode(b),
  }),
  [MybarpoolEvent.VarSet]: (b) => ({ name: "VarSet", data: getVarSetEventDecoder().decode(b) }),
};

/** An inner instruction as `getTransaction` returns it (any encoding): the data is base58. */
export interface InnerInstructionLike {
  readonly programIdIndex: number;
  readonly data: string;
}

/** The part of a transaction's `meta` the decoder reads. */
export interface MetaLike {
  readonly innerInstructions?:
    readonly { readonly instructions: readonly InnerInstructionLike[] }[] | null;
  readonly loadedAddresses?: {
    readonly writable: readonly Address[];
    readonly readonly: readonly Address[];
  } | null;
}

/**
 * Decode one event payload (the bytes *after* the tag, discriminator included), or `undefined`
 * when the discriminator is unknown or the body is short.
 */
export function decodeEvent(payload: ReadonlyUint8Array): MyBarPoolEvent | undefined {
  if (payload.length < 8) return undefined;
  let kind: MybarpoolEvent;
  try {
    kind = identifyMybarpoolEvent(payload);
  } catch {
    return undefined;
  }
  try {
    return decoders[kind](payload);
  } catch {
    return undefined;
  }
}

/**
 * Every event the transaction emitted from the program at `programAddress`, in order.
 * `accountKeys` is the message's static accounts; the loaded addresses (writable, then
 * readonly) follow them, as the runtime indexes. A truncated payload is skipped, not thrown.
 */
export function decodeEvents(
  meta: MetaLike | null | undefined,
  accountKeys: readonly Address[],
  programAddress: Address,
): MyBarPoolEvent[] {
  if (!meta?.innerInstructions) return [];
  const keys = [
    ...accountKeys,
    ...(meta.loadedAddresses?.writable ?? []),
    ...(meta.loadedAddresses?.readonly ?? []),
  ];
  const base58 = getBase58Encoder();
  const out: MyBarPoolEvent[] = [];
  for (const group of meta.innerInstructions) {
    for (const ix of group.instructions) {
      if (keys[ix.programIdIndex] !== programAddress) continue;
      let data: ReadonlyUint8Array;
      try {
        data = base58.encode(ix.data);
      } catch {
        continue;
      }
      if (data.length < 16 || !startsWithTag(data)) continue;
      const event = decodeEvent(data.slice(8));
      if (event) out.push(event);
    }
  }
  return out;
}

function startsWithTag(data: ReadonlyUint8Array): boolean {
  for (let i = 0; i < 8; i++) if (data[i] !== EVENT_IX_TAG[i]) return false;
  return true;
}

/** The static account keys of a base64-encoded transaction, any version. */
export function accountKeysOf(transactionBase64: string): Address[] {
  const bytes = getBase64Encoder().encode(transactionBase64);
  const tx = getTransactionDecoder().decode(bytes);
  const message = getCompiledTransactionMessageDecoder().decode(tx.messageBytes);
  return [...message.staticAccounts];
}

/** A decoded event with the transaction it came from. */
export interface EmittedEvent {
  readonly signature: Signature;
  readonly slot: bigint;
  readonly event: MyBarPoolEvent;
}

/**
 * The events of a confirmed transaction: fetched with `maxSupportedTransactionVersion: 1` and
 * `encoding: "base64"` (the runtime refuses base58 for a v1 transaction), then decoded from the
 * inner instructions. `SdkError("NotFound")` when the transaction is unknown.
 */
export async function eventsOf(
  client: MyBarPoolClient,
  signature: Signature,
): Promise<EmittedEvent[]> {
  const tx = await client.rpc
    .getTransaction(signature, {
      maxSupportedTransactionVersion: 1,
      encoding: "base64",
      commitment: "confirmed",
    })
    .send();
  if (!tx) throw new SdkError("NotFound", { address: signature });
  const keys = accountKeysOf(tx.transaction[0]);
  return decodeEvents(tx.meta, keys, client.programAddress).map((event) => ({
    signature,
    slot: tx.slot,
    event,
  }));
}
