/**
 * The generated types the chain layer's own API names, under the root so an app needs nothing
 * from the `./generated` subpath to type what it reads. The shapes are Codama's, rendered from
 * the IDL; the doc lines are the IDL's own. Everything else generated stays behind the subpath.
 */
import type * as g from "../generated/index.js";

/** PROGRAM §3.1 `PlatformConfig`: the account the rest of the program reads. */
export type PlatformConfig = g.PlatformConfig;
/** PROGRAM §3.2 `GameRecord`. */
export type GameRecord = g.GameRecord;
/** PROGRAM §3.3 `Pool`. */
export type Pool = g.Pool;
/** PROGRAM §3.5 `CreatorCounter`. */
export type CreatorCounter = g.CreatorCounter;
/** PROGRAM §3.6 `WalletOverride`: per-wallet creator limits. */
export type WalletOverride = g.WalletOverride;
/** PROGRAM §3.7 `Sponsorship`. */
export type Sponsorship = g.Sponsorship;
/** PROGRAM §3.1 `TokenRule`: one per token index (0 SOL, 1 SKR, 2 ORE). */
export type TokenRule = g.TokenRule;
/** PROGRAM §7 `BoxesBought`: a purchase landed (by `buy`, or by `create_pool` for the creator). */
export type BoxesBoughtEvent = g.BoxesBoughtEvent;
/** PROGRAM §7 `BoxesReclaimed`: a box owner took their own boxes back after the 30-day clock (§4.6 `reclaim`). */
export type BoxesReclaimedEvent = g.BoxesReclaimedEvent;
/** PROGRAM §7 `BoxesReturned`: `return_boxes` paid one owner the purchase price of their unreturned boxes (§4.6). */
export type BoxesReturnedEvent = g.BoxesReturnedEvent;
/** PROGRAM §7 `BoxesSplit`: `split` paid one owner `split_amount` per unreturned box (§4.6). */
export type BoxesSplitEvent = g.BoxesSplitEvent;
/** PROGRAM §7 `ConfigUpdated`: the full config snapshot after `initialize` or `update_config`. */
export type ConfigUpdatedEvent = g.ConfigUpdatedEvent;
/** PROGRAM §7 `DigitsDrawn`: `draw` derived both axes from the `Var`'s value. */
export type DigitsDrawnEvent = g.DigitsDrawnEvent;
/** PROGRAM §7 `GameCreated`: a `GameRecord` was created by `create_game`. */
export type GameCreatedEvent = g.GameCreatedEvent;
/** PROGRAM §7 `GameMarked`: the admin marked the record `Postponed`, `Cancelled` or `Suspended`. */
export type GameMarkedEvent = g.GameMarkedEvent;
/** PROGRAM §7 `GateKeyRotated`: the creator rotated a `Link` pool's gate key. */
export type GateKeyRotatedEvent = g.GateKeyRotatedEvent;
/** PROGRAM §7 `KickoffUpdated`: `update_kickoff` moved `recorded_kickoff`. */
export type KickoffUpdatedEvent = g.KickoffUpdatedEvent;
/** PROGRAM §7 `OverrideClosed`: a `WalletOverride` was closed; carries the values that were closed so an indexer needs no prior state. */
export type OverrideClosedEvent = g.OverrideClosedEvent;
/** PROGRAM §7 `OverrideSet`: a `WalletOverride` was created or updated. */
export type OverrideSetEvent = g.OverrideSetEvent;
/** PROGRAM §7 `PoolCancelled`: the admin moved an unpaid pool to `Returned` (§4.6 `cancel_pool`). */
export type PoolCancelledEvent = g.PoolCancelledEvent;
/** PROGRAM §7 `PoolClosed`: a terminal pool's vault and account closed to `destination`. */
export type PoolClosedEvent = g.PoolClosedEvent;
/** PROGRAM §7 `PoolCreated`: `create_pool` wrote a `Pool`. */
export type PoolCreatedEvent = g.PoolCreatedEvent;
/** PROGRAM §7 `PoolLocked`: the 25th box sold. */
export type PoolLockedEvent = g.PoolLockedEvent;
/** PROGRAM §7 `QuarterSettled`: one quarter paid (or recorded, on a zero share). */
export type QuarterSettledEvent = g.QuarterSettledEvent;
/** PROGRAM §7 `ScoresPosted`: `post_scores` landed a quarter. */
export type ScoresPostedEvent = g.ScoresPostedEvent;
/** PROGRAM §7 `Sponsored`: a `sponsor` call landed. */
export type SponsoredEvent = g.SponsoredEvent;
/** PROGRAM §7 `SponsorshipClosed`: a committed sponsorship's account closed on a terminal pool (`close_sponsorship`, §4.6); the amount stays in the pool. */
export type SponsorshipClosedEvent = g.SponsorshipClosedEvent;
/** PROGRAM §7 `SponsorshipReturned`: a sponsorship paid back in full to its wallet and its account closed (`return_sponsorship` or `reclaim_sponsorship`, §4.6). */
export type SponsorshipReturnedEvent = g.SponsorshipReturnedEvent;
/** PROGRAM §7 `VarReplaced`: the admin bound a replacement `Var`. */
export type VarReplacedEvent = g.VarReplacedEvent;
/** PROGRAM §7 `VarSampled`: `sample_var` recorded a hash it matched against SlotHashes. */
export type VarSampledEvent = g.VarSampledEvent;
/** PROGRAM §7 `VarSet`: `set_var` bound a `Var` to the pool. */
export type VarSetEvent = g.VarSetEvent;

// The three enums are values as well as types and are re-exported as they are; the
// discriminant is the on-chain byte.
export { AccessType, GameStatus, PoolStatus } from "../generated/index.js";
