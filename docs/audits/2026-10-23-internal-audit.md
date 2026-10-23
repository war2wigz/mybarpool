# Internal security audit of the MyBarPool program — 2026-10-23

An internal audit, not a third-party attestation. This report records what was reviewed, by what method, what was found, what was changed and what was accepted, so that a reader can check each statement against the code and the tests rather than take it on trust. What it is not is a statement by an outside firm; [SECURITY.md](../../SECURITY.md) says what a reader can verify instead.

## 1. Scope

- Program: `programs/mybarpool` (Anchor 1.2.1, 27 instructions, 6 account types, 25 events, 66 errors at the time of review), at public commit `a183969` (2026-10-23; the program source is byte-identical to `1890ed3`, the box-sponsorship commit, where the reviewers read it).
- Specification the code was reviewed against: [`docs/PROGRAM.md`](../PROGRAM.md) (the mechanical spec) and [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md) (the trust model and the randomness argument). Where the two disagreed, the stricter reading was treated as the spec and the disagreement is a finding.
- Entry point given to the reviewers: [`programs/mybarpool/AUDIT-READINESS.md`](../../programs/mybarpool/AUDIT-READINESS.md).
- The Entropy fork (`war2wigz/entropy@5a869c8`) was in scope as the program the draw depends on; its fixture bytecode was what the proofs of concept ran against. Its own source was read only where the program's checks depend on its behaviour (`Open`, `Close`, `Sample`, `Reveal`, `Next`).
- Out of scope: the keeper, the scores service, the apps, the SDK beyond its role as the §6 reference implementation, and infrastructure. Those were reviewed separately on 2026-10-22 (the keeper, scores and app audits in the private repository).

## 2. Method

Four reviewers, each a fresh session with no prior context, were given the repository read-only, the specification sections for their slice, the readiness document, and the audit report format from the build plan (Blocker / High / Medium / Low; each finding with `file:line`, what the spec says, what the code does, a concrete scenario, the smallest fix and a proposed test; a one-line verdict). Each was told to read the actual code paths before asserting anything, to prefer a short list of real findings over a long list of speculative ones, and to run tests. Each could build a scratch copy of the program and run Mollusk against the real SBF bytecode; three of the four did, and every finding below marked *confirmed* has a proof of concept that ran.

| Slice | Questions | Verdict returned |
|---|---|---|
| Money and arithmetic | fee and prize math, every transfer's amount and destination, each terminal path's conservation, dust, rent, the `returned` bitmap, reclaim shares | merge; 2 Medium, 2 Low |
| Draw, randomness and time | `set_var` → `sample_var` → `draw` and `replace_var`; what the keeper can do after seeing the slot hash; Entropy CPI defence in depth; box assignment; every Clock gate; `update_kickoff`; scores and settlement order; reclaim; keeper vs admin powers | fix and re-audit; 1 Blocker, 2 High, 2 Medium, 1 Low |
| Accounts and access | every account constraint, `has_one` and `address` coverage, unchecked accounts, PDA seeds, signer checks, optional accounts, closes, two questions: can a wallet other than the intended one receive funds; can the keeper do anything the trust table reserves for the admin | fix and re-audit; 1 High, 1 Medium, 4 Low |
| Gating, configuration, state machines, denial of service | `initialize` and `update_config` invariants, pause semantics, Link and Allowlist gates, creator limits and overrides, every state transition, events and errors against the IDL, layout, compute, griefs | merge; 2 Medium, 2 Low |

The findings were triaged by the author, each one re-read against the code and either reproduced or reasoned through again, and every one was given a decision: fix, or document with the reason. The fixes were then re-run past the full suite, and the draw reviewer's proofs of concept were re-expressed as permanent tests.

Tooling the reviewers and the author relied on, at the fixed commit:

- Mollusk unit tests on the SBF ELF: 346 before the audit, 372 after, all passing (`cargo test -p mybarpool --locked` after `anchor build --arch v3`).
- Localnet lifecycle against a Surfpool mainnet fork: 17 files, 130 tests, all passing after the audit (`anchor test`).
- Fuzzer: fixtures ejected from every unit test, replayed and mutated under proptest, checked against the PROGRAM §10 money oracle. On the pre-fix bytecode: 6,193,692 cases in 1,200 s over 772 seeds, clean (seed 16502337001721866570). On the fixed bytecode: §7.
- `anchor-lints` (dylint), `cargo audit`, the schema snapshot test, the layout test, the errors test.

