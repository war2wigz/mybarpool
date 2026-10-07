# MyBarPool program specification

The mechanical specification of the on-chain program: every account, seed, field, instruction, check, state transition, event and error, plus the exact algorithms for box assignment, the digit draw and the winner. [ARCHITECTURE.md](ARCHITECTURE.md) says what the rules are and why; this file says precisely how the program implements them. Where the two seem to differ, the rule in ARCHITECTURE is the intent and this file has a bug: fix this file, and say so in the commit.

Conventions: all times are Unix seconds (`i64`) read from the Clock sysvar. All amounts are integer base units of the pool's token (`u64`). All arithmetic is checked; overflow is an error, never a wrap. Box indices are `0–24` inside the program and on-chain; every label a person sees is `index + 1`, converted once in `packages/shared`. Byte layouts are little-endian. PDAs use the seeds given; bumps are stored on the account.

## 1. Constants

Hard-coded in the program. Changing any of these is a program upgrade.

| Constant | Value | Meaning |
|---|---|---|
| `BOXES` | 25 | Boxes per pool |
| `LANES` | 5 | Positions per axis; lane `l` holds digits at positions `l` and `l + 5` |
| `QUARTERS` | 4 | Settlement periods; the fourth is the final score |
| `PLATFORM_BPS_MAX` | 500 | Ceiling on the platform share |
| `CREATOR_BPS_MAX` | 500 | Ceiling on the creator base share |
| `ADDON_BUDGET_BPS_MAX` | 500 | Ceiling on `creator_addon_bps + integrator_bps` |
| `TOTAL_BPS_MAX` | 1500 | Ceiling on platform + creator base + add-on budget |
| `MIN_QUARTER_SECONDS` | 900 | A quarter's worth of football takes at least this long in real time |
| `KICKOFF_UPDATE_BOUND` | 259 200 | 72 hours; how far a recorded kickoff may move from the scheduled one |
| `RECLAIM_DELAY` | 2 592 000 | 30 days; abandoned-pool reclaim opens this long after the scheduled kickoff |
| `MAX_OWN_BOXES_ABSOLUTE` | 25 | Upper bound on any configured creator box cap |
| `ENTROPY_PROGRAM` | MyBarPool's Entropy deployment (id fixed in Step 5b, published in the README) | The platform's deployment of the Entropy fork (ARCHITECTURE › Randomness). Through Step 5 the constant is Regolith's `3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X`, the bytecode those tests run against; Step 5b moves it |

Payout presets, an enum with exactly three values. The program rejects any other discriminant.

| Preset | Discriminant | Q1 | Q2 | Q3 | Final |
|---|---|---|---|---|---|
| `Standard` | 0 | 20 | 20 | 20 | 40 |
| `Even` | 1 | 25 | 25 | 25 | 25 |
| `FinalOnly` | 2 | 0 | 0 | 0 | 100 |

## 2. Identifiers

### Game key
A game is identified on-chain by a canonical key, not by any source's ID, so the record does not depend on ESPN, API-Sports or Sportradar numbering:

```
GameKey { season: u16, week: u8, home: u8, away: u8 }
```

- `season`: the year the regular season starts in (2026 for the 2026–27 season).
- `week`: 1–18 regular season; 19 Wild Card, 20 Divisional, 21 Conference Championships, 22 Super Bowl. Preseason, if ever enabled, is 101–103 so it can never collide.
- `home`, `away`: index 0–31 into the team table in `packages/shared`, which is sorted by abbreviation and frozen; adding a team is a table change and a shared-package major version.

The mapping from each source's IDs to a `GameKey` lives in `packages/shared` and in the scores service. A rescheduled game keeps its key but gets a new `scheduled_kickoff`, so it gets a new record (below).

### Team table order
`ARI, ATL, BAL, BUF, CAR, CHI, CIN, CLE, DAL, DEN, DET, GB, HOU, IND, JAX, KC, LAC, LAR, LV, MIA, MIN, NE, NO, NYG, NYJ, PHI, PIT, SEA, SF, TB, TEN, WAS` = indices 0–31.

### Token index
`0` = SOL (native), `1` = SKR, `2` = ORE. The config holds one `TokenRule` per index; a pool stores the index and the mint.

## 3. Accounts

Every account has an 8-byte Anchor discriminator and the fields below in order. The long-lived accounts (`PlatformConfig`, `GameRecord`, `Pool`) end with `reserved` padding so fields can be appended without a migration; the small accounts (`CreatorCounter`, `WalletOverride`, `Sponsorship`) have none, and a new field on one of them is a new account type. Sizes are the serialized sizes; rent follows from them.

### 3.1 `PlatformConfig` — seeds `["config"]`

One per deployment.

| Field | Type | Notes |
|---|---|---|
| `admin` | Pubkey | The Squads vault on mainnet |
| `score_authority` | Pubkey | The keeper's KMS key |
| `entropy_provider` | Pubkey | The Entropy provider whose `Var`s `set_var` accepts: the keeper's key, since the platform runs the provider on its own Entropy deployment (the Step 5 tests use Regolith's provider key `AKBXJ7jQ2DiqLQKzgPn791r1ZVNvLchTFH6kpesPAAWF`, read from the live ORE `Var`); changeable by `update_config` |
| `fee_wallet` | Pubkey | Platform fee destination (SOL directly; SPL to its ATA) |
| `platform_bps` | u16 | ≤ `PLATFORM_BPS_MAX`; initial 500 |
| `creator_bps` | u16 | ≤ `CREATOR_BPS_MAX`; initial 500 |
| `addon_budget_bps` | u16 | ≤ `ADDON_BUDGET_BPS_MAX`; initial 500 |
| `default_preset` | u8 | Which preset clients pre-select; initial 0 |
| `max_open_pools` | u8 | Per creator per game; initial 3 |
| `max_own_boxes` | u8 | Creator's boxes in own pool; initial 5 |
| `preseason_enabled` | bool | Initial false; `create_game` rejects weeks ≥ 101 when false |
| `paused` | bool | When true, `create_pool`, `buy` and `sponsor` fail; nothing else is affected |
| `tokens` | [TokenRule; 3] | Indexed by token index |
| `bump` | u8 | |
| `reserved` | [u8; 256] | |

`TokenRule`:

| Field | Type | Notes |
|---|---|---|
| `enabled` | bool | |
| `mint` | Pubkey | `Pubkey::default()` for SOL |
| `token_program` | Pubkey | Token or Token-2022; `Pubkey::default()` for SOL |
| `decimals` | u8 | 9 SOL, 11 ORE, SKR per mint |
| `min_price` | u64 | Base units |
| `step` | u64 | Base units |
| `max_price` | u64 | Base units |
| `max_sponsorship` | u64 | Per pool, base units; initial `25 × max_price` |

