import { describe, expect, it } from "vitest";

import {
  AMOUNT_MAX_DECIMALS,
  ORE,
  SOL,
  formatAmount,
  formatPercent,
  formatPrice,
  skr,
  shortAddress,
} from "../src/format.js";

describe("formatting (DESIGN §10.3; ARCHITECTURE › Buying)", () => {
  it("token descriptors", () => {
    expect(SOL).toEqual({ symbol: "SOL", decimals: 9, priceDecimals: 2 });
    expect(ORE).toEqual({ symbol: "ORE", decimals: 11, priceDecimals: 2 });
    expect(skr(6)).toEqual({ symbol: "SKR", decimals: 6, priceDecimals: 0 });
    expect(() => skr(-1)).toThrow(RangeError);
    expect(AMOUNT_MAX_DECIMALS).toBe(4);
  });

  it("box prices in natural precision: SOL/ORE two decimals, SKR whole", () => {
    expect(formatPrice(50_000_000n, SOL)).toBe("0.05 SOL");
    expect(formatPrice(1_000_000_000n, SOL)).toBe("1.00 SOL");
    expect(formatPrice(5_000_000_000n, ORE)).toBe("0.05 ORE");
    expect(formatPrice(100n * 10n ** 6n, skr(6))).toBe("100 SKR");
    expect(formatPrice(5_000n * 10n ** 6n, skr(6))).toBe("5,000 SKR");
    expect(formatPrice(100n, skr(0))).toBe("100 SKR");
  });

  it("prizes and fees to at most four decimals, half-up, trimmed to at least two for SOL/ORE", () => {
    expect(formatAmount(450_000_000n, SOL)).toBe("0.45 SOL"); // DESIGN §10.3 "0.45 SOL"
    expect(formatAmount(281_250_000n, SOL)).toBe("0.2813 SOL"); // ARCHITECTURE › Buying: 0.28125 → 0.2813
    expect(formatAmount(225_000_000n, SOL)).toBe("0.225 SOL");
    expect(formatAmount(62_500_000n, SOL)).toBe("0.0625 SOL");
    expect(formatAmount(87_500_000n, SOL)).toBe("0.0875 SOL");
    expect(formatAmount(212_500_000n, SOL)).toBe("0.2125 SOL");
    expect(formatAmount(1_100_000_000n, SOL)).toBe("1.10 SOL");
    expect(formatAmount(25_000_000_000n, SOL)).toBe("25.00 SOL");
    expect(formatAmount(0n, SOL)).toBe("0.00 SOL");
    expect(formatAmount(42_000_000_000n, ORE)).toBe("0.42 ORE");
    // Half-up at the fifth decimal: 0.00004 → 0.00, 0.00005 → 0.0001.
    expect(formatAmount(40_000n, SOL)).toBe("0.00 SOL");
    expect(formatAmount(50_000n, SOL)).toBe("0.0001 SOL");
    expect(formatAmount(999_950_000n, SOL)).toBe("1.00 SOL");
  });

  it("SKR amounts trim to no decimals and keep thousands separators", () => {
    expect(formatAmount(2_250n * 10n ** 6n, skr(6))).toBe("2,250 SKR"); // DESIGN §10.3 "2,250 SKR"
    expect(formatAmount(1_250n * 10n ** 6n, skr(6))).toBe("1,250 SKR");
    expect(formatAmount(125_000n * 10n ** 9n, skr(9))).toBe("125,000 SKR");
    expect(formatAmount(2_250_500_000n, skr(6))).toBe("2,250.5 SKR");
    expect(formatAmount(1_234_567n, skr(0))).toBe("1,234,567 SKR");
    expect(formatAmount(999n, skr(0))).toBe("999 SKR");
    expect(formatAmount(1_000n, skr(0))).toBe("1,000 SKR");
    // A mint with fewer than four decimals is exact, never padded.
    expect(formatAmount(12_345n, skr(2))).toBe("123.45 SKR");
  });

  it("the exact base-unit amount is untouched by display rounding", () => {
    const exact = 281_250_000n;
    formatAmount(exact, SOL);
    expect(exact).toBe(281_250_000n);
  });

  it("percentages as integers; a fractional bps shows its fraction rather than lying", () => {
    expect(formatPercent(1200)).toBe("12%"); // DESIGN §10.3 "Fee 12%"
    expect(formatPercent(1000)).toBe("10%");
    expect(formatPercent(500)).toBe("5%");
    expect(formatPercent(700)).toBe("7%");
    expect(formatPercent(0)).toBe("0%");
    expect(formatPercent(1250)).toBe("12.5%");
    expect(formatPercent(1225)).toBe("12.25%");
    expect(() => formatPercent(-1)).toThrow(RangeError);
    expect(() => formatPercent(1.5)).toThrow(RangeError);
  });

  it("rejects negative amounts", () => {
    expect(() => formatAmount(-1n, SOL)).toThrow(RangeError);
    expect(() => formatPrice(-1n, SOL)).toThrow(RangeError);
  });
});

describe("shortAddress (DESIGN §10.3)", () => {
  it("first four, an ellipsis, the last two; short strings untouched", () => {
    expect(shortAddress("7kq2abcdefghijklmnopqrstuvwxyz9a")).toBe("7kq2…9a");
    expect(shortAddress("abcdefg")).toBe("abcdefg");
    expect(shortAddress("abcdefgh")).toBe("abcd…gh");
  });
});
