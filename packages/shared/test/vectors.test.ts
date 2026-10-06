/**
 * Shared test vectors (PROGRAM §6: "cross-tested against the program with
 * shared vector files"). The program's Rust tests load `src/vectors/*.json`
 * in Steps 4 and 5; this test regenerates every file from the TypeScript
 * implementation and fails if the committed copy differs, so they cannot
 * drift. Regenerate with `npm run vectors`.
 *
 * Formats: bytes are lowercase hex; u64 amounts are decimal strings (JSON
 * numbers cannot hold a u64); box and digit indices are plain numbers. An
 * unowned box is 32 zero bytes, the program's `Pubkey::default()`.
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { assignBoxes, type Owner } from "../src/assignment.js";
import { drawAxes } from "../src/axes.js";
import { BOXES } from "../src/boxes.js";
import { toHex } from "../src/bytes.js";
import { PayoutPreset, feeAmounts, pot, prizePool, quarterPrizes } from "../src/fees.js";
import { INITIAL_LADDERS, priceLadderSteps, skrLadder } from "../src/price.js";
import { winningBox } from "../src/winner.js";
import { Prng } from "./prng.js";

const DIR = fileURLToPath(new URL("../src/vectors/", import.meta.url));
const UPDATE = process.env["UPDATE_VECTORS"] === "1";
const ENTRIES = 24;

const ZERO = toHex(new Uint8Array(32));
const ownerHex = (o: Owner) => (o === null ? ZERO : toHex(o));

function assignmentVectors() {
  const rng = new Prng("vectors/assignment");
  const entries = [];
  for (let t = 0; t < ENTRIES; t++) {
    const owners: Owner[] = Array.from({ length: BOXES }, () => null);
    // Cover the edges: empty grid, one box left, and random fills between.
    const sold = t === 0 ? 0 : t === 1 ? 24 : rng.int(BOXES);
    const taken = new Set<number>();
    while (taken.size < sold) taken.add(rng.int(BOXES));
    for (const i of taken) owners[i] = rng.pubkey();
    const count = t === 2 ? BOXES - sold : rng.between(1, BOXES - sold);
    const slothash = rng.bytes(32);
    const buyer = rng.pubkey();
    const result = assignBoxes({ slothash, buyer, sold, count, owners });
    entries.push({
      slothash: toHex(slothash),
      buyer: toHex(buyer),
      sold,
      count,
      owners: owners.map(ownerHex),
      boxes: result.boxes,
      ownersAfter: result.owners.map(ownerHex),
    });
  }
  return { spec: "PROGRAM §6.1 box assignment", entries };
}

function axesVectors() {
  const rng = new Prng("vectors/axes");
  const entries = [];
  for (let t = 0; t < ENTRIES; t++) {
    const value =
      t === 0 ? new Uint8Array(32) : t === 1 ? new Uint8Array(32).fill(0xff) : rng.bytes(32);
    const { home, away } = drawAxes(value);
    entries.push({ value: toHex(value), home: [...home], away: [...away] });
  }
  return { spec: "PROGRAM §6.2 axis shuffle; home = columns, away = rows", entries };
}

function winnerVectors() {
  const rng = new Prng("vectors/winner");
  const entries = [];
  for (let t = 0; t < ENTRIES; t++) {
    const { home: homeAxis, away: awayAxis } = drawAxes(rng.bytes(32));
    const home = t % 4 === 0 ? rng.int(10) : rng.int(70);
    const away = t % 4 === 1 ? rng.int(10) : rng.int(70);
    entries.push({
      homeAxis: [...homeAxis],
      awayAxis: [...awayAxis],
      home,
      away,
      box: winningBox({ home, away, homeAxis, awayAxis }),
    });
  }
  return { spec: "PROGRAM §6.3 winner; box = row × 5 + col, 0-based", entries };
}

function feeVectors() {
  const rng = new Prng("vectors/fees");
  const ladders = [
    { name: "SOL", ladder: INITIAL_LADDERS.SOL },
    { name: "ORE", ladder: INITIAL_LADDERS.ORE },
    { name: "SKR@6", ladder: skrLadder(6) },
  ];
  const entries = [];
  // The ARCHITECTURE worked examples first, then random cases.
  const fixed = [
    {
      price: 50_000_000n,
      platformBps: 500,
      creatorBps: 500,
      creatorAddonBps: 0,
      integratorBps: 0,
      sponsoredTotal: 0n,
      preset: PayoutPreset.Standard,
      token: "SOL",
    },
    {
      price: 50_000_000n,
      platformBps: 500,
      creatorBps: 500,
      creatorAddonBps: 0,
      integratorBps: 0,
      sponsoredTotal: 0n,
      preset: PayoutPreset.Even,
      token: "SOL",
    },
    {
      price: 50_000_000n,
      platformBps: 500,
      creatorBps: 500,
      creatorAddonBps: 200,
      integratorBps: 0,
      sponsoredTotal: 0n,
      preset: PayoutPreset.Standard,
      token: "SOL",
    },
    {
      price: 50_000_000n,
      platformBps: 500,
      creatorBps: 500,
      creatorAddonBps: 200,
      integratorBps: 0,
      sponsoredTotal: 1_000_000_000n,
      preset: PayoutPreset.Standard,
      token: "SOL",
    },
    {
      price: 50_000_000n,
      platformBps: 500,
      creatorBps: 500,
      creatorAddonBps: 500,
      integratorBps: 0,
      sponsoredTotal: 0n,
      preset: PayoutPreset.Standard,
      token: "SOL",
    },
    {
      price: 50_000_000n,
      platformBps: 500,
      creatorBps: 500,
      creatorAddonBps: 0,
      integratorBps: 200,
      sponsoredTotal: 0n,
      preset: PayoutPreset.FinalOnly,
      token: "SOL",
    },
  ];
  const cases = [...fixed];
  while (cases.length < ENTRIES) {
    const { name, ladder } = rng.pick(ladders);
    const price = rng.pick(priceLadderSteps(ladder));
    const creatorAddonBps = rng.int(501);
    cases.push({
      price,
      platformBps: rng.int(501),
      creatorBps: rng.int(501),
      creatorAddonBps,
      integratorBps: rng.int(501 - creatorAddonBps),
      sponsoredTotal:
        rng.int(2) === 0 ? 0n : price + rng.bigint(25n * ladder.maxPrice - price + 1n),
      preset: rng.pick([PayoutPreset.Standard, PayoutPreset.Even, PayoutPreset.FinalOnly]),
      token: name,
    });
  }
  for (const c of cases) {
    const P = pot(c.price);
    const fees = feeAmounts(c);
    const pool = prizePool({ pot: P, fees, sponsoredTotal: c.sponsoredTotal });
    const { quarters, dust } = quarterPrizes(pool, c.preset);
    entries.push({
      token: c.token,
      price: c.price.toString(),
      platformBps: c.platformBps,
      creatorBps: c.creatorBps,
      creatorAddonBps: c.creatorAddonBps,
      integratorBps: c.integratorBps,
      sponsoredTotal: c.sponsoredTotal.toString(),
      preset: c.preset,
      pot: P.toString(),
      platformFee: fees.platformFee.toString(),
      creatorFee: fees.creatorFee.toString(),
      integratorFee: fees.integratorFee.toString(),
      prizePool: pool.toString(),
      quarters: quarters.map((q) => q.toString()),
      dust: dust.toString(),
    });
  }
  return {
    spec: "PROGRAM §5.1–5.2 fees and prizes; amounts are u64 base units as decimal strings",
    entries,
  };
}

/**
 * Step 4's 1,000-vector acceptance check, compact: the owner keys are irrelevant to the
 * algorithm (only which boxes are taken matters), so `owned` lists the taken indices and the
 * program plants arbitrary keys there.
 */