## 3. Findings and decisions

Severities are the reviewers' as returned, with the author's confirmation noted. A line reference is to the pre-fix source.

### Blocker

**B1 — the keeper could re-roll the slot hash by closing and re-opening the bound `Var`.** Draw slice; confirmed by proof of concept against the fork's bytecode.

- Where: `sample_var.rs:60-84`, `draw.rs:54-59`, `set_var.rs:71`, `state/pool.rs:120`. `pool.var_end_at` was written by `set_var` and `replace_var` and read nowhere.
- Spec: PROGRAM §4.4 "no re-rolls … one draw per pool, enforced"; ARCHITECTURE › Randomness "players rely on the platform for liveness, not for fairness".
- Code: the pool pinned the `Var` by address and commit, not by end slot. The keeper is the `Var`'s authority; Entropy lets an authority `Close` a `Var` and `Open` the same address again with the same commit and a later `end_at`. `sample_var` proved the sysvar entry for the `Var`'s own `end_at`, so the new slot's hash was recorded; `draw` recomputed `keccak(slot_hash ‖ seed ‖ 1)` against the pool's commit, which still held since the seed was the keeper's. No event, `var_replacements` stayed 0, no admin signature. Unbounded tries.
- Proof of concept: `Open` → `set_var` → `Close` → `Open(same id, same commit, end_at + 200)` → `sample_var` succeeded → `Reveal` → `draw` succeeded with different axes than the first slot's hash would have given.
- Fix: `sample_var` requires `var.end_at == pool.var_end_at` before the CPI and again after the reload, and proves the sysvar entry for `pool.var_end_at`; `draw` requires the same. New error `VarEndAtMismatch` (6066). The fork's `Open` refuses `end_at <= Clock.slot`, so once the bound slot has passed no re-opened `Var` can carry it again.
- Tests: `draw::keeper_cannot_rebind_by_closing_and_reopening_the_var` (the proof of concept, now failing at `sample_var`), `draw::draw_refuses_a_var_whose_end_slot_is_not_the_recorded_one` (a planted, otherwise self-consistent `Var`).

### High

**H1 — `replace_var` had no missed-window condition.** Draw slice; confirmed.