Invariants enforced on every write: `platform_bps + creator_bps + addon_budget_bps ≤ TOTAL_BPS_MAX`; `min_price ≥ 1`; `step ≥ 1`; `min_price ≤ max_price`; `(max_price − min_price) % step == 0`; `max_open_pools ≥ 1`; `1 ≤ max_own_boxes ≤ MAX_OWN_BOXES_ABSOLUTE`; `admin` is never the default pubkey (a zeroed admin would leave the config unchangeable).

Initial ladders: SOL `0.05 / 0.05 / 1`; SKR `100 / 100 / 5 000`; ORE `0.05 / 0.05 / 1` (in whole tokens; stored in base units).

### 3.2 `GameRecord` — seeds `["game", season u16, week u8, home u8, away u8, scheduled_kickoff i64]`

One per scheduled game. Created and rent-paid by the platform. Never closed.

| Field | Type | Notes |
|---|---|---|
| `key` | GameKey | |
| `scheduled_kickoff` | i64 | Seed value; never changes |
| `recorded_kickoff` | i64 | Starts equal to `scheduled_kickoff`; moved by `update_kickoff` |
| `status` | u8 | `GameStatus`; an enum in the program and the IDL, stored as its one-byte discriminant |
| `quarters_posted` | u8 | 0–4 |
| `home_score` | [u16; 4] | Cumulative home score at the end of Q1..Q3 and final |
| `away_score` | [u16; 4] | Same for away |
| `posted_at` | [i64; 4] | When each post landed |
| `final_had_overtime` | bool | Informational, set on the final post |
| `marked_at` | i64 | When `mark_game` last ran; 0 if never |
| `bump` | u8 | |
| `reserved` | [u8; 64] | |

`GameStatus`: `Scheduled = 0`, `Postponed = 1`, `Cancelled = 2`, `Suspended = 3`, `Final = 4`.

### 3.3 `Pool` — seeds `["pool", game_record, creator, nonce u64]`

`nonce` is chosen by the client (random `u64`); the PDA must not already exist. It exists only to let one creator have several pools on one game with no shared counter in the seed.

| Field | Type | Notes |
|---|---|---|
| `game` | Pubkey | The `GameRecord` |
| `creator` | Pubkey | |
| `nonce` | u64 | |
| `token` | u8 | Token index |
| `mint` | Pubkey | Copied from config at creation |
| `token_program` | Pubkey | Copied from config at creation |
| `vault` | Pubkey | The vault account (3.4) |
| `price` | u64 | Per box |
| `preset` | u8 | `PayoutPreset`; an enum in the program and the IDL, stored as its one-byte discriminant |
| `access_type` | u8 | `AccessType`: `Public = 0`, `Link = 1`, `Allowlist = 2`; an enum in the program and the IDL, one byte |
| `gate_key` | Pubkey | Required co-signer when `access_type == Link`; else default |
| `allowlist_root` | [u8; 32] | Merkle root when `access_type == Allowlist`; else zero |
| `creator_addon_bps` | u16 | 0–500 |
| `integrator` | Pubkey | Default when unset |
| `integrator_bps` | u16 | 0–500; 0 when unset |
| `platform_fee` | u64 | Fixed at creation (§5.1) |
| `creator_fee` | u64 | Fixed at creation; base + add-on |
| `integrator_fee` | u64 | Fixed at creation |
| `status` | u8 | `PoolStatus`; an enum in the program and the IDL, stored as its one-byte discriminant |
| `sold` | u8 | Boxes sold, 0–25 |
| `owners` | [Pubkey; 25] | `Pubkey::default()` = unsold |
| `creator_boxes` | u8 | Boxes the creator holds in this pool |
| `sponsored_total` | u64 | Sum of all sponsorships |
| `sponsor_count` | u16 | Distinct sponsor wallets ever |
| `sponsorships_open` | u16 | `Sponsorship` accounts not yet closed |
| `var` | Pubkey | Entropy `Var` recorded at lock; default until then |
| `var_end_at` | u64 | The `Var`'s `end_at` as recorded by `set_var`; 0 until then |
| `sampled_slot` | u64 | Slot at which this program's `sample_var` ran; 0 until then |
| `sampled_hash` | [u8; 32] | The slot hash `sample_var` verified against `SlotHashes`; `draw` requires the `Var` still carries it |
| `var_replacements` | u8 | Admin `replace_var` calls so far; max 2 |
| `drawn` | bool | |
| `home_axis` | [u8; 10] | Shuffled digits; lane `l` = positions `l`, `l+5` |
| `away_axis` | [u8; 10] | |
| `prize_pool` | u64 | Fixed at the first settlement; 0 before |
| `quarter_prize` | [u64; 4] | Fixed at the first settlement |
| `quarters_settled` | u8 | 0–4 |
| `winning_box` | [u8; 4] | Per quarter; 255 until settled |
| `fees_paid` | bool | True once the first non-zero prize has been paid |
| `unpaid_prize_pool` | u64 | `prize_pool` minus prizes paid so far |
| `returned` | u32 | Bitmap over boxes: returned, split or reclaimed |
| `split_amount` | u64 | Per-box amount fixed when the pool enters `Split` or is first reclaimed after a payout |
| `cancelled_by_admin` | bool | |
| `abandoned` | bool | Set by the first `reclaim` or `reclaim_sponsorship` |
| `created_at` | i64 | |
| `locked_at` | i64 | 0 until locked |
| `bump` | u8 | |
| `vault_bump` | u8 | |
| `var_commit` | [u8; 32] | The `Var`'s `commit` as recorded by `set_var` / `replace_var`; `draw` requires `keccak(var.seed)` to equal it. Added in Step 5b out of `reserved`, here, after `vault_bump`, so no earlier offset moves |
| `reserved` | [u8; 96] | 128 through Step 5 |

`PoolStatus`: `Open = 0`, `Locked = 1`, `Drawn = 2`, `Settled = 3`, `Returned = 4`, `Split = 5`. "Live" is not a program state: clients derive it as `Drawn` with `now ≥ game.recorded_kickoff`.

Approximate size: 8 (discriminator) + 800 (owners) + ~465 (the other fields: 8 pubkeys, 10 u64s, the arrays and flags) + 128 (reserved) ≈ 1.4 KB. Rent ≈ 0.011 SOL. The vault adds ≈ 0.0009 SOL (system account) or ≈ 0.002 SOL (token account); the counter ≈ 0.001 SOL. Measured values are recorded in the Step 4 `NOTES.md`; the app's "Creation fee" label uses the measured total.

### 3.4 Vault — seeds `["vault", pool]`

- SOL pools: a system-program-owned account at the PDA, holding lamports. Transfers out are `system_program::transfer` signed with the vault seeds. It must hold its own rent-exempt minimum at all times; that minimum is paid by the creator at creation and is part of the creation fee, and it is what closes to the rent destination at `close_pool`.
- SPL pools: a token account at the PDA for the pool's mint, `owner = pool` PDA. Transfers out are `transfer_checked` signed with the pool seeds. The mint's own token program (Token or Token-2022, from `TokenRule.token_program`) is used throughout.

