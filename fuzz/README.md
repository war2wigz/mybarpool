# mybarpool-fuzz

Property tests for the program, seeded from the unit suite. Every Mollusk test in
`programs/mybarpool/tests` can eject the instruction it ran as a fixture; the harness replays
each fixture unchanged, then draws mutated variants of them under `proptest` and checks the
PROGRAM §10 Money oracle on every result. The bytecode under test is the same
`target/deploy/mybarpool.so` the unit and localnet suites run.

## Eject

```sh
anchor build --arch v3
EJECT_FUZZ_FIXTURES="$PWD/target/fuzz-fixtures" cargo test -p mybarpool --locked --features fuzz-fixtures
```

One `.fix` file per `process_instruction` call (about 770, 200 MB; `target/` is ignored by
git). The path must be absolute: a relative one resolves against the package directory. The
`fuzz-fixtures` feature turns on Mollusk's `fuzz` feature for the program's dev-dependencies
and is never on in `anchor build`. The fixture crate's build script needs `protoc` on `PATH`
(`apt-get install protobuf-compiler`).

## Run

```sh
RUST_LOG=error cargo run -p mybarpool-fuzz --release --locked --features runner -- \
  --fixtures target/fuzz-fixtures --filter all --seconds 600
```

| Flag | Meaning |
|---|---|
| `--fixtures <dir>` | The ejected fixtures (required). |
| `--filter buy\|settle\|all` | Which instruction's seeds to mutate; `all` is every instruction of this program (default). |
| `--seconds <n>` / `--cases <n>` | The budget: wall-clock seconds or a case count (default 1,000 cases). |
| `--seed <u64>` | Reproduce a run exactly; every run prints the seed it used. |
| `--program`, `--entropy` | The program ELF and the Entropy fork fixture (defaults are the repository paths). |
| `--inspect <fixture.fix>` | Print one fixture: the instruction, its metas, every account before and after (pools and token accounts decoded, §3.4's amount for each pool), the recorded result. |

`RUST_LOG=error` silences the SVM's per-instruction program logs; without it every run
prints them.

The run replays every selected seed first and compares `program_result` and every resulting
account with the recorded effects; a mismatch is a harness or determinism bug and stops the
run. Then each case picks one seed and one to three mutations:

- **data** — flip a byte after the discriminator; truncate; append 1–64 bytes; set the `count`
  byte to 0, 26 or 255; for `buy`, replace the proof with a random-length vector.
- **signer** — clear `is_signer` on a meta that had it; set it on one that did not.
- **alias** — give one meta another meta's key (the duplicate-account attack); give one a fresh
  key backed by an empty system account.
- **lamports** — set a non-vault account's lamports anywhere below `u64::MAX / 2`; set the
  vault's between zero and twice its recorded value.
- **data-plant** — write a random byte at a random offset of the pool, game or config account.
- **clock** — shift the `Clock` sysvar's `unix_timestamp` by up to sixty days either way.

## Oracle

Checked on every mutated run (`src/oracle.rs`):

- **O1 no crash** — the program never fails to complete (a panic, an abort or compute
  exhaustion); every failure is a `Custom(code)` or one of the runtime's own account checks.
- **O2 atomic** — on any failure every resulting account equals its input (lamports, data, owner).
- **O3 money, on success** — lamports are conserved over the account set; per mint, token
  amounts are conserved; the accounts whose lamports or token balance increased are a subset of
  the destinations the instruction allows, read from the *resulting* pool and config: `buy` the
  vault only; `settle` the winner, `fee_wallet`, `creator`, `integrator` or their token
  accounts; every other instruction a box owner, a sponsor's recorded wallet, `fee_wallet`,
  `creator`, `integrator`, the vault, or a state account receiving its rent.
- **O4 vault invariant, on success** — PROGRAM §3.4 holds for every pool in the result whose
  vault is in the context, whenever the input satisfied it: the fuzzer plants vaults and money
  fields anywhere, and a pool that was already off §3.4, or that breaks a §3.3 relation the
  program maintains (`sold` is the number of owned boxes, `integrator` iff `integrator_bps` iff
  `integrator_fee`, `prize_pool = pot − fees + sponsored_total` once a quarter is settled,
  `returned` bits only in `Returned`/`Split`, …), is a state the program never writes and is
  counted as *not applicable* rather than judged. The run prints both counts.
- **O5 `buy` relations, on success** — `sold` grew by `count`; exactly `count` boxes went from
  default to the buyer and nothing else in `owners` changed; the own-box cap with override
  precedence; `Locked` iff `sold == 25`, then `locked_at = now` and the counter decremented.

## What is not a transaction

Mollusk trusts the `is_signer` flags (signature verification belongs to the transaction
layer), so a mutation that makes a PDA — or an off-curve random key — a signer asks the SVM a
question no transaction can ask. The `signer`, `alias` and `fresh` mutations therefore only
produce signers that can sign, and a case whose mutation still introduced an off-curve signer
is counted and skipped. The unit suite's own signers are `Pubkey::new_unique()` keys, off the
curve about half the time; those are the tests' premise and are left alone.

Mollusk's post-execution rent check (on by default) is also a model difference: the program
succeeded and an account ended below its rent-exempt minimum, where the runtime would reject
the whole transaction with nothing changed. The harness reports it as
`AccountNotRentExempt (post-execution rent check)` and does not evaluate O2 for it.

## Reproduce a failure

A violation prints the fixture file, the instruction, the mutations (after shrinking), the
plan as data, the result and the oracle lines, and exits 1. Re-run with the printed
`--seed` for the same sequence, or apply the printed plan to the fixture by hand.

## Harness correctness

`tests/oracle.rs` plants three bugs in synthetic outcomes — a success that pays a stranger
(O3), a success that leaves the vault one lamport short (O4), a failure that changed an
account (O2) — and checks each is caught while its healthy twin passes. `cargo test -p
mybarpool-fuzz` runs them without Mollusk or `protoc`.

## Why the library is pure

The library (`model`, `seeds`, `mutate`, `oracle`) has no Mollusk dependency, so
`cargo dylint --workspace -- --lib` lints it with the program and the oracle tests build
anywhere. Mollusk and the fixture crate sit behind the `runner` feature the binary requires:
they are Agave 4.3 crates whose `rust-version` the lint job's pinned nightly cannot meet.
