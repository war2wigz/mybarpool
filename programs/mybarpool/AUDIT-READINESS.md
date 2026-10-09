# Audit readiness

The entry point for an outside reviewer of the MyBarPool program. It is written for someone
who has read nothing else in this repository; the specification it refers to is
[`docs/PROGRAM.md`](../../docs/PROGRAM.md) (the mechanical spec: accounts, instructions,
algorithms, errors, invariants) and [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md)
(the why). Section references like §4.3 are to PROGRAM.md unless marked otherwise.

The program is **frozen** at this commit for the external audit: no instruction, account,
event or error changes until the findings come back.

## 1. Scope and build

**Crate:** `programs/mybarpool` (Anchor 1.2.0, `crate-type = ["cdylib", "lib"]`).
**Program id:** `declare_id!("3jk4YM9xnpoMYK3nuaUHZ59SCDxwExfT9EHxwP2wRMBw")` is a
placeholder until the mainnet deployment; CI syncs the id to a fresh keypair on every run
(`anchor keys sync`), and the committed IDL is compared to the build with the id
normalised. The deployed id and the config PDA will be published in the README at deploy.

**Build:** `anchor build --arch v3`. The ELF must be SBPFv3 (`readelf -h
target/deploy/mybarpool.so` shows `Flags: 0x3`; CI fails otherwise). The verifiable build
(`.github/workflows/verifiable-build.yml`) runs `solana-verify build --arch v3` (0.5.2) in the
Solana Foundation's image pinned by digest and compares the hash with the plain build.

**Suites** (all run in CI on every push; `.github/workflows/ci.yml`, `lints.yml`):

| Suite | Command | What it covers |
|---|---|---|
| Mollusk unit tests | `cargo test -p mybarpool --locked` | 346 tests over every instruction in isolation and in chains, with the `Clock` and `SlotHashes` sysvars set directly; the Entropy fork's verified bytecode loaded as a second program; the shared vector files cross-checked against the SDK |
| Compute-unit table | `cargo bench -p mybarpool --locked` | Rewrites `compute_units.md` (53 rows); a changed row is a review item, not a failure |
| Localnet | `anchor test` (Surfpool 1.6.0 forking mainnet) | 16 files, 119 tests: the per-step suites and eight lifecycle scripts under `tests/lifecycle/`, one per path from creation to `close_pool`, against the real SKR and ORE mints |
| Fuzzer | `fuzz/` — see its [README](../../fuzz/README.md) | Mollusk fixtures ejected from every unit-test instruction (772), replayed and mutated under proptest for a fixed budget, checked against the §10 Money oracle; CI runs 600 s. At this commit: 2,906,583 cases in 600 s (seed 215118630108621193) and 7,007,087 in 1,800 s (seed 16355487222879973167), both clean |
| Schema snapshot | `cargo test -p mybarpool --test schema` | `tests/fixtures/schema-snapshot.json` against the committed IDL: additive changes only (§10 Layout) |
| Lints | `DYLINT_RUSTFLAGS="-D warnings" cargo dylint --all --workspace -- --lib` | OtterSec anchor-lints at a pinned commit; see §5 |
| Dependency audit | `cargo audit --deny warnings` | RustSec against `Cargo.lock`, with the ignores in `.cargo/audit.toml`; see §6 |
| SDK | `npm ci && npm test` in `packages/shared` | The reference algorithms (§6) with 100 % branch coverage on the money and winner functions |

**Pinned toolchain** (ARCHITECTURE › Toolchain; bumps are their own commit with the suite
green before and after):

| Tool | Version |
|---|---|
| Agave / Solana CLI | 4.3.0 |
| Anchor | 1.2.0 |
| platform-tools | v1.57 (SBF rustc 1.95) |
| Host rustc | `rust-toolchain.toml`; crate `rust-version` 1.89 |
| Mollusk | 0.16.0 |
| Surfpool | 1.6.0 |
| `solana-verify` | 0.5.2 |
| cargo-dylint / anchor-lints | 6.1.0 / `otter-sec/anchor-lints@d8116dd`, nightly-2025-09-18 |
| cargo-audit | 0.22.2 |
| `@solana/kit` / Node | 8.4.0 / 22 |
| Entropy fork | `war2wigz/entropy@486225b` (fixture `tests/fixtures/entropy-486225b.so`, rebuilt and compared in CI) |

## 2. What the program does

A pool is a 5 × 5 grid of 25 boxes on one NFL game. Players buy boxes at a fixed price; the
program assigns positions from the SlotHashes sysvar (§6.1). When the 25th box sells the
pool locks; the digits 0–9 are then shuffled onto both axes from a value sampled on the
platform's own Entropy deployment (commit-reveal randomness with a slot-hash component,
§4.4, §6.2) — bound to the pool before the sample, verified against the commit at the draw
(`var_commit`). As each quarter's score is posted the program pays that quarter's winner
(§4.5, §6.3): the box at the row and column of the two scores' last digits. Fees — platform
5 %, creator 5 % plus an optional add-on, an optional integrator share, 15 % in total at most,
all fixed on the pool at creation — move with the first non-zero prize. Sponsors may add to
the prizes before kickoff; a sponsorship is returned if the pool never plays. A pool that does
not fill, or whose game is postponed, cancelled or suspended, is returned (or, after a payout,
split evenly) by the keeper; a pool nobody resolves can be reclaimed by its owners thirty days
after the scheduled kickoff. Every account closes once it holds nothing: rent goes to the
platform's fee wallet, or to the creator when the pool was abandoned.

**Who may do what** (ARCHITECTURE › Trust model):

| Key | Can do | Held in |
|---|---|---|
| Admin | `update_config` within hard-coded ceilings (`platform_bps ≤ 500`, `creator_bps ≤ 500`, add-on budget ≤ 500, total ≤ 1500); pause inflows; per-wallet limit overrides; `mark_game`; `cancel_pool`; `replace_var` on a `Var` that missed its window | Squads multisig |
| Score authority (keeper) | `create_game`, `update_kickoff`, `post_scores`, `settle`; `set_var`; `return_boxes`, `return_sponsorship`, `split` | Cloud KMS |
| Upgrade authority | Deploy new versions; `initialize` (it becomes the first admin) | Squads multisig |
| Anyone | `buy`, `create_pool`, `sponsor`, `rotate_gate_key` (own pools), `sample_var`, `reclaim` / `reclaim_sponsorship` (own boxes / sponsorship), `close_pool`, `close_sponsorship`, `close_counter` | Their wallets |

**What the keeper cannot do.** It supplies no destination the program does not verify: the
winner is recomputed from the stored axes and scores (`WinnerMismatch`), fee destinations
are `config.fee_wallet`, `pool.creator`, `pool.integrator` by `address` constraint, returns
go to the owners recorded on the pool and the wallet recorded on each `Sponsorship`. It cannot
settle out of order, without a draw, before a score is posted, or twice; cannot replace a `Var`
once sampled; cannot touch `config`. **What a pause does:** blocks `create_pool`, `buy`,
`sponsor` — the three inflows — and nothing else; every outflow (settle, return, split,
reclaim, close) runs while paused, so a pause can never trap funds.

## 3. Invariants and their tests

PROGRAM §10, bullet by bullet. Mollusk tests are `programs/mybarpool/tests/<file>.rs ::
<name>`; localnet items are `tests/<file>.test.ts` by item number; lifecycle scripts are
`tests/lifecycle/NN-*.test.ts`; fuzz lines are `fuzz/src/oracle.rs`.

**Money** — vault balance = §3.4 at every point; total out = total in; funds only move to a
box owner, a sponsor's recorded wallet, `fee_wallet`, `creator`, `integrator`.
- Mollusk: `settle::settle_q3_and_q4_in_turn_settle_the_pool_and_money_in_equals_money_out`,
  `settle::the_fee_transfers_are_atomic_with_the_prize`,
  `settle::settle_with_wrong_fee_wallet_creator_game_or_config`,
  `settle::close_pool_with_the_wrong_fee_wallet_or_creator_is_fee_account_mismatch`,
  `returns::return_boxes_is_atomic_when_the_vault_is_short`,
  `returns::return_sponsorship_pays_the_wallet_and_closes_the_account`, every `close.rs` chain
  (`assert_invariant` after each step, `total_out` at the end).
- Localnet: every lifecycle script calls `expectVaultInvariant` after each phase and asserts
  in = out at `close_pool` (01, 02, 05, 06 explicitly).
- Fuzz: O3 (conservation and destinations), O4 (§3.4 for every pool whose input satisfied it),
  O2 (a failure moves nothing).