Vault balance invariants, checked in tests, not enforced by the program: while `Open`/`Locked`/`Drawn` before the first settlement, `sold × price + sponsored_total`; after the first settlement, `unpaid_prize_pool + dust` where `dust = prize_pool − Σ quarter_prize`.

### 3.5 `CreatorCounter` — seeds `["counter", creator, game_record]`

| Field | Type | Notes |
|---|---|---|
| `creator` | Pubkey | |
| `game` | Pubkey | |
| `open_count` | u8 | Pools in `Open` |
| `bump` | u8 | |

Created by `create_pool` when absent (creator pays rent), incremented there, decremented whenever one of the creator's pools on that game leaves `Open` (lock, first return call, admin cancel, first reclaim). Closed by `close_counter` when `open_count == 0`; rent to `fee_wallet`.

### 3.6 `WalletOverride` — seeds `["override", wallet]`

| Field | Type | Notes |
|---|---|---|
| `wallet` | Pubkey | |
| `max_open_pools` | u8 | ≥ 1 |
| `max_own_boxes` | u8 | 1 ≤ … ≤ `MAX_OWN_BOXES_ABSOLUTE` |
| `bump` | u8 | |

Admin-created. When present it replaces both config values for that wallet. The slot at `["override", creator]` is a required account of `create_pool` and `buy` (seeds-checked, so the canonical address is the only one accepted); the program reads it only when it holds this program's data and otherwise uses the config's two values. A client therefore cannot omit an existing override to escape a lower limit, and `OverrideRequired` is never raised (§8).

### 3.7 `Sponsorship` — seeds `["sponsorship", pool, wallet]`

| Field | Type | Notes |
|---|---|---|
| `pool` | Pubkey | |
| `wallet` | Pubkey | The sponsor; the only possible return destination |
| `amount` | u64 | Cumulative |
| `bump` | u8 | |

Created by the first `sponsor` from that wallet (sponsor pays rent), topped up by later ones, closed by `return_sponsorship` (return path) or `close_sponsorship` (committed path), rent to `wallet` in both cases.

## 4. Instructions

Each entry: who signs, what is checked, what changes, what is emitted. "Admin" is `config.admin`; "keeper" is `config.score_authority`. Every instruction that touches a pool also takes `config` and the pool's `game` unless stated. Every error is one from §8.

### 4.1 Administration

**`initialize(params)`** — signer: the deploying key, which becomes `admin` until changed. "The deploying key" is the program's upgrade authority, checked against the `ProgramData` account of this program (its `upgrade_authority_address`), so nobody else can create the single `["config"]` account first.
Creates `PlatformConfig` with the given fields; enforces the §3.1 invariants. Emits `ConfigUpdated`.

**`update_config(params)`** — signer: admin.
Any subset of: `admin`, `score_authority`, `entropy_provider`, `fee_wallet`, `platform_bps`, `creator_bps`, `addon_budget_bps`, `default_preset`, `max_open_pools`, `max_own_boxes`, `preseason_enabled`, `paused`, and any `TokenRule`. Enforces the §3.1 invariants; bps may go down or up but never above the constants. Emits `ConfigUpdated` with the full new config. Existing pools are untouched: their fee amounts and price are on their own account.

**`set_wallet_override(wallet, max_open_pools, max_own_boxes)`** — signer: admin. Creates or updates `WalletOverride`. Emits `OverrideSet`.

**`close_wallet_override(wallet)`** — signer: admin. Closes it; rent to admin. Emits `OverrideClosed`.

### 4.2 Games

**`create_game(key, scheduled_kickoff)`** — signer: keeper; payer: keeper.
Checks: `scheduled_kickoff > now` (`KickoffInPast`); `key.week` is 1–22, or 101–103 with `preseason_enabled`; `home != away`; both < 32 (each `InvalidGameKey`). `paused` is not consulted (§3.1). Creates `GameRecord` with `recorded_kickoff = scheduled_kickoff`, `status = Scheduled`. Emits `GameCreated`.

**`update_kickoff(new_time)`** — signer: keeper.
Checks, all required, in this order: `status == Scheduled` (`GameNotScheduled`); `quarters_posted == 0` and `now < recorded_kickoff` (both `KickoffUpdateTooLate`); `new_time > now` (`KickoffInPast`); `new_time ≤ scheduled_kickoff + KICKOFF_UPDATE_BOUND` (`KickoffOutOfBounds`; the bound is from the scheduled kickoff, never the recorded one). Sets `recorded_kickoff = new_time`. Emits `KickoffUpdated { old, new }`. Earlier moves are allowed under the same checks; `new_time == recorded_kickoff` is accepted and still emits.

**`post_scores(quarter, home, away, is_final, had_overtime)`** — signer: keeper. `quarter` is 1–4; `had_overtime` must be `false` unless `is_final`.
Checks, in this order: `status == Scheduled` (`GameNotScheduled`, so a terminal record is inert whatever the arguments); `quarter == quarters_posted + 1` (`QuarterOutOfOrder`: a repeat, a skip, 0 or above 4); if `quarter == 1`, `now ≥ recorded_kickoff + MIN_QUARTER_SECONDS`, else `now ≥ posted_at[quarter − 2] + MIN_QUARTER_SECONDS` (`QuarterTooSoon`); `home ≥ home_score[quarter − 2]` and `away ≥ away_score[quarter − 2]` when `quarter > 1` (`ScoreDecreased`); `is_final` must be `true` when `quarter == 4` and `false` otherwise, and `had_overtime` needs `is_final` (both `FinalFlagMismatch`; the fourth post is the final score, after any overtime; the keeper waits for the sources to report final). Sets the scores and `posted_at`, increments `quarters_posted`; when `quarter == 4` sets `status = Final` and `final_had_overtime = had_overtime`. Emits `ScoresPosted`.

**`mark_game(new_status)`** — signer: admin. `new_status ∈ {Postponed, Cancelled, Suspended}`; `Scheduled` and `Final` are `InvalidGameStatus`, and a discriminant outside the enum does not deserialise.
Checks: `status == Scheduled` (`GameNotScheduled` for a `Final` record, `GameAlreadyMarked` for one already marked); for `Postponed` and `Cancelled`, `quarters_posted == 0` (`InvalidGameStatus`; a game with scores can only be suspended). Sets status and `marked_at`. Emits `GameMarked`. Irreversible; a game marked in error stays marked and its pools are returned; the corrected game is a new record.

### 4.3 Pool creation and buying

