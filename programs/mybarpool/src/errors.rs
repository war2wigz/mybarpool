//! Errors, PROGRAM §8. Numbered from 6000 in declaration order; names are the
//! contract and the numbers freeze when the program ships. The 59 §8 names
//! come first, then the ones appended before anything shipped: `InvalidConfig`
//! (6059, Step 2) for a violated §3.1 invariant, `InvalidGameKey` (6060) and
//! `InvalidGameStatus` (6061, both Step 3) for the §4.2 key rules and the
//! `mark_game` status rules, `SampleTooEarly` (6062) and `VarAlreadySampled`
//! (6063, both Step 5) for two `sample_var` / `replace_var` checks, which §8
//! had no error for.

use anchor_lang::prelude::*;

#[error_code]
pub enum MybarpoolError {
    /// 6000: the signer is not the key this instruction requires.
    #[msg("Signer is not authorized for this instruction")]
    Unauthorized,
    /// 6001: `config.paused` is set; create_pool, buy and sponsor are refused.
    #[msg("MyBarPool is paused; nothing has moved")]
    Paused,
    /// 6002: the game record's status is not `Scheduled` (PROGRAM §4.2, §9): `update_kickoff`
    /// and `post_scores` on a marked or `Final` record, `mark_game` on a `Final` one, and
    /// later `create_pool` on a marked one. Terminal records are inert.
    #[msg("Game is not scheduled: the record is final or has been marked")]
    GameNotScheduled,
    /// 6003: `mark_game` on a record already marked `Postponed`, `Cancelled` or `Suspended`
    /// (PROGRAM §4.2: marks are irreversible; a corrected game is a new record).
    #[msg("Game is already marked; marks are irreversible")]
    GameAlreadyMarked,
    /// 6004: now is at or after the recorded kickoff.
    #[msg("Sales closed at kickoff")]
    SalesClosed,
    /// 6005: a kickoff time at or before the current Clock time (PROGRAM §4.2): `create_game`
    /// with `scheduled_kickoff ≤ now`, `update_kickoff` with `new_time ≤ now`.
    #[msg("Kickoff must be later than the current time")]
    KickoffInPast,
    /// 6006: `update_kickoff` with `new_time > scheduled_kickoff + KICKOFF_UPDATE_BOUND`
    /// (PROGRAM §1: 72 hours), measured from the scheduled kickoff the record was seeded with,
    /// never from the recorded one. Anything later is a postponement, marked by the admin.
    #[msg("Kickoff may not move more than 72 hours past the scheduled kickoff")]
    KickoffOutOfBounds,
    /// 6007: `update_kickoff` once `now ≥ recorded_kickoff` (sales have closed) or once any
    /// quarter has been posted (PROGRAM §4.2).
    #[msg("Kickoff can no longer be updated: the recorded kickoff has passed")]
    KickoffUpdateTooLate,
    /// 6008: `post_scores` with `quarter != quarters_posted + 1` (PROGRAM §4.2): a repeat, a
    /// skip, quarter 0 or a quarter above 4.
    #[msg("Quarter must be the next unposted one (1 to 4, in order)")]
    QuarterOutOfOrder,
    /// 6009: `post_scores` sooner than `MIN_QUARTER_SECONDS` (PROGRAM §1: 15 minutes) after the
    /// recorded kickoff (Q1) or after the previous post (Q2–Q4).
    #[msg("Quarter posted sooner than 15 minutes after kickoff or the previous post")]
    QuarterTooSoon,
    /// 6010: a cumulative home or away score lower than the previous post's (PROGRAM §4.2).
    #[msg("A cumulative score is lower than the previous post")]
    ScoreDecreased,
    /// 6011: `is_final` not equal to `quarter == 4`, or `had_overtime` without `is_final`
    /// (PROGRAM §4.2: the fourth post is the final score, after any overtime).
    #[msg("is_final must be set on the fourth post only; had_overtime needs is_final")]
    FinalFlagMismatch,
    /// 6012: the token rule is disabled.
    #[msg("Token is disabled")]
    TokenDisabled,
    /// 6013: the price is not `min + k × step` within the cap.
    #[msg("Price is off the ladder")]
    PriceOffLadder,
    /// 6014: not one of the three payout presets.
    #[msg("Invalid payout preset")]
    InvalidPreset,
    /// 6015: not one of the three access types, or its key/root is missing.
    #[msg("Invalid access type")]
    InvalidAccessType,
    /// 6016: a link pool needs its gate key.
    #[msg("Gate key missing")]
    GateKeyMissing,
    /// 6017: the gate key did not sign.
    #[msg("Gate key is not a signer")]
    GateKeyNotSigner,
    /// 6018: the Merkle proof does not match the allowlist root.
    #[msg("Allowlist proof is invalid")]
    AllowlistProofInvalid,
    /// 6019: creator add-on plus integrator fee exceed the add-on budget.
    #[msg("Add-on budget exceeded")]
    AddonBudgetExceeded,
    /// 6020: `integrator_bps` set without an integrator, or vice versa.
    #[msg("Integrator and integrator fee do not match")]
    IntegratorMismatch,
    /// 6021: the creator is at their open-pool limit for this game.
    #[msg("Open pool limit reached")]
    OpenPoolLimit,
    /// 6022: the creator would exceed their own-box cap.
    #[msg("Own box limit reached")]
    OwnBoxLimit,
    /// 6023: a WalletOverride exists for this wallet and must be passed.
    #[msg("Wallet override account is required")]
    OverrideRequired,
    /// 6024: count is zero.
    #[msg("Nothing to buy")]
    NothingToBuy,
    /// 6025: count exceeds the boxes remaining.
    #[msg("Too many boxes")]
    TooManyBoxes,
    /// 6026: the pool is not Open.
    #[msg("Pool is not open")]
    PoolNotOpen,
    /// 6027: the pool's status is not `Locked` (PROGRAM §4.4: `set_var`, `sample_var`, `draw`
    /// and `replace_var` all need a locked pool; `draw` moves it to `Drawn`).
    #[msg("Pool is not locked")]
    PoolNotLocked,
    /// 6028: the pool is not Drawn.
    #[msg("Pool is not drawn")]
    PoolNotDrawn,
    /// 6029: a sponsorship is below one box price.
    #[msg("Sponsorship is below one box price")]
    SponsorshipTooSmall,
    /// 6030: the pool's sponsorship cap would be exceeded.
    #[msg("Sponsorship cap exceeded")]
    SponsorshipCapExceeded,
    /// 6031: `set_var` on a pool that already has a `Var`; `replace_var` with the pool's own
    /// `Var` as the replacement (PROGRAM §4.4 "One call per pool").
    #[msg("Var is already set on this pool")]
    VarAlreadySet,
    /// 6032: `sample_var` or `draw` on a pool with no `Var` bound (PROGRAM §4.4).
    #[msg("No Var is set on this pool")]
    VarNotSet,
    /// 6033: the `Var` passed is not `pool.var` (PROGRAM §4.4 `var == pool.var`).
    #[msg("Var does not match the one recorded on the pool")]
    VarMismatch,
    /// 6034: the account is not a `Var` the Entropy program owns and formats as one: wrong
    /// owner, not 240 bytes (the two legacy 232-byte accounts included), or a non-zero
    /// discriminator (PROGRAM §4.4).
    #[msg("Account is not an Entropy Var")]
    VarNotEntropy,
    /// 6035: `var.provider != config.entropy_provider` (PROGRAM §4.4).
    #[msg("Var provider does not match the configured provider")]
    VarProviderMismatch,
    /// 6036: a `Var` offered to `set_var` / `replace_var` is not committed, unsampled and
    /// unrevealed with `samples == 1`, `is_auto == 0` and `end_at` ahead (PROGRAM §4.4).
    #[msg("Var is not fresh: it must be committed, unsampled, unrevealed, single-sample, manual, and end in the future")]
    VarNotFresh,
    /// 6037: `draw` with a zero seed or value, or a value that does not recompute as
    /// `keccak(slot_hash ‖ seed ‖ samples)` (PROGRAM §4.4).
    #[msg("Var is not revealed, or its value does not recompute")]
    VarNotRevealed,
    /// 6038: `draw` before `sample_var` ran, or on a `Var` whose `slot_hash` is no longer the one
    /// `sample_var` verified (rolled with `Next`, re-opened, or rewritten) (PROGRAM §4.4).
    #[msg("Var was not sampled through this program, or no longer carries the verified hash")]
    VarNotSampledHere,
    /// 6039: `sample_var` found no SlotHashes entry for `end_at`, or one that differs from the
    /// `Var`'s hash — the window closed and the fallback was written (PROGRAM §4.4).
    #[msg("Sample window missed: SlotHashes does not confirm the Var's hash for end_at")]
    SampleWindowMissed,
    /// 6040: `draw` with `slot_hash == keccak(end_at)`, Entropy's predictable fallback
    /// (PROGRAM §4.4, defence in depth behind `SampleWindowMissed`).
    #[msg("Var carries the keccak(end_at) fallback hash")]
    VarFallbackHash,
    /// 6041: a third `replace_var` (PROGRAM §4.4 "capped at two").
    #[msg("Var has already been replaced twice")]
    TooManyVarReplacements,
    /// 6042: `draw` or `replace_var` on a pool whose digits are drawn (PROGRAM §4.4).
    #[msg("Digits are already drawn")]
    AlreadyDrawn,
    /// 6043: the quarter's scores are not posted.
    #[msg("Scores not posted")]
    ScoresNotPosted,
    /// 6044: the passed winner is not the owner of the winning box.
    #[msg("Winner does not match")]
    WinnerMismatch,
    /// 6045: a fee destination does not match the pool or config.
    #[msg("Fee account does not match")]
    FeeAccountMismatch,
    /// 6046 (PROGRAM §4.6): `return_boxes` with no precondition holding (a `Settled` or `Split`
    /// pool included; `Locked`/`Drawn` on a `Scheduled` game however late; `Open` before the
    /// recorded kickoff; a marked or suspended game once fees are paid); `cancel_pool` outside
    /// `Open`/`Locked`/`Drawn`; `reclaim` or `reclaim_sponsorship` on `Settled`;
    /// `return_sponsorship` on a pool that is not `Returned`.
    #[msg("Pool is not returnable")]
    NotReturnable,
    /// 6047 (PROGRAM §4.6, §8): `cancel_pool`, `return_sponsorship` and `reclaim_sponsorship`
    /// on a pool whose fees have been paid — the pool can only be split and the sponsorship
    /// is committed. Reserved through Step 6; raised from Step 7.
    #[msg("Fees already paid")]
    FeesAlreadyPaid,
    /// 6048 (PROGRAM §4.6): `split` on a game not marked `Suspended`.
    #[msg("Game is not suspended")]
    NotSuspended,
    /// 6049 (PROGRAM §4.6): `split` before any payout (`return_boxes` returns the pool in full)
    /// or outside `Drawn`/`Split`.
    #[msg("Pool is not splittable")]
    NotSplittable,
    /// 6050 (PROGRAM §4.6): `reclaim` or `reclaim_sponsorship` before
    /// `scheduled_kickoff + RECLAIM_DELAY` — the scheduled kickoff, never the recorded one.
    #[msg("Reclaim is not yet available")]
    ReclaimTooEarly,
    /// 6051 (PROGRAM §4.6): `reclaim` by a wallet that owns no box in the pool.
    #[msg("Signer is not the owner")]
    NotOwner,
    /// 6052 (PROGRAM §4.6): a `return_boxes` or `split` that neither changed the status nor paid
    /// a box (the retry of a finished batch), or a `reclaim` by an owner whose boxes are all
    /// returned.
    #[msg("Nothing to return")]
    NothingToReturn,
    /// 6053: Sponsorship accounts are still open.
    #[msg("Sponsorships still open")]
    SponsorshipsStillOpen,
    /// 6054: sold boxes have not all been returned.
    #[msg("Boxes still outstanding")]
    BoxesStillOutstanding,
    /// 6055: the pool is not Settled, Returned or Split.
    #[msg("Pool is not terminal")]
    PoolNotTerminal,
    /// 6056: the creator counter is not zero.
    #[msg("Counter is not empty")]
    CounterNotEmpty,
    /// 6057: the mint has a transfer fee or transfer hook.
    #[msg("Mint has an unsupported extension")]
    UnsupportedMintExtension,
    /// 6058: checked arithmetic overflowed.
    #[msg("Arithmetic overflow")]
    MathOverflow,
    /// 6059: a PROGRAM §3.1 or §3.6 invariant or token-shape rule is violated.
    #[msg("Invalid configuration")]
    InvalidConfig,
    /// 6060: `create_game` key rules (PROGRAM §2, §4.2): `week` outside 1–22 and not a
    /// preseason week 101–103 with `preseason_enabled`; a team index of 32 or more; `home == away`.
    #[msg("Invalid game key: week, team index or home == away")]
    InvalidGameKey,
    /// 6061: `mark_game` with a status that is not a mark (`Scheduled`, `Final`), or
    /// `Postponed` / `Cancelled` once a quarter has been posted (PROGRAM §4.2, §9: only
    /// `Suspended` applies to a game with scores).
    #[msg("Invalid game status for mark_game")]
    InvalidGameStatus,
    /// 6062: `sample_var` before `Clock.slot ≥ var.end_at` (PROGRAM §4.4).
    #[msg("Sample window not open yet")]
    SampleTooEarly,
    /// 6063: `sample_var` on a pool that already has a verified sample; `replace_var` on such a
    /// pool (a `Var` with a verified sample is never abandoned: its value is fixed, only the
    /// reveal is outstanding).
    #[msg("Var already has a sample recorded")]
    VarAlreadySampled,
    /// 6064 (Step 5b): `draw` on a `Var` whose revealed `seed` does not hash to the commit
    /// `set_var` / `replace_var` recorded on the pool (PROGRAM §4.4, §8).
    #[msg("Var seed does not hash to the commit recorded on the pool")]
    VarCommitMismatch,
}