**Ordering** — quarters settle in order; scores post in order and never decrease; no
settlement without a draw; no draw without a sampled, revealed `Var` whose seed hashes to the
recorded commit; no `Var` replaced once sampled.
- Mollusk: `settle::settle_out_of_order_is_quarter_out_of_order`,
  `games::post_scores_out_of_order_is_quarter_out_of_order`,
  `games::post_scores_lower_than_the_previous_post_is_score_decreased`,
  `settle::settle_needs_a_drawn_pool`,
  `settle::settle_a_quarter_whose_score_is_not_posted_is_scores_not_posted`,
  `draw::draw_refuses_an_unrevealed_or_inconsistent_value`,
  `draw::draw_binding_errors_not_sampled_here_mismatch_not_set`,
  `draw::draw_refuses_a_var_whose_commit_field_matches_but_whose_seed_does_not_hash_to_it`,
  `draw::draw_refuses_a_self_consistent_var_whose_seed_matches_a_different_commit`,
  `draw::replace_var_on_a_sampled_pool_is_var_already_sampled`,
  `draw::sample_var_binding_errors_var_not_set_var_mismatch_var_already_sampled`.
- Localnet: `settlement.test.ts` items 1, 4, 6; `draw.test.ts`; lifecycle 01–02, 05–07 (the
  full order), 04 and 08 (no settlement without the game / the draw).

**Timing** — no `buy`/`sponsor` at or after `recorded_kickoff`; no unfilled `return_boxes`
before it; no `update_kickoff` once passed; no `reclaim` before `scheduled_kickoff + 30 d`
regardless of `update_kickoff`.
- Mollusk: `buy::buy_after_kickoff_on_a_marked_game_or_while_paused_is_refused`,
  `sponsor::sponsor_is_allowed_on_open_locked_and_drawn_and_refused_on_terminal_pools`,
  `returns::return_boxes_unfilled_before_kickoff_is_not_returnable`,
  `returns::return_boxes_unfilled_reads_the_recorded_kickoff`,
  `games::update_kickoff_later_moves_recorded_only_and_emits_kickoff_updated` (and its
  too-late case), `reclaim::reclaim_is_measured_from_the_scheduled_kickoff`,
  `reclaim::reclaim_sponsorship_too_early_or_after_fees`.
- Localnet: `pools.test.ts` item 7; `unresolved.test.ts` items 1–2; lifecycle 03 (SalesClosed
  past kickoff, NotReturnable before), 08 (ReclaimTooEarly, then the window).
- Fuzz: the `clock` mutation (±60 days) under O2–O4.

**Authority** — admin and keeper instructions refuse each other's key; `replace_var`,
`mark_game`, `cancel_pool` admin-only; the keeper never supplies an unverified destination.
- Mollusk: `games::create_game_by_the_admin_or_a_stranger_is_unauthorized`,
  `settle::settle_by_the_admin_or_a_stranger_is_unauthorized`,
  `returns::return_boxes_by_anyone_but_the_keeper_is_unauthorized`,
  `returns::return_sponsorship_by_the_admin_is_unauthorized`,
  `returns::cancel_pool_by_anyone_but_the_admin_is_unauthorized`,
  `split::split_by_the_admin_is_unauthorized`,
  `draw::replace_var_by_the_admin_on_an_unsampled_pool_rebinds_and_counts` (and the keeper
  refused), `config::set_wallet_override_creates_then_updates_in_place_and_rejects_the_keeper`,
  `settle::settle_with_the_wrong_winner_is_winner_mismatch`,
  `settle::integrator_variant_without_or_with_the_wrong_integrator_is_fee_account_mismatch`.
