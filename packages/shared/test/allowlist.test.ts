/**
 * PROGRAM §6.4 allowlist root and proof: the hand-asserted single-wallet case, then
 * properties over `Prng`-generated lists. The cross-language check against the program is
 * `vectors.test.ts` → `src/vectors/allowlist.json` → the program's `tests/vectors.rs`.
 */
import { sha256 } from "@noble/hashes/sha2.js";
import { describe, expect, it } from "vitest";

import {
  ALLOWLIST_MAX_PROOF,
  allowlistLeaf,
  allowlistNode,
  allowlistProof,
  allowlistRoot,
  canonicalAllowlist,
  verifyAllowlistProof,
} from "../src/allowlist.js";
import { Prng } from "./prng.js";

const hex = (b: Uint8Array) => Buffer.from(b).toString("hex");

describe("allowlist (PROGRAM §6.4)", () => {
  it("one wallet: root = leaf = sha256(0x00 || w), proof = [] (§6.4)", () => {
    const w = new Uint8Array(32).fill(7);
    const leaf = sha256(new Uint8Array([0x00, ...w]));
    expect(hex(allowlistLeaf(w))).toBe(hex(leaf));
    expect(hex(allowlistRoot([w]))).toBe(hex(leaf));
    expect(allowlistProof([w], w)).toEqual([]);
    expect(verifyAllowlistProof(leaf, w, [])).toBe(true);
    expect(verifyAllowlistProof(leaf, new Uint8Array(32).fill(8), [])).toBe(false);
  });

  it("node hashes 0x01 || min || max, so node(a, b) == node(b, a) (§6.4)", () => {
    const a = new Uint8Array(32).fill(1);
    const b = new Uint8Array(32).fill(2);
    expect(hex(allowlistNode(a, b))).toBe(hex(sha256(new Uint8Array([0x01, ...a, ...b]))));
    expect(hex(allowlistNode(b, a))).toBe(hex(allowlistNode(a, b)));
  });

  it("two wallets: root = node(leaf(a), leaf(b)); each proof is the other's leaf", () => {
    const a = new Uint8Array(32).fill(3);
    const b = new Uint8Array(32).fill(4);
    const root = allowlistRoot([b, a]);
    expect(hex(root)).toBe(hex(allowlistNode(allowlistLeaf(a), allowlistLeaf(b))));
    expect(allowlistProof([a, b], a).map(hex)).toEqual([hex(allowlistLeaf(b))]);
    expect(allowlistProof([a, b], b).map(hex)).toEqual([hex(allowlistLeaf(a))]);
  });

  it("canonicalAllowlist sorts bytewise and deduplicates; the root does not depend on order", () => {
    const rng = new Prng("allowlist/canonical");
    const base = Array.from({ length: 20 }, () => rng.pubkey());
    const shuffled = [...base, ...base.slice(0, 7)].sort(() => rng.int(3) - 1);
    const canonical = canonicalAllowlist(shuffled);
    expect(canonical.length).toBe(20);
    for (let i = 1; i < canonical.length; i++) {
      expect(Buffer.compare(canonical[i - 1]!, canonical[i]!)).toBeLessThan(0);
    }
    expect(hex(allowlistRoot(shuffled))).toBe(hex(allowlistRoot(base)));
    expect(allowlistProof(shuffled, base[3]!).map(hex)).toEqual(
      allowlistProof(base, base[3]!).map(hex),
    );
  });

  it("every member verifies, a non-member never does, lists of 1–200 (property)", () => {
    const rng = new Prng("allowlist/property");
    for (let trial = 0; trial < 40; trial++) {
      const n = rng.between(1, 200);
      const wallets = Array.from({ length: n }, () => rng.pubkey());
      const root = allowlistRoot(wallets);
      const depth = Math.ceil(Math.log2(Math.max(n, 1)));
      for (const w of wallets.filter((_, i) => i % 23 === 0 || i === n - 1)) {
        const proof = allowlistProof(wallets, w);
        expect(verifyAllowlistProof(root, w, proof), `n=${n}`).toBe(true);
        // At most ceil(log2 n) entries; fewer where the node was carried up unpaired, which can
        // happen at more than one level (n = 73: the last wallet's proof has 5 entries, not 6).
        expect(proof.length, `n=${n}`).toBeLessThanOrEqual(depth);
        if (n > 1) expect(proof.length).toBeGreaterThanOrEqual(1);
      }
      // The first canonical wallet is paired at every level, so its proof is exactly the depth.
      expect(allowlistProof(wallets, canonicalAllowlist(wallets)[0]!).length).toBe(depth);
      const stranger = rng.pubkey();
      expect(verifyAllowlistProof(root, stranger, allowlistProof(wallets, wallets[0]!))).toBe(
        false,
      );
    }
  }, 20_000);

  it("the root changes when any wallet changes", () => {
    const rng = new Prng("allowlist/change");
    const wallets = Array.from({ length: 33 }, () => rng.pubkey());
    const root = hex(allowlistRoot(wallets));
    for (let i = 0; i < wallets.length; i += 4) {
      const changed = wallets.map((w, j) => (j === i ? rng.pubkey() : w));
      expect(hex(allowlistRoot(changed))).not.toBe(root);
    }
  });

  it("a proof of more than 32 entries is invalid even when a prefix would verify", () => {
    const w = new Uint8Array(32).fill(9);
    const siblings = Array.from({ length: 33 }, (_, i) => new Uint8Array(32).fill(0x50 + i));
    let acc = allowlistLeaf(w);
    for (const s of siblings.slice(0, ALLOWLIST_MAX_PROOF)) acc = allowlistNode(acc, s);
    expect(verifyAllowlistProof(acc, w, siblings.slice(0, 32))).toBe(true);
    expect(verifyAllowlistProof(acc, w, siblings)).toBe(false);
    expect(ALLOWLIST_MAX_PROOF).toBe(32);
  });

  it("refuses bad input", () => {
    expect(() => allowlistRoot([])).toThrow(RangeError);
    expect(() => allowlistLeaf(new Uint8Array(31))).toThrow(RangeError);
    const w = new Uint8Array(32).fill(1);
    expect(() => allowlistProof([w], new Uint8Array(32).fill(2))).toThrow(RangeError);
    expect(canonicalAllowlist([])).toEqual([]);
  });
});
