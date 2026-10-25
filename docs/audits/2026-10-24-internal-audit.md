# Internal security audit of the MyBarPool program — 2026-10-24

An internal audit, not a third-party attestation, run the day after [the first one](2026-10-23-internal-audit.md) on the one program change since — the six token slots — and, with it, on the whole program again. The format is the same: what was reviewed, how, what was found, what changed, what was accepted, each statement checkable against the code and the tests. [SECURITY.md](../../SECURITY.md) says what a reader can verify instead of taking this on trust.

## 1. Scope

- Program: `programs/mybarpool` (Anchor 1.2.0, 27 instructions, 6 account types, 25 events, 69 errors at the time of review), on the `cursor/token-slots-8cfd` branch at `67c55eb` and, after the rebase, `9f23303` (identical program source), on top of public `4ec2af8`. The branch widens `PlatformConfig.tokens` from three rules to six; everything else is the audited program of 2026-10-23.
- Specification the code was reviewed against: [`docs/PROGRAM.md`](../PROGRAM.md) as amended on the branch for the six slots, and [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md). The stricter reading was the spec where the two differed.
- Entry point: [`programs/mybarpool/AUDIT-READINESS.md`](../../programs/mybarpool/AUDIT-READINESS.md), with the 2026-10-23 report so the reviewers could check that its fixes hold.
- The Entropy fork (`war2wigz/entropy@5a869c8`) was in scope as before; the SPL Token, Token-2022 and Associated Token programs were in scope for the first time as programs a payee can act on between purchase and payout.
- Out of scope: the keeper, the scores service, the apps, the SDK beyond the §6 reference implementation, infrastructure.

## 2. Method

Four reviewers, each a fresh session with no prior context, with the same brief and the same report format as the day before (Blocker / High / Medium / Low; `file:line`, what the spec says, what the code does, a scenario, the smallest fix, a proposed test; a one-line verdict), each with a scratch worktree, the SBF build and Mollusk against the real bytecode. Each was asked two things in addition to its slice: whether the six-slot change introduced anything, and whether the 2026-10-23 fixes actually hold under an adversarial read.

| Slice | Questions | Verdict returned |
|---|---|---|
| Config and admin | the six slots: `initialize` and `update_config` with placeholders, mint accounts by index, the shape and ladder invariants, `check_mint`, offsets and the snapshot, `ConfigUpdated`, transaction size, the Squads path; what `update_config` can do to existing pools | merge with one fix; 1 Low, 3 Info |
| Money and arithmetic | every transfer's amount and destination after the config change; conservation on each terminal path; the 2026-10-23 fixes re-read | merge; 0 findings, 2 Info |
| Draw, randomness and time | `set_var` → `sample_var` → `draw` and `replace_var` once more, with the question "what can the party holding the seed do between lock and settlement"; every Clock gate against the game's timeline | fix and re-audit; 1 High, 1 Low, 2 Info |
| Accounts and access | every account constraint; what a payee can do to their own destination account before a payout; keeper versus admin powers; griefs that hold a pool | fix and re-audit; 1 High, 1 Low, 2 Info |

Triage as before: every finding re-read against the code and reproduced or reasoned through again, then a decision — fix, or document with the reason. Three reviewers' scratch tests were re-expressed as permanent tests. The reviewers found nothing in the six-slot change itself beyond the Low below; the two Highs are in code that was in scope on 2026-10-23 and passed, which is the argument for a second pass by different eyes.

Tooling at the fixed commit: Mollusk 373 tests before, 383 after, all passing; the Surfpool lifecycle, 17 files, 130 tests; the fuzzer (§7); `anchor-lints`, `cargo audit` (five accepted advisories, all dev-side), the schema snapshot, layout and errors tests.

## 3. Findings and decisions

Severities are the reviewers' as returned. A line reference is to the pre-fix source at `9f23303`.

### High

