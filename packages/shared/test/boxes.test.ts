import { describe, expect, it } from "vitest";

import { BOXES, LANES, QUARTERS, colOf, indexAt, rowOf, toIndex, toLabel } from "../src/boxes.js";

describe("grid constants (PROGRAM §1)", () => {
  it("25 boxes, 5 lanes, 4 quarters", () => {
    expect(BOXES).toBe(25); // PROGRAM §1 BOXES
    expect(LANES).toBe(5); // PROGRAM §1 LANES
    expect(QUARTERS).toBe(4); // PROGRAM §1 QUARTERS
    expect(LANES * LANES).toBe(BOXES);
  });
});

describe("box labels (ARCHITECTURE › Buying › Numbering; PROGRAM conventions)", () => {
  it("index 0 is label 1, top-left", () => {
    expect(toLabel(0)).toBe(1);
    expect(rowOf(0)).toBe(0);
    expect(colOf(0)).toBe(0);
  });

  it("index 24 is label 25, bottom-right", () => {
    expect(toLabel(24)).toBe(25);
    expect(rowOf(24)).toBe(4);
    expect(colOf(24)).toBe(4);
  });

  it("index 7 is label 8, row 1 col 2: numbering runs left to right, then down", () => {
    expect(toLabel(7)).toBe(8);
    expect(rowOf(7)).toBe(1);
    expect(colOf(7)).toBe(2);
    expect(indexAt(1, 2)).toBe(7);
  });

  it("round-trips every box", () => {
    for (let i = 0; i < BOXES; i++) {
      expect(toIndex(toLabel(i))).toBe(i);
      expect(indexAt(rowOf(i), colOf(i))).toBe(i);
    }
    for (let label = 1; label <= BOXES; label++) {
      expect(toLabel(toIndex(label))).toBe(label);
    }
  });

  it("rejects out-of-range indices and labels", () => {
    expect(() => toLabel(-1)).toThrow(RangeError);
    expect(() => toLabel(25)).toThrow(RangeError);
    expect(() => toLabel(1.5)).toThrow(RangeError);
    expect(() => toIndex(0)).toThrow(RangeError);
    expect(() => toIndex(26)).toThrow(RangeError);
    expect(() => rowOf(25)).toThrow(RangeError);
    expect(() => colOf(-1)).toThrow(RangeError);
    expect(() => indexAt(5, 0)).toThrow(RangeError);
    expect(() => indexAt(0, -1)).toThrow(RangeError);
  });
});
