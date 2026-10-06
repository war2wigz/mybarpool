/**
 * PDA seeds for the pool-side accounts, PROGRAM §3.3–§3.7, in the order the
 * program derives them, as byte arrays ready for `getProgramDerivedAddress`.
 * `gameRecordSeeds` (PROGRAM §3.2) lives in `gameKey.ts`.
 */
import { ascii, assertU64 } from "./bytes.js";

const PUBKEY_BYTES = 32;

function assertPubkey(bytes: Uint8Array, what: string): Uint8Array {
  if (bytes.length !== PUBKEY_BYTES) {
    throw new RangeError(`${what} must be ${PUBKEY_BYTES} bytes, got ${bytes.length}`);
  }
  return bytes;
}

function u64le(value: bigint): Uint8Array {
  assertU64(value, "nonce");
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, value, true);
  return out;
}

/** PROGRAM §3.3 `Pool`: `["pool", game_record, creator, nonce u64 LE]`. */
export function poolSeeds(game: Uint8Array, creator: Uint8Array, nonce: bigint): Uint8Array[] {
  return [
    ascii("pool"),
    assertPubkey(game, "game"),
    assertPubkey(creator, "creator"),
    u64le(nonce),
  ];
}

/** PROGRAM §3.4 vault: `["vault", pool]`. */
export function vaultSeeds(pool: Uint8Array): Uint8Array[] {
  return [ascii("vault"), assertPubkey(pool, "pool")];
}

/** PROGRAM §3.5 `CreatorCounter`: `["counter", creator, game_record]`. */
export function counterSeeds(creator: Uint8Array, game: Uint8Array): Uint8Array[] {
  return [ascii("counter"), assertPubkey(creator, "creator"), assertPubkey(game, "game")];
}

/** PROGRAM §3.7 `Sponsorship`: `["sponsorship", pool, wallet]`. */
export function sponsorshipSeeds(pool: Uint8Array, wallet: Uint8Array): Uint8Array[] {
  return [ascii("sponsorship"), assertPubkey(pool, "pool"), assertPubkey(wallet, "wallet")];
}