**D1 — the keeper could choose which pools to draw after seeing the score.** Draw slice; confirmed by code reading and a Mollusk scenario (a draw four hours after kickoff with nothing posted, then four posts inside 45 minutes, all accepted).

- Where: `draw.rs:59`. The only timing check was `game.quarters_posted == 0` (`DrawAfterScores`, the 2026-10-23 fix).
- Spec: ARCHITECTURE › Randomness "players rely on the platform for liveness, not for fairness"; PROGRAM §4.4 "no re-rolls". The 2026-10-23 report's H2 said a draw after the first post would be a choice of winner, and the fix bound the draw to the *post*; nothing bound it to the *game*.
- Code: the keeper holds every seed and posts every score. Between kickoff and the first post — fifteen minutes at the least, and the keeper decides when to post — it knows the real Q1 score and, for any sampled pool, can compute the value and the axes the reveal would give. It could then reveal and draw only the pools where that value pays a box it cares about, and leave the rest `Locked`; those are returned in full once the first post lands (`DrawAfterScores`), at no cost to anyone but the platform's fee. No event distinguishes a pool the keeper chose not to draw from one it could not. The one-way nature of the commit does not help: the choice is not of the value but of whether the pool gets one.
- Fix: `draw` requires `game.status == Scheduled` (`GameNotScheduled`) and `now < min(game.scheduled_kickoff, game.recorded_kickoff)` (`DrawAfterKickoff` 6069). From kickoff the game has a shape, whether or not a score is posted, so the window closes there; the earlier of the two kickoffs so that a keeper-pushed `recorded_kickoff` cannot reopen it. `DrawAfterScores` stays as defence in depth. `return_boxes` gains an arm — `status == Locked`, `!drawn`, `now ≥ min(scheduled_kickoff, recorded_kickoff)` — so a pool still undrawn at that instant is returned by the keeper like an unfilled one, without the multisig. `GameRecord::draw_deadline()` is the one place the instant is computed.
- Cost: a pool that locks inside the last two minutes or so before kickoff cannot be drawn (open, sample window, reveal) and is returned. Accepted; the alternative — binding the draw to the post and trusting the keeper not to look — is what the finding is about.
- Tests (`tests/draw.rs`): `draw_at_or_after_kickoff_is_refused_and_the_second_before_is_accepted` (at kickoff, a second after, four hours after; the second before succeeds), `draw_deadline_is_the_earlier_of_the_scheduled_and_recorded_kickoff` (kickoff pushed an hour later: refused at the scheduled time, accepted a second before it; pulled an hour earlier: refused at the recorded time), `draw_on_a_marked_game_is_game_not_scheduled`. `tests/returns.rs`: `return_boxes_locked_undrawn_at_the_draw_deadline_returns_every_box`, `return_boxes_locked_undrawn_reads_the_earlier_kickoff`, and `return_boxes_drawn_on_a_scheduled_game_waits` (a `Drawn` pool is still not returnable however late; a `Locked` one is not returnable a second before the deadline).

**A1 — a legacy-Token payee could hold every payout of a pool by reassigning their token account.** Access slice; confirmed by proof of concept against the Token program's bytecode under Mollusk.

