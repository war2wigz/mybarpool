/**
 * Display formatting, DESIGN §10.3 and ARCHITECTURE › Buying.
 *
 * Box prices in each token's natural precision (SOL/ORE two decimals, SKR
 * whole). Prizes and fees to at most four decimals, rounded half-up for display
 * only, trailing zeros trimmed to at least the natural precision. Thousands
 * separators always. `bigint` all the way to the string; the exact base-unit
 * amount is what the transaction carries.
 */
import { pow10 } from "./price.js";

export interface TokenDescriptor {
  readonly symbol: string;
  /** Base-unit decimals of the mint (9 SOL, 11 ORE, SKR per mint). */
  readonly decimals: number;
  /** Natural precision for prices and the minimum shown on amounts: 2 for SOL/ORE, 0 for SKR. */
  readonly priceDecimals: number;
}

export const SOL: TokenDescriptor = { symbol: "SOL", decimals: 9, priceDecimals: 2 };
export const ORE: TokenDescriptor = { symbol: "ORE", decimals: 11, priceDecimals: 2 };

/** SKR's decimals are per mint (PROGRAM §3.1); the caller supplies them. */
export function skr(decimals: number): TokenDescriptor {
  pow10(decimals);
  return { symbol: "SKR", decimals, priceDecimals: 0 };
}

/** At most this many fraction digits on a prize or fee (DESIGN §10.3). */
export const AMOUNT_MAX_DECIMALS = 4;

function groupThousands(integer: string): string {
  let out = "";
  for (let i = 0; i < integer.length; i++) {
    const fromEnd = integer.length - i;
    if (i > 0 && fromEnd % 3 === 0) out += ",";
    out += integer[i];
  }
  return out;
}

/**
 * `amount` base units → `[integer, fraction]` strings with exactly `digits`
 * fraction digits, rounded half-up at that precision.
 */
function roundHalfUp(amount: bigint, decimals: number, digits: number): [string, string] {
  if (amount < 0n) throw new RangeError("amount must be ≥ 0");
  if (!Number.isInteger(digits) || digits < 0)
    throw new RangeError(`digits must be ≥ 0: ${digits}`);
  // Rescale to `digits` fraction digits: multiply up or divide down (half-up).
  let scaled: bigint;
  if (digits >= decimals) {
    scaled = amount * pow10(digits - decimals);
  } else {
    const divisor = pow10(decimals - digits);
    scaled = (amount + divisor / 2n) / divisor;
  }
  const unit = pow10(digits);
  const integer = (scaled / unit).toString();
  const fraction = digits === 0 ? "" : (scaled % unit).toString().padStart(digits, "0");
  return [integer, fraction];
}

function trimFraction(fraction: string, minDigits: number): string {
  let end = fraction.length;
  while (end > minDigits && fraction[end - 1] === "0") end--;
  return fraction.slice(0, end);
}

function join(integer: string, fraction: string, symbol: string): string {
  const number =
    fraction.length > 0 ? `${groupThousands(integer)}.${fraction}` : groupThousands(integer);
  return `${number} ${symbol}`;
}

/** A box price in natural precision: `"0.05 SOL"`, `"100 SKR"`. */
export function formatPrice(amount: bigint, token: TokenDescriptor): string {
  const [integer, fraction] = roundHalfUp(amount, token.decimals, token.priceDecimals);
  return join(integer, fraction, token.symbol);
}

/** A prize or fee: `"0.45 SOL"`, `"0.2813 SOL"`, `"2,250 SKR"`. */
export function formatAmount(amount: bigint, token: TokenDescriptor): string {
  const digits = Math.max(AMOUNT_MAX_DECIMALS, token.priceDecimals);
  const [integer, fraction] = roundHalfUp(amount, token.decimals, digits);
  return join(integer, trimFraction(fraction, token.priceDecimals), token.symbol);
}

/** Basis points as a percent: `1200` → `"12%"`. Whole percents by design (DESIGN §10.3); a fraction is shown only if the bps carry one. */
export function formatPercent(bps: number): string {
  if (!Number.isInteger(bps) || bps < 0)
    throw new RangeError(`bps must be a non-negative integer: ${bps}`);
  const whole = Math.floor(bps / 100);
  const rest = bps % 100;
  if (rest === 0) return `${whole}%`;
  const fraction = rest.toString().padStart(2, "0").replace(/0+$/, "");
  return `${whole}.${fraction}%`;
}
