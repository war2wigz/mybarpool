import { describe, expect, it } from "vitest";

import {
  ANDROID_PACKAGE_PATTERN,
  SPONSOR_DIRECTORY,
  dappStoreLink,
  sponsorByWallet,
  validateSponsorEntry,
} from "../src/sponsors.js";

const ORE_WALLET = "oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp"; // the ORE mint, a well-formed base58 key
const OK = {
  wallet: ORE_WALLET,
  displayName: "ORE",
  logo: "https://ore.supply/logo.png",
  website: "https://ore.supply",
  androidPackage: "com.ore.app",
};

describe("sponsor directory (ARCHITECTURE › Sponsorship › Directory entry)", () => {
  it("starts empty and frozen", () => {
    expect(SPONSOR_DIRECTORY).toEqual([]);
    expect(Object.isFrozen(SPONSOR_DIRECTORY)).toBe(true);
    expect(sponsorByWallet(ORE_WALLET)).toBeUndefined();
  });

  it("accepts a well-formed entry, with or without an Android package", () => {
    expect(validateSponsorEntry(OK)).toBe(OK);
    const { androidPackage: _omit, ...noApp } = OK;
    expect(validateSponsorEntry(noApp)).toBe(noApp);
    expect(
      validateSponsorEntry({ ...OK, website: "https://ore.supply/about?x=1#top" }),
    ).toBeTruthy();
    expect(validateSponsorEntry({ ...OK, website: "https://sub.ore.supply:8443/" })).toBeTruthy();
  });

  it("rejects non-https websites and logos", () => {
    expect(() => validateSponsorEntry({ ...OK, website: "http://ore.supply" })).toThrow(/https/);
    expect(() => validateSponsorEntry({ ...OK, website: "javascript:alert(1)" })).toThrow(/https/);
    expect(() => validateSponsorEntry({ ...OK, website: "ore.supply" })).toThrow(/https/);
    expect(() => validateSponsorEntry({ ...OK, website: "https://" })).toThrow(/https/);
    expect(() => validateSponsorEntry({ ...OK, website: "https:// ore.supply" })).toThrow(/https/);
    expect(() => validateSponsorEntry({ ...OK, logo: "http://ore.supply/logo.png" })).toThrow(
      /logo/,
    );
  });

  it("rejects a bad wallet or an empty name", () => {
    expect(() => validateSponsorEntry({ ...OK, wallet: "not-base58-0OIl" })).toThrow(/wallet/);
    expect(() => validateSponsorEntry({ ...OK, wallet: "" })).toThrow(/wallet/);
    expect(() => validateSponsorEntry({ ...OK, displayName: "  " })).toThrow(/displayName/);
  });

  it("accepts com.ore.app; rejects ore, 1com.x, com..x", () => {
    expect(ANDROID_PACKAGE_PATTERN.test("com.ore.app")).toBe(true);
    expect(ANDROID_PACKAGE_PATTERN.test("com.ore_app.v2")).toBe(true);
    for (const bad of ["ore", "1com.x", "com..x", "com.1x", ".com.x", "com.x."]) {
      expect(ANDROID_PACKAGE_PATTERN.test(bad), bad).toBe(false);
      expect(() => validateSponsorEntry({ ...OK, androidPackage: bad })).toThrow(/androidPackage/);
    }
  });

  it("forms the dApp Store deep link Solana Mobile documents", () => {
    expect(dappStoreLink("com.ore.app")).toBe("solanadappstore://details?id=com.ore.app");
    expect(() => dappStoreLink("ore")).toThrow(/androidPackage/);
  });

  it("looks up a wallet in a supplied directory", () => {
    expect(sponsorByWallet(ORE_WALLET, [OK])).toBe(OK);
    expect(sponsorByWallet("11111111111111111111111111111111", [OK])).toBeUndefined();
  });
});
