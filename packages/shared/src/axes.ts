/**
 * Axis shuffle, PROGRAM §6.2. The reference the program matches in Step 5.
 *
 * ```
 * fn axis(value, label):                 // label = b"home" or b"away"
 *     seed = sha256(value || label)
 *     a = [0,1,2,3,4,5,6,7,8,9]
 *     for i in 9 down to 1:
 *         r = sha256(seed || [i])
 *         j = u64_le(r[0..8]) mod (i + 1)
 *         swap(a[i], a[j])
 *     return a
 * ```
 *
 * Lane `l` (0–4) covers digits `a[l]` and `a[l + 5]`; the five lanes partition 0–9.
 * Home is the columns (across the top), away the rows (down the side).
 */
import { assertLane, LANES } from "./boxes.js";
import { ascii, sha256, u64le, u8 } from "./bytes.js";

export const DIGITS = 10;

/** A permutation of 0–9 as the program records it on the pool. */
export type Digits = readonly [
  number,
  number,
  number,
  number,
  number,
  number,
  number,
  number,
  number,
  number,
];

export type AxisLabel = "home" | "away";

export const ENTROPY_VALUE_BYTES = 32;

export function assertDigits(digits: readonly number[]): Digits {
  if (digits.length !== DIGITS) throw new RangeError(`axis must have ${DIGITS} digits`);
  const seen = new Set<number>();
  for (const d of digits) {
    if (!Number.isInteger(d) || d < 0 || d >= DIGITS || seen.has(d)) {
      throw new RangeError(`axis is not a permutation of 0–9: ${digits.join(",")}`);
    }
    seen.add(d);
  }
  return digits as unknown as Digits;
}

export function axis(value: Uint8Array, label: AxisLabel): Digits {
  if (value.length !== ENTROPY_VALUE_BYTES) {
    throw new RangeError(`value must be ${ENTROPY_VALUE_BYTES} bytes, got ${value.length}`);
  }
  const seed = sha256(value, ascii(label));
  const a = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
  for (let i = 9; i >= 1; i--) {
    const r = sha256(seed, u8(i));
    const j = Number(u64le(r) % BigInt(i + 1));
    [a[i], a[j]] = [a[j]!, a[i]!];
  }
  return a as unknown as Digits;
}

export function drawAxes(value: Uint8Array): { home: Digits; away: Digits } {
  return { home: axis(value, "home"), away: axis(value, "away") };
}

/** The two digits lane `lane` covers: `[a[lane], a[lane + 5]]`. */
export function laneDigits(digits: Digits, lane: number): [number, number] {
  assertLane(lane);
  return [digits[lane]!, digits[lane + LANES]!];
}

/** The lane (0–4) whose two digits include `digit`; exactly one exists for a permutation. */
export function laneOfDigit(digits: Digits, digit: number): number {
  if (!Number.isInteger(digit) || digit < 0 || digit >= DIGITS) {
    throw new RangeError(`digit out of range 0–9: ${digit}`);
  }
  const position = digits.indexOf(digit);
  if (position < 0) throw new RangeError(`axis does not contain digit ${digit}`);
  return position % LANES;
}