- Where: `payout.rs:51`, `close_pool.rs:180`, `vault.rs:290`. Every SPL payout called the Associated Token program's `create_idempotent` on the derived address first.
- Spec: PROGRAM §5.4 "created idempotently in the same instruction when missing"; the 2026-10-23 H3 fix (the memo) closed one way a destination's owner could block a payout and the readiness document said the create is "a no-op when the account exists with this wallet and mint".
- Code: on the legacy Token program (ORE) an associated token account carries no `ImmutableOwner`, so its owner can `SetAuthority(AccountOwner)` and hand it to any key. The account still exists at the derived address, for the right mint, but with another owner, and the ATA program's idempotent create then fails with `InvalidOwner` rather than being a no-op. Proof of concept: `SetAuthority` on the Q1 winner's ORE ATA, then `settle(1)` failed with the pool untouched; and because a quarter cannot be skipped, so would every later quarter, every `return_boxes` or `split` page naming that owner, and `close_pool` for a `fee_wallet` or creator who did the same. One payee, for the price of one instruction, held the pool for everyone until `RECLAIM_DELAY`. Token-2022 ATAs refuse the reassignment, so SKR pools were not exposed.
- Fix: `create_ata_idempotent` first reads the destination and returns at once when it is already an initialised token account for the pool's mint owned by the pool's token program, whoever its owner is now (`vault::is_token_account_for`; `StateWithExtensions` unpacks a 165-byte legacy account as well as a Token-2022 one, and an uninitialised or empty account is `false`, so a missing ATA still goes through the create). The `transfer_checked` that follows pays the derived address as before: the payee has paid whoever they gave the account to, and nobody else is held. A side effect: an SPL payout to an existing account no longer spends the no-op create's compute (`settle_q1_ore_atas_exist` 79,348 → 42,763 CU; `return_boxes_ore_11_owners_atas_exist` 254,350 → 122,291).
- Tests (`tests/settle.rs`): `ore_settle_pays_a_derived_ata_whose_owner_was_reassigned` (the proof of concept, now expecting success, the account still the stranger's and holding the prize), `ore_settle_with_a_foreign_token_account_at_the_derived_address_still_fails` (a token account for another mint planted at the derived address is not skipped and the ATA program refuses it), and `ore_settle_q1_with_every_ata_present_creates_nothing` now asserts no ATA-program CPI at all.

### Low

**A2 — `settle` on a suspended game let the keeper choose between settling and splitting.** Access slice; confirmed.

- Where: `settle.rs:135-142`. The checks were `Drawn`, the quarter order and `quarters_posted ≥ quarter`; `game.status` was not read.
- Spec: ARCHITECTURE › Returns and splits "every prize not yet paid, including the quarter in progress, is split"; the readiness document said a settle attempt on a suspended game "stops at `ScoresNotPosted` before the game status is consulted" — true only when no quarter is posted and unsettled at the mark.
- Code: with Q2 posted and unsettled when the admin marks the game, the keeper could settle Q2 (paying its winner) and then split, or split at once (dividing Q2 among all 25); either was accepted, and the keeper's choice decided who got Q2's prize.
- Fix: `settle` requires `game.status != Suspended` (`GameSuspended` 6070), checked right after `Drawn`, before the ordering and scores checks. What is paid at the mark is paid; what is not is divided. A working keeper settles a posted quarter within minutes and the admin marks a game once the league's word is in, so in practice nothing is left between; the ordering is the admin's to respect, not the program's to enforce.
- Tests (`tests/settle.rs`): `settle_on_a_suspended_game_is_game_suspended` (nothing settled with one or four posted, Q2 after Q1, Q4 after Q3, and a wrong quarter — all `GameSuspended`, the pool untouched). The Surfpool lifecycle `05-suspended-after-q2` now expects 6070 where it expected 6043.

**C1 — PROGRAM §2's "an index is never reused for a different mint" was not enforced.** Config slice; confirmed.

- Where: `update_config.rs:126-130` (`*slot = *r` for every `Some`), `state/config.rs` (`validate()` judged each rule alone), `sponsor.rs:121` (`config.tokens[pool.token].max_sponsorship`, read live).
- Spec: §2 (the sentence above, added with the six slots); §4.3 `sponsor` "the cap as configured now, not as it was at creation".
- Code: the admin could point a filled slot at another mint, or at the placeholder, in one `update_config`. Existing pools keep their own mint and price, so no transfer was misdirected, but `sponsor` would from then on judge their caps against a rule written for another token's decimals — an 11-decimal ORE cap read against a 6-decimal token is a cap of a hundred thousand times less, or more. Two slots could also name the same mint, with two rules, which the spec says is one index per mint.
- Fix: `update_config` requires `r.mint == slot.mint` for a slot whose mint is not the default (`InvalidConfig`), so a filled slot may change its ladder, cap and `enabled` flag but never its mint and never go back to a placeholder; `PlatformConfig::validate()` requires every non-default mint to appear in one slot only. A retired token is disabled in place, as §2 says.
- Tests (`tests/config.rs`): `a_filled_slot_keeps_its_mint_for_ever` (the ORE ladder changes with the same mint; a different mint with its account present is refused and the config is unchanged; back to the placeholder is refused), `one_mint_may_fill_only_one_slot` (the ORE mint in a spare slot by `update_config` and by `initialize`).

**D2 — the keeper can veto a sample window by closing its own `Var` inside it.** Draw slice; confirmed by a Mollusk scenario against the fork's bytecode. Documented, not changed.

- Where: Entropy's `Close` (authority only, any time); `sample_var.rs` (the owner constraint fails on a closed, system-owned account).
- Spec: ARCHITECTURE › Randomness disclosed the best-of-three as "a keeper that deliberately lets a window pass (no `sample_var` from anyone for 512 slots)". The reviewer showed the keeper need not wait for the window to lapse unsampled: it can `Close` the `Var` at `end_at + 1`, after seeing the hash, and nobody can sample a closed account; once the window is gone the admin's `replace_var` goes through as designed.
- Decision: the same best-of-three by another route, with the same controls — the replacement cap of two, a `VarReplaced` event and a count on the pool for each — and one more cost (the `Var`'s rent). The program cannot forbid a foreign program's `Close`, and binding the draw to a `Var` the program itself owns is the redesign the 2026-10-23 report already declined. Written into ARCHITECTURE › Randomness and PROGRAM §4.4; the keeper runbook treats a `Var` that vanished before its sample as an incident, not a retry.

### Info

- PROGRAM §8's name list stopped at `BoxAlreadySponsored` and three localnet suites still asserted 69 errors; the `tests/pools.rs` comment "token ≥ 3 has no rule" was stale; the readiness document's §7 compute table predated the six slots. All fixed (`VarEndAtMismatch`, `SampleWindowOpen`, `DrawAfterScores`, `DrawAfterKickoff`, `GameSuspended` appended; the table regenerated).
- `draw` accepted a pool on a game the admin had marked Postponed, Cancelled or Suspended (harmless — no score can follow — but a status an instruction ignores is a question an auditor asks). Now `GameNotScheduled`, with the D1 fix.
- `rotate_gate_key` has no pool-status check: the creator can rotate the gate key of a `Returned` or `Settled` pool. Nothing reads the gate key after sales close. Left as it is.
- `reclaim_sponsorship` on a `Settled` pool returns `FeesAlreadyPaid` rather than `NotReturnable` (the fees check is first). Cosmetic; left as it is.
- Money slice: the §5.1 and §5.2 arithmetic, every destination and every terminal path's conservation were re-derived and hold with six slots (`pool.token` indexes the same way; `check_mint` runs for every filled slot; placeholders validate); the 2026-10-23 fixes — the end-slot binding, `SampleWindowOpen`, the memo path, the default-pubkey batch, the saturating dust — all re-verified against the tests that pin them.
- The fuzz harness's own tests (`fuzz/tests/oracle.rs`, the three planted bugs) had not compiled since `Pool` gained `sponsored_boxes`; CI ran the fuzzer but never `cargo test -p mybarpool-fuzz`. Found while re-running the fuzzer on the A1 fix: its §10 Money oracle permitted a token account only by its owner field, so the first no-op mutation of the reassigned-owner fixture was reported as a violation — the oracle, not the program. The oracle now also permits a permitted wallet's associated token address for the account's mint and token program (the rule §5.4 states); a fourth planted-bug test pins it both ways (a stranger's own token account gaining is still a violation); the fixture was repaired and the CI fuzz job runs the harness's tests before the fuzzer.
- Two operational notes for the readiness document, not findings: a SOL ladder has a rent floor (a split share to a wallet that holds nothing must clear `minimum_balance(0)`; the launch ladder is forty times above it), and a Squads-wrapped `update_config` carries one or two token rules, since six rules make a 1,017-byte inner message.

## 4. Checked and sound

| Area | Verified |
|---|---|
| Six slots | `TOKEN_COUNT = 6`; six `TokenRule` of 98 bytes at 147 … 637, `bump` 735, `reserved` 736, 992 bytes, pinned by the layout test and the snapshot; `TokenRule::PLACEHOLDER` validates under `validate_ladder` and `validate_shape` for indices 1–5 and is refused at index 0; a filled spare slot needs its mint in its own position (`mint_3` for slot 3, not `mint_4`); `initialize` 703 bytes of data (1,044-byte legacy transaction with ORE alone, 1,172 with five mints); `update_config` with one rule 124 bytes; `ConfigUpdated` carries six rules; `create_pool` with `token ∈ {1, 3, 6, 255}` is `TokenDisabled` |
| `check_mint` | key, owner and decimals against the rule, the six refused extensions, run for every filled slot at `initialize` and for every changed slot at `update_config`; a mint account in the wrong slot's position does not count |
| Money | fee and prize math unchanged; `pool.token` and `pool.mint` fixed at creation; `sponsor`'s live cap read is now of a slot whose mint cannot change (C1); every destination fixed by on-chain state in the same instruction; the `returned` bitmap pays a box once across the three payers |
| Draw timing | `draw` before the deadline, refused at it; `DrawAfterScores` still refused independently; `set_var`'s lead, the end-slot binding and `SampleWindowOpen` hold; `replace_var` still refused with a sample recorded; the fork's `Open` refuses a past `end_at` so a post-sample re-open is dead |
| Returns | the five arms plus the new undrawn one are disjoint from `Settled` and `Split`; the counter is decremented only on leaving `Open`; an undrawn return reads the same instant as the draw refusal, so no pool is both drawable and returnable |
| Payout destinations | the derived address is checked before any create (`RequireKeysEqViolated` for a substitute); the create is skipped only for an initialised account of the pool's mint under the pool's token program; a foreign-mint account at the address still goes to the ATA program and is refused; the memo path is unchanged |
| Settlement | `Drawn`, not `Suspended`, in order, posted, winner recomputed; fees once; `FinalOnly` unchanged |
| Authority | every keeper instruction `has_one = score_authority`, every admin one `has_one = admin`; the keeper's new power (returning an undrawn pool at kickoff) pays owners at the purchase price and nothing else |
| Events, errors, layout | 25 events; 71 errors in declaration order, frozen by the errors test and the snapshot; no account list changed, so the snapshot was extended, not re-frozen |
| Compute | user-facing maximum 53,548 CU (`create_pool_spl`); worst keeper row 282,278 (`return_boxes_ore_9_owners_atas_missing`); `initialize` 26,387; the two `atas_exist` rows fell by the no-op create |

## 5. State transitions as implemented

The 2026-10-23 tables hold with these rows changed or added.

**Pool**

| From | Instruction | Guard | To |
|---|---|---|---|
| Locked | `draw` | game `Scheduled`, `now < min(scheduled_kickoff, recorded_kickoff)`, no quarter posted, sampled, revealed, commit and end slot match | Drawn |
| Locked | `return_boxes` | `!drawn` and `now ≥ min(scheduled_kickoff, recorded_kickoff)`; or game marked / admin / 30 days as before | Returned |
| Drawn | `settle(q)` | game not `Suspended`, in order, score posted | Drawn / Settled |

**Config**

| From | Instruction | Guard | To |
|---|---|---|---|
| slot with a mint | `update_config` | `r.mint == slot.mint`; the whole config validates; the mint account present and matching | slot with the same mint, new ladder / cap / flag |
| slot with the default mint | `update_config` | the mint named by `r` in no other slot; its account in the slot's position | slot filled |

## 6. What changed in the program

All before any deployment, so no account on any cluster is affected. No instruction's account list changed; the schema snapshot was extended by two errors.

| File | Change |
|---|---|
| `errors.rs` | `DrawAfterKickoff` 6069, `GameSuspended` 6070 |
| `state/game.rs` | `GameRecord::draw_deadline()` = `min(scheduled_kickoff, recorded_kickoff)` |
| `draw.rs` | `game.require_scheduled()`; `now < game.draw_deadline()` before the scores check |
| `return_boxes.rs` | the undrawn arm: `Locked && !drawn && now ≥ draw_deadline()` |
| `settle.rs` | `game.status != Suspended` after the `Drawn` check |
| `vault.rs` | `is_token_account_for`; `create_ata_idempotent` returns early when it holds |
| `update_config.rs` | a filled slot keeps its mint |
| `state/config.rs` | `validate()` requires non-default mints unique across slots |

The IDL, the generated client (`MYBARPOOL_ERROR_NAMES`), the compute-unit table and the test counts (`errors.rs`, `schema.rs`, `buy.rs`, `close.rs`, and the localnet and SDK suites: 69 → 71) were regenerated or updated. Ten Mollusk tests were added and one rewritten. Outside the program: `fuzz/src/oracle.rs` (O3 permits a permitted wallet's associated token address), `fuzz/tests/oracle.rs` (repaired; a fourth planted bug), `.github/workflows/ci.yml` (the harness's tests run in the fuzz job).

## 7. Verification after the fixes

- Mollusk: 383 tests across 21 files, all passing on the rebuilt ELF (`cargo test -p mybarpool` after `anchor build --arch v3`).
- Localnet: 17 files, 130 tests against the Surfpool mainnet fork, all passing with the fixes (`anchor test`); `05-suspended-after-q2` exercises `GameSuspended`, and every lifecycle draw lands before kickoff as it always did.
- Fuzzer on the fixed bytecode, 904 fixtures re-ejected from the new suite (856 seeds after the filter): 2,954,050 cases in 600 s, clean (proptest seed 10672233035546288790), after the oracle learned §5.4's derived-address rule (the Info above); the harness's own four planted-bug tests pass.
- `anchor-lints`, `cargo audit` (five accepted advisories, dev-side), `cargo fmt`, `cargo clippy --lib -D warnings`, prettier, typecheck, the SDK's 227 vitest cases: clean.

## 8. Accepted and open

| Item | Status |
|---|---|
| Keeper window veto by `Close` (D2) | accepted; the same best-of-three as the 2026-10-23 H1 residual, disclosed in ARCHITECTURE › Randomness and PROGRAM §4.4; the runbook treats it as an incident |
| A pool that locks inside the last two minutes before kickoff is returned, not drawn (D1 cost) | accepted; clients should say so as kickoff nears |
| A posted quarter unsettled at the suspension is split, not settled (A2) | accepted; the admin's ordering, written into §4.5 and §4.6 |
| `rotate_gate_key` without a status check; `reclaim_sponsorship` on `Settled` says `FeesAlreadyPaid` | accepted; harmless, cosmetic |
| Everything open from 2026-10-23 (`PoolStatusChanged`, pull-payment fallback, two-step admin, default `fee_wallet` at `initialize`) | unchanged |

## 9. Why internal

As on 2026-10-23: the program is 27 instructions with a mechanical specification, the harness proves the mechanics on the real bytecode, and what an auditor adds is an adversarial read by someone who did not write it. This second pass is the case for doing that more than once: both Highs were in code the first pass had in scope and passed — one in the fix the first pass itself proposed — and were found by asking a different question of the same lines ("what can the seed holder do between lock and settlement", "what can a payee do to their own account"). Before the first mainnet deployment the program will have been read adversarially by eight reviewers across two days, with every confirmed finding fixed and pinned by a test that fails without the fix. Should a third-party review be commissioned, this report and the previous one are its starting point.
