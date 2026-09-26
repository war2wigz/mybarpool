# MyBarPool

**NFL boxes for your bar — on Solana.**

It's the game bars have always run on a paper grid, also known as football squares. Buy a box on a 5×5 grid. Numbers are drawn on-chain once the grid sells out, and the last digit of each team's score at the end of every quarter picks the winner. Prizes are sent straight to the winner's wallet as soon as the quarter is official. Nobody claims anything, nobody disputes anything, and money never leaves the vault without a rule saying so.

**Every game. Every week.** Most people know boxes from Super Bowl Sunday. MyBarPool runs a grid on every NFL game, Thursday night through Monday night, from Week 1 to the Super Bowl: 272 regular-season games plus the playoffs, not one Sunday a year.

- **Mobile:** Solana Seeker (Solana dApp Store), Android first
- **Web:** [mybarpool.com](https://mybarpool.com), the same app
- **Status:** design complete, program and app in development. Nothing is deployed to mainnet yet.

This repository is the open-source part of MyBarPool: the Solana program that holds the money, the client SDK, and the full design. See [What is in this repo](#what-is-in-this-repo).

---

## Table of contents

1. [How a pool works](#how-a-pool-works)
2. [The rules, precisely](#the-rules-precisely)
3. [Fees](#fees)
4. [What happens when things go wrong](#what-happens-when-things-go-wrong)
5. [Randomness](#randomness)
6. [Trust model](#trust-model)
7. [Building on the program](#building-on-the-program)
8. [What is in this repo](#what-is-in-this-repo)
9. [Design and screens](#design-and-screens)
10. [Toolchain](#toolchain)
11. [Licence, trademarks, security](#licence-trademarks-security)

---

## How a pool works

1. **Create a pool.** Pick an NFL game, a token (SOL, SKR or ORE), a box price, an optional add-on fee (0–5%), and a payout split. You pay a creation fee of about 0.014 SOL (account rent) and can buy up to 5 of your own boxes in the same transaction. Every creator earns 5% of the pot automatically, paid with the first prize.
2. **Sell out the grid.** Anyone can buy any number of boxes in one tap. The program assigns positions at random, since every box has identical odds before the draw. Funds sit in a program-owned vault. Sales close at kickoff.
3. **Lock and draw.** When the 25th box sells, the pool locks and digits 0–9 are shuffled onto both axes using [Regolith Labs' Entropy](https://github.com/regolith-labs/entropy) (commit-reveal + slothash). Nobody can know the digits while boxes are on sale.
4. **Play.** After each quarter, the keeper posts the official end-of-quarter score on-chain and the program pays that quarter's winner immediately. Q4 uses the final score, so overtime replaces the end-of-regulation score and Q4 pays when the game is final.
5. **Verify.** Every payout is recorded on the pool account with the score, winning box, wallet and amount, and linked to its transaction in the app.

If the grid doesn't sell out by kickoff, or the game is postponed or cancelled, every buyer's full purchase is sent back automatically with no fees. If a game is suspended and never finished, unpaid prizes are split equally across all 25 boxes.

## The rules, precisely

| | |
|---|---|
| League | NFL only |
| Games | Every regular-season and postseason game, Week 1 through the Super Bowl. Pools open as soon as a week's kickoff times are published. Preseason off at launch (config switch) |
| Grid | 5×5. Boxes numbered 1–25, top-left is 1, left to right then down. Two digits per row and column, so each box covers 4 of the 100 last-digit pairs: 4% per box per quarter, and a full grid always has a winner |
| Axes | Home team across the top, away team down the side. Each axis is shuffled independently; the same pair can appear on a column and a row, which changes nobody's odds |
| Tokens | SOL, SKR, ORE — one token per pool |
| Box price | SOL 0.05–1 in 0.05 steps · SKR 100–5,000 in 100s · ORE 0.05–1 in 0.05s, the same ladder as SOL. The program rejects any price off the step. Limits live in platform config and can move with prices without a program upgrade |
| Buying | Any wallet, any number of boxes, any number of pools. Positions are assigned by the program, never chosen. Sales close at the scheduled kickoff |
| Locking | Only when all 25 boxes are sold. A pool that isn't full at kickoff is returned |
| Payout split | Chosen by the creator from presets. Default Q1 20 / Q2 20 / Q3 20 / Q4 40. Also 25/25/25/25 and Q4 100% |
| Scoring | Q1–Q3 use the official end-of-quarter line score. Q4 uses the final score after any overtime; the Q4 prize is paid when the game is final |
| Payouts | Pushed to the winning wallet by the settle instruction. Players never claim. Every payout is recorded on the pool account and emitted as an event |
| Creator limits | Max 3 open (unfilled) pools per wallet per game. Max 5 boxes in a pool you created. Both are config values with optional per-wallet overrides. Players are unlimited |
| Private pools | Supported by the program from v1: `link` (gate-key co-signer carried in the invite link or a printable QR) and `allowlist` (Merkle root of wallets). Same fees, draw and settlement as public pools. The first version of the app creates public pools only |
| Scores | ESPN and API-Sports, both must agree before anything is posted |
| Randomness | Regolith Labs Entropy, one variable per pool, no re-rolls |
| Environments | Localnet and mainnet only. No devnet |

The full rulebook, with the reasoning behind each rule, is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Fees

Fees are a fixed percentage of the pot, set at creation, shown on every pool, and never changed afterwards.

| Share | Rate | Goes to |
|---|---|---|
| Platform | 5% | The platform, for running the keeper, the score feeds and the randomness on every pool, including pools created from third-party clients |
| Creator | 5% | Whoever created the pool, automatically |
| Add-on budget | 0–5% | Optional, chosen by the creator at creation. Split between an extra creator fee and, for third-party clients, an integrator fee. `creator_addon + integrator ≤ 5%` is enforced on-chain |
| **Total** | **10–15%** | Players see one number: "Fee 10%", "Fee 12%", never more than 15% |

**When fees move.** One rule: *no fee leaves the vault until the pool has paid a prize.* Every share is transferred in the same `settle` transaction as the first non-zero prize (Q1 for most splits, the final for Q4 100%), atomically with it. Until then the vault holds the whole pot, so every return is 100% of every purchase. After it, the fee is final.

**Amounts**, computed once from the pot `P = 25 × price` and stored on the pool:

```
platform_fee   = floor(P × 500 / 10000)
creator_fee    = floor(P × (500 + creator_addon_bps) / 10000)
integrator_fee = floor(P × integrator_bps / 10000)         // 0 when unset
prize_pool     = P − platform_fee − creator_fee − integrator_fee
quarter[q]     = floor(prize_pool × split[q] / 100)         // dust swept to the platform at close
```

Worked example, 0.05 SOL boxes, 2% add-on, 20/20/20/40: pot 1.25 SOL, fee 12% = 0.15 SOL (platform 0.0625, creator 0.0875), prize pool 1.1 SOL, quarters 0.22 / 0.22 / 0.22 / 0.44.

**Creation fee.** The creator pays the rent for the pool account, its vault and the per-game counter, about 0.014 SOL. When the pool closes in any outcome the program reclaims that rent to the platform. It is labelled a creation fee, never a deposit.

**Ceilings are hard-coded.** `platform_bps ≤ 500` and `base + add-on ≤ 1500` are constants in the program. Config can lower them, never raise them; raising them would need a program upgrade, which is public, verifiable and announced in advance. No change of any kind touches an existing pool.

## What happens when things go wrong

| Situation | What the program does |
|---|---|
| Grid not full at kickoff | Every purchase returned in full. No fee |
| Game delayed (weather etc.) | Pool waits |
| Game postponed or cancelled | Every purchase returned in full, even after lock and draw. No fee, creator earns nothing |
| Game suspended and not finished | Every unpaid prize, including the quarter in progress, split equally across all 25 boxes. Fees already taken stay taken |
| Score sources disagree or status is unfamiliar | Pool stays locked and the team is alerted. Waiting is always safe because nothing leaves the vault without a decision |
| Draw can't finalise before kickoff | Pool returned |
| The platform stops operating | 30 days after kickoff, any buyer of an unresolved pool can call `reclaim` and take back their own share (purchase price, or `unpaid_prize_pool / 25` after partial settlement). No key or permission needed. This is the only path that isn't platform-controlled, and it can only fire if the platform has stopped |

Returns and splits are executed by the platform keeper. Players and creators never have to claim, request or dispute anything.

## Randomness

- One Entropy `Var` per pool, opened by the keeper at lock with an end slot about a minute later. The provider publishes a seed after the end slot; the value is `hash(seed, slothash)`. The provider can't predict the slothash and validators can't see the seed, so neither can steer the result.
- Two independent Fisher–Yates shuffles of 0–9 seeded with `hash(value, "home")` and `hash(value, "away")`. The digit assignment is recorded on the pool account and emitted as an event, so anyone can re-derive it.
- No re-rolls. The pool records the `Var` address at lock and the draw instruction only accepts a value from that account. Replacing a `Var` is an admin (multisig) instruction that emits an event, never a keeper action.

## Trust model

MyBarPool is platform-operated. Every state change after a purchase is performed by keys the platform controls, and players rely on the platform to run the keeper honestly and keep it running. What the program guarantees regardless:

| Key | Can do | Held in |
|---|---|---|
| Platform admin | Update config (fees within the hard-coded ceilings, price steps, payout presets, creator limits), per-wallet overrides, cancel/return pools, split suspended pools, update kickoff times | Squads multisig |
| Score authority (keeper) | Post scores, settle quarters, run the Entropy draw, return unfilled pools | Cloud KMS, single purpose |
| Program upgrade authority | Deploy new versions | Squads multisig |
| Players and creators | Buy boxes, create pools within the limits, rotate the gate key on their own private pools | Their own wallets |

Program-enforced checks on the keeper, so a bug or a stolen score key can't drain pools in one transaction:

- Quarters must be posted in order, one per game; scores can only increase.
- Wall-clock floors on every quarter: Q1 needs at least 15 real minutes after kickoff, each later quarter at least 15 minutes after the previous post. These are sanity checks, not triggers; scores are posted when both feeds report the quarter over.
- Returns of unfilled pools are only accepted after kickoff.
- Cancels and splits need the admin key, not the score key.
- Every admin capability is an explicit, documented instruction. There is no privileged path in `buy` or `settle`.

The keeper is closed source and its details are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) only to the extent needed to understand the trust model. The program is what you are trusting; the program is what is published.

## Building on the program

Anyone may build a frontend, bot or integration on the program. Everything that matters is enforced on-chain, not in a client:

- The platform fee, creator fee, add-on and integrator fee are paid inside `settle`.
- Price steps and creator limits are checked on `create_pool` and `buy`.
- Private-pool gating is checked on `buy`.
- The keeper serves every pool on the program, whichever client created it, at the platform's cost.

What a client chooses: token, price, split, add-on, integrator fee, public or private, and which of the allowed tokens its UI offers. What the platform sets: the list of allowed tokens and mints, price steps, fee ceilings, creator limits.

How a client earns: create pools itself on its own site (5% creator share plus up to 5% add-on, with a per-wallet override for the open-pool limit on request), or let its users create pools and set itself as `integrator` for a share of the add-on budget. A private `link` pool whose gate key is held by the client's server is effectively public on that site and absent from the MyBarPool app; that is an intended use.

Third parties pay for their own RPC. The SDK takes a connection as a parameter and ships with no default endpoint. Pool state is read directly from the chain, so a client needs nothing from MyBarPool's servers to work. A public pools/games listing API will exist as a best-effort convenience, rate-limited and without SLA.

Clients must show a box as **won** only after the settle event, never from live scores. Live scores may mark a box as **leading**, clearly provisional.

Integration guide, SDK reference and the official program ID and config PDA will be published here when the program is deployed. Check the program ID against this repo before trusting a deployment; forks are possible and will not be served by the keeper.

## What is in this repo

```
programs/mybarpool/   Solana program (Anchor) — Apache-2.0                       (coming)
packages/shared/      TypeScript SDK: types, instruction builders, PDAs,
                      grid and payout math, 1–25 box labelling, team table — Apache-2.0   (coming)
docs/ARCHITECTURE.md  The full design: rules, lifecycle, fees, randomness, trust model,
                      program accounts, private pools, decisions log
docs/DESIGN.md        Brand, design system, information architecture, screen wireframes, flows
docs/mockups/         High-fidelity HTML mockups of every screen and rendered PNGs
docs/brand/           Logo concept
LICENSE               Apache-2.0
TRADEMARKS.md         What the licence does not cover
SECURITY.md           How to report a vulnerability
CONTRIBUTING.md       Issues welcome; program PRs not accepted in v1
CHANGELOG.md          Every deploy and every breaking change, announced in advance
```

The app (Expo, one codebase for Seeker Android and mybarpool.com), the keeper, the scores service and the infrastructure are closed source and are not in this repo.

## Design and screens

The app is dark, numbers-first, and calm: a neon-sign logo, condensed numerals for scores and prices, one accent per meaning (purple actions, teal "leading", green "won", amber waiting, red returned). Every screen is mocked up in [docs/mockups/screens.html](docs/mockups/screens.html); rendered frames are in [docs/mockups/png/](docs/mockups/png/).

![All screens](docs/mockups/png/overview.png)

Design principles, the full information architecture and every flow are in [docs/DESIGN.md](docs/DESIGN.md).

## Toolchain

Latest stable Solana tooling only, pinned in-repo, verifiable builds. At the time of writing: Agave/Solana CLI 4.3.0, Anchor 1.2.0, platform-tools v1.57. The deployed program will be verified with `solana-verify` so anyone can confirm mainnet runs the code in this repo. Localnet tests run against the real Entropy program and the real SKR and ORE mints cloned from mainnet.

## Licence, trademarks, security

- Code in this repository is licensed under [Apache-2.0](LICENSE).
- The licence covers the code, not the name or the sign. "MyBarPool", the neon-sign logo and mybarpool.com are trademarks; see [TRADEMARKS.md](TRADEMARKS.md). NFL team names and logos belong to the NFL and its clubs.
- Found a vulnerability? See [SECURITY.md](SECURITY.md). Please don't open a public issue for it.
- An independent audit of the program will be completed and published before the first mainnet deploy.