**`create_pool(nonce, token, price, preset, access_type, gate_key, allowlist_root, creator_addon_bps, integrator, integrator_bps, initial_boxes)`** — signer: creator; payer: creator. Accounts: config, game, pool (init), vault (funded for SOL, created and initialised as a token account for SPL), counter (init if needed), the creator's override slot (§3.6, required, read when initialised), the SlotHashes sysvar (required; read when `initial_boxes > 0`), system program; for an SPL token also the rule's mint, the token program, and the creator's token account when `initial_boxes > 0`.
Checks: `!config.paused`; `game.status == Scheduled`; `now < game.recorded_kickoff`; `tokens[token].enabled`; `price == min_price + k × step` for some integer `k` and `price ≤ max_price`; `preset` is a valid discriminant; `access_type` valid, with `gate_key != default` iff `Link` and `allowlist_root != 0` iff `Allowlist` (until Step 8 lands gating in `buy`, `access_type` must also be `Public`, else `InvalidAccessType`); `creator_addon_bps + integrator_bps ≤ config.addon_budget_bps`; `integrator_bps == 0` iff `integrator == default`; `counter.open_count < limit.max_open_pools`; `initial_boxes ≤ limit.max_own_boxes`.
Effects: computes and stores fee amounts (§5.1); writes every field; `status = Open`; `winning_box = [255; 4]`; increments `counter.open_count`; if `initial_boxes > 0`, runs the `buy` logic (below) for the creator in the same instruction. Emits `PoolCreated`, then `BoxesBought` if boxes were bought, then `PoolLocked` if those boxes were the 25th.

**`buy(count)`** — signer: buyer; payer: buyer. Accounts: config, game, pool, vault, the creator's counter (decremented on lock), the creator's override slot (§3.6, required; read only when the buyer is the creator), the SlotHashes sysvar, system program; for an SPL pool also the pool's mint, the token program and the buyer's token account; plus `gate_key` as a co-signer when `access_type == Link`, plus a Merkle proof argument when `Allowlist` (Step 8).
Checks: `!config.paused`; `status == Open`; `now < game.recorded_kickoff`; `game.status == Scheduled`; `1 ≤ count ≤ 25 − sold`; if buyer is creator, `creator_boxes + count ≤ limit.max_own_boxes`; gating satisfied.
Effects: transfers `count × price` from buyer to vault; assigns boxes (§6.1); `sold += count`; if buyer is creator, `creator_boxes += count`; if `sold == 25`: `status = Locked`, `locked_at = now`, counter decremented. Emits `BoxesBought { buyer, boxes[], count, sold_after }` and, on lock, `PoolLocked`.

**`sponsor(amount)`** — signer: sponsor; payer: sponsor. Accounts: config, game, pool, vault, sponsorship (init if needed), system program; for an SPL pool also the pool's mint, the token program and the sponsor's token account.
Checks: `!config.paused`; `status ∈ {Open, Locked, Drawn}` (a full pool is usually drawn well before kickoff and can still be sponsored), else `PoolNotOpen`; `now < game.recorded_kickoff`; `game.status == Scheduled`; `amount ≥ price`; `sponsored_total + amount ≤ tokens[token].max_sponsorship` (the cap as configured now, not as it was at creation).
Effects: transfers `amount` to the vault; creates `Sponsorship` (increment `sponsor_count`, `sponsorships_open`) or adds to it; `sponsored_total += amount`. Emits `Sponsored { sponsor, amount, sponsored_total }`.

**`rotate_gate_key(new_key)`** — signer: creator (`Unauthorized` otherwise). Checks `access_type == Link` (`InvalidAccessType`), `new_key != default` (`GateKeyMissing`). Emits `GateKeyRotated` (the new key is public information; only signatures from it matter).

A wrong or missing client-side account — an SPL pool without its mint, token account or token program, a token account of another mint, a token program other than the pool's, a pool, vault or counter at a non-canonical address — fails with Anchor's own constraint errors (`ConstraintAccountIsNone`, `ConstraintTokenMint`, `RequireKeysEqViolated`, `ConstraintSeeds`, `ConstraintHasOne`, `InvalidProgramId`), not with a §8 code.

### 4.4 Draw

