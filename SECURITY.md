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

An independent audit of the program is planned before the first mainnet deploy and the report will be published in this repository. A bug bounty with published amounts will follow once there is enough value in the vaults to justify one.