function assignmentBulkVectors() {
  const rng = new Prng("vectors/assignment-bulk");
  const entries = [];
  for (let t = 0; t < 1_000; t++) {
    const sold = rng.int(BOXES);
    const taken = new Set<number>();
    while (taken.size < sold) taken.add(rng.int(BOXES));
    const owned = [...taken].sort((a, b) => a - b);
    const owners: Owner[] = Array.from({ length: BOXES }, () => null);
    for (const i of owned) owners[i] = rng.pubkey();
    const count = rng.between(1, BOXES - sold);
    const slothash = rng.bytes(32);
    const buyer = rng.pubkey();
    const { boxes } = assignBoxes({ slothash, buyer, sold, count, owners });
    entries.push({ slothash: toHex(slothash), buyer: toHex(buyer), sold, count, owned, boxes });
  }
  return { spec: "PROGRAM §6.1 box assignment, 1,000 random purchases", entries };
}

/** Step 4's fee cross-check over random inputs; §5.1 amounts only, no prize split. */
function feeBulkVectors() {
  const rng = new Prng("vectors/fees-bulk");
  const ladders = [INITIAL_LADDERS.SOL, INITIAL_LADDERS.ORE, skrLadder(6)];
  const entries = [];
  for (let t = 0; t < 200; t++) {
    const price = rng.pick(priceLadderSteps(rng.pick(ladders)));
    const creatorAddonBps = rng.int(501);
    const c = {
      price,
      platformBps: rng.int(501),
      creatorBps: rng.int(501),
      creatorAddonBps,
      integratorBps: rng.int(501 - creatorAddonBps),
    };
    const fees = feeAmounts(c);
    entries.push({
      price: price.toString(),
      platformBps: c.platformBps,
      creatorBps: c.creatorBps,
      creatorAddonBps: c.creatorAddonBps,
      integratorBps: c.integratorBps,
      platformFee: fees.platformFee.toString(),
      creatorFee: fees.creatorFee.toString(),
      integratorFee: fees.integratorFee.toString(),
    });
  }
  return { spec: "PROGRAM §5.1 fee amounts, 200 random inputs; u64 as decimal strings", entries };
}

const FILES = {
  "assignment.json": assignmentVectors,
  "axes.json": axesVectors,
  "winner.json": winnerVectors,
  "fees.json": feeVectors,
  "assignment-bulk.json": assignmentBulkVectors,
  "fees-bulk.json": feeBulkVectors,
} as const;

describe("shared vector files (PROGRAM §6)", () => {
  for (const [file, generate] of Object.entries(FILES)) {
    it(`${file} matches the implementation${UPDATE ? " (rewritten)" : ""}`, () => {
      const generated = JSON.stringify(generate(), null, 2) + "\n";
      const path = DIR + file;
      if (UPDATE) {
        mkdirSync(DIR, { recursive: true });
        writeFileSync(path, generated);
      }
      expect(existsSync(path), `${file} is missing; run npm run vectors`).toBe(true);
      expect(readFileSync(path, "utf8")).toBe(generated);
      expect(JSON.parse(generated).entries.length).toBeGreaterThanOrEqual(20);
    });
  }
});
