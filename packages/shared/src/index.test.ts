import { describe, expect, it } from "vitest";

import { BOXES, LANES, QUARTERS } from "./index.js";

// PROGRAM §1 constants. These are the only numbers the empty package knows.
describe("grid constants", () => {
  it("has 25 boxes in 5 lanes per axis", () => {
    expect(BOXES).toBe(25); // PROGRAM §1 BOXES
    expect(LANES).toBe(5); // PROGRAM §1 LANES
    expect(LANES * LANES).toBe(BOXES);
  });

  it("settles four quarters", () => {
    expect(QUARTERS).toBe(4); // PROGRAM §1 QUARTERS
  });
});
