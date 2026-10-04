import { describe, expect, it } from "vitest";

import {
  U64_MAX,
  ascii,
  assertU64,
  bytesEqual,
  concatBytes,
  fromHex,
  i64le,
  sha256,
  toHex,
  u16le,
  u64le,
  u8,
} from "../src/bytes.js";

describe("byte helpers", () => {
  it("sha256 hashes the concatenation of its parts (the hashv contract, PROGRAM §6)", () => {
    // FIPS 180-4 test vector: sha256("abc").
    expect(toHex(sha256(ascii("abc")))).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    expect(toHex(sha256(ascii("a"), ascii("bc")))).toBe(toHex(sha256(ascii("abc"))));
  });

  it("u64le reads the first eight bytes little-endian as a bigint", () => {
    expect(u64le(fromHex("0100000000000000ff"))).toBe(1n);
    expect(u64le(fromHex("ffffffffffffffff"))).toBe(U64_MAX);
    expect(u64le(fromHex("0001000000000000"))).toBe(256n);
    expect(() => u64le(new Uint8Array(7))).toThrow(RangeError);
  });

  it("fixed-width little-endian encoders reject out-of-range values", () => {
    expect([...u8(255)]).toEqual([255]);
    expect(() => u8(256)).toThrow(RangeError);
    expect(() => u8(-1)).toThrow(RangeError);
    expect([...u16le(2026)]).toEqual([0xea, 0x07]);
    expect(() => u16le(65_536)).toThrow(RangeError);
    expect(toHex(i64le(-1n))).toBe("ffffffffffffffff");
    expect(toHex(i64le(1n))).toBe("0100000000000000");
    expect(() => i64le(1n << 63n)).toThrow(RangeError);
    expect(() => i64le(-(1n << 63n) - 1n)).toThrow(RangeError);
  });

  it("ascii rejects non-ASCII and hex round-trips", () => {
    expect([...ascii("game")]).toEqual([0x67, 0x61, 0x6d, 0x65]);
    expect(() => ascii("é")).toThrow(RangeError);
    expect(toHex(fromHex("00ff10"))).toBe("00ff10");
    expect(() => fromHex("abc")).toThrow(RangeError);
    expect(() => fromHex("zz")).toThrow(RangeError);
  });

  it("assertU64 and bytesEqual", () => {
    expect(assertU64(0n)).toBe(0n);
    expect(assertU64(U64_MAX)).toBe(U64_MAX);
    expect(() => assertU64(-1n)).toThrow(/u64/);
    expect(() => assertU64(U64_MAX + 1n, "pot")).toThrow(/pot/);
    expect(bytesEqual(fromHex("0102"), fromHex("0102"))).toBe(true);
    expect(bytesEqual(fromHex("0102"), fromHex("0103"))).toBe(false);
    expect(bytesEqual(fromHex("01"), fromHex("0102"))).toBe(false);
    expect(toHex(concatBytes())).toBe("");
  });
});