- Where: `replace_var.rs:50-64`.
- Spec: trust table "replace a pool's Entropy `Var` (only one that missed its sampling window)"; §4.4 "a new fair draw, not a choice between two outcomes".
- Code: nothing compared `Clock.slot` or `SlotHashes` with `pool.var_end_at`; a replacement at `end_at + 1`, with the value already knowable to the keeper, raced any honest `sample_var`. Proof of concept: `replace_var` succeeded at slot 900 before an `end_at` of 1,000, and at 1,003 with the entry for 1,000 present.
- Fix: `ReplaceVar` takes the `SlotHashes` sysvar (`address`-constrained); the handler requires `Clock.slot > pool.var_end_at` and no sysvar entry for `pool.var_end_at` — aged out of the 512-slot window or a skipped slot. New error `SampleWindowOpen` (6067). Because the admin now has to wait for the window to pass, `set_var` bounds the end slot: `end_at ≤ slot + VAR_END_AT_MAX_LEAD` (1,500 slots, about five minutes at 200 ms; the keeper's own lead is 150), `VarNotFresh` otherwise, so a keeper cannot park a pool behind a far-future `Var` the admin cannot replace. `replace_var` is not bound by the lead: the keeper opens the replacement 36,000 slots out because a multisig needs hours between the operator's release and the execute, the admin chooses it, and the admin can always `cancel_pool` instead. The readiness document had listed the missing upper bound as a known item.
- Tests: `draw::replace_var_only_after_the_window_is_gone` (before `end_at`, with the entry present, after it has aged out, on a skipped slot), `draw::set_var_refuses_an_end_slot_more_than_the_lead_ahead`; `draw::replace_var_is_capped_at_two` now advances the clock past each window.
- Residual, disclosed rather than argued away (ARCHITECTURE › Randomness): across the original `Var` and up to two replacements, the platform's keys could, by letting a public window lapse unsampled, choose which of at most three independent fair draws a pool gets. Never a specific outcome; each lapse is a 100-second window in which anyone can sample; each replacement is an event. The alternative considered was removing `replace_var` altogether and making a missed window end in `cancel_pool` — a full return of every box at the network's skip rate, of the order of one pool in twenty to fifty. The best-of-three was kept and written down.

**H2 — `draw` was allowed after scores were posted.** Draw slice; confirmed by code reading.

- Where: `draw.rs:49-53`; `settle.rs:129-137` ordered settlement after the draw but nothing ordered the draw before the first post.
- Spec: ARCHITECTURE › Randomness "the real deadline is the first score post" was stated as an operation, not enforced.
- Code: a pool left `Locked` through the game could be drawn after `Final`; with B1 or H1 that was a choice of winner against known scores (with three tries a given box's chance of winning Q1 rose from 4% to about 11.5%; with B1, to certainty).
- Fix: `Draw` takes the game record (`has_one = game` on the pool) and requires `game.quarters_posted == 0`. New error `DrawAfterScores` (6068). A pool not drawn by the first post goes to `cancel_pool` and a return, which was the documented operation.
- Tests: `draw::draw_after_the_first_score_post_is_refused` (and a substituted game record is Anchor's `ConstraintHasOne`).

**H3 — a Token-2022 destination that requires memos blocked `settle`, and with it every later quarter.** Accounts slice; confirmed by proof of concept. The money slice raised the same mechanism as a Medium together with the frozen-account case.

- Where: `payout.rs:29-58`, `vault.rs:244-253`.
- Spec: §5.4 refuses six mint-level extensions; `MemoTransfer` is account-level, set by the account's owner, so no mint check could rule it out.
- Code: `pay_out` did `create_ata_idempotent` then a bare `transfer_checked`. Token-2022 refuses the transfer unless the previous sibling instruction at the same depth is a Memo (`NoMemo`, 0x24). A creator or box owner who enabled required memos on their own account made `settle` for that quarter fail deterministically; the quarter cannot be skipped, so every later quarter failed too, and after `Final` the only exit was the 30-day reclaim at a split share.
- Fix: before every SPL payout the program reads the destination token account and, when `MemoTransfer.require_incoming_transfer_memos` is set, CPIs the Memo program with the fixed text `MyBarPool` and no signers, then transfers. The seven paying instructions (`settle`, `close_pool`, `return_boxes`, `return_sponsorship`, `split`, `reclaim`, `reclaim_sponsorship`) take `memo_program: Option<UncheckedAccount>` (`address = MEMO_PROGRAM, executable`), required on SPL pools, `None` on SOL. A legacy-Token account has no extensions and never takes the memo path. New constant `MEMO_PROGRAM`.
- Tests: `settle::settle_pays_a_token_2022_destination_that_requires_memos`, `settle::split_pays_a_token_2022_owner_that_requires_memos`, `settle::an_ore_destination_never_takes_the_memo_path`. The compute cost of the memo path is one CPI; the bench rows moved by at most a few hundred CU.

### Medium

**M1 — `update_kickoff` had no lower bound.** Draw and accounts slices.

- Where: `update_kickoff.rs:54-60`.
- Spec: PROGRAM §4.2 allowed earlier moves without bound; ARCHITECTURE's trust model said updates "must stay within 72 hours of the scheduled kickoff". The doc drift was the finding; the risk was blast radius for a stolen keeper key (pull kickoff to now, close sales on every pool of a game days out, post four scores an hour later).
- Fix: `new_time ≥ scheduled_kickoff − KICKOFF_UPDATE_BOUND` (`KickoffOutOfBounds`), and §4.2 now says the bound is symmetric. It does not help a game less than 72 hours out, and the trust model says so.
- Test: `games::update_kickoff_bound_is_symmetric`.

**M2 — `split` refused a pool `reclaim` had moved into `Split` unless the game was `Suspended`.** Money slice.

- Where: `split.rs:81-90` against `reclaim.rs:104-108`.
- Spec: §9 "`split` and `reclaim` keep paying inside `Split` until every sold box has its bit set"; §10 Rent "nothing is left un-closable in any terminal state".
- Code: after an owner's reclaim on a `Final` game the keeper's `split` was `NotSuspended`, `settle` was `PoolNotDrawn`, and `mark_game(Suspended)` is refused on a `Final` record; the remaining owners each had to reclaim themselves and the pool's rent waited on the least active one.
- Fix: `game.status == Suspended || pool.status == Split`. Once `Split`, the per-box amount is fixed and the batch chooses only who is paid, which is §4.6's safety argument.
- Tests: `split::split_continues_a_pool_an_owner_reclaimed_into_split_on_a_final_game`, `split::split_with_another_game_record_is_constraint_has_one`.

**M3 — `security.txt` was not embedded in the binary.** Gating slice.

- Where: `lib.rs`; `solana-security-txt` was pinned in the workspace and never depended on by the program crate. ARCHITECTURE had adopted it as practice on 2026-10-01.
- Fix: `solana_security_txt::security_txt!` under `#[cfg(not(feature = "no-entrypoint"))]` with the contact, policy, source and this report as `auditors`.

**M4 — a frozen destination, and `check_mint` accepting a mint with a freeze authority.** Money and gating slices. Documented, not changed.

- A token account's owner cannot freeze it; only the mint's freeze authority can. A frozen winner account fails the transfer and the pool follows the path H3 described: the quarter cannot settle, and after `RECLAIM_DELAY` every other owner reclaims at a split share. Refusing every mint with a freeze authority would refuse most of the tokens worth listing, so this is a configuration-time judgement (the admin lists a token whose freeze authority they trust not to act against a pool's participants), written into §5.4 and the admin runbook. The money reviewer's further suggestion — a time-delayed `split` on a `Final` game, or a pull-payment fallback the winner triggers — is a design change and is logged for after the first season.

**M5 — `draw` was missing from the keeper's row of the trust tables.** Accounts slice. ARCHITECTURE and the readiness document now list it, with the note that `draw` applies a value and chooses nothing.

### Low

**L1 — `close_pool` reported SOL dust with a `checked_sub` of the rent minimum.** Money slice. Under a rent schedule raised after the vault was funded the subtraction underflowed and the pool could not close. Fixed with `saturating_sub` (the event value only; the closing transfer moves the whole balance either way). Test: `settle::close_pool_sol_under_a_raised_rent_minimum_still_closes_and_reports_no_dust`.

**L2 — a `Link` pool's gate key could be set to the program id.** Gating slice. Anchor passes an absent optional account as the program id, so `buy` read such a gate as missing and nobody could ever buy. Self-inflicted and recoverable by `rotate_gate_key`; fixed anyway with `require_keys_neq!(gate_key, crate::ID, GateKeyMissing)` at `create_pool` and `rotate_gate_key`. Tests in `pools.rs` and `buy.rs`.

**L3 — `has_one = game` untested on the return family.** Accounts slice. The constraint was present and held (a substituted older record was `ConstraintHasOne`, not a bypassed reclaim clock); the gap was in the tests. Added: `reclaim_with_another_game_record_is_constraint_has_one`, `reclaim_sponsorship_with_another_game_record_is_constraint_has_one`, `return_boxes_with_another_game_record_is_constraint_has_one`, `split_with_another_game_record_is_constraint_has_one`.

**L4 — one `Var` could be bound to several pools.** Draw and accounts slices. Nothing marks a `Var` as taken, so the keeper could give two pools identical axes. No bias and no misdirected money; visible on the verify page. Accepted as a keeper-trust item and written into ARCHITECTURE › Randomness. The draw's commit binding limits it to pools whose commit the keeper chose to share.

**L5 — no dedicated event for a status change to `Returned` or `Split`, or for `abandoned`.** Gating slice. Spec-consistent today (an unfilled pool with no box sold becomes `Returned` with no event, and the tests assert it). A `PoolStatusChanged` event is additive under §10 Layout and is logged for a later release; indexers read the account.

**L6 — a closed pool's address can be re-created with the same nonce.** Accounts slice, informational. `init` on `["pool", game, creator, nonce]` after `close_pool` is a new pool at an old address; a client that keeps history by pool address must also keep the creation signature or slot with it. No change to the program.

**L7 — keeper and block-producer collusion on the end slot.** Draw slice. The commit-reveal removes the keeper-alone and validator-alone cases, not the pair: a keeper who chose an end slot in a colluding leader's turn and shared the seed would let the leader grind block contents toward a value. Inherent to every slot-hash scheme; the platform runs no validator; written into ARCHITECTURE › Randomness as an accepted residual.

## 4. Checked and sound

Condensed from the four reviewers' tables; each row names what was verified so it can be re-checked.

| Area | Verified |
|---|---|
| Fee and prize math (§5.1, §5.2) | `u128` products, floor per fee, `try_from` back to `u64`; fees stored on the pool at creation and moved from the pool, never recomputed from config; `prize_pool = 25p − fees + sponsored`; dust ≤ 3 base units stays in `unpaid_prize_pool`; sum of prizes, fees and dust equals the pot plus sponsorships |
| Arithmetic hygiene | no `unwrap` or `expect` in `src/`; the only `as` narrowing is a box count ≤ 25; `saturating_sub` only where documented; every other operation checked |
| Terminal paths (§5.3) | `Returned ⇒ !fees_paid` and `Split ⇒ fees_paid` hold on every entry path; `split_amount` fixed once; the `returned` bitmap pays a box at most once across `return_boxes`, `split` and `reclaim`; `abandoned` and `cancelled_by_admin` mutually exclusive |
| Destinations | every outbound transfer is to a destination fixed by on-chain state in the same instruction: `owners[box]`, `config.fee_wallet`, `pool.creator`, `pool.integrator`, `sponsorship.wallet`; the keeper and the reclaiming party choose which, never where; the default pubkey in a batch collects nothing |
| `sample_var` proof | reload after the CPI; `hash_for_slot(end_at) == Some(var.slot_hash)`; the fallback `keccak(end_at)` can never match a real sysvar entry; fails in slot `end_at` itself and after the window; CPI target constant |
| `draw` defence in depth | `var == pool.var`, `sampled_slot != 0`, `slot_hash == sampled_hash`, not the fallback hash, revealed, `keccak(seed) == pool.var_commit` (the pool's copy), `samples == 1`, value recomputed; `Next` and a post-sample re-open both refused |
| Box assignment (§6.1) | seed is the newest `SlotHashes` entry with the buyer, `sold` and `count`, read from the sysvar account, never instruction data; a buyer can choose a box by waiting for a slot, which the spec says is harmless because the axes come later from an independent value — true now that B1–H2 are fixed |
| Clock gates | kickoff, the 15-minute floors, the 30-day reclaim and `create_game` compare `unix_timestamp` in seconds; `end_at` compares slots; nothing is finer than a boundary a leader could shift under Alpenglow's timestamp rule |
| Scores and settlement | keeper-only; exactly the next quarter; floors from the recorded kickoff or the previous post; cumulative and non-decreasing; `is_final` iff Q4; `Final` makes the record inert; winner recomputed in-program (`WinnerMismatch`); no double settle |
| `mark_game` | only Postponed, Cancelled, Suspended; `Final` refused; already marked refused; Postponed and Cancelled need `quarters_posted == 0`; irreversible |
| Reclaim | 30 days from `scheduled_kickoff` on both reclaims, which `update_kickoff` cannot move; `abandoned` only from an unresolved pool; a second reclaim is `NothingToReturn`; a late keeper call after a reclaim is refused by status or continues at the fixed amount |
| Authority matrix | every keeper instruction `has_one = score_authority`, every admin instruction `has_one = admin`; the admin can rotate `score_authority`, never the reverse; `initialize` by the upgrade authority through `ProgramData` |
| `initialize` and `update_config` | one-time `init`; bps ceilings 500/500/500 and 1,500 total; preset, limits, ladder and token-shape invariants; mint key, owner, decimals and six extensions in `check_mint` |
| Pause | read only by `create_pool`, `buy`, `sponsor`; `reclaim` takes no config; tested that settle, return and reclaim run while paused |
| Gates | Link: present, equal to `pool.gate_key`, signing, all `GateKeyNotSigner`, evaluated before the transfer; Allowlist: `sha256(0x00‖wallet)` leaves, `sha256(0x01‖min‖max)` nodes, ≤ 32 entries, leaf from the signer, SDK byte-identical by shared vectors; the root has no setter |
| Creator limits | one counter per creator and game across nonces; `open_count == #Open pools` on every path; override slot pinned by seeds and read only when owned and non-empty |
| Events, errors, layout | 25 events in IDL order; 66 (now 69) errors in declaration order, frozen by test and snapshot; account sizes and offsets asserted; `reserved` zeroed; no `realloc`, no `unsafe`, no direct lamport writes |
| Compute | user-facing maximum 51,369 CU (`create_pool_spl`); only keeper batches need a budget instruction; worst row 281,448 CU, a fifth of the 1.4 M maximum |
| Griefs | pre-funded vault handled; default-pubkey batch entry handled; the 25th-box front-run is a purchase; `create_pool` and `sponsor` spam cost rent and are capped; no fund-trapping grief found other than H3 |

## 5. State transitions as implemented

Recorded by the gating reviewer and checked against PROGRAM §9; every illegal transition is refused with the named error.

**Pool**

| From | Instruction | Guard | To |
|---|---|---|---|
| Open | `buy` / `create_pool` (25th box) | `!paused`, game `Scheduled`, `now < recorded_kickoff`, gate | Locked; counter −1 |
| Open | `return_boxes` | `now ≥ recorded_kickoff`, or game marked | Returned; counter −1 |
| Open | `cancel_pool` | admin | Returned, `cancelled_by_admin`; counter −1 |
| Open | `reclaim` / `reclaim_sponsorship` | 30 days from `scheduled_kickoff` | Returned, `abandoned`; counter −1 |
| Locked | `set_var`, `sample_var`, `replace_var` | §4.4 | Locked |
| Locked | `draw` | sampled, revealed, commit and end slot match, no quarter posted | Drawn |
| Locked | `return_boxes` / `cancel_pool` / reclaims | game marked / admin / 30 days | Returned |
| Drawn | `sponsor` | `now < recorded_kickoff`, game `Scheduled` | Drawn |
| Drawn | `settle(q < 4)` | in order, score posted | Drawn (may set `fees_paid`) |
| Drawn | `settle(4)` | | Settled |
| Drawn | `return_boxes` / `cancel_pool` | game marked and `!fees_paid` / admin and `!fees_paid` | Returned |
| Drawn | `split` | game `Suspended` and `fees_paid` | Split |
| Drawn | `reclaim` | 30 days; `!fees_paid` → Returned, else Split | Returned / Split, `abandoned` |
| Returned | `return_boxes`, `return_sponsorship`, reclaims | pays at `price` | Returned |
| Split | `split`, `reclaim`, `close_sponsorship` | pays at `split_amount`; `split` no longer needs `Suspended` here | Split |
| Settled / Returned / Split | `close_pool` | `sponsorships_open == 0`; every sold box returned on Returned / Split | closed; rent and dust to `fee_wallet`, or to the creator if `abandoned` |

**Game**

| From | Instruction | Guard | To |
|---|---|---|---|
| Scheduled | `update_kickoff` | `quarters_posted == 0`, `now < recorded_kickoff`, `new > now`, `scheduled − 72 h ≤ new ≤ scheduled + 72 h` | Scheduled |
| Scheduled | `post_scores(1..3)` | in order, 15-minute floor, non-decreasing, not final | Scheduled |
| Scheduled | `post_scores(4)` | `is_final` | Final |
| Scheduled | `mark_game(Postponed, Cancelled)` | admin, `quarters_posted == 0` | Postponed / Cancelled |
| Scheduled | `mark_game(Suspended)` | admin | Suspended |
| Final | any writer | | `GameNotScheduled` |
| Postponed / Cancelled / Suspended | `mark_game` | | `GameAlreadyMarked` |

## 6. What changed in the program

All before any deployment, so no account on any cluster is affected. The schema snapshot was re-frozen for the account-list changes (`draw` gains `game`; `replace_var` gains `slot_hashes`; the seven paying instructions gain `memo_program`); that is the last re-freeze before mainnet, after which the snapshot is only ever extended.

| File | Change |
|---|---|
| `constants.rs` | `VAR_END_AT_MAX_LEAD = 1_500`, `MEMO_PROGRAM`, `PAYOUT_MEMO` |
| `errors.rs` | `VarEndAtMismatch` 6066, `SampleWindowOpen` 6067, `DrawAfterScores` 6068 |
| `set_var.rs` | `end_at ≤ slot + VAR_END_AT_MAX_LEAD` (`require_lead`, `set_var` only) |
| `sample_var.rs` | `end_at == pool.var_end_at` before and after the CPI; proves the sysvar entry for the pool's slot |
| `draw.rs` | `game` account with `has_one`; `quarters_posted == 0`; `end_at == pool.var_end_at` |
| `replace_var.rs` | `slot_hashes` account; `Clock.slot > var_end_at` and no entry for it |
| `update_kickoff.rs` | lower bound at `scheduled − 72 h` |
| `split.rs` | `Suspended` or already `Split` |
| `close_pool.rs` | `saturating_sub` for the reported dust; memo program through to the transfer |
| `create_pool.rs`, `rotate_gate_key.rs` | gate key may not be the program id |
| `vault.rs`, `payout.rs` | `memo_if_required` before `transfer_out_spl`; the memo program threaded through `pay_out` |
| seven account structs | `memo_program: Option<UncheckedAccount>` after the associated-token program |
| `lib.rs`, `Cargo.toml` | `security_txt!`; `solana-security-txt` and the Mollusk memo program as a dev-dependency |
| `slot_hashes.rs` | `newest_slot` removed (unused once `replace_var` used the Clock) |

The IDL, the SDK's generated client, the test helpers, the compute-unit table and the fuzz runner (which now loads the Memo program) were regenerated or updated to match. Twenty-six tests were added; the counts in `errors.rs`, `schema.rs`, `buy.rs` and `close.rs` moved from 66 to 69.

## 7. Verification after the fixes

- Mollusk: 372 tests across 19 files, all passing on the rebuilt ELF.
- Localnet: 17 files, 130 tests, all passing against the Surfpool mainnet fork, including the SDK lifecycle that draws through the generated client with the new `game` account.
- Fuzzer on the fixed bytecode, fixtures re-ejected from the new suite (807 seeds, the memo-path fixtures replaying with the Memo program loaded): 4,271,419 cases in 900 s, clean (seed 9023521409103873441), and after the last change (the lead bound narrowed to `set_var`) 2,820,630 cases in 600 s, clean (seed 2114127522115581660).
- `anchor-lints`, `cargo audit` (the four accepted advisories, dev-side), prettier, typecheck, typedoc, the SDK's 224 vitest cases: clean.

## 8. Accepted and open

| Item | Status |
|---|---|
| Best-of-three draw by lapsing public windows (H1 residual) | accepted; disclosed in ARCHITECTURE › Randomness and PROGRAM §4.4 |
| Keeper and leader collusion on the end slot (L7) | accepted; inherent to slot-hash schemes; disclosed |
| One `Var` on two pools (L4) | accepted; keeper-trust; disclosed |
| Frozen destination; freeze authority accepted by `check_mint` (M4) | accepted; configuration-time judgement, written into §5.4 and the admin runbook |
| `PoolStatusChanged` event (L5) | open; additive, after the first deployment |
| Time-delayed `split` or pull-payment fallback on a `Final` game with a stuck destination (M4) | open; a design change, logged for after the first season |
| Two-step admin transfer; default `fee_wallet` / `score_authority` refused at `initialize` | open from the readiness document's known list; unchanged |

## 9. Why internal

An external firm was considered and not engaged. The program is 27 instructions and under 7,000 lines of Rust with a mechanical specification to review against; the harness already proves the mechanics on the real bytecode; and the thing an auditor adds — an adversarial read by someone who did not write it — was done here by four reviewers with no prior context, with proofs of concept for what they found. This report says what they covered and what they did not. A paid external review remains an option once the vaults hold enough to justify one, alongside the bounty. The decision is in ARCHITECTURE's decisions log under 2026-10-23.
