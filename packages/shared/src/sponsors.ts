/**
 * Sponsor directory shape (ARCHITECTURE › Sponsorship, "Directory entry").
 *
 * The chain records a sponsor's wallet and amount, nothing else. Display
 * names, logos and links come only from this reviewed list, so nobody can put
 * arbitrary text, an image or a link on a pool page for the price of a
 * sponsorship. The list starts empty; entries arrive by owner review.
 */

export interface SponsorEntry {
  /** The sponsor's wallet, base58. */
  readonly wallet: string;
  readonly displayName: string;
  /** `https:` URL of the logo. */
  readonly logo: string;
  /** `https:` only; opens in the browser, never inside the app. */
  readonly website: string;
  /** Android application id of the sponsor's Seeker app, for `solanadappstore://details?id=<package>`. */
  readonly androidPackage?: string;
}

export const SPONSOR_DIRECTORY: readonly SponsorEntry[] = Object.freeze([]);

/** Android application id: dot-separated segments, each starting with a letter. */
export const ANDROID_PACKAGE_PATTERN = /^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$/;

const BASE58 = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;

// Scheme and a non-empty host; no URL global so this runs without DOM or Node libs.
const HTTPS_URL =
  /^https:\/\/[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)*(?::\d{1,5})?(?:[/?#]\S*)?$/;

function isHttpsUrl(value: string): boolean {
  return HTTPS_URL.test(value);
}

export function validateSponsorEntry(entry: SponsorEntry): SponsorEntry {
  if (!BASE58.test(entry.wallet)) throw new RangeError("wallet must be a base58 public key");
  if (entry.displayName.trim().length === 0) throw new RangeError("displayName must not be empty");
  if (!isHttpsUrl(entry.logo)) throw new RangeError("logo must be an https: URL");
  if (!isHttpsUrl(entry.website)) throw new RangeError("website must be an https: URL");
  if (entry.androidPackage !== undefined && !ANDROID_PACKAGE_PATTERN.test(entry.androidPackage)) {
    throw new RangeError("androidPackage must be a valid Android application id");
  }
  return entry;
}

/** `solanadappstore://details?id=<package>`, the scheme Solana Mobile documents. */
export function dappStoreLink(androidPackage: string): string {
  if (!ANDROID_PACKAGE_PATTERN.test(androidPackage)) {
    throw new RangeError("androidPackage must be a valid Android application id");
  }
  return `solanadappstore://details?id=${androidPackage}`;
}

export function sponsorByWallet(
  wallet: string,
  directory: readonly SponsorEntry[] = SPONSOR_DIRECTORY,
): SponsorEntry | undefined {
  return directory.find((entry) => entry.wallet === wallet);
}