- Localnet: `games.test.ts`, `config.test.ts` (wrong signer per instruction);
  `pools.test.ts` item 9 (a stranger's `rotate_gate_key`).
- Fuzz: the `signer` and `alias` mutations under O2 (a refusal moves nothing) and O3.

**Limits** — price ladder, presets, add-on budget, open-pool and own-box caps with override
precedence, sponsorship minimum and cap.
- Mollusk: `pools::price_must_be_on_the_ladder`, `config::default_preset_outside_the_three_is_invalid_preset`,
  `pools::addon_budget_is_the_configs_not_the_constant`,
  `pools::open_pool_limit_with_override_precedence`,
  `pools::initial_boxes_is_capped_by_max_own_boxes_with_override_precedence`,
  `buy::the_creator_is_capped_at_max_own_boxes_with_override_precedence`,
  `sponsor::below_one_box_price_is_sponsorship_too_small`,
  `sponsor::the_per_token_cap_is_exact_and_read_from_the_config_now`,
  `config::invariant_*` (the four config invariants), `config::set_wallet_override_rejects_zero_and_above_absolute_limits`.
- Localnet: `pools.test.ts` items 2, 3, 4, 6.
- Fuzz: O5 (`creator_boxes ≤ max_own_boxes` with override precedence on every successful
  `buy`).

**Fees** — independent of `sponsored_total`; paid exactly once; never on a returned pool;
`FinalOnly` pays them with the final.
- Mollusk: `settle::settle_q2_pays_the_prize_only_fees_once`,
  `settle::settle_moves_the_stored_fees_not_a_recomputation_from_the_config`,
  `settle::final_only_q1_to_q3_record_the_box_and_move_nothing`,
  `settle::final_only_q4_pays_everything_and_the_fees_now`,
  `returns::return_boxes_final_only_after_three_empty_settlements_returns_the_price`,
  `returns::cancel_pool_on_a_terminal_pool_is_not_returnable` and the `FeesAlreadyPaid`
  cases in `returns.rs` / `reclaim.rs`.
- Localnet: `settlement.test.ts` items 1–3, 6; lifecycle 01 (fees at Q1 only), 02 (fees
  unaffected by two sponsorships), 05 (`return_sponsorship` → `FeesAlreadyPaid`), 07
  (FinalOnly: nothing through Q3, everything at Q4).

**Rent** — every closable account closes to its destination; nothing un-closable in any
terminal state.
- Mollusk: `settle::close_pool_sol_sends_both_rents_to_the_fee_wallet`,
  `settle::close_pool_abandoned_goes_to_the_creator`,
  `settle::close_pool_ore_sweeps_the_dust_creates_the_ata_and_closes_the_vault`,
  `settle::close_pool_skr_token_2022_closes_the_vault`,
  `settle::close_pool_needs_a_terminal_pool_without_open_sponsorships_or_outstanding_boxes`,
  `close::chain_*` (one per terminal path, every account closed),
  `close::close_counter_once_a_return_took_the_last_open_pool_to_zero`,
  `sponsor::close_counter_at_zero_sends_rent_to_fee_wallet_without_a_signer`,
  `config::close_wallet_override_returns_rent_to_admin_and_emits_the_closed_values`.
- Localnet: `settlement.test.ts` item 7, `returns.test.ts` item 5, `unresolved.test.ts`;
  every lifecycle script ends in `close_pool` with the destination and both rents asserted
  (01 also `close_counter`; 02 and 05 `close_sponsorship`; 08 to the creator).

**Layout** — a snapshot test freezes account offsets and event schemas after Step 8; changes
additive only.
- `tests/schema.rs` (five tests, three of them negatives naming the item) against
  `tests/fixtures/schema-snapshot.json`; `tests/layout.rs` pins every byte offset and size;
  `tests/errors.rs` the 65 codes.

**Composability** — no instruction reads the instructions sysvar or depends on its
neighbours.
- Mollusk: `buy::buy_and_sponsor_are_unaffected_by_neighbouring_instructions`,
  `buy::buy_gated_is_composable`, `settle::settle_is_unaffected_by_neighbouring_instructions`,
  `returns::return_boxes_composes_with_neighbours`, `reclaim::reclaim_composes_with_neighbours`,
  `games::post_scores_and_update_kickoff_are_unaffected_by_neighbouring_instructions`,
  `config::update_config_and_set_override_are_unaffected_by_neighbouring_instructions`,
  `draw::set_var_sample_var_and_draw_are_unaffected_by_neighbouring_instructions`.
- Localnet: `pools.test.ts` item 14 (a memo before, a compute-budget instruction after, on a
  gated buy); every `sample_var`, `settle` and return transaction carries a compute-budget
  instruction in front.

**Account types** — seed-only and destination wallets are `UncheckedAccount` with an
`address` / `has_one` constraint, never `SystemAccount`; every CPI target is a fixed address;
every account read after a CPI is reloaded.
- §4 below lists every `UncheckedAccount`; `draw::the_reload_test_sampled_hash_is_the_sysvar_entry_though_the_var_was_zero_before_the_cpi`
  covers the one post-CPI read (`sample_var` reloads the `Var`); the lints
  `overconstrained_seed_account`, `arbitrary_cpi_call`, `missing_account_reload` run in CI
  (§5).

## 4. Account constraints

**PDA seeds** (`src/constants.rs`; bumps stored on the account and re-derived with `bump =`):

| Account | Seeds |
|---|---|
| `PlatformConfig` | `["config"]` |
| `WalletOverride` | `["override", wallet]` |
| `GameRecord` | `["game", season (u16 LE), week, home, away, scheduled_kickoff (i64 LE)]` |
| `Pool` | `["pool", game, creator, nonce (u64 LE)]` |
| vault | `["vault", pool]` (a system account for SOL, a token account for SPL; signer seeds for every outbound transfer) |
| `CreatorCounter` | `["counter", creator, game]` |
| `Sponsorship` | `["sponsorship", pool, sponsor wallet]` |
| event authority | Anchor's `["__event_authority"]` (the `emit_cpi!` pair on every emitting instruction) |

**Every `UncheckedAccount`** (38; the `CHECK` line is the one Anchor requires, the constraint
column is the `#[account]` attribute; "checked in the handler" names the `require!` that
pins it):

| File | Field | CHECK | Constraint |
|---|---|---|---|
| `buy.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `buy.rs` | `wallet_override` | seeds pin it to the canonical address; decoded only when it holds this program's data. | `seeds = [OVERRIDE_SEED, pool.creator.as_ref()], bump` |
| `buy.rs` | `slot_hashes` | `address = sysvar::slot_hashes::ID`. | `address = sysvar::slot_hashes::ID` |
| `buy.rs` | `gate_key?` | read for its key and `is_signer` only, inside the handler, against `pool.gate_key`; never written, never a CPI target. | `(none; checked in the handler)` |
| `close_counter.rs` | `fee_wallet` | `has_one = fee_wallet` on `config`; a lamport destination only. | `mut` |
| `close_pool.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `close_pool.rs` | `fee_wallet` | `address = config.fee_wallet`. | `mut, address = config.fee_wallet @ MybarpoolError::FeeAccountMismatch` |
| `close_pool.rs` | `creator` | `address = pool.creator`. | `mut, address = pool.creator @ MybarpoolError::FeeAccountMismatch` |
| `close_pool.rs` | `destination_token_account?` | compared in the handler to the destination's ATA for the pool's mint. | `mut` |
| `close_sponsorship.rs` | `sponsor` | `address = sponsorship.wallet`, the rent's only destination. | `mut, address = sponsorship.wallet` |
| `create_pool.rs` | `vault` | seeds pin it to the pool; this handler funds or initialises it and only it. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump` |
| `create_pool.rs` | `wallet_override` | seeds pin it to the canonical address; decoded only when it holds this program's data. | `seeds = [OVERRIDE_SEED, creator.key().as_ref()], bump` |
| `create_pool.rs` | `slot_hashes` | `address = sysvar::slot_hashes::ID`. | `address = sysvar::slot_hashes::ID` |
| `draw.rs` | `var` | owner `ENTROPY_PROGRAM` by constraint, `== pool.var` and every field by the handler. | `owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy` |
| `reclaim.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `reclaim.rs` | `box_owner_token_account?` | compared to the owner's derived associated token account in the handler. SPL | `mut` |
| `reclaim_sponsorship.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `reclaim_sponsorship.rs` | `sponsor_token_account?` | compared to the sponsor's derived associated token account in the handler. SPL | `mut` |
| `replace_var.rs` | `new_var` | owner `ENTROPY_PROGRAM` by constraint; every `set_var` check by the handler. | `owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy` |
| `return_boxes.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `return_sponsorship.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `return_sponsorship.rs` | `sponsor` | `address = sponsorship.wallet`, the only destination a return can use. | `mut, address = sponsorship.wallet` |
| `return_sponsorship.rs` | `sponsor_token_account?` | compared to the sponsor's derived associated token account in the handler. | `mut` |
| `sample_var.rs` | `var` | owner `ENTROPY_PROGRAM` by constraint, `== pool.var` and every field by the | `mut, owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy` |
| `sample_var.rs` | `slot_hashes` | the SlotHashes sysvar by address; read by Entropy's `Sample` and again by this | `address = sysvar::slot_hashes::ID` |
| `sample_var.rs` | `entropy_program` | the Entropy program by address (PROGRAM §1 `ENTROPY_PROGRAM`), executable; the | `address = ENTROPY_PROGRAM, executable` |
| `set_var.rs` | `var` | owner `ENTROPY_PROGRAM` by constraint; length, discriminator and every field by | `owner = ENTROPY_PROGRAM @ MybarpoolError::VarNotEntropy` |
| `settle.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `settle.rs` | `winner` | compared in the handler to `pool.owners[winning box]` (`WinnerMismatch`). | `mut` |
| `settle.rs` | `fee_wallet` | `address = config.fee_wallet`. | `mut, address = config.fee_wallet @ MybarpoolError::FeeAccountMismatch` |
| `settle.rs` | `creator` | `address = pool.creator`. | `mut, address = pool.creator @ MybarpoolError::FeeAccountMismatch` |
| `settle.rs` | `integrator?` | compared in the handler to `pool.integrator` when the pool has one. | `mut` |
| `settle.rs` | `winner_token_account?` | compared in the handler to the winner's ATA for the pool's mint and token program. | `mut` |
| `settle.rs` | `fee_token_account?` | compared in the handler to `fee_wallet`'s ATA. | `mut` |
| `settle.rs` | `creator_token_account?` | compared in the handler to the creator's ATA. | `mut` |
| `settle.rs` | `integrator_token_account?` | compared in the handler to the integrator's ATA when the pool has an integrator. | `mut` |
| `split.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |
| `sponsor.rs` | `vault` | seeds + the stored `vault_bump` pin it to `pool.vault`. | `mut, seeds = [VAULT_SEED, pool.key().as_ref()], bump = pool.vault_bump` |

The handler-checked ones, in detail: `gate_key?` in `buy` is compared to `pool.gate_key` and
required to sign, inside `gate()` after the count and cap checks (§4.3; `GateKeyNotSigner`
for absent, wrong or not signing); `var` / `new_var` in `set_var`, `replace_var`, `sample_var`,
`draw` are owner-checked by constraint and then decoded field by field (`VarNot*` errors);
every `*_token_account?` is compared to the derived associated token account of its wallet
for the pool's mint (`RequireKeysEqViolated` on a mismatch) and created idempotently when
missing; `destination_token_account?` in `close_pool` likewise for the destination.

**`address =` / `has_one` destinations:** `fee_wallet` is `config.fee_wallet` (`has_one` on
`config` or `address = config.fee_wallet`); `creator` is `pool.creator`; `integrator` is
`pool.integrator`; `sponsor` is `sponsorship.wallet`; a box owner is matched to `pool.owners`
in the handler (`NotOwner` / the owner set of a batch). The winner is never an input the
program trusts: `settle` recomputes it from the stored axes and the posted score and refuses
a different wallet (`WinnerMismatch`).

**CPI targets, all fixed:** `system_program` (`Program<'info, System>`), the pool's token
program (`Interface<'info, TokenInterface>` checked against `pool.token_program`, which was
copied from the config's `TokenRule` at creation), the associated-token program
(`Program<'info, AssociatedToken>`), `ENTROPY_PROGRAM` (`address = ENTROPY_PROGRAM,
executable` in `sample_var`; the `Var` accounts are `owner = ENTROPY_PROGRAM`), and the
program itself for `emit_cpi!`. No instruction takes a caller-supplied program as a CPI
target.

## 5. Lints

OtterSec `anchor-lints` at `d8116dd` through cargo-dylint, every lint enabled, warnings as
errors: `arbitrary_cpi_call`, `ata_should_use_init_if_needed`, `cpi_no_result`,
`direct_lamport_cpi_dos`, `duplicate_mutable_accounts`, `example_lint`,
`missing_account_field_init`, `missing_account_reload`, `missing_mut_constraint`,
`missing_owner_check`, `missing_signer_validation`, `overconstrained_seed_account`,
`pda_signer_account_overlap`, `unsafe_pyth_price_account`.

Two lints misfire on this code. Each `allow` below was removed in turn and the lint run; the
ones that no longer fired were deleted, so every remaining `allow` is a confirmed false
positive that still fires at the pinned commit. The two reproductions are small enough to
file upstream.

**`missing_mut_constraint`.** The lint reads a MIR temporary derived from a *read* of a field
of an Anchor account struct as a write to that account and asks for `#[account(mut)]`.
Reproduction (ten lines; fires on `config`):

```rust
#[program] pub mod repro { use super::*;
    pub fn read_only(ctx: Context<ReadOnly>) -> Result<()> {
        require!(!ctx.accounts.config.paused, MyError::Paused); Ok(())
    } }
#[account] pub struct Config { pub paused: bool }
#[derive(Accounts)] pub struct ReadOnly<'info> {
    #[account(seeds = [b"config"], bump)] pub config: Account<'info, Config> }
#[error_code] pub enum MyError { Paused }
```

Where it is allowed (each site reads `config`, `game`, a mint, or the gate key, and writes
none of them): `src/instructions/buy.rs` (`handle_buy`, `gate`), `close_pool.rs`,
`create_game.rs`, `create_pool.rs`, `draw.rs`, `initialize.rs`, `mint_check.rs`,
`set_wallet_override.rs`, `settle.rs`, `sponsor.rs`, `update_config.rs`;
`src/state/config.rs` (`PlatformConfig::validate`), `src/state/game.rs` (`GameRecord`'s
method); and the fuzz crate's `model.rs`, `mutate.rs`, `oracle.rs`, which decode the
program's state to judge it and own no instruction at all.

**`arbitrary_cpi_call`.** A CPI whose program id arrives as a function parameter is reported
as user-controlled even when every caller passes a `Program<'info, T>` (a fixed address by
type) or an `Interface` the handler has checked. The report is attached to the *handler* the
lint started its analysis from, not to the helper, so the allow sits on the four handlers that
reach the vault's outbound helpers through `payout.rs` and on their `lib.rs` entry points
(each root is analysed on its own; both are needed): `close_pool`, `split`,
`return_sponsorship`, `reclaim_sponsorship`. Reproduction (fires on `helper`'s CPI):

```rust
#[program] pub mod repro { use super::*;
    pub fn pay(ctx: Context<Pay>, amount: u64) -> Result<()> {
        helper(&ctx.accounts.vault, &ctx.accounts.to, &ctx.accounts.system_program.to_account_info(), amount)
    } }
pub fn helper<'info>(from: &AccountInfo<'info>, to: &AccountInfo<'info>, system_program: &AccountInfo<'info>, amount: u64) -> Result<()> {
    system_program::transfer(CpiContext::new(*system_program.key,
        system_program::Transfer { from: from.clone(), to: to.clone() }), amount)
}
#[derive(Accounts)] pub struct Pay<'info> {
    #[account(mut)] pub vault: SystemAccount<'info>, #[account(mut)] pub to: SystemAccount<'info>,
    pub system_program: Program<'info, System> }
```

One more thing a reviewer should know about this lint set: its type matching is on the
*printed* path of a type, and `Pubkey` in the 3.x Solana crates is an alias of
`solana_address::Address`. Until this step no module in the crate named
`anchor_lang::prelude::Pubkey` in a signature, rustc printed the type as
`solana_address::Address`, and `arbitrary_cpi_call` saw no pubkey anywhere — a clean run
that was blind. `src/allowlist.rs` (`leaf(wallet: &Pubkey)`) changed how rustc prints the
type, the lint woke up, and the allows above (and the deletion of nine others that stopped
firing for the same reason) followed. Treat a clean `anchor-lints` run as necessary, not
sufficient.

## 6. Dependencies

**The compiled program's tree** (`cargo tree -p mybarpool --target bpfel-unknown-none -e
normal`; 185 distinct crates, `(*)` marks a subtree already shown):

```text
mybarpool v0.1.0 (/workspaces/mybarpool-public/programs/mybarpool)
├── anchor-lang v1.2.0
│   ├── anchor-attribute-access-control v1.2.0 (proc-macro)
│   │   ├── proc-macro2 v1.0.107
│   │   │   └── unicode-ident v1.0.26
│   │   ├── quote v1.0.47
│   │   │   └── proc-macro2 v1.0.107 (*)
│   │   └── syn v2.0.119
│   │       ├── proc-macro2 v1.0.107 (*)
│   │       ├── quote v1.0.47 (*)
│   │       └── unicode-ident v1.0.26
│   ├── anchor-attribute-account v1.2.0 (proc-macro)
│   │   ├── anchor-syn v1.2.0
│   │   │   ├── bs58 v0.5.1
│   │   │   ├── heck v0.3.3
│   │   │   │   └── unicode-segmentation v1.13.3
│   │   │   ├── proc-macro2 v1.0.107 (*)
│   │   │   ├── quote v1.0.47 (*)
│   │   │   ├── serde v1.0.229
│   │   │   │   ├── serde_core v1.0.229
│   │   │   │   └── serde_derive v1.0.229 (proc-macro)
│   │   │   │       ├── proc-macro2 v1.0.107 (*)
│   │   │   │       ├── quote v1.0.47 (*)
│   │   │   │       └── syn v3.0.6
│   │   │   │           ├── proc-macro2 v1.0.107 (*)
│   │   │   │           ├── quote v1.0.47 (*)
│   │   │   │           └── unicode-ident v1.0.26
│   │   │   ├── sha2 v0.11.0
│   │   │   │   ├── cfg-if v1.0.5
│   │   │   │   ├── cpufeatures v0.3.1
│   │   │   │   └── digest v0.11.3
│   │   │   │       ├── block-buffer v0.12.1
│   │   │   │       │   └── hybrid-array v0.4.15
│   │   │   │       │       └── typenum v1.20.1
│   │   │   │       ├── const-oid v0.10.2
│   │   │   │       └── crypto-common v0.2.2
│   │   │   │           └── hybrid-array v0.4.15 (*)
│   │   │   ├── syn v2.0.119 (*)
│   │   │   └── thiserror v1.0.69
│   │   │       └── thiserror-impl v1.0.69 (proc-macro)
│   │   │           ├── proc-macro2 v1.0.107 (*)
│   │   │           ├── quote v1.0.47 (*)
│   │   │           └── syn v2.0.119 (*)
│   │   ├── proc-macro2 v1.0.107 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-attribute-constant v1.2.0 (proc-macro)
│   │   ├── anchor-syn v1.2.0 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-attribute-error v1.2.0 (proc-macro)
│   │   ├── anchor-syn v1.2.0 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-attribute-event v1.2.0 (proc-macro)
│   │   ├── anchor-syn v1.2.0 (*)
│   │   ├── proc-macro2 v1.0.107 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-attribute-program v1.2.0 (proc-macro)
│   │   ├── anchor-lang-idl v0.1.4
│   │   │   ├── anchor-lang-idl-spec v0.1.0
│   │   │   │   ├── anyhow v1.0.104
│   │   │   │   └── serde v1.0.229 (*)
│   │   │   ├── anyhow v1.0.104
│   │   │   ├── heck v0.3.3 (*)
│   │   │   ├── serde v1.0.229 (*)
│   │   │   ├── serde_json v1.0.151
│   │   │   │   ├── itoa v1.0.18
│   │   │   │   ├── memchr v2.8.3
│   │   │   │   ├── serde_core v1.0.229
│   │   │   │   └── zmij v1.0.23
│   │   │   └── sha2 v0.10.9
│   │   │       ├── cfg-if v1.0.5
│   │   │       ├── cpufeatures v0.2.17
│   │   │       └── digest v0.10.7
│   │   │           ├── block-buffer v0.10.4
│   │   │           │   └── generic-array v0.14.7
│   │   │           │       └── typenum v1.20.1
│   │   │           └── crypto-common v0.1.7
│   │   │               ├── generic-array v0.14.7 (*)
│   │   │               └── typenum v1.20.1
│   │   ├── anchor-syn v1.2.0 (*)
│   │   ├── anyhow v1.0.104
│   │   ├── heck v0.3.3 (*)
│   │   ├── proc-macro2 v1.0.107 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-derive-accounts v1.2.0 (proc-macro)
│   │   ├── anchor-syn v1.2.0 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-derive-serde v1.2.0 (proc-macro)
│   │   ├── anchor-syn v1.2.0 (*)
│   │   ├── proc-macro-crate v3.5.0
│   │   │   └── toml_edit v0.25.15+spec-1.1.0
│   │   │       ├── indexmap v2.14.2
│   │   │       │   ├── equivalent v1.0.2
│   │   │       │   └── hashbrown v0.17.1
│   │   │       ├── toml_datetime v1.1.1+spec-1.1.0
│   │   │       ├── toml_parser v1.1.3+spec-1.1.0
│   │   │       │   └── winnow v1.0.4
│   │   │       └── winnow v1.0.4
│   │   ├── proc-macro2 v1.0.107 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-derive-space v1.2.0 (proc-macro)
│   │   ├── proc-macro2 v1.0.107 (*)
│   │   ├── quote v1.0.47 (*)
│   │   └── syn v2.0.119 (*)
│   ├── anchor-lang-error v1.2.0
│   │   ├── anchor-attribute-error v1.2.0 (proc-macro) (*)
│   │   ├── borsh v1.8.1
│   │   │   └── borsh-derive v1.8.1 (proc-macro)
│   │   │       ├── once_cell v1.21.4
│   │   │       ├── proc-macro-crate v3.5.0 (*)
│   │   │       ├── proc-macro2 v1.0.107 (*)
│   │   │       ├── quote v1.0.47 (*)
│   │   │       └── syn v3.0.6 (*)
│   │   ├── solana-msg v3.1.0
│   │   ├── solana-program-error v3.0.1
│   │   │   └── borsh v1.8.1 (*)
│   │   └── solana-pubkey v3.0.0
│   │       └── solana-address v1.1.0
│   │           └── solana-address v2.9.0
│   │               ├── borsh v1.8.1 (*)
│   │               ├── bytemuck v1.25.2
│   │               │   └── bytemuck_derive v1.12.1 (proc-macro)
│   │               │       ├── proc-macro2 v1.0.107 (*)
│   │               │       ├── quote v1.0.47 (*)
│   │               │       └── syn v3.0.6 (*)
│   │               ├── bytemuck_derive v1.12.1 (proc-macro) (*)
│   │               ├── five8 v1.0.0
│   │               │   └── five8_core v1.0.0
│   │               ├── five8_const v1.0.0
│   │               │   └── five8_core v1.0.0
│   │               ├── serde v1.0.229
│   │               │   ├── serde_core v1.0.229
│   │               │   └── serde_derive v1.0.229 (proc-macro) (*)
│   │               ├── serde_derive v1.0.229 (proc-macro) (*)
│   │               ├── sha2-const-stable v0.1.0
│   │               ├── solana-atomic-u64 v3.0.1
│   │               ├── solana-define-syscall v5.2.0
│   │               ├── solana-nullable v1.3.0
│   │               │   └── bytemuck v1.25.2 (*)
│   │               ├── solana-program-error v3.0.1 (*)
│   │               ├── solana-sanitize v3.0.1
│   │               └── solana-sha256-hasher v3.1.0
│   │                   ├── solana-define-syscall v4.0.1
│   │                   └── solana-hash v4.7.0
│   │                       ├── bytemuck v1.25.2 (*)
│   │                       ├── bytemuck_derive v1.12.1 (proc-macro) (*)
│   │                       ├── five8 v1.0.0 (*)
│   │                       ├── serde v1.0.229 (*)
│   │                       └── serde_derive v1.0.229 (proc-macro) (*)
│   ├── base64 v0.21.7
│   ├── bincode v1.3.3
│   │   └── serde v1.0.229 (*)
│   ├── borsh v1.8.1 (*)
│   ├── bytemuck v1.25.2 (*)
│   ├── const-crypto v0.3.0
│   │   ├── keccak-const v0.2.0
│   │   └── sha2-const-stable v0.1.0
│   ├── solana-account-info v3.1.1
│   │   ├── solana-address v2.9.0 (*)
│   │   ├── solana-program-error v3.0.1 (*)
│   │   └── solana-program-memory v3.1.0
│   │       └── solana-define-syscall v4.0.1
│   ├── solana-clock v3.2.1
│   │   └── solana-clock v4.0.0
│   │       ├── serde v1.0.229 (*)
│   │       ├── serde_derive v1.0.229 (proc-macro) (*)
│   │       ├── solana-get-sysvar v1.0.0
│   │       │   ├── solana-address v2.9.0 (*)
│   │       │   └── solana-program-error v3.0.1 (*)
│   │       ├── solana-sdk-ids v3.1.0
│   │       │   └── solana-address v2.9.0 (*)
│   │       ├── solana-sdk-macro v3.0.1 (proc-macro)
│   │       │   ├── bs58 v0.5.1
│   │       │   ├── proc-macro2 v1.0.107 (*)
│   │       │   ├── quote v1.0.47 (*)
│   │       │   └── syn v2.0.119 (*)
│   │       └── solana-sysvar-id v3.1.0
│   │           ├── solana-address v2.9.0 (*)
│   │           └── solana-sdk-ids v3.1.0 (*)
│   ├── solana-cpi v3.1.0
│   │   ├── solana-account-info v3.1.1 (*)
│   │   ├── solana-instruction v3.5.1
│   │   │   ├── solana-instruction v4.0.0
│   │   │   │   ├── bincode v1.3.3 (*)
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   │   └── solana-pubkey v4.4.0
│   │   │   │       └── solana-address v2.9.0 (*)
│   │   │   └── solana-instruction-error v2.5.0
│   │   │       ├── num-traits v0.2.19
│   │   │       └── solana-program-error v3.0.1 (*)
│   │   ├── solana-program-error v3.0.1 (*)
│   │   └── solana-pubkey v4.4.0 (*)
│   ├── solana-define-syscall v3.0.0
│   ├── solana-feature-gate-interface v3.1.0
│   │   ├── solana-program-error v3.0.1 (*)
│   │   ├── solana-pubkey v4.4.0 (*)
│   │   └── solana-sdk-ids v3.1.0 (*)
│   ├── solana-instruction v3.5.1 (*)
│   ├── solana-instructions-sysvar v3.0.1
│   │   ├── bitflags v2.13.2
│   │   ├── solana-account-info v3.1.1 (*)
│   │   ├── solana-instruction v3.5.1 (*)
│   │   ├── solana-instruction-error v2.5.0 (*)
│   │   ├── solana-program-error v3.0.1 (*)
│   │   ├── solana-sanitize v3.0.1
│   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   ├── solana-serialize-utils v3.1.2
│   │   │   ├── solana-instruction-error v2.5.0 (*)
│   │   │   ├── solana-pubkey v4.4.0 (*)
│   │   │   └── solana-sanitize v3.0.1
│   │   └── solana-sysvar-id v3.1.0 (*)
│   ├── solana-invoke v0.5.0
│   │   ├── solana-account-info v3.1.1 (*)
│   │   ├── solana-define-syscall v3.0.0
│   │   ├── solana-instruction v3.5.1 (*)
│   │   ├── solana-program-entrypoint v3.1.1
│   │   │   ├── solana-account-info v3.1.1 (*)
│   │   │   ├── solana-define-syscall v4.0.1
│   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   └── solana-pubkey v4.4.0 (*)
│   │   └── solana-stable-layout v3.0.1
│   │       ├── solana-instruction v3.5.1 (*)
│   │       └── solana-pubkey v4.4.0 (*)
│   ├── solana-loader-v3-interface v6.1.1
│   │   ├── serde v1.0.229 (*)
│   │   ├── serde_bytes v0.11.19
│   │   │   └── serde_core v1.0.229
│   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   ├── solana-instruction v3.5.1 (*)
│   │   ├── solana-pubkey v4.4.0 (*)
│   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   └── solana-system-interface v3.3.0
│   │       ├── num-traits v0.2.19
│   │       ├── serde v1.0.229 (*)
│   │       ├── serde_derive v1.0.229 (proc-macro) (*)
│   │       ├── solana-address v2.9.0 (*)
│   │       ├── solana-instruction v3.5.1 (*)
│   │       ├── solana-msg v3.1.0
│   │       └── solana-program-error v3.0.1 (*)
│   ├── solana-msg v3.1.0
│   ├── solana-program-entrypoint v3.1.1 (*)
│   ├── solana-program-error v3.0.1 (*)
│   ├── solana-program-memory v3.1.0 (*)
│   ├── solana-program-option v3.1.0
│   ├── solana-program-pack v3.1.0
│   │   └── solana-program-error v3.0.1 (*)
│   ├── solana-pubkey v3.0.0 (*)
│   ├── solana-sdk-ids v3.1.0 (*)
│   ├── solana-stake-interface v2.0.2
│   │   ├── num-traits v0.2.19
│   │   ├── serde v1.0.229 (*)
│   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   ├── solana-clock v3.2.1 (*)
│   │   ├── solana-cpi v3.1.0 (*)
│   │   ├── solana-instruction v3.5.1 (*)
│   │   ├── solana-program-error v3.0.1 (*)
│   │   ├── solana-pubkey v3.0.0 (*)
│   │   ├── solana-system-interface v2.0.0
│   │   │   ├── num-traits v0.2.19
│   │   │   ├── serde v1.0.229 (*)
│   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   ├── solana-instruction v3.5.1 (*)
│   │   │   ├── solana-msg v3.1.0
│   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   └── solana-pubkey v3.0.0 (*)
│   │   ├── solana-sysvar v3.1.1
│   │   │   ├── base64 v0.22.1
│   │   │   ├── bincode v1.3.3 (*)
│   │   │   ├── lazy_static v1.5.1
│   │   │   ├── serde v1.0.229 (*)
│   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   ├── solana-account-info v3.1.1 (*)
│   │   │   ├── solana-clock v3.2.1 (*)
│   │   │   ├── solana-epoch-rewards v3.2.0
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   │   ├── solana-get-sysvar v1.0.0 (*)
│   │   │   │   ├── solana-hash v4.7.0 (*)
│   │   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   │   ├── solana-sdk-macro v3.0.1 (proc-macro) (*)
│   │   │   │   └── solana-sysvar-id v3.1.0 (*)
│   │   │   ├── solana-epoch-schedule v3.4.0
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   │   ├── solana-get-sysvar v1.0.0 (*)
│   │   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   │   ├── solana-sdk-macro v3.0.1 (proc-macro) (*)
│   │   │   │   └── solana-sysvar-id v3.1.0 (*)
│   │   │   ├── solana-fee-calculator v3.4.0
│   │   │   │   ├── log v0.4.34
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   └── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   ├── solana-hash v4.7.0 (*)
│   │   │   ├── solana-instruction v3.5.1 (*)
│   │   │   ├── solana-last-restart-slot v3.2.0
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   │   ├── solana-get-sysvar v1.0.0 (*)
│   │   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   │   ├── solana-sdk-macro v3.0.1 (proc-macro) (*)
│   │   │   │   └── solana-sysvar-id v3.1.0 (*)
│   │   │   ├── solana-program-entrypoint v3.1.1 (*)
│   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   ├── solana-program-memory v3.1.0 (*)
│   │   │   ├── solana-pubkey v4.4.0 (*)
│   │   │   ├── solana-rent v3.1.0
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   │   ├── solana-sdk-macro v3.0.1 (proc-macro) (*)
│   │   │   │   └── solana-sysvar-id v3.1.0 (*)
│   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   ├── solana-sdk-macro v3.0.1 (proc-macro) (*)
│   │   │   ├── solana-slot-hashes v3.2.0
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   │   ├── solana-get-sysvar v1.0.0 (*)
│   │   │   │   ├── solana-hash v4.7.0 (*)
│   │   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   │   └── solana-sysvar-id v3.1.0 (*)
│   │   │   ├── solana-slot-history v3.2.0
│   │   │   │   ├── bv v0.11.1
│   │   │   │   │   └── serde v1.0.229 (*)
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   │   ├── solana-get-sysvar v1.0.0 (*)
│   │   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   │   └── solana-sysvar-id v3.1.0 (*)
│   │   │   └── solana-sysvar-id v3.1.0 (*)
│   │   └── solana-sysvar-id v3.1.0 (*)
│   ├── solana-system-interface v2.0.0 (*)
│   ├── solana-sysvar v3.1.1 (*)
│   ├── solana-sysvar-id v3.1.0 (*)
│   └── thiserror v1.0.69
│       └── thiserror-impl v1.0.69 (proc-macro) (*)
├── anchor-spl v1.2.0
│   ├── anchor-lang v1.2.0 (*)
│   ├── spl-associated-token-account-interface v2.0.0
│   │   ├── solana-instruction v3.5.1 (*)
│   │   └── solana-pubkey v3.0.0 (*)
│   ├── spl-token-2022-interface v2.1.0
│   │   ├── arrayref v0.3.9
│   │   ├── bytemuck v1.25.2 (*)
│   │   ├── num-derive v0.4.2 (proc-macro)
│   │   │   ├── proc-macro2 v1.0.107 (*)
│   │   │   ├── quote v1.0.47 (*)
│   │   │   └── syn v2.0.119 (*)
│   │   ├── num-traits v0.2.19
│   │   ├── num_enum v0.7.6
│   │   │   ├── num_enum_derive v0.7.6 (proc-macro)
│   │   │   │   ├── proc-macro-crate v3.5.0 (*)
│   │   │   │   ├── proc-macro2 v1.0.107 (*)
│   │   │   │   ├── quote v1.0.47 (*)
│   │   │   │   └── syn v2.0.119 (*)
│   │   │   └── rustversion v1.0.23 (proc-macro)
│   │   ├── solana-account-info v3.1.1 (*)
│   │   ├── solana-instruction v3.5.1 (*)
│   │   ├── solana-program-error v3.0.1 (*)
│   │   ├── solana-program-option v3.1.0
│   │   ├── solana-program-pack v3.1.0 (*)
│   │   ├── solana-pubkey v3.0.0 (*)
│   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   ├── solana-zk-sdk v4.0.0
│   │   │   ├── aes-gcm-siv v0.11.1
│   │   │   │   ├── aead v0.5.2
│   │   │   │   │   ├── crypto-common v0.1.7
│   │   │   │   │   │   ├── generic-array v0.14.7
│   │   │   │   │   │   │   └── typenum v1.20.1
│   │   │   │   │   │   ├── rand_core v0.6.4
│   │   │   │   │   │   │   └── getrandom v0.2.17
│   │   │   │   │   │   │       └── cfg-if v1.0.5
│   │   │   │   │   │   └── typenum v1.20.1
│   │   │   │   │   └── generic-array v0.14.7 (*)
│   │   │   │   ├── aes v0.8.4
│   │   │   │   │   ├── cfg-if v1.0.5
│   │   │   │   │   └── cipher v0.4.4
│   │   │   │   │       ├── crypto-common v0.1.7 (*)
│   │   │   │   │       └── inout v0.1.4
│   │   │   │   │           └── generic-array v0.14.7 (*)
│   │   │   │   ├── cipher v0.4.4 (*)
│   │   │   │   ├── ctr v0.9.2
│   │   │   │   │   └── cipher v0.4.4 (*)
│   │   │   │   ├── polyval v0.6.2
│   │   │   │   │   ├── cfg-if v1.0.5
│   │   │   │   │   ├── opaque-debug v0.3.1
│   │   │   │   │   └── universal-hash v0.5.1
│   │   │   │   │       ├── crypto-common v0.1.7 (*)
│   │   │   │   │       └── subtle v2.6.1
│   │   │   │   ├── subtle v2.6.1
│   │   │   │   └── zeroize v1.9.0
│   │   │   │       └── zeroize_derive v1.5.0 (proc-macro)
│   │   │   │           ├── proc-macro2 v1.0.107 (*)
│   │   │   │           ├── quote v1.0.47 (*)
│   │   │   │           └── syn v2.0.119 (*)
│   │   │   ├── base64 v0.22.1
│   │   │   ├── bincode v1.3.3 (*)
│   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   ├── bytemuck_derive v1.12.1 (proc-macro) (*)
│   │   │   ├── curve25519-dalek v4.1.3
│   │   │   │   ├── cfg-if v1.0.5
│   │   │   │   ├── digest v0.10.7
│   │   │   │   │   ├── block-buffer v0.10.4
│   │   │   │   │   │   └── generic-array v0.14.7 (*)
│   │   │   │   │   ├── crypto-common v0.1.7 (*)
│   │   │   │   │   └── subtle v2.6.1
│   │   │   │   ├── rand_core v0.6.4 (*)
│   │   │   │   ├── serde v1.0.229 (*)
│   │   │   │   ├── subtle v2.6.1
│   │   │   │   └── zeroize v1.9.0 (*)
│   │   │   ├── itertools v0.12.1
│   │   │   │   └── either v1.18.0
│   │   │   ├── merlin v3.0.0
│   │   │   │   ├── byteorder v1.5.0
│   │   │   │   ├── keccak v0.1.6
│   │   │   │   ├── rand_core v0.6.4 (*)
│   │   │   │   └── zeroize v1.9.0 (*)
│   │   │   ├── num-derive v0.4.2 (proc-macro) (*)
│   │   │   ├── num-traits v0.2.19
│   │   │   ├── rand v0.8.8
│   │   │   │   ├── rand_chacha v0.3.1
│   │   │   │   │   ├── ppv-lite86 v0.2.21
│   │   │   │   │   │   └── zerocopy v0.8.59
│   │   │   │   │   └── rand_core v0.6.4 (*)
│   │   │   │   └── rand_core v0.6.4 (*)
│   │   │   ├── serde v1.0.229 (*)
│   │   │   ├── serde_derive v1.0.229 (proc-macro) (*)
│   │   │   ├── serde_json v1.0.151
│   │   │   │   ├── itoa v1.0.18
│   │   │   │   ├── memchr v2.8.3
│   │   │   │   ├── serde_core v1.0.229
│   │   │   │   └── zmij v1.0.23
│   │   │   ├── sha3 v0.10.9
│   │   │   │   ├── digest v0.10.7 (*)
│   │   │   │   └── keccak v0.1.6
│   │   │   ├── solana-derivation-path v3.0.0
│   │   │   │   ├── derivation-path v0.2.0
│   │   │   │   ├── qstring v0.7.2
│   │   │   │   │   └── percent-encoding v2.3.2
│   │   │   │   └── uriparse v0.6.4
│   │   │   │       ├── fnv v1.0.7
│   │   │   │       └── lazy_static v1.5.1
│   │   │   ├── solana-instruction v3.5.1 (*)
│   │   │   ├── solana-pubkey v3.0.0 (*)
│   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   ├── solana-seed-derivable v3.0.0
│   │   │   │   └── solana-derivation-path v3.0.0 (*)
│   │   │   ├── solana-seed-phrase v3.0.0
│   │   │   │   ├── hmac v0.12.1
│   │   │   │   │   └── digest v0.10.7 (*)
│   │   │   │   ├── pbkdf2 v0.11.0
│   │   │   │   │   └── digest v0.10.7 (*)
│   │   │   │   └── sha2 v0.10.9
│   │   │   │       ├── cfg-if v1.0.5
│   │   │   │       └── digest v0.10.7 (*)
│   │   │   ├── solana-signature v3.6.0
│   │   │   │   ├── five8 v1.0.0 (*)
│   │   │   │   └── solana-sanitize v3.0.1
│   │   │   ├── solana-signer v3.0.1
│   │   │   │   ├── solana-pubkey v4.4.0 (*)
│   │   │   │   ├── solana-signature v3.6.0 (*)
│   │   │   │   └── solana-transaction-error v3.4.0
│   │   │   │       ├── solana-instruction-error v2.5.0 (*)
│   │   │   │       └── solana-sanitize v3.0.1
│   │   │   ├── subtle v2.6.1
│   │   │   ├── thiserror v2.0.21
│   │   │   │   └── thiserror-impl v2.0.21 (proc-macro)
│   │   │   │       ├── proc-macro2 v1.0.107 (*)
│   │   │   │       ├── quote v1.0.47 (*)
│   │   │   │       └── syn v3.0.6 (*)
│   │   │   └── zeroize v1.9.0 (*)
│   │   ├── spl-pod v0.7.4
│   │   │   ├── borsh v1.8.1 (*)
│   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   ├── bytemuck_derive v1.12.1 (proc-macro) (*)
│   │   │   ├── num-derive v0.4.2 (proc-macro) (*)
│   │   │   ├── num-traits v0.2.19
│   │   │   ├── num_enum v0.7.6 (*)
│   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   ├── solana-program-option v3.1.0
│   │   │   ├── solana-pubkey v3.0.0 (*)
│   │   │   ├── solana-zero-copy v1.3.0
│   │   │   │   ├── borsh v1.8.1 (*)
│   │   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   │   └── bytemuck_derive v1.12.1 (proc-macro) (*)
│   │   │   ├── solana-zk-sdk v4.0.0 (*)
│   │   │   └── thiserror v2.0.21 (*)
│   │   ├── spl-token-confidential-transfer-proof-extraction v0.5.1
│   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   ├── solana-account-info v3.1.1 (*)
│   │   │   ├── solana-curve25519 v3.1.14
│   │   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   │   ├── bytemuck_derive v1.12.1 (proc-macro) (*)
│   │   │   │   ├── curve25519-dalek v4.1.3 (*)
│   │   │   │   ├── subtle v2.6.1
│   │   │   │   └── thiserror v2.0.21 (*)
│   │   │   ├── solana-instruction v3.5.1 (*)
│   │   │   ├── solana-instructions-sysvar v3.0.1 (*)
│   │   │   ├── solana-msg v3.1.0
│   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   ├── solana-pubkey v3.0.0 (*)
│   │   │   ├── solana-sdk-ids v3.1.0 (*)
│   │   │   ├── solana-zk-sdk v4.0.0 (*)
│   │   │   ├── spl-pod v0.7.4 (*)
│   │   │   └── thiserror v2.0.21 (*)
│   │   ├── spl-token-confidential-transfer-proof-generation v0.5.1
│   │   │   ├── curve25519-dalek v4.1.3 (*)
│   │   │   ├── solana-zk-sdk v4.0.0 (*)
│   │   │   └── thiserror v2.0.21 (*)
│   │   ├── spl-token-group-interface v0.7.2
│   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   ├── num-derive v0.4.2 (proc-macro) (*)
│   │   │   ├── num-traits v0.2.19
│   │   │   ├── num_enum v0.7.6 (*)
│   │   │   ├── solana-address v2.9.0 (*)
│   │   │   ├── solana-instruction v3.5.1 (*)
│   │   │   ├── solana-nullable v1.3.0 (*)
│   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   ├── solana-zero-copy v1.3.0 (*)
│   │   │   ├── spl-discriminator v0.5.2
│   │   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   │   ├── solana-sha256-hasher v3.1.0 (*)
│   │   │   │   └── spl-discriminator-derive v0.2.0 (proc-macro)
│   │   │   │       ├── quote v1.0.47 (*)
│   │   │   │       ├── spl-discriminator-syn v0.2.1
│   │   │   │       │   ├── proc-macro2 v1.0.107 (*)
│   │   │   │       │   ├── quote v1.0.47 (*)
│   │   │   │       │   ├── sha2 v0.10.9 (*)
│   │   │   │       │   ├── syn v2.0.119 (*)
│   │   │   │       │   └── thiserror v1.0.69 (*)
│   │   │   │       └── syn v2.0.119 (*)
│   │   │   └── thiserror v2.0.21 (*)
│   │   ├── spl-token-metadata-interface v0.8.0
│   │   │   ├── borsh v1.8.1 (*)
│   │   │   ├── num-derive v0.4.2 (proc-macro) (*)
│   │   │   ├── num-traits v0.2.19
│   │   │   ├── solana-borsh v3.0.2
│   │   │   │   └── borsh v1.8.1 (*)
│   │   │   ├── solana-instruction v3.5.1 (*)
│   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   ├── solana-pubkey v3.0.0 (*)
│   │   │   ├── spl-discriminator v0.5.2 (*)
│   │   │   ├── spl-pod v0.7.4 (*)
│   │   │   ├── spl-type-length-value v0.9.1
│   │   │   │   ├── bytemuck v1.25.2 (*)
│   │   │   │   ├── num-derive v0.4.2 (proc-macro) (*)
│   │   │   │   ├── num-traits v0.2.19
│   │   │   │   ├── num_enum v0.7.6 (*)
│   │   │   │   ├── solana-account-info v3.1.1 (*)
│   │   │   │   ├── solana-program-error v3.0.1 (*)
│   │   │   │   ├── solana-zero-copy v1.3.0 (*)
│   │   │   │   ├── spl-discriminator v0.5.2 (*)
│   │   │   │   └── thiserror v2.0.21 (*)
│   │   │   └── thiserror v2.0.21 (*)
│   │   ├── spl-type-length-value v0.9.1 (*)
│   │   └── thiserror v2.0.21 (*)
│   └── spl-token-interface v2.0.0
│       ├── arrayref v0.3.9
│       ├── bytemuck v1.25.2 (*)
│       ├── num-derive v0.4.2 (proc-macro) (*)
│       ├── num-traits v0.2.19
│       ├── num_enum v0.7.6 (*)
│       ├── solana-instruction v3.5.1 (*)
│       ├── solana-program-error v3.0.1 (*)
│       ├── solana-program-option v3.1.0
│       ├── solana-program-pack v3.1.0 (*)
│       ├── solana-pubkey v3.0.0 (*)
│       ├── solana-sdk-ids v3.1.0 (*)
│       └── thiserror v2.0.21 (*)
├── solana-keccak-hasher v3.1.0
│   ├── solana-define-syscall v4.0.1
│   └── solana-hash v4.7.0 (*)
├── solana-sdk-ids v3.1.0 (*)
└── solana-sha256-hasher v3.1.0 (*)
```

Direct dependencies: `anchor-lang` 1.2.0 (`event-cpi`, `init-if-needed`), `anchor-spl` 1.2.0
(`token`, `token_2022`, `associated_token`), `solana-sha256-hasher` 3.1 (the `hashv`
syscall, §6.1 and §6.4), `solana-sdk-ids` 3.1 (the SlotHashes sysvar id),
`solana-keccak-hasher` 3.1 (Entropy's formulas, §4.4). This step added nothing to this tree;
its additions are dev-side (the `fuzz-fixtures` feature on the program's dev-dependencies;
the `fuzz/` crate with `mollusk-svm-fuzz-fixture`, `mollusk-svm-programs-token`, `proptest`).

**`cargo audit`** (RustSec database as of this commit): 0 vulnerabilities; 4 `unmaintained`
warnings, every one transitive and the upstream owner's to replace, ignored in
`.cargo/audit.toml` with the path recorded — `bincode` 1.3.3 (RUSTSEC-2025-0141, through
`anchor-lang`), `derivative` 2.2.0 (RUSTSEC-2024-0388) and `paste` 1.0.15
(RUSTSEC-2024-0436) (both through the `ark-*` crates under `solana-poseidon` ←
`solana-syscalls`, dev-side here via Mollusk), `libsecp256k1` 0.7.2 (RUSTSEC-2025-0161,
`solana-syscalls`, dev-side). A fifth warning fails the Lints workflow. **`npm audit`**: 0
vulnerabilities with and without dev dependencies; no ignores.

Dependabot opens version PRs on both the program and the SDK; a person reads every one (the
pins are deliberate and a major is never merged without a toolchain decision).

## 7. Compute and size

Every row of `compute_units.md` with the client-side limit DESIGN §10.4 prescribes
(`measured × 1.2 + 30,000` for the instructions a wallet may append). A row above 1,000,000
after the rule would be a finding; the worst is 365,596, a quarter of the 1.4 M transaction
maximum.

| Row | CU | Limit to set |
|---|---|---|
| `initialize` | 20,281 | 54,337 |
| `update_config_full` | 15,218 | 48,261 |
| `set_wallet_override_create` | 13,160 | 45,792 |
| `set_wallet_override_update` | 10,820 | 42,984 |
| `close_wallet_override` | 9,391 | 41,269 |
| `create_game` | 18,058 | 51,669 |
| `update_kickoff` | 10,936 | 43,123 |
| `post_scores_q1` | 10,955 | 43,146 |
| `post_scores_final` | 10,967 | 43,160 |
| `mark_game` | 10,401 | 42,481 |
| `create_pool_sol` | 41,562 | 79,874 |
| `create_pool_sol_5_boxes` | 47,654 | 87,184 |
| `create_pool_spl` | 51,317 | 91,580 |
| `buy_1` | 29,419 | 65,302 |
| `buy_3` | 29,797 | 65,756 |
| `buy_25th_locks` | 31,389 | 67,666 |
| `buy_spl_1` | 31,537 | 67,844 |
| `sponsor_new` | 26,380 | 61,656 |
| `sponsor_top_up` | 23,985 | 58,782 |
| `rotate_gate_key` | 11,204 | 43,444 |
| `buy_link_sol` | 29,621 | 65,545 |
| `buy_allowlist_sol_depth_1` | 29,819 | 65,782 |
| `buy_allowlist_sol_depth_10` | 31,671 | 68,005 |
| `buy_allowlist_sol_depth_32` | 36,179 | 73,414 |
| `close_counter` | 6,570 | 37,884 |
| `set_var` | 16,086 | 49,303 |
| `sample_var` | 29,691 | 65,629 |
| `sample_var_already_sampled` | 14,429 | 47,314 |
| `draw` | 19,173 | 53,007 |
| `replace_var` | 16,359 | 49,630 |
| `settle_q1_sol` | 26,525 | 61,830 |
| `settle_q2_sol` | 22,033 | 56,439 |
| `settle_q4_sol` | 22,034 | 56,440 |
| `settle_q1_final_only` | 21,019 | 55,222 |
| `settle_q1_sol_integrator` | 28,425 | 64,110 |
| `settle_q1_ore` | 105,084 | 156,100 |
| `settle_q1_ore_atas_exist` | 77,841 | 123,409 |
| `close_pool_sol` | 17,436 | 50,923 |
| `close_pool_ore` | 48,987 | 88,784 |
| `return_boxes_first_sol_2_owners` | 29,352 | 65,222 |
| `return_boxes_25_owners_sol` | 135,270 | 192,324 |
| `return_boxes_ore_3_owners_atas_missing` | 106,171 | 157,405 |
| `return_boxes_ore_9_owners_atas_missing` | 279,664 | 365,596 |
| `return_boxes_ore_11_owners_atas_exist` | 251,579 | 331,894 |
| `return_sponsorship_sol` | 22,778 | 57,333 |
| `return_sponsorship_ore_ata_missing` | 48,043 | 87,651 |
| `cancel_pool_open` | 17,041 | 50,449 |
| `split_first_sol_3_owners` | 31,893 | 68,271 |
| `split_ore_3_owners_atas_missing` | 105,995 | 157,194 |
| `reclaim_sol_open` | 21,284 | 55,540 |
| `reclaim_ore_locked_ata_missing` | 43,778 | 82,533 |
| `reclaim_sponsorship_sol` | 23,043 | 57,651 |
| `close_sponsorship` | 14,524 | 47,428 |

**Transaction size and the instruction trace, measured on localnet** (Step 7 and the
lifecycle scripts): a SOL `return_boxes` of 24 owners with the counter is 1,247 bytes, over
the 1,232-byte packet, so the keeper pages SOL returns at twelve owners (851 and 819 bytes for
a full pool). An SPL page whose token accounts must be created costs seven inner instructions
per owner against the runtime's trace limit of 64; the bench row above measures nine owners as
a single instruction (`return_boxes_ore_9_owners_atas_missing`, 279,664 CU), but a real
transaction also carries the compute-budget instruction the page needs, and the ninth owner
takes the trace to 65: `tests/lifecycle/04-postponed-after-draw.test.ts` shows the nine-owner
page refused with "Max instruction trace length exceeded" and pages eight, eight, eight and
one (269,747 CU for an eight-owner page). Eleven owners fit when their token accounts exist
(`return_boxes_ore_11_owners_atas_exist`). The gated `buy` grows by about 200 CU per proof
entry (depth 1 → 32: 29,819 → 36,179).

## 8. Known limits and decisions a reviewer will ask about

- **`OverrideRequired` (6023) is reserved and never raised.** The override slot is a required
  account of `create_pool` and `buy` (§3.6), so it cannot be omitted; the number stays because
  the list is frozen in declaration order. `FeesAlreadyPaid` (6047) was reserved the same way
  through the settlement step and is raised by the return paths since.
- **Box positions are program-assigned** (§6.1) from the most recent SlotHashes entry: a
  buyer chooses a count, never a position, so no position has better odds before the draw.
  `boxIndex` is 0-based on-chain; people see 1–25, converted once in the SDK.
- **The allowlist tree** (§6.4) is sha256 with a `0x00` leaf prefix and a `0x01` node prefix,
  children hashed in bytewise order (a proof carries sibling hashes only), the wallet list
  sorted and deduplicated so a root is recomputable from a published list. At most 32 proof
  entries, checked before the fold. The SDK builds roots and proofs; the program only
  verifies; `packages/shared/src/vectors/allowlist.json` cross-tests the two.
- **The gate key is a bearer credential.** Anyone the link is forwarded to can buy; that is
  the intended behaviour for a bar, and `allowlist` exists for when it is not.
  `rotate_gate_key` kills a leaked link. The creator's `initial_boxes` at creation are not
  gated (the creator sets the gate in that instruction); every later `buy`, the creator's
  included, is.
- **`create_idempotent` on every associated-token payment.** A payee's token account may be
  closed between purchase and payout; the program recreates it (rent from the transaction
  payer) rather than fail a settlement or return. A wrong account in the slot is refused
  before the create (`RequireKeysEqViolated`).
- **No `Paused` check on returns and settlements.** The pause flag blocks the three inflows
  and nothing else, so an incident response can never trap funds (ARCHITECTURE › Trust model).
- **The keeper's SPL page is eight owners when token accounts must be created** (see §7),
  not the nine the bench row suggests; the program accepts any batch length and every return
  is idempotent through the `returned` bitmap.
- **Reclaim is measured from `scheduled_kickoff`**, not `recorded_kickoff`, so an
  `update_kickoff` can never push the thirty-day window out (§10 Timing).
- **`settle` on a suspended game.** No score is posted after a suspension, so a settle attempt
  stops at `ScoresNotPosted` (6043) before the game status is consulted; `split` is the path.
- **The `Var` is read after the `Sample` CPI** in `sample_var` through `reload`-equivalent
  re-decoding of the raw account (the only post-CPI read in the program), and `draw` verifies
  the revealed seed against the commit the program recorded at `set_var`, so an upgrade of
  the Entropy deployment cannot change a draw's outcome.
- **Mollusk's post-execution rent check** reports an account left below its rent-exempt
  minimum as a failure where the runtime would reject the transaction; the vault helpers never
  take a SOL vault below its own minimum (`return_boxes_is_atomic_when_the_vault_is_short`),
  and the fuzzer classifies that report separately.

## 9. Out of scope

The keeper (scores, settlement and return automation), the scores service and its two
sources, the Android app and the web app, the Entropy fork's own repository and audit trail
(`war2wigz/entropy`; this repository pins its commit and rebuilds its bytecode in CI), and
infrastructure. The SDK (`packages/shared`) is in scope only as the reference implementation
of §6 that the program's vector tests compare against.
