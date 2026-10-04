import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import {
  ADDON_BUDGET_BPS_MAX,
  CREATOR_BPS_MAX,
  PLATFORM_BPS_MAX,
  PRESET_SPLITS,
  PayoutPreset,
  TOTAL_BPS_MAX,
  assertPayoutPreset,
  creatorBreakEvenBoxes,
  feeAmounts,
  isPayoutPreset,
  pot,
  prizePool,
  quarterPrizes,
  validateFeeConfig,
} from "../src/fees.js";
import { INITIAL_LADDERS, priceLadderSteps, skrLadder } from "../src/price.js";
import { Prng } from "./prng.js";

const SOL = 1_000_000_000n; // 9 decimals
const sol = (whole: number, fraction = 0n, fractionDigits = 0) =>
  BigInt(whole) * SOL + fraction * 10n ** BigInt(9 - fractionDigits);

// ARCHITECTURE › Fees: base fee 10% as 5% platform / 5% creator, add-on budget 5%.
const BASE = { platformBps: 500, creatorBps: 500 };

describe("constants and presets (PROGRAM §1)", () => {
  it("has the hard-coded ceilings", () => {
    expect(PLATFORM_BPS_MAX).toBe(500); // PROGRAM §1 PLATFORM_BPS_MAX
    expect(CREATOR_BPS_MAX).toBe(500); // PROGRAM §1 CREATOR_BPS_MAX
    expect(ADDON_BUDGET_BPS_MAX).toBe(500); // PROGRAM §1 ADDON_BUDGET_BPS_MAX
    expect(TOTAL_BPS_MAX).toBe(1500); // PROGRAM §1 TOTAL_BPS_MAX
  });

  it("has exactly three presets with the PROGRAM §1 discriminants and splits", () => {
    expect(PayoutPreset.Standard).toBe(0);
    expect(PayoutPreset.Even).toBe(1);
    expect(PayoutPreset.FinalOnly).toBe(2);
    expect(PRESET_SPLITS[PayoutPreset.Standard]).toEqual([20, 20, 20, 40]);
    expect(PRESET_SPLITS[PayoutPreset.Even]).toEqual([25, 25, 25, 25]);
    expect(PRESET_SPLITS[PayoutPreset.FinalOnly]).toEqual([0, 0, 0, 100]);
    expect(isPayoutPreset(3)).toBe(false);
    expect(() => assertPayoutPreset(3)).toThrow(RangeError);
    expect(() => assertPayoutPreset(-1)).toThrow(RangeError);
    expect(assertPayoutPreset(1)).toBe(PayoutPreset.Even);
  });
});

