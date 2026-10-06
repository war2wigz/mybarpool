//! Errors, PROGRAM §8. Numbered from 6000 in declaration order; names are the
//! contract and the numbers freeze when the program ships. The 59 §8 names
//! come first, then the ones appended before anything shipped: `InvalidConfig`
//! (6059, Step 2) for a violated §3.1 invariant, `InvalidGameKey` (6060) and
//! `InvalidGameStatus` (6061, both Step 3) for the §4.2 key rules and the
//! `mark_game` status rules, which §8 had no error for.

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
    /// 6027: the pool is not Locked.
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
    /// 6031: the pool already has a Var.
    #[msg("Var is already set")]
    VarAlreadySet,
    /// 6032: the pool has no Var.
    #[msg("Var is not set")]
    VarNotSet,
    /// 6033: the passed Var is not the one recorded on the pool.
    #[msg("Var does not match the pool")]
    VarMismatch,
    /// 6034: the Var is not owned by the Entropy program.
    #[msg("Var is not an Entropy account")]
    VarNotEntropy,
    /// 6035: the Var's provider is not the configured one.
    #[msg("Var provider does not match config")]
    VarProviderMismatch,
    /// 6036: the Var is already sampled or revealed, or not committed.
    #[msg("Var is not fresh")]
    VarNotFresh,
    /// 6037: the Var's value is missing or does not recompute.
    #[msg("Var is not revealed")]
    VarNotRevealed,
    /// 6038: the Var was not sampled through this program.
    #[msg("Var was not sampled here")]
    VarNotSampledHere,
    /// 6039: the end slot is no longer in SlotHashes, or the hash differs.
    #[msg("Sample window missed")]
    SampleWindowMissed,
    /// 6040: the Var carries the keccak(end_at) fallback hash.
    #[msg("Var carries the fallback hash")]
    VarFallbackHash,
    /// 6041: the Var has already been replaced twice.
    #[msg("Too many Var replacements")]
    TooManyVarReplacements,
    /// 6042: the pool's digits are already drawn.
    #[msg("Already drawn")]
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
    /// 6046: no return precondition holds.
    #[msg("Pool is not returnable")]
    NotReturnable,
    /// 6047: fees have already been paid.
    #[msg("Fees already paid")]
    FeesAlreadyPaid,
    /// 6048: the game is not marked suspended.
    #[msg("Game is not suspended")]
    NotSuspended,
    /// 6049: the pool cannot be split.
    #[msg("Pool is not splittable")]
    NotSplittable,
    /// 6050: fewer than 30 days since the scheduled kickoff.
    #[msg("Reclaim is not yet available")]
    ReclaimTooEarly,
    /// 6051: the signer owns none of the boxes in question.
    #[msg("Signer is not the owner")]
    NotOwner,
    /// 6052: every box in the batch is already returned.
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
}
