# Security

The program holds player money. If you find a way to take, freeze or misdirect it, or to influence a draw or a settlement, please tell us privately first.

## Reporting

Use GitHub's private vulnerability reporting on this repository: **Security → Report a vulnerability**. It reaches the maintainers directly and nobody else.

If that doesn't work for you, email **security@mybarpool.com**.

Please include what you found, how to reproduce it, and what you believe the impact is. Proof-of-concept code on localnet is welcome. Don't test against mainnet pools that hold other people's money.

## What to expect

- Acknowledgement within 2 business days.
- An assessment and a plan within 7 days for anything that touches funds, draws or settlement.
- Credit in the changelog and the fix commit if you want it, once the fix is deployed.
- We will not take legal action against good-faith research that follows this policy.

## Scope

In scope:

- The Solana program in `programs/mybarpool/` and the SDK in `packages/shared/`, at the commit deployed to mainnet (listed in `CHANGELOG.md` once there is one).
- The official deployment and its config account, program ID and config PDA as published in the README.

Out of scope:

- Forks of the program deployed by others.
- Third-party clients and their infrastructure.
- Third-party programs we depend on (Entropy, the SPL token programs). Report those to their maintainers; we are glad to be copied.
- The closed-source keeper and scores service, unless the finding is reachable through the program (in which case it is very much in scope).

## Audit and bounty

The program was reviewed before the first mainnet deploy by an internal security audit, not by an outside firm: four reviewers given the code, the specification and no prior context, one slice of the program each, with a proof-of-concept against the real bytecode for every finding they confirmed — and then, the next day, four more with the same brief on the whole program again. The reports, with every finding, its severity, its fix and its test, are [`docs/audits/2026-10-23-internal-audit.md`](docs/audits/2026-10-23-internal-audit.md) and [`docs/audits/2026-10-24-internal-audit.md`](docs/audits/2026-10-24-internal-audit.md); each is a record of what was checked and what was found, not a third-party attestation, and says what it did not cover. What a reader can check instead of taking our word: the specification ([`docs/PROGRAM.md`](docs/PROGRAM.md)) is the contract and every rule in it names its test; the test suite runs on the SBF binary in CI (Mollusk unit tests, a localnet lifecycle against a mainnet fork, a fuzz run, the schema snapshot that refuses a layout change, `anchor-lints`); and the deployed program is a verifiable build (`solana-verify`) of a commit in this repository. Reviewers start at [`programs/mybarpool/AUDIT-READINESS.md`](programs/mybarpool/AUDIT-READINESS.md). A paid external review remains an option once the vaults hold enough to justify one. A bug bounty with published amounts will follow at the same point.