describe("worked numbers (ARCHITECTURE › Fees, Payouts, Buying; PROGRAM §5.3; DESIGN §4.3)", () => {
  const price = 50_000_000n; // 0.05 SOL

  it("0.05 SOL boxes, Standard, 10% base, no add-on → 0.225 / 0.225 / 0.225 / 0.45 (DESIGN §4.3 payouts table)", () => {
    const P = pot(price);
    expect(P).toBe(sol(1, 25n, 2)); // 1.25 SOL
    const fees = feeAmounts({ price, ...BASE, creatorAddonBps: 0, integratorBps: 0 });
    expect(fees).toEqual({
      platformFee: sol(0, 625n, 4),
      creatorFee: sol(0, 625n, 4),
      integratorFee: 0n,
    }); // 0.0625 / 0.0625 / 0
    const pool = prizePool({ pot: P, fees, sponsoredTotal: 0n });
    expect(pool).toBe(sol(1, 125n, 3)); // 1.125 SOL
    const { quarters, dust } = quarterPrizes(pool, PayoutPreset.Standard);
    expect(quarters).toEqual([sol(0, 225n, 3), sol(0, 225n, 3), sol(0, 225n, 3), sol(0, 45n, 2)]);
    expect(dust).toBe(0n);
  });

  it("0.05 SOL boxes, Even, 10% base → each quarter 0.28125 SOL = 281_250_000 base units (ARCHITECTURE › Buying)", () => {
    const fees = feeAmounts({ price, ...BASE, creatorAddonBps: 0, integratorBps: 0 });
    const pool = prizePool({ pot: pot(price), fees, sponsoredTotal: 0n });
    const { quarters, dust } = quarterPrizes(pool, PayoutPreset.Even);
    expect(quarters).toEqual([281_250_000n, 281_250_000n, 281_250_000n, 281_250_000n]);
    expect(dust).toBe(0n);
  });

  it("0.05 SOL, 2% add-on, Standard → fees 0.0625 / 0.0875 / 0; prize pool 1.1; quarters 0.22 × 3 / 0.44 (ARCHITECTURE › Fees worked example)", () => {
    const fees = feeAmounts({ price, ...BASE, creatorAddonBps: 200, integratorBps: 0 });
    expect(fees.platformFee).toBe(sol(0, 625n, 4)); // 0.0625
    expect(fees.creatorFee).toBe(sol(0, 875n, 4)); // 0.0875 = 0.0625 base + 0.025 add-on
    expect(fees.integratorFee).toBe(0n);
    const pool = prizePool({ pot: pot(price), fees, sponsoredTotal: 0n });
    expect(pool).toBe(sol(1, 1n, 1)); // 1.1 SOL
    const { quarters } = quarterPrizes(pool, PayoutPreset.Standard);
    expect(quarters).toEqual([sol(0, 22n, 2), sol(0, 22n, 2), sol(0, 22n, 2), sol(0, 44n, 2)]);
    // The Q1 settle moves 0.22 + 0.0625 + 0.0875: vault 1.25 → 0.88.
    expect(pot(price) - quarters[0] - fees.platformFee - fees.creatorFee).toBe(sol(0, 88n, 2));
  });

  it("same pool + 1 SOL sponsorship → fees unchanged; prize pool 2.1; quarters 0.42 × 3 / 0.84 (ARCHITECTURE › Fees; PROGRAM §5.3)", () => {
    const fees = feeAmounts({ price, ...BASE, creatorAddonBps: 200, integratorBps: 0 });
    const plain = prizePool({ pot: pot(price), fees, sponsoredTotal: 0n });
    const sponsored = prizePool({ pot: pot(price), fees, sponsoredTotal: sol(1) });
    expect(sponsored).toBe(sol(2, 1n, 1)); // 2.1 SOL
    expect(sponsored - plain).toBe(sol(1));
    const { quarters } = quarterPrizes(sponsored, PayoutPreset.Standard);
    expect(quarters).toEqual([sol(0, 42n, 2), sol(0, 42n, 2), sol(0, 42n, 2), sol(0, 84n, 2)]);
    // Vault 2.25 at kickoff → 1.68 after Q1.
    expect(pot(price) + sol(1) - quarters[0] - fees.platformFee - fees.creatorFee).toBe(
      sol(1, 68n, 2),
    );
  });

  it("0.05 SOL, 15% total fee, 20% share → smallest quarter prize 0.2125 SOL (ARCHITECTURE › Payouts; PROGRAM §5.3)", () => {
    const fees = feeAmounts({ price, ...BASE, creatorAddonBps: 500, integratorBps: 0 });
    const pool = prizePool({ pot: pot(price), fees, sponsoredTotal: 0n });
    const { quarters } = quarterPrizes(pool, PayoutPreset.Standard);
    expect(quarters[0]).toBe(sol(0, 2125n, 4));
  });

  it("integrator fee shares the add-on budget: 2% integrator, 0 add-on → creator 0.0625, client 0.025", () => {
    // ARCHITECTURE › Fees: "Fee 12% · platform 5% · creator 5% · client 2%".
    const fees = feeAmounts({ price, ...BASE, creatorAddonBps: 0, integratorBps: 200 });
    expect(fees).toEqual({
      platformFee: sol(0, 625n, 4),
      creatorFee: sol(0, 625n, 4),
      integratorFee: sol(0, 25n, 3),
    });
  });

  it("creator break-even is 12.5 boxes at the base fee (ARCHITECTURE › Limits)", () => {
    expect(creatorBreakEvenBoxes({ ...BASE, creatorAddonBps: 0, integratorBps: 0 })).toBe(12.5);
    // 7% creator of 12% total: 7 × 25 / 12.
    expect(creatorBreakEvenBoxes({ ...BASE, creatorAddonBps: 200, integratorBps: 0 })).toBeCloseTo(
      14.5833,
      4,
    );
  });

  it("largest pools: 25 SOL / 125,000 SKR / 25 ORE from the ladder maxima (ARCHITECTURE › Buying table)", () => {
    expect(pot(INITIAL_LADDERS.SOL.maxPrice)).toBe(25n * SOL);
    expect(pot(INITIAL_LADDERS.ORE.maxPrice)).toBe(25n * 10n ** 11n);
    expect(pot(skrLadder(6).maxPrice)).toBe(125_000n * 10n ** 6n);
    expect(pot(skrLadder(9).maxPrice)).toBe(125_000n * 10n ** 9n);
  });
});

