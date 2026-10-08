# MyBarPool

**NFL boxes for your bar — on Solana.**

It's the game bars have always run on a paper grid, also known as football squares. Buy a box on a 5×5 grid. Numbers are drawn on-chain once the grid sells out, and the last digit of each team's score at the end of every quarter picks the winner. Prizes are sent straight to the winner's wallet as soon as the quarter is official. Nobody claims anything, nobody disputes anything, and money never leaves the vault without a rule saying so.

**Every game. Every week.** Most people know boxes from Super Bowl Sunday. MyBarPool runs a grid on every NFL game, Thursday night through Monday night, from Week 1 to the Super Bowl: 272 regular-season games plus the playoffs, not one Sunday a year.

- **Mobile:** Solana Seeker (Solana dApp Store), Android first
- **Web:** [mybarpool.com](https://mybarpool.com), the same app
- **Status:** design complete, program and app in development. Nothing is deployed to mainnet yet.

This repository is the open-source part of MyBarPool: the full design, and the Solana program that will hold the money and the client SDK, published commit by commit as they are written. See [What is in this repo](#what-is-in-this-repo).

---

## Table of contents

1. [How a pool works](#how-a-pool-works)
2. [The rules, precisely](#the-rules-precisely)
3. [Fees](#fees)
4. [If a game doesn't go to plan](#if-a-game-doesnt-go-to-plan)
5. [Randomness](#randomness)
6. [How winners are decided](#how-winners-are-decided)
7. [Building on the program](#building-on-the-program)
8. [What is in this repo](#what-is-in-this-repo)
9. [Design and screens](#design-and-screens)
10. [Toolchain](#toolchain)
11. [Licence, trademarks, security](#licence-trademarks-security)

---

## How a pool works

1. **Create a pool.** Pick an NFL game, a token (SOL, SKR or ORE), a box price and a payout split. As the creator you earn 5% of the pot automatically, paid with the first prize; that 5% is built into the 10% fee every pool carries, so you don't have to set anything to get it. If you want more, you can add up to 5% on top (0–5%, default 0), which is shown to buyers as part of one total fee, 10–15%. You pay a creation fee of about 0.014 SOL (account rent) that is kept whether or not the pool fills, and you can buy up to 5 of your own boxes in the same transaction.
2. **Sell out the grid.** Anyone can buy any number of boxes in one tap. The program assigns positions at random, since every box has identical odds before the draw. Funds sit in a program-owned vault. Sales close at kickoff. Until then anyone can also add to the prizes as a sponsor (the pool then shows, for example, "Sponsored by ORE · +1 ORE"): no fee is taken on it, it goes to the winners in full, and it comes back to the sponsor if the pool doesn't play.
3. **Lock and draw.** When the 25th box sells, the pool locks and digits 0–9 are shuffled onto both axes using Entropy, the commit-reveal + slothash randomness program [Regolith Labs wrote for ORE](https://github.com/regolith-labs/entropy), run as MyBarPool's own deployment (see [Randomness](#randomness)). Nobody can know the digits while boxes are on sale.
4. **Play.** Within a couple of minutes of each quarter ending, once the two score sources agree, the keeper posts the official end-of-quarter score on-chain and the program pays that quarter's winner in the same breath; the money is in the winner's wallet while the next quarter is starting. Q4 uses the final score, so overtime replaces the end-of-regulation score and Q4 pays when the game is final.
5. **Verify.** Every payout is recorded on-chain: the pool account keeps the quarter, winning box and amount, and the settle event carries the full detail (score, winning box, wallet, amount). The app links each one to its transaction.

If the grid doesn't sell out by kickoff, or the game is postponed or cancelled, every buyer's full purchase (and every sponsorship) is sent back automatically with nothing taken out. The creator's creation fee is the one thing that is not returned: it paid for the accounts, and a pool that didn't fill still used them. If a game is suspended and never finished, unpaid prizes are split equally across all 25 boxes.

## The rules, precisely

| | |
|---|---|
| League | NFL only |
| Games | Every regular-season and postseason game, Week 1 through the Super Bowl. Pools open as soon as a week's kickoff times are published. Preseason off at launch (config switch) |
| Grid | 5×5. Boxes numbered 1–25, top-left is 1, left to right then down. Two digits per row and column, so each box covers 4 of the 100 last-digit pairs: 4% per box per quarter, and a full grid always has a winner |
| Axes | Home team across the top, away team down the side. Each axis is shuffled independently; the same pair can appear on a column and a row, which changes nobody's odds |
| Tokens | SOL, SKR, ORE — one token per pool |
| Box price | SOL 0.05–1 in 0.05 steps · SKR 100–5,000 in 100s · ORE 0.05–1 in 0.05s, the same ladder as SOL. The program rejects any price off the step. Limits live in platform config and can move with prices without a program upgrade |
| Buying | Any wallet, any number of boxes, any number of pools. Positions are assigned by the program, never chosen. Sales close at the pool's recorded kickoff, which follows league schedule changes announced before the game (the keeper may move it only while it is still in the future, only to a future time, and only within 72 hours of the original schedule) |
| Locking | Only when all 25 boxes are sold. A pool that isn't full at kickoff is returned |
| Payout split | Chosen by the creator from exactly three presets fixed in the program: Q1 20 / Q2 20 / Q3 20 / Q4 40 (default), 25/25/25/25, and Q4 100%. No other split is accepted |
| Sponsorship | A company or community can put tokens straight into a pool's prize pot before kickoff, without buying boxes, and the pool shows who it was, for example "Sponsored by ORE · +1 ORE". Minimum one box price, up to a per-token cap in config, in the pool's token. The money is added to the prizes in full (no fee is taken on it) and is divided across the quarters the same way as the rest of the pot. If the pool doesn't play, it goes back to the sponsor; once the first prize is paid it belongs to the winners. Sponsors listed in a directory the MyBarPool team maintains get their name, logo and a link to their site or Seeker app on the pool page, the link preview, the saved grid image and the printed board; anyone else shows as a shortened address. The platform never sponsors its own pools |
| Scoring | Q1–Q3 use the official end-of-quarter line score. Q4 uses the final score after any overtime; the Q4 prize is paid when the game is final |
| Payouts | Pushed to the winning wallet by the settle instruction. Players never claim. Every payout is recorded on the pool account and emitted as an event |
| Creator limits | Max 3 open (unfilled) pools per wallet per game. Max 5 boxes in a pool you created. Both are config values with optional per-wallet overrides. Players are unlimited |
| Private pools | Supported by the program from v1: `link` (gate-key co-signer carried in the invite link or a printable QR) and `allowlist` (Merkle root of wallets). Same fees, draw and settlement as public pools. The first version of the app creates public pools only |
| Links | Every pool has one URL, `mybarpool.com/pools/{address}`, that reads the pool straight from the chain and needs nothing from MyBarPool's servers; a short form `mybarpool.com/p/{code}` redirects to it. Links open the app on Seeker and the web app anywhere else, with a live grid image as the preview. No referral codes, no tracking parameters |
| Scores | Two primary sources that must agree: API-Sports and Polymarket's public sports stream (Sportradar data). ESPN is standby: it stands in when a primary is silent, and a disagreement from it alerts the team without stopping a post. If the two sources relied on disagree, the game halts until it's resolved |
| Randomness | Entropy (Regolith Labs' commit-reveal program), MyBarPool's own deployment, one variable per pool, no re-rolls |
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
prize_pool     = P − platform_fee − creator_fee − integrator_fee + sponsored_total   // no fee on sponsorship
quarter[q]     = floor(prize_pool × split[q] / 100)         // dust to the platform at close (to the creator on an abandoned close)
```

Worked example, 0.05 SOL boxes, 2% add-on, 20/20/20/40: pot 1.25 SOL, fee 12% = 0.15 SOL (platform 0.0625, creator 0.0875), prize pool 1.1 SOL, quarters 0.22 / 0.22 / 0.22 / 0.44. With a 1 SOL sponsorship added before kickoff: fees unchanged, prize pool 2.1 SOL, quarters 0.42 / 0.42 / 0.42 / 0.84.

**Creation fee.** The creator pays the rent for the pool account, its vault and (first time per game) a per-creator counter, about 0.014 SOL. It is a fee, not a deposit: the platform keeps it in every outcome, whether the pool filled, was returned unfilled, or was postponed. Mechanically, the pool and vault rent go to the platform when the pool closes, and the counter's rent when the counter closes at zero open pools. One exception: after an abandoned-pool reclaim (the platform stopped running for 30 days), the pool and vault rent and any rounding dust go to the creator instead, since a pool can only reach that state if the platform is no longer resolving pools.

**Ceilings are hard-coded.** `platform_bps ≤ 500` and `base + add-on ≤ 1500` are constants in the program. Config can lower them, never raise them; raising them would need a program upgrade, which is public, verifiable and announced in advance. No change of any kind touches an existing pool.

## If a game doesn't go to plan

| Situation | What the program does |
|---|---|
| Grid not full at kickoff | Every purchase returned in full, and every sponsorship back to its sponsor. Nothing taken out; the creator's creation fee is not returned |
| Game delayed (weather etc.) | Pool waits |
| Game postponed or cancelled | Every purchase and every sponsorship returned in full, even after lock and draw. Nothing taken out; the creator earns nothing and the creation fee is not returned. A postponed game is never carried to its new date; the rescheduled game gets a new game record and creators open fresh pools on it |
| Kickoff moved by the league before the game | The NFL's flexible scheduling can move a game to a different slot, for example a Sunday 1:00 game to Sunday night, with about two weeks' notice; kickoffs also shift for broadcast or weather reasons. When that happens the pool's kickoff moves with it, so open pools keep selling until the new time and nothing is returned for a game that hasn't started. The keeper posts the new time only once both score sources agree on it, and the program accepts it only while the current kickoff is still in the future, to a future time, within 72 hours of the original schedule; anything else is a postponement and follows the row above |
| Game suspended and not finished | Every unpaid prize, including the quarter in progress, split equally across all 25 boxes. Fees already taken stay taken, and so does a sponsorship once a prize has been paid; before that it goes back to the sponsor |
| Score sources disagree or status is unfamiliar | Pool stays locked and the team is alerted. Waiting is always safe because nothing leaves the vault without a decision |
| Draw hasn't finalised by the time the first score is posted | A pool that fills in the last minute still gets its draw; the deadline is the first score post, not kickoff, and `settle` refuses a pool with no digits. If the draw is stuck, a replacement draw is opened by the multisig (with a public event); if that also fails before the first score is posted, the pool is cancelled and returned |
| Pool still unresolved 30 days after kickoff | The program guarantees no pool stays unresolved. A running keeper pays each quarter within minutes of it ending and returns an unfilled pool at kickoff, so this state is never reached in normal operation; if it ever were, 30 days after the game's original scheduled kickoff (never a later keeper update) any buyer can call `reclaim` and take back their own share directly from the program (purchase price, or `unpaid_prize_pool / 25` after partial settlement), with a button on the pool page and no permission needed. Fees already taken stay taken; fees not yet taken are never taken. It is the one path that isn't platform-controlled, and it is what makes the vault a guarantee rather than a promise |

Returns and splits are executed by the platform keeper. Players and creators never have to claim, request or dispute anything.

## Randomness

- The randomness program is Entropy, the commit-reveal + slothash program [Regolith Labs](https://github.com/regolith-labs/entropy) wrote for ORE. Regolith's own deployment no longer lets anyone else open a variable, so MyBarPool runs its own deployment of a fork, [war2wigz/entropy](https://github.com/war2wigz/entropy), at program id `ASo8r4EEFLPAMDk1w3XdKbEmq4c1GynbsHGa6RGG83fH`. The fork changes three things and nothing else: the program id, the `Open` instruction re-enabled, and the `security.txt` contact. It is built and deployed through the same verifiable-build pipeline as this program, with the same multisig as upgrade authority. The provider (the party that picks the seed and reveals it) is MyBarPool's keeper.
- One Entropy `Var` per pool, opened by the keeper at lock with an end slot about a minute later. The keeper publishes `commit = hash(seed)` when it opens the `Var`, reveals the seed after the end slot, and the value is `hash(slothash, seed)`. The keeper can't predict the slothash and nobody else knows the seed, so neither party can steer the result on its own.
- Two independent Fisher–Yates shuffles of 0–9 seeded with `hash(value, "home")` and `hash(value, "away")`. The digit assignment is recorded on the pool account and emitted as an event, so anyone can re-derive it.
- No re-rolls. The pool records the `Var` address and its commit at lock; the draw instruction only accepts a value from that account, whose revealed seed hashes to that commit, and refuses a `Var` that fell back to a hash of its own end slot. Replacing a `Var` is an admin (multisig) instruction that emits an event, is refused once a sample is recorded, and is never a keeper action.

## How winners are decided

Winners come from the real score. Nothing else.

After each quarter ends on the game clock, the official end-of-quarter score is posted on-chain and the program pays whoever holds the box whose digits match the last digit of each team's score. Nobody picks a winner and nobody approves a payout. The score decides, the program pays.

What makes that reliable, in plain terms:

- **The money is in a program vault, not in anyone's wallet.** Only the program can move it, by the rules in the deployed program version, which is published here with verifiable builds so anyone can confirm mainnet runs this code. Not the team, not the creator, not the keeper.
- **Scores come from two primary sources that must agree** (API-Sports and Polymarket's public sports stream, which carries Sportradar data), with ESPN as a standby. If a primary goes quiet, ESPN stands in for it and still has to agree with the other. If the two sources being relied on say different things, nothing is paid for that game until it's resolved. If ESPN alone disagrees, the team is alerted but the post goes ahead; ESPN never decides anything on its own.
- **Quarters go in order and can't be rushed.** Scores are posted one quarter at a time, in order, and can only go up. Because a quarter is 15 minutes of game clock, the program refuses any score posted sooner than 15 minutes after kickoff (Q1) or after the previous post (later quarters) — so a whole game's scores can't be posted in seconds.
- **The digits are drawn on-chain, after the grid is full,** from a public commit-reveal randomness program (Entropy, Regolith Labs' design, run as MyBarPool's own verified deployment; see [Randomness](#randomness)), and recorded on the pool. Nobody can know them while boxes are on sale, and there are no re-rolls.
- **Every payout is written on-chain.** The pool account keeps the quarter, the box and the amount; the settle event carries the score, the box, the wallet and the amount; and the app links each one to its transaction. Anyone can check any result against the chain's history.
- **Nothing about a pool changes after it's created.** Price, split, fees and rules are fixed on the pool account at creation.
- **Fee ceilings are constants in the program**, not settings: 5% platform, 15% total. Lowering them is a config change; raising them would need a new program version, which is public, verifiable and announced in advance.
- **If a game isn't played, everyone gets their money back in full.** Fees only ever come out of a pool that has actually paid a prize, and a sponsor's money follows the same line: back to them before the first prize, in the prizes after it.
- **Sponsored prizes are extra, never a cut.** When a company or a community adds to a pool's prizes, no fee is taken on it and the platform can't do it itself; what a sponsor puts in is what the winners get.

Who holds which keys:

| Who | Can do | Can't do |
|---|---|---|
| The keeper (MyBarPool's automated service) | Post scores and kickoff changes, run the draw, pay quarters, return unfilled pools, carry out returns and splits the team has approved | Pay anyone but the box that matches the score; skip or reorder a quarter; touch fees, prices or splits; return or split a locked pool without the team's mark (returning an unfilled pool after kickoff needs no mark) |
| The MyBarPool team (multisig) | Adjust config within the hard-coded ceilings (fee rates, price ladders, sponsorship caps, the default payout preset, creator limits), mark a game postponed, cancelled or suspended, cancel a pool before its first settlement, replace a failed draw (publicly, with an event), publish new program versions | Move money to itself; redirect a payout; change a pool's price, split or fees. It can only stop a pool through the documented cancel/suspend instructions, each of which emits an event |
| You | Buy boxes, create pools within the limits, run your own private pools, add to any pool's prizes before kickoff | Nothing that needs asking for: no claims, no disputes, no approvals |

The keeper's code is closed; how it makes decisions is described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). The program that holds the money and enforces every rule above is what's published here, with verifiable builds, so the keeper's actions can be checked against the program.

## Building on the program

Anyone may build a frontend, bot or integration on the program. Everything that matters is enforced on-chain, not in a client:

- The platform fee, creator fee, add-on and integrator fee are paid inside `settle`.
- Price steps and creator limits are checked on `create_pool` and `buy`.
- Private-pool gating is checked on `buy`.
- The keeper serves every pool on the program, whichever client created it, at the platform's cost.

What a client chooses: token, price, split, add-on, integrator fee, public or private, which of the allowed tokens its UI offers, and whether to expose sponsoring. What the platform sets: the list of allowed tokens and mints, price steps, fee ceilings, sponsorship caps, creator limits. Sponsorship is program-level, so a pool sponsored through one client shows as sponsored in every client that reads the chain.

How a client earns: create pools itself on its own site (5% creator share plus up to 5% add-on, with a per-wallet override for the open-pool limit on request), or let its users create pools and set itself as `integrator` for a share of the add-on budget. A private `link` pool whose gate key is held by the client's server is effectively public on that site and absent from the MyBarPool app; that is an intended use.

Third parties pay for their own RPC. The SDK takes a connection as a parameter and ships with no default endpoint. Pool state is read directly from the chain, so a client needs nothing from MyBarPool's servers to work. A public pools/games listing API may be offered as a best-effort convenience, rate-limited and without SLA; nothing in the SDK depends on it.

Clients must show a box as **won** only after the settle event, never from live scores. Live scores may mark a box as **leading**, clearly provisional.

The program specification, [docs/PROGRAM.md](docs/PROGRAM.md), is the contract: every account and its fields, every instruction with its checks, every event and error, and the exact box-assignment, shuffle and winner algorithms. It is what the program is built and audited against, and what an integrator's own verification can be built against. Integration guide, SDK reference and the official program ID and config PDA will be published here when the program is deployed. Check the program ID against this repo before trusting a deployment; forks are possible and will not be served by the keeper.

## What is in this repo

```
programs/mybarpool/   Solana program (Anchor) — Apache-2.0, in development step by step
packages/shared/      TypeScript SDK: types, instruction builders, PDAs,
                      grid and payout math, 1–25 box labelling, team table — Apache-2.0, likewise
docs/ARCHITECTURE.md  The full design: rules, lifecycle, fees, randomness, trust model,
                      program accounts, private pools, decisions log
docs/PROGRAM.md       Program specification: accounts, seeds, instructions, checks, events,
                      errors, state machines, algorithms
docs/DESIGN.md        Brand, design system, information architecture, screen wireframes, flows,
                      app build specification
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

<table>
<tr><td align="center"><img src="docs/mockups/png/games.png" width="390" alt="Games"><br><sub>Games, the home screen</sub></td><td align="center"><img src="docs/mockups/png/pool-open.png" width="390" alt="Pool open"><br><sub>A pool selling boxes</sub></td></tr>
<tr><td align="center"><img src="docs/mockups/png/pool-live.png" width="390" alt="Pool live"><br><sub>The same pool live: Q1 paid, Q2 leading</sub></td><td align="center"><img src="docs/mockups/png/buy.png" width="390" alt="Buy sheet"><br><sub>Buying three boxes</sub></td></tr>
<tr><td align="center"><img src="docs/mockups/png/create.png" width="390" alt="Create"><br><sub>Creating a pool</sub></td><td align="center"><img src="docs/mockups/png/pool-sponsored.png" width="390" alt="Pool sponsored"><br><sub>A sponsored pool</sub></td></tr>
</table>

All eleven frames, including the share sheet, the sponsor sheet and My Boxes, are in [docs/DESIGN.md](docs/DESIGN.md#rendered-mockups). Design principles, the full information architecture and every flow, including how a pool is shared (link, preview, QR, printable board), are in [docs/DESIGN.md](docs/DESIGN.md).

## Toolchain

Latest stable Solana tooling only, pinned in-repo, verifiable builds. Pinned today: Agave/Solana CLI 4.3.0, Anchor 1.2.0, platform-tools v1.57, Surfpool 1.6.0, Mollusk 0.16.0, `solana-verify` 0.5.2, `@solana/kit` 8.4.0, Node 22. The program is built as SBPFv3 (`anchor build --arch v3`; CI fails unless the ELF header says so), and the deployed program will be verified with `solana-verify` so anyone can confirm mainnet runs the code in this repo. Localnet tests run against the Entropy fork's verified bytecode (the fixture in `programs/mybarpool/tests/fixtures/`, which CI rebuilds from the fork at the pinned commit and compares by hash) placed at its program id, and against the real SKR and ORE mints, fetched from mainnet by Surfpool on first use.

To build and test locally with the pins: install Rust (`rust-toolchain.toml` picks the version), the Solana CLI 4.3.0, Anchor 1.2.0 and Surfpool 1.6.0, then

```bash
anchor build --arch v3        # program + IDL; readelf -h target/deploy/mybarpool.so shows Flags: 0x3
cargo test -p mybarpool       # Mollusk unit tests
cargo bench -p mybarpool      # rewrites programs/mybarpool/compute_units.md
npm ci && npm test            # packages/shared
anchor test --skip-build      # Surfpool localnet suite, forking mainnet
```

## Licence, trademarks, security

- Code in this repository is licensed under [Apache-2.0](LICENSE).
- The licence covers the code, not the name or the sign. "MyBarPool", the neon-sign logo and mybarpool.com are trademarks; see [TRADEMARKS.md](TRADEMARKS.md). NFL team names and logos belong to the NFL and its clubs; MyBarPool is not affiliated with or endorsed by them, and uses no team logos (teams are shown by abbreviation and colors on a generic helmet).
- Found a vulnerability? See [SECURITY.md](SECURITY.md). Please don't open a public issue for it.
- An independent audit of the program will be completed and published before the first mainnet deploy.