What Entropy is (read from `regolith-labs/entropy` at `f26ae03`, the commit `verify.osec.io` reports for Regolith's deployment `3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X`, and confirmed against that bytecode on 2026-10-07), and which deployment this program uses: the platform's own, a fork of that commit with `Open` re-enabled, the program id changed and the `security.txt` contact changed, nothing else (ARCHITECTURE › Randomness). `ENTROPY_PROGRAM` names it from Step 5b; the Step 5 instructions below were built and tested against Regolith's bytecode, which is identical in everything they touch.

- `Var` account, 240 bytes: an 8-byte steel discriminator (`EntropyAccount::Var = 0`, so all zero), then the `#[repr(C)]` struct, in order and naturally aligned: `authority: Pubkey` (offset 8), `id: u64` (40), `provider: Pubkey` (48), `commit: [u8;32]` (80), `seed: [u8;32]` (112), `slot_hash: [u8;32]` (144), `value: [u8;32]` (176), `samples: u64` (208), `is_auto: u64` (216), `start_at: u64` (224), `end_at: u64` (232). PDA seeds `["var", authority, id.to_le_bytes()]`. There is no field recording when or by whom it was sampled. The program declares this layout and the `Sample` instruction data in its own module rather than depending on the `entropy-api` crate (which is on an older `solana-program` major than Anchor 1.2); a test decodes a `Var` fetched from mainnet (ORE's `BWCaDY96Xe4WkFq1M7UiCCRcChsJ3p51L5KrGzhxgm2E`) against the declaration.
- `Open(id, commit, is_auto, samples, end_at)`, discriminator 0, accounts `[authority (w, s), payer (w, s), provider (w), var PDA (w), system]`: the provider is **not** required to sign (the check is commented out in Regolith's source and the fork leaves it so), so the Entropy program does not prove the commit came from the provider; `set_var`'s provider check is a label, and the commit binding below is what makes a self-made commit worthless. `end_at` must be in the future. Disabled in Regolith's deployment (the dispatcher arm is commented out; it fails with `InvalidInstructionData`); enabled in the platform's.
- `Sample`, discriminator 5, accounts `[signer (s), var (w), SlotHashes]`: any signer, once `Clock.slot ≥ end_at`; silent no-op if already sampled. Reads the `SlotHashes` sysvar for `end_at`. **If that slot is no longer in the sysvar (older than 512 slots — about 100 seconds at the 200 ms slot time mainnet runs from epoch 1052; windows here are counted in slots, not seconds), it records `keccak(end_at.to_le_bytes())` instead** — a value anyone could compute at `Open`. A `Var` sampled that way is not random and must never be drawn on. About 127k compute units on the deployed bytecode (it decodes the whole sysvar).
- `Reveal(seed)`, discriminator 4, accounts `[signer (s), var (w)]`: any signer, once `Clock.slot ≥ end_at` and `slot_hash != 0`; silent no-op if the seed is already set; requires `keccak(seed) == commit` (else `InvalidInstructionData`); sets `value = keccak(slot_hash ‖ seed ‖ samples.to_le_bytes())`.
- `Next(end_at)`, discriminator 2: authority only; rolls `commit = seed` and clears the rest for the next value. `Close`, discriminator 1, accounts `[signer (w, s), var (w), system]` — three accounts, the processor's `let [signer_info, var_info, system_program] = accounts`, not the two Regolith's `sdk::close` builds: authority only, returns the rent.

Entropy flow, in order: the keeper, acting as the provider named in `config.entropy_provider`, generates a 32-byte seed and stores it durably before anything is sent; opens a `Var` on `ENTROPY_PROGRAM` (authority = provider = keeper, `commit = keccak(seed)`, `samples = 1`, `is_auto = 0`, `end_at` ≈ 150 slots ahead); calls `set_var`, which records the `Var`, its `end_at` and its `commit` on the pool; at `end_at` someone — the keeper first, anyone else if it is late — calls `sample_var` (this program CPIs `Sample` and verifies the hash); the keeper calls Entropy `Reveal(seed)` directly; the keeper calls `draw`; then closes the `Var` through Entropy to recover its rent. `sample_var` and Entropy's `Sample` and `Reveal` are permissionless, so the SDK exposes all three and the public verify page offers them: if the keeper is down, anyone can keep a pool's draw honest by sampling in the window (a reveal needs the seed, which only the keeper has, so a reveal from elsewhere happens only if the keeper has published the seed by other means).

**`set_var(var)`** — signer: keeper.
Checks: `status == Locked` (`PoolNotLocked`); `pool.var == default` (`VarAlreadySet`); `var` is owned by `ENTROPY_PROGRAM`, 240 bytes long with a zero discriminator (`VarNotEntropy`); `var.provider == config.entropy_provider` (else `VarProviderMismatch`; a label the opener sets, not a proof — it keeps the verify page honest and tells an auditor which key holds the seed, while the commit binding and the rules in ARCHITECTURE › Randomness are what make a self-made commit useless); `var.commit != 0`; `var.seed == 0`, `var.slot_hash == 0`, `var.value == 0` (committed, unsampled, unrevealed); `var.samples == 1`; `var.is_auto == 0`; `var.end_at > Clock.slot` (each `VarNotFresh`). Sets `pool.var`, `var_end_at = var.end_at`, and (Step 5b) `var_commit = var.commit`. Emits `VarSet`. One call per pool; the admin's `replace_var` is the only way to change it.

**`sample_var()`** — signer: anyone (the CPI's signer and nothing else; no `config` account). Accounts: pool, the recorded `var`, `SlotHashes` sysvar, Entropy program (`address = ENTROPY_PROGRAM`, a constant, never a caller-supplied key).
Checks: `status == Locked`; `pool.var != default` (`VarNotSet`); `var == pool.var` (`VarMismatch`); `sampled_slot == 0` (`VarAlreadySampled`); `Clock.slot ≥ var.end_at` (`SampleTooEarly`). If `var.slot_hash == 0`, CPIs Entropy `Sample`, then re-reads the `Var` from the account's live buffer — with Anchor's typed accounts this is `reload()`; with an `UncheckedAccount` and the program's own decoder it is a second decode. Either way the copy from before the CPI is never reused: without the re-read the program would see a zero `slot_hash` and fail the check below against stale data (OtterSec's `missing_account_reload` lint catches exactly this; the Step 5 suite has a test for it). Then, whether this call sampled or someone else did, reads `SlotHashes` itself and requires that it still contains `var.end_at` and that `var.slot_hash` equals that entry (else `SampleWindowMissed`); this is what proves the recorded hash is the real slot hash and not the `keccak(end_at)` fallback. Sets `sampled_slot = Clock.slot`, `sampled_hash = var.slot_hash`. Emits `VarSampled`. Permissionless because the sampling window is the one place a single absent party could otherwise decide a draw's fate: with anyone able to sample, neither a keeper outage nor a keeper that would rather miss the window can leave a pool unsampled while the verify page is open.

**`draw()`** — signer: keeper. Accounts: pool, the recorded `var`.
Checks: `status == Locked`; `!drawn` (`AlreadyDrawn`); `pool.var != default` (`VarNotSet`); `var == pool.var` (`VarMismatch`); `sampled_slot != 0` (`VarNotSampledHere`); `var.slot_hash == pool.sampled_hash` (the `Var` has not been rolled with `Next` or re-opened; `VarNotSampledHere`); `var.slot_hash != keccak(var.end_at.to_le_bytes())` (defence in depth against the fallback, `VarFallbackHash`); `var.seed != 0` and `var.value != 0` (revealed, `VarNotRevealed`); (Step 5b) `keccak(var.seed) == pool.var_commit` (`VarCommitMismatch`) and `var.samples == 1` (`VarNotFresh`); `var.value == keccak(var.slot_hash ‖ var.seed ‖ var.samples.to_le_bytes())` (recomputed, `VarNotRevealed` if not).
Effects: derives both axes (§6.2) from `var.value`; `drawn = true`; `status = Drawn`. Emits `DigitsDrawn { home_axis, away_axis, var, value }`.
Why the two Step 5b checks: with them, `value` is determined by the commit this program recorded before the end slot, the slot hash this program verified against `SlotHashes`, and a seed whose keccak is that commit — three things the Entropy program has no say in. The draw's outcome therefore does not depend on the Entropy deployment behaving, which is what lets the platform operate that deployment (and hold its upgrade authority) without the draw's fairness resting on it.

**`replace_var(new_var)`** — signer: admin.
Checks: `status == Locked`; `!drawn`; `var_replacements < 2` (else `TooManyVarReplacements`); `sampled_slot == 0` (else `VarAlreadySampled`: a `Var` with a verified sample is never abandoned — its value is fixed and only the reveal is outstanding, which the provider can always do; the only reason left to replace is a missed window); `new_var != pool.var` (`VarAlreadySet`); `new_var` passes every `set_var` check from the layout check onward. Sets `pool.var = new_var`, `var_end_at`, (Step 5b) `var_commit`, `var_replacements += 1`; `sampled_slot` and `sampled_hash` are already zero. Emits `VarReplaced { old, new }`. Never callable by the keeper. Why a cap: a `Var` only ever needs replacing when the sample window was missed; a third miss is an incident, not bad luck, and the admin's remaining move is `cancel_pool` (full return). Without a cap, a party who knew a seed and could delay sampling could re-roll until a value suited them; with it, that party gets at most three tries, every try is an event on a public verify page, and since anyone can sample in the window, delaying the sample is not in any one party's gift. Why no replacement after a sample: once the slot hash is on the `Var` the value is decided, and a provider who could swap the `Var` at that point could choose between outcomes; refusing it leaves a provider who dislikes the value exactly one move, `cancel_pool` and a full return, which gains nothing.

### 4.5 Settlement

**`settle(quarter)`** — signer: keeper. `quarter` 1–4. Accounts: config, game, pool, vault, winner wallet (and ATA for SPL), `fee_wallet` (and ATA), creator (and ATA), integrator (and ATA) when set, token program, associated-token program, system program. The keeper is the payer for any ATA creation.
Checks: `status == Drawn`; `quarter == quarters_settled + 1`; `game.quarters_posted ≥ quarter`; the passed winner account equals `owners[winning box]` (§6.3), so the winner is computed in the program and the passed account is only verified against it; the fee, creator and integrator accounts match the pool and config.
Effects, in one transaction:
1. If `quarters_settled == 0`: `prize_pool = 25 × price − platform_fee − creator_fee − integrator_fee + sponsored_total`; `quarter_prize[q] = floor(prize_pool × split[q] / 100)` for all four; `unpaid_prize_pool = prize_pool`.
2. `winning_box[quarter − 1] = box`.
3. If `quarter_prize[quarter − 1] > 0`: transfer it to the winner; `unpaid_prize_pool −= it`. If additionally `!fees_paid`: transfer `platform_fee` to `fee_wallet`, `creator_fee` to `creator`, `integrator_fee` to `integrator` (skipped when zero), and set `fees_paid = true`.
4. `quarters_settled += 1`; if it reaches 4, `status = Settled`.
Emits `QuarterSettled { quarter, home, away, box, winner, amount, fees_paid_now: bool, platform_fee, creator_fee, integrator_fee }`. A zero-share quarter (Q1–Q3 on `FinalOnly`) still records the winning box and emits the event with `amount = 0`; it moves no funds and does not pay fees.

Idempotency: a repeated `settle` for the same quarter fails on the ordering check, so the keeper can retry blindly after a timeout.

**`close_pool()`** — permissionless. Accounts: pool, vault, game, counter (if it exists), `fee_wallet`, creator, config.
Checks: `status ∈ {Settled, Returned, Split}`; `sponsorships_open == 0`; if `Returned` or `Split`, every sold box has its `returned` bit set. Effects: transfers the vault's remaining balance (dust, and for SOL the vault's rent) and the pool account's rent to the destination: `creator` if `abandoned`, else `fee_wallet`. Closes the vault (SPL: `close_account` after the token balance is swept) and the pool. Emits `PoolClosed { destination, dust }`.

**`close_counter()`** — permissionless (no signer). Accounts: counter, config (`has_one = fee_wallet`), `fee_wallet`. Checks `open_count == 0` (`CounterNotEmpty`). Rent to `fee_wallet`. No event. Built in Step 4, because that step's acceptance needs the counter to close at zero.

### 4.6 Returns, splits, cancellation, reclaim

Returns are executed per owner set so a full pool fits in a few transactions. All of them are idempotent through the `returned` bitmap.

**`return_boxes()`** — signer: keeper. Remaining accounts: a list of owner wallets (each followed by its ATA for SPL). Payer for ATA creation: keeper.
Precondition, one of:
- unfilled: `status == Open` and `now ≥ game.recorded_kickoff`;
- marked: `game.status ∈ {Postponed, Cancelled}` and `!fees_paid`;
- suspended before any payout: `game.status == Suspended` and `!fees_paid`;
- cancelled: `cancelled_by_admin`;
- already returning: `status == Returned` and `!fees_paid` (continuation calls).
Effects: on the first call (`status != Returned`): if `status == Open` decrement the counter; set `status = Returned`. Then for every box `b` with `owners[b]` in the passed set and bit `b` clear: transfer `price`, set bit `b`. One transfer per owner (sum of their boxes). Emits `BoxesReturned { owner, boxes[], amount }` per owner.

**`return_sponsorship()`** — signer: keeper. Accounts: pool, vault, sponsorship, `sponsorship.wallet` (and ATA).
Checks: `status == Returned` and `!fees_paid`; the destination account equals `sponsorship.wallet`. Effects: transfer `amount`; close the `Sponsorship` with rent to the wallet; `sponsorships_open −= 1`. Emits `SponsorshipReturned`.

**`cancel_pool()`** — signer: admin. Checks: `status ∈ {Open, Locked, Drawn}`; `!fees_paid`. Effects: `cancelled_by_admin = true`; if `Open`, decrement counter; `status = Returned`. Emits `PoolCancelled`. The keeper then runs `return_boxes` and `return_sponsorship`.

**`split()`** — signer: keeper. Remaining accounts as for `return_boxes`.
Checks: `game.status == Suspended`; `fees_paid`; `status ∈ {Drawn, Split}`. Effects: on the first call, `split_amount = floor(unpaid_prize_pool / 25)`, `status = Split`. For each unreturned box owned by a passed owner: transfer `split_amount`, set the bit. Emits `BoxesSplit { owner, boxes[], amount }`. Sponsorships are inside `unpaid_prize_pool` and go with it; `return_sponsorship` fails because `fees_paid`.

**`reclaim()`** — signer: the box owner. Accounts: pool, vault, game, owner (and ATA), counter if the pool is `Open`.
Checks: `status != Settled`; `now ≥ game.scheduled_kickoff + RECLAIM_DELAY`; the signer owns at least one unreturned box. (A pool already in `Returned` or `Split` whose keeper never finished the batches is covered too: after 30 days the owner takes what the pool already owes them, at the amount the pool already fixed.)
Effects: if `status ∈ {Open, Locked, Drawn}` (the pool was never resolved): `abandoned = true`; if `status == Open` decrement the counter; if `!fees_paid`, `status = Returned`, else `status = Split` and `split_amount = floor(unpaid_prize_pool / 25)`. Then for every unreturned box the signer owns: transfer `price` (if `!fees_paid`) or `split_amount`, set the bit. Emits `BoxesReclaimed`.

**`reclaim_sponsorship()`** — signer: the sponsor. Checks: `!fees_paid`; `now ≥ scheduled_kickoff + RECLAIM_DELAY`; `status != Settled`. Effects: as the unresolved-pool step above if needed; transfer `amount` to `sponsorship.wallet`; close the account to the wallet; `sponsorships_open −= 1`. Emits `SponsorshipReturned`.

**`close_sponsorship()`** — permissionless. Checks: `status ∈ {Settled, Split}` (the sponsorship is committed and the pool is terminal). Closes the account with rent to `sponsorship.wallet`; `sponsorships_open −= 1`. Emits `SponsorshipClosed`.

## 5. Money

### 5.1 Fee amounts, fixed at creation

```
P              = 25 × price
platform_fee   = floor(P × config.platform_bps / 10_000)
creator_fee    = floor(P × (config.creator_bps + creator_addon_bps) / 10_000)
integrator_fee = floor(P × integrator_bps / 10_000)      // 0 when unset
```

Stored on the pool; config changes later never touch them.

### 5.2 Prizes, fixed at the first settlement

```
prize_pool       = P − platform_fee − creator_fee − integrator_fee + sponsored_total
quarter_prize[q] = floor(prize_pool × split[q] / 100)
dust             = prize_pool − Σ quarter_prize      // ≤ 3 base units on any preset
```

`sponsored_total` cannot change after kickoff (`sponsor` requires `now < recorded_kickoff`) and the first settlement cannot happen before kickoff + 15 minutes, so computing prizes at the first settlement is well defined.

### 5.3 Where money goes, by outcome

| Outcome | Buyers | Sponsors | Platform | Creator | Integrator |
|---|---|---|---|---|---|
| Settled | Prizes to winners | In the prizes | `platform_fee` + dust + pool/vault rent at close | `creator_fee` | `integrator_fee` |
| Returned before any payout | `price` per box | Full amount + account rent | Pool/vault rent at close; counter rent | Nothing | Nothing |
| Suspended after a payout (Split) | Paid prizes stay; `unpaid_prize_pool / 25` per box | Committed (inside the split) | Fees already taken + dust + rent | Fee already taken | Fee already taken |
| Abandoned, never paid | `price` per box via `reclaim` | Full amount via `reclaim_sponsorship` | Counter rent only | Pool/vault rent + dust at close | Nothing |
| Abandoned, partly paid | `unpaid_prize_pool / 25` via `reclaim` | Committed | Fees already taken; counter rent | Fee already taken; pool/vault rent + dust | Fee already taken |

Worked numbers (SOL, 0.05 per box, 2% creator add-on, `Standard`): `P` 1.25; fees 0.0625 / 0.0875 / 0; prize pool 1.1; quarters 0.22 / 0.22 / 0.22 / 0.44; vault 1.25 → 0.88 after Q1. With a 1 SOL sponsorship: prize pool 2.1; quarters 0.42 / 0.42 / 0.42 / 0.84; vault 2.25 → 1.68 after Q1. Smallest possible quarter prize: 0.05 SOL boxes, 15% total fee, 20% share = 0.2125 SOL.

### 5.4 Token transfers

- SOL: `system_program::transfer` from the vault PDA (signed with vault seeds) to the recipient wallet. Recipient accounts are plain system accounts; no ATA logic.
- SPL: `transfer_checked` from the vault token account (authority = pool PDA, signed with pool seeds) to the recipient's associated token account for the pool's mint, created idempotently in the same instruction when missing, with the transaction payer covering its rent. Token-2022 mints are handled through the same interface; the program never assumes the legacy Token program. Mints with transfer fees or transfer hooks are not supported: `update_config` rejects a `TokenRule` whose mint has either extension.

## 6. Algorithms

Reference implementations live in `packages/shared` and are cross-tested against the program with shared vector files. `sha256` is the Solana `hashv` syscall.

### 6.1 Box assignment

Inputs: `slothash` = the 32-byte hash of the most recent entry in the SlotHashes sysvar (the entry for the slot the transaction lands in, as measured on Surfpool; a client reproducing a purchase reads the sysvar's entry for the transaction's own slot); `buyer` (32 bytes); `sold` (u8, before this purchase); `count` (u8).

```
seed      = sha256(slothash || buyer || [sold] || [count])
remaining = [b for b in 0..25 if owners[b] == default]    // ascending
for k in 0..count:
    r   = sha256(seed || [k])
    idx = u64_le(r[0..8]) mod len(remaining)
    box = remaining.remove(idx)                              // removes and shifts
    owners[box] = buyer
```

Deterministic given the inputs; `packages/shared` can reproduce a purchase from the transaction's slot for tests. Predictability is harmless: positions carry no value before the draw.

### 6.2 Axis shuffle

Inputs: `value` = the 32-byte revealed Entropy value.

```
fn axis(value, label):                 // label = b"home" or b"away"
    seed = sha256(value || label)
    a = [0,1,2,3,4,5,6,7,8,9]
    for i in 9 down to 1:
        r = sha256(seed || [i])
        j = u64_le(r[0..8]) mod (i + 1)
        swap(a[i], a[j])
    return a
home_axis = axis(value, b"home"); away_axis = axis(value, b"away")
```

Lane `l` (0–4) of an axis covers digits `a[l]` and `a[l + 5]`. Each lane covers exactly two digits and the five lanes partition 0–9.

### 6.3 Winner

Inputs: `home`, `away` cumulative scores for the quarter (u16), both axes.

```
hd = home mod 10; ad = away mod 10
col = the l in 0..5 with hd in {home_axis[l], home_axis[l+5]}
row = the l in 0..5 with ad in {away_axis[l], away_axis[l+5]}
box = row × 5 + col          // 0–24; label = box + 1
```

Columns are the home team (across the top), rows the away team (down the side). Exactly one box matches for any pair of scores.

## 7. Events

All events are additive: fields are appended, never removed or reordered. Every event carries `pool` (or `game` for game events) and the Unix time.

Events are emitted with Anchor's `emit_cpi!` (a self-CPI whose instruction data is the event), not `emit!` (program logs). Logs are truncated by the runtime when a transaction logs too much, and the indexer, the app and third parties treat `QuarterSettled` as the only proof of a result, so a result must never be lost to truncation. ORE does the same with its `Log` instruction for the same reason. `emit_cpi!` adds the event-authority PDA and the program itself to every emitting instruction's account list; the SDK supplies them.

| Event | Fields |
|---|---|
| `ConfigUpdated` | full config snapshot |
| `OverrideSet` / `OverrideClosed` | wallet, values |
| `GameCreated` | game, key, scheduled_kickoff |
| `KickoffUpdated` | game, old, new |
| `ScoresPosted` | game, quarter, home, away, is_final, had_overtime |
| `GameMarked` | game, status |
| `PoolCreated` | pool, game, creator, token, mint, price, preset, access_type, creator_addon_bps, integrator, integrator_bps, platform_fee, creator_fee, integrator_fee |
| `BoxesBought` | pool, buyer, boxes (0-based indices), count, sold_after |
| `PoolLocked` | pool, locked_at |
| `Sponsored` | pool, sponsor, amount, sponsored_total |
| `GateKeyRotated` | pool |
| `VarSet` | pool, var, end_at, commit (Step 5b) |
| `VarReplaced` | pool, old_var, new_var, end_at, replacements, commit (Step 5b) |
| `VarSampled` | pool, var, sampler, slot, end_at, slot_hash |
| `DigitsDrawn` | pool, var, value, home_axis, away_axis |
| `QuarterSettled` | pool, quarter, home, away, box, winner, amount, fees_paid_now, platform_fee, creator_fee, integrator_fee |
| `PoolCancelled` | pool |
| `BoxesReturned` / `BoxesSplit` / `BoxesReclaimed` | pool, owner, boxes, amount |
| `SponsorshipReturned` / `SponsorshipClosed` | pool, sponsor, amount |
| `PoolClosed` | pool, destination, dust |

Clients show a box as **won** only on `QuarterSettled`; nothing else is a result.

## 8. Errors

Numbered from 6000 (Anchor custom errors). Names are the contract; numbers follow declaration order and are frozen once the program ships.

`Unauthorized`, `Paused`, `GameNotScheduled`, `GameAlreadyMarked`, `SalesClosed`, `KickoffInPast`, `KickoffOutOfBounds`, `KickoffUpdateTooLate`, `QuarterOutOfOrder`, `QuarterTooSoon`, `ScoreDecreased`, `FinalFlagMismatch`, `TokenDisabled`, `PriceOffLadder`, `InvalidPreset`, `InvalidAccessType`, `GateKeyMissing`, `GateKeyNotSigner`, `AllowlistProofInvalid`, `AddonBudgetExceeded`, `IntegratorMismatch`, `OpenPoolLimit`, `OwnBoxLimit`, `OverrideRequired`, `NothingToBuy`, `TooManyBoxes`, `PoolNotOpen`, `PoolNotLocked`, `PoolNotDrawn`, `SponsorshipTooSmall`, `SponsorshipCapExceeded`, `VarAlreadySet`, `VarNotSet`, `VarMismatch`, `VarNotEntropy`, `VarProviderMismatch`, `VarNotFresh`, `VarNotRevealed`, `VarNotSampledHere`, `SampleWindowMissed`, `VarFallbackHash`, `TooManyVarReplacements`, `AlreadyDrawn`, `ScoresNotPosted`, `WinnerMismatch`, `FeeAccountMismatch`, `NotReturnable`, `FeesAlreadyPaid`, `NotSuspended`, `NotSplittable`, `ReclaimTooEarly`, `NotOwner`, `NothingToReturn`, `SponsorshipsStillOpen`, `BoxesStillOutstanding`, `PoolNotTerminal`, `CounterNotEmpty`, `UnsupportedMintExtension`, `MathOverflow`, `InvalidConfig`, `InvalidGameKey`, `InvalidGameStatus`, `SampleTooEarly`, `VarAlreadySampled`, `VarCommitMismatch`.

`InvalidConfig` (6059) is the error for a violated §3.1 or §3.6 invariant on any write (`initialize`, `update_config`, `set_wallet_override`), and for a `TokenRule` whose shape does not fit its index (index 0 is native SOL with the default mint and program; a rule with the default mint elsewhere is a disabled placeholder; a rule with a mint needs that mint account passed, matching key, owner and decimals). `default_preset` outside 0–2 is `InvalidPreset`; a transfer-fee or transfer-hook mint is `UnsupportedMintExtension`. It was added in Step 2, after the list above had been written and before anything shipped, so every other number is unchanged.

`InvalidGameKey` (6060) is the error for a `create_game` key that breaks the §2 rules (`week` outside 1–22 and not a preseason week with `preseason_enabled`; a team index of 32 or more; `home == away`). `InvalidGameStatus` (6061) is the error for a `mark_game` whose `new_status` is not a mark (`Scheduled`, `Final`) or is `Postponed`/`Cancelled` once a quarter has been posted. Both were added in Step 3, the same way, before anything shipped.

`SampleTooEarly` (6062) is `sample_var` before `var.end_at`, and `VarAlreadySampled` (6063) is a second `sample_var` on a pool with a sample recorded, or `replace_var` on one (§4.4). Both added in Step 5. `VarCommitMismatch` (6064) is `draw` on a `Var` whose revealed `seed` does not hash to the commit `set_var` recorded on the pool; added in Step 5b with the `var_commit` field.

`OverrideRequired` (6023) is reserved and never raised: the override slot is a required account of `create_pool` and `buy` (§3.6), so there is no way to omit it. The number stays because the list is frozen in declaration order. Client-side account mistakes on the §4.3 instructions fail with Anchor's own constraint errors, not with a code from this list (see the end of §4.3).

## 9. State machines

### Game

```
Scheduled --mark--> Postponed | Cancelled      (only while quarters_posted == 0)
Scheduled --mark--> Suspended                  (any time before Final)
Scheduled --post_scores(4)--> Final
Scheduled --update_kickoff--> Scheduled   (only while quarters_posted == 0 and now < recorded_kickoff)
Scheduled --post_scores(1..3)--> Scheduled
```

Terminal: `Postponed`, `Cancelled`, `Suspended`, `Final`.

### Pool

```
Open   --buy (25th)-------------> Locked
Open   --return_boxes (kickoff passed) | cancel_pool | return_boxes (game marked) | reclaim--> Returned
Locked --set_var, sample_var, draw--> Drawn
Locked --cancel_pool | return_boxes (game marked / suspended, no payout) | reclaim--> Returned
Drawn  --settle × 4--> Settled
Drawn  --cancel_pool | return_boxes (game marked / suspended, no payout) | reclaim (no payout)--> Returned
Drawn  --split (suspended, after payout) | reclaim (after payout)--> Split
Settled | Returned | Split --close_pool--> (account closed)
```

Terminal: `Settled`, `Returned`, `Split`. `sponsor` is allowed in `Open`, `Locked` and `Drawn` while `now < recorded_kickoff`.

## 10. Invariants the test suite must prove

- Money: at every point, vault balance = the §3.4 invariant; total out over a pool's life = total in; no instruction can move funds to an account other than a box owner, a sponsor's recorded wallet, `fee_wallet`, `creator`, or `integrator`.
- Ordering: quarters settle in order; scores post in order and never decrease; no settlement without a draw; no draw without a sampled, revealed `Var` recorded on the pool whose seed hashes to the commit recorded there; no `Var` replaced once a sample is recorded.
- Timing: no `buy`/`sponsor` at or after `recorded_kickoff`; no `return_boxes` (unfilled) before it; no `update_kickoff` once it has passed; no `reclaim` before `scheduled_kickoff + 30 d` regardless of `update_kickoff`.
- Authority: every admin instruction fails with the keeper's key and vice versa; `replace_var` and `mark_game` and `cancel_pool` are admin-only; the keeper never supplies a destination the program doesn't verify.
- Limits: price ladder, presets, add-on budget, open-pool and own-box caps with override precedence, sponsorship minimum and cap.
- Fees: independent of `sponsored_total`; paid exactly once; never paid on a returned pool; `FinalOnly` pays them with the final.
- Rent: every closable account closes to the specified destination; nothing is left un-closable in any terminal state.
- Layout: a snapshot test freezes account byte offsets and event schemas after Step 8; changes are additive only.
- Composability: no instruction reads the instructions sysvar or depends on its position in the transaction or on its neighbours. Wallets add instructions of their own — Seed Vault Wallet appends Lighthouse assertion instructions after the dApp's — so the test suite runs every user-facing instruction with unrelated instructions before and after it (a memo, a compute-budget change, a Lighthouse-style assertion against the signer's balance) and expects identical results.
- Account types, so that nothing locks funds by over-checking: a wallet that is only a PDA seed or a payout destination (`creator`, `integrator`, `fee_wallet`, a box owner, `sponsorship.wallet`) is `UncheckedAccount` with an `address`/`has_one` constraint in every instruction after the one that creates the account it seeds, never `SystemAccount` — if that wallet's owner ever changes (it becomes a token account, say), `close_pool` and the return paths must still work (OtterSec's `overconstrained_seed_account` lint). Every CPI target (`system_program`, the token programs, `ENTROPY_PROGRAM`) is a fixed address, never a caller-supplied program (`arbitrary_cpi_call`). Every account whose data is read after a CPI is reloaded first (`missing_account_reload`). The full `anchor-lints` set runs in CI and must be clean.