describe("validation", () => {
  it("validateFeeConfig enforces every PROGRAM §3.1 / §4.3 bound", () => {
    const ok = { platformBps: 500, creatorBps: 500, addonBudgetBps: 500 };
    expect(() => validateFeeConfig(ok, { creatorAddonBps: 300, integratorBps: 200 })).not.toThrow();
    expect(() =>
      validateFeeConfig({ ...ok, platformBps: 501 }, { creatorAddonBps: 0, integratorBps: 0 }),
    ).toThrow(/platformBps/);
    expect(() =>
      validateFeeConfig({ ...ok, creatorBps: 501 }, { creatorAddonBps: 0, integratorBps: 0 }),
    ).toThrow(/creatorBps/);
    expect(() =>
      validateFeeConfig({ ...ok, addonBudgetBps: 501 }, { creatorAddonBps: 0, integratorBps: 0 }),
    ).toThrow(/addonBudgetBps/);
    expect(() => validateFeeConfig(ok, { creatorAddonBps: 300, integratorBps: 201 })).toThrow(
      /add-on budget/,
    );
    expect(() =>
      validateFeeConfig({ ...ok, addonBudgetBps: 100 }, { creatorAddonBps: 100, integratorBps: 1 }),
    ).toThrow(/add-on budget/);
    expect(() => validateFeeConfig(ok, { creatorAddonBps: -1, integratorBps: 0 })).toThrow(
      /creatorAddonBps/,
    );
    expect(() => validateFeeConfig(ok, { creatorAddonBps: 0, integratorBps: 1.5 })).toThrow(
      /integratorBps/,
    );
  });

  it("the 1500 ceiling is implied by the three 500 ceilings (PROGRAM §3.1), so validateFeeConfig need not check it", () => {
    expect(PLATFORM_BPS_MAX + CREATOR_BPS_MAX + ADDON_BUDGET_BPS_MAX).toBe(TOTAL_BPS_MAX);
    expect(() =>
      validateFeeConfig(
        { platformBps: 500, creatorBps: 500, addonBudgetBps: 500 },
        { creatorAddonBps: 500, integratorBps: 0 },
      ),
    ).not.toThrow();
  });

  it("feeAmounts rejects bad bps and a zero price", () => {
    const price = 50_000_000n;
    expect(() => feeAmounts({ price: 0n, ...BASE, creatorAddonBps: 0, integratorBps: 0 })).toThrow(
      /price/,
    );
    expect(() => feeAmounts({ price: -1n, ...BASE, creatorAddonBps: 0, integratorBps: 0 })).toThrow(
      RangeError,
    );
    expect(() =>
      feeAmounts({
        price,
        platformBps: 501,
        creatorBps: 500,
        creatorAddonBps: 0,
        integratorBps: 0,
      }),
    ).toThrow(/platformBps/);
    expect(() =>
      feeAmounts({
        price,
        platformBps: 500,
        creatorBps: 501,
        creatorAddonBps: 0,
        integratorBps: 0,
      }),
    ).toThrow(/creatorBps/);
    expect(() => feeAmounts({ price, ...BASE, creatorAddonBps: 501, integratorBps: 0 })).toThrow(
      /creatorAddonBps/,
    );
    expect(() => feeAmounts({ price, ...BASE, creatorAddonBps: 0, integratorBps: 501 })).toThrow(
      /integratorBps/,
    );
    expect(() => feeAmounts({ price, ...BASE, creatorAddonBps: 300, integratorBps: 300 })).toThrow(
      /exceeds 500/,
    );
  });

  it("prizePool rejects negative inputs and fees above the pot", () => {
    const fees = { platformFee: 1n, creatorFee: 1n, integratorFee: 0n };
    expect(() => prizePool({ pot: -1n, fees, sponsoredTotal: 0n })).toThrow(RangeError);
    expect(() => prizePool({ pot: 10n, fees, sponsoredTotal: -1n })).toThrow(RangeError);
    expect(() => prizePool({ pot: 1n, fees, sponsoredTotal: 0n })).toThrow(/exceed/);
    expect(prizePool({ pot: 2n, fees, sponsoredTotal: 5n })).toBe(5n);
  });

  it("quarterPrizes rejects a bad preset and negative pools; pot rejects u64 overflow", () => {
    expect(() => quarterPrizes(100n, 7 as PayoutPreset)).toThrow(RangeError);
    expect(() => quarterPrizes(-1n, PayoutPreset.Standard)).toThrow(RangeError);
    expect(() => pot((1n << 64n) - 1n)).toThrow(/u64/);
    expect(() => pot(1n << 64n)).toThrow(/u64/);
  });

  it("creatorBreakEvenBoxes rejects out-of-range bps and a zero total", () => {
    expect(() =>
      creatorBreakEvenBoxes({
        platformBps: 501,
        creatorBps: 500,
        creatorAddonBps: 0,
        integratorBps: 0,
      }),
    ).toThrow(/platformBps/);
    expect(() =>
      creatorBreakEvenBoxes({
        platformBps: 0,
        creatorBps: 0,
        creatorAddonBps: 0,
        integratorBps: 0,
      }),
    ).toThrow(/zero/);
    expect(
      creatorBreakEvenBoxes({
        platformBps: 500,
        creatorBps: 0,
        creatorAddonBps: 0,
        integratorBps: 500,
      }),
    ).toBe(0);
  });
});

