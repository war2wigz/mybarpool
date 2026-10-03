# Changelog

Every mainnet deploy of the program is listed here with its commit, the verified build hash and what changed. Breaking changes to account layouts, instructions or events are announced here at least one NFL week before they are deployed.

## Unreleased

- Scaffold (build plan Step 0): Anchor 1.2.0 workspace with an empty `programs/mybarpool` that builds as SBPFv3, Mollusk unit tests and a committed compute-unit table; `packages/shared` as an empty TypeScript package on `@solana/kit` 8 with build and tests; Surfpool 1.6.0 as the localnet, forking mainnet at 200 ms slots, with a first test that reads the real Entropy program; CI for all of it plus the Solana Foundation's reusable verifiable-build workflow (deploy wired, off) and OtterSec's anchor-lints. Toolchain pinned in `rust-toolchain.toml`, `Anchor.toml` and `package.json`.
- Design published: rules, fees, lifecycle, randomness, trust model, private pools, screens.
- Program specification published (`docs/PROGRAM.md`): accounts, instructions, events, errors, algorithms. Sponsorship (add to a pool's prizes before kickoff, no fee) added to the rules.
- Program and SDK: in development. No mainnet deployment yet.
