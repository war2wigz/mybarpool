# @mybarpool/shared

The MyBarPool client SDK. Everything a client needs to create pools, buy boxes,
read pools and follow settlements on the MyBarPool program, with no dependency
on the platform's servers: the package takes a Solana connection as a parameter
and ships with no default RPC endpoint.

Status: in development. Build plan Steps 1–9: the pure core and the chain layer; no mainnet
deployment yet.

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
`format`. See the repository README for the program's rules and `docs/PROGRAM.md` for the
specification.

## The chain layer

Everything above is pure. The chain layer, `src/chain/`, is what talks to the program; it is
built on `@solana/kit` 8 only (no `web3.js`, no `@solana-program/*`).

### Install

```
npm i @mybarpool/shared @solana/kit
```

The package is ESM with `exports`; set `"moduleResolution": "NodeNext"` (or `"bundler"`) in
`tsconfig.json`. `@solana/kit` is a peer dependency so your app and the SDK share one copy.

### Client

```ts
import { createMyBarPoolClient } from "@mybarpool/shared";
import { createSolanaRpc, createSolanaRpcSubscriptions } from "@solana/kit";

const client = createMyBarPoolClient({
  rpc: createSolanaRpc(RPC_URL),
  rpcSubscriptions: createSolanaRpcSubscriptions(WS_URL), // optional; needed by watch* and keypair sends
  // programAddress: defaults to MYBARPOOL_PROGRAM_ADDRESS, the declared id
});
```

The SDK ships with no RPC endpoint: the connection is yours (third parties pay for their own
RPC). `programAddress` reaches every derivation and every generated call, so a fork or a
localnet deployment is one parameter away. `rpc` is typed as the narrow intersection of the
RPC methods the layer uses (`MyBarPoolRpcApi`), never the whole `SolanaRpcApi`, so a narrowed
transport type-checks.

### Instruction helpers

Each takes the client and returns `{ instruction, …derivedAddresses }` plus
`createdAccountBytes` where the instruction may create an account. None sends.

| Helper                            | Instruction           | Signer            | Reads                 |
| --------------------------------- | --------------------- | ----------------- | --------------------- |
| `createPoolInstruction`           | `create_pool`         | creator           | `config`, `game`      |
| `buyInstruction`                  | `buy`                 | buyer (+gate key) | pool, buyer's ATA     |
| `sponsorInstruction`              | `sponsor`             | sponsor           | pool, sponsorship     |
| `rotateGateKeyInstruction`        | `rotate_gate_key`     | creator           | —                     |
| `reclaimInstruction`              | `reclaim`             | box owner         | pool, owner's ATA     |
| `reclaimSponsorshipInstruction`   | `reclaim_sponsorship` | sponsor           | pool, sponsor's ATA   |
| `closeSponsorshipInstruction`     | `close_sponsorship`   | none (anyone)     | —                     |
| `sampleVarInstruction`            | `sample_var`          | anyone            | pool                  |
| `closePoolInstruction`            | `close_pool`          | none (anyone)     | pool, `config`        |
| `closeCounterInstruction`         | `close_counter`       | none (anyone)     | `config`              |

`createPoolInstruction` fails a paused platform, a disabled token, an off-ladder price and a
game that is not open for sales with the program's own error name before anything is
simulated; `buyInstruction` builds the allowlist proof (and throws `NotAllowlisted` before any
RPC call for a wallet not on the list), passes the gate key as a co-signer on a `Link` pool, and
throws `TokenAccountMissing` for an SPL pool whose buyer has no token account yet. Every
account list is the generated builder's, so a helper can never disagree with the IDL. Keeper
and admin instructions have no hand-written helper; their generated builders are under
`@mybarpool/shared/generated`.

### Transactions

```ts
const { instruction, createdAccountBytes } = await buyInstruction(client, { buyer, pool, count: 2 });
const prepared = await prepareTransaction(client, {
  feePayer: buyer,
  instructions: [instruction],
  format: transactionFormatFor(wallet.supportedTransactionVersions),
  priorityFee: await suggestPriorityFee(client, [pool]),
  createdAccountBytes,
});
const signature = await signAndSend(client, prepared);
```

`prepareTransaction` simulates, sets the compute-unit limit (`ceil(units × 1.2) + 30,000`,
capped at 1,400,000) and the loaded-accounts data-size limit (simulated + created + 8 KiB),
converts the priority fee, asserts the size against the format's limit and fetches the
blockhash last. A wallet signs only the transaction versions it lists; `transactionFormatFor`
returns `"v1"` when the list includes `1` and `"legacy"` otherwise — a v1 message carries its
limits in the message itself (4,096 bytes), a legacy one as ComputeBudget instructions
(1,232 bytes). `signAndSend` confirms at `confirmed`, never resends, and classifies failures as
`SdkError` codes: `SimulationFailed` and `ProgramError` carry the PROGRAM §8 error name,
`BlockhashExpired` means nothing landed and `refreshBlockhash` gives the one allowed retry.
`checkFunds` reports a lamport or token shortfall before the wallet opens.

### Reads and subscriptions

`getConfig`, `getGame`, `getPool`, `getSponsorship`, `getWalletOverride`, `getCreatorCounter`
return `{ address, data }` or `null`. `listPools({ game?, creator?, status? })`,
`listSponsors(pool)` and `listGames({ status? })` use `getProgramAccounts` with `dataSize` and
`memcmp` filters on proven offsets; `poolsHoldingBoxesOf(pools, wallet)` is the wallet view.
`eventsOf(signature)` decodes the program's events from a transaction's inner instructions
(the program emits with `emit_cpi!`, never through logs). `watchPool`, `watchEvents` and
`watchSettlements` subscribe and return an unsubscribe function; errors go to an `onError`
option, never an unhandled rejection.

### Leading vs. won

`leadingBox(pool, home, away)` is the box the live scores point at right now: display only,
provisional, `undefined` until the pool is drawn, and it moves with every score. `wonBoxes(pool)`
lists the quarters the program has settled, from the pool's own state, and the `QuarterSettled`
event carries the same facts; a box is **won** only there, with the settle transaction as its
proof. `displayStatus(pool, game, now)` is DESIGN §10.2's derived state and `poolView` is the
labelled view of a pool: every box named 1–25, owner, returned bit and won quarters.

### `@mybarpool/shared/generated`

The Codama-rendered client for the whole program, straight from `idl/mybarpool.json`: every
account decoder, instruction builder, event decoder, PDA finder and the error table. Rendered
by `npm run generate`, committed, regenerated and diffed in CI, never hand-edited. The root
re-exports the account and event types the chain layer's API names; use the subpath for the
keeper's and the admin's builders and for anything the hand-written layer does not cover.

### Constants

`MYBARPOOL_PROGRAM_ADDRESS` (from the generated client) is the declared program id; the config
PDA is `configPda(client)`. Check the program id against the repository before trusting a
deployment. The canonical pool link is `https://mybarpool.com/pools/{poolAddress}` with the
pool PDA in base58 — a format the SDK documents but never forms; linking is the app's.

The API reference is generated by `npm run docs` (typedoc) into `docs-api/`.

Boxes are labelled 1–25 everywhere a person sees them; the program stores
0–24. This package is the one place that conversion happens.

Licensed under Apache-2.0.