describe("properties", () => {
  const rng = new Prng("fees");
  const ladders = [INITIAL_LADDERS.SOL, INITIAL_LADDERS.ORE, skrLadder(6), skrLadder(9)];
  const presets = [PayoutPreset.Standard, PayoutPreset.Even, PayoutPreset.FinalOnly];

  it("fees + Σ quarters + dust == pot + sponsoredTotal, and fees ignore the sponsorship (1,000 cases)", () => {
    for (let t = 0; t < 1_000; t++) {
      const ladder = rng.pick(ladders);
      const price = rng.pick(priceLadderSteps(ladder));
      const platformBps = rng.int(PLATFORM_BPS_MAX + 1);
      const creatorBps = rng.int(CREATOR_BPS_MAX + 1);
      const creatorAddonBps = rng.int(ADDON_BUDGET_BPS_MAX + 1);
      const integratorBps = rng.int(ADDON_BUDGET_BPS_MAX - creatorAddonBps + 1);
      // Sponsorship is 0, or at least one box price and at most 25 × max price (PROGRAM §3.1, §4.3).
      const sponsoredTotal =
        rng.int(2) === 0 ? 0n : price + rng.bigint(25n * ladder.maxPrice - price + 1n);
      const preset = rng.pick(presets);

      const P = pot(price);
      const fees = feeAmounts({ price, platformBps, creatorBps, creatorAddonBps, integratorBps });
      const feesWithout = feeAmounts({
        price,
        platformBps,
        creatorBps,
        creatorAddonBps,
        integratorBps,
      });
      expect(fees).toEqual(feesWithout);
      const pool = prizePool({ pot: P, fees, sponsoredTotal });
      const { quarters, dust } = quarterPrizes(pool, preset);
      const paid = quarters.reduce((s, q) => s + q, 0n);
      expect(fees.platformFee + fees.creatorFee + fees.integratorFee + paid + dust).toBe(
        P + sponsoredTotal,
      );
      expect(dust).toBeLessThanOrEqual(3n); // PROGRAM §5.2: dust ≤ 3 base units on any preset
      expect(dust).toBeGreaterThanOrEqual(0n);
    }
  });

  it("dust is at most divisor − 1 per share and never positive on FinalOnly", () => {
    for (let t = 0; t < 200; t++) {
      const pool = rng.bigint(10n ** 12n);
      expect(quarterPrizes(pool, PayoutPreset.FinalOnly).dust).toBe(0n);
      expect(quarterPrizes(pool, PayoutPreset.Even).dust).toBeLessThanOrEqual(3n);
      expect(quarterPrizes(pool, PayoutPreset.Standard).dust).toBeLessThanOrEqual(3n);
    }
  });
});

describe("no floating point in money paths", () => {
  it("fees.ts, price.ts and format.ts never call Number( on their way to an amount", () => {
    for (const file of ["fees.ts", "price.ts", "format.ts"]) {
      const source = readFileSync(
        fileURLToPath(new URL(`../src/${file}`, import.meta.url)),
        "utf8",
      );
      expect(source.includes("Number("), `${file} contains Number(`).toBe(false);
      expect(source.includes("parseFloat("), `${file} contains parseFloat(`).toBe(false);
    }
  });
});
