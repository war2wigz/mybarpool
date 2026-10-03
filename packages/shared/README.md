# @mybarpool/shared

The MyBarPool client SDK. Everything a client needs to create pools, buy boxes,
read pools and follow settlements on the MyBarPool program, with no dependency
on the platform's servers: the package takes a Solana connection as a parameter
and ships with no default RPC endpoint.

Status: in development. Step 0 of the build plan (an empty package with a build
and a test runner). The grid, payout and labelling math lands in Step 1; the
chain layer generated from the program's IDL in Step 9. See the repository
README for the program's rules and `docs/PROGRAM.md` for the specification.

Boxes are labelled 1–25 everywhere a person sees them; the program stores
0–24. This package is the one place that conversion happens.

Licensed under Apache-2.0.
