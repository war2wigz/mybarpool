# @mybarpool/shared

The MyBarPool client SDK. Everything a client needs to create pools, buy boxes,
read pools and follow settlements on the MyBarPool program, with no dependency
on the platform's servers: the package takes a Solana connection as a parameter
and ships with no default RPC endpoint.

Status: in development. Step 1 of the build plan: the pure core, with no chain
dependency.

- `boxes` — index 0–24 ↔ label 1–25, row and column. The only place the product adds or
  subtracts 1 on a box number.
- `assignment`, `axes`, `winner` — the reference implementations of PROGRAM §6.1–6.3 that
  the program must reproduce byte for byte; `src/vectors/*.json` are the shared test vectors
  the program's own tests load.
- `fees`, `price`, `format` — PROGRAM §5 money arithmetic in `bigint` base units (fees,
  prize pool, quarter prizes, dust; price ladders), and DESIGN §10.3 display formatting.
- `gameKey`, `teams`, `sources`, `sponsors` — the canonical game key and `GameRecord` seeds,
  the frozen 32-team table, the score-source ID map shape, and the sponsor directory shape
  with its validator.

Home is the columns (across the top), away the rows (down the side); `box = row × 5 + col`.
Money is `bigint` end to end; `floor` is `bigint` division; half-up rounding exists only in
`format`. The chain layer generated from the program's IDL arrives in Step 9. See the
repository README for the program's rules and `docs/PROGRAM.md` for the specification.

Boxes are labelled 1–25 everywhere a person sees them; the program stores
0–24. This package is the one place that conversion happens.

Licensed under Apache-2.0.
