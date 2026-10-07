/**
 * Regolith Entropy on the localnet (Step 5): the `Var` PDA, planting a `Var`
 * through `surfnet_setAccount` (the deployed program refuses `Open`), reading
 * one back, and raw instruction builders for `Open`, `Sample`, `Reveal`,
 * `Next` and `Close` so the tests drive the real bytecode. Data encoders come
 * from `@mybarpool/shared`.
 */
import {
  decodeVar,
  encodeClose,
  encodeNext,
  encodeOpen,
  encodeReveal,
  encodeSample,
  encodeVar,
  varSeeds,
  VAR_LEN,
  type Var,
} from "@mybarpool/shared";
import {
  AccountRole,
  getAddressEncoder,
  getProgramDerivedAddress,
  type Address,
  type TransactionSigner,
} from "@solana/kit";

import { Localnet } from "../../scripts/localnet.js";
import {
  ENTROPY_PROGRAM_ADDRESS,
  fetchAccountData,
  rpc,
  SLOT_HASHES_SYSVAR,
  SYSTEM_PROGRAM,
  type SignedInstruction,
  type SignedMetas,
} from "./mybarpool.js";

const enc = getAddressEncoder();
const localnet = new Localnet();

/** `["var", authority, id LE]` under the Entropy program. */
export async function entropyVarPda(authority: Address, id: bigint): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: ENTROPY_PROGRAM_ADDRESS,
    seeds: varSeeds(Uint8Array.from(enc.encode(authority)), id),
  });
  return pda;
}

/** Rent for a 240-byte account, asked of the validator once. */
let varRent: bigint | undefined;
export async function rentForVar(): Promise<bigint> {
  varRent ??= await rpc.getMinimumBalanceForRentExemption(BigInt(VAR_LEN)).send();
  return varRent;
}

/** `surfnet_setAccount` takes the data as hex, not base64 (Surfpool 1.6.0). */
export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Write a `Var` where `Open` would have put it. `executable` and `rentEpoch` are omitted;
 * the NOTES record whether Surfpool accepted that.
 */
export async function plantVar(varAddress: Address, fields: Var): Promise<void> {
  await localnet.setAccount(varAddress, {
    lamports: Number(await rentForVar()),
    data: hex(encodeVar(fields)),
    owner: ENTROPY_PROGRAM_ADDRESS,
  });
}

export async function fetchVar(varAddress: Address): Promise<Var | null> {
  const data = await fetchAccountData(varAddress);
  return data === null ? null : decodeVar(data);
}

export async function currentSlot(): Promise<bigint> {
  return rpc.getSlot({ commitment: "confirmed" }).send();
}

/** Poll every 200 ms until the confirmed slot is at least `slot`. */
export async function waitForSlot(slot: bigint, timeoutMs = 60_000): Promise<bigint> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const now = await currentSlot();
    if (now >= slot) return now;
    if (Date.now() > deadline) throw new Error(`slot ${slot} not reached (at ${now})`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

/** The `SlotHashes` sysvar as `(slot, hash)` pairs, newest first. */
export async function slotHashes(): Promise<{ slot: bigint; hash: Uint8Array }[]> {
  const data = await fetchAccountData(SLOT_HASHES_SYSVAR);
  if (data === null) throw new Error("SlotHashes sysvar missing");
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const count = Number(view.getBigUint64(0, true));
  const out: { slot: bigint; hash: Uint8Array }[] = [];
  for (let i = 0; i < count && 8 + 40 * (i + 1) <= data.length; i++) {
    const at = 8 + 40 * i;
    out.push({ slot: view.getBigUint64(at, true), hash: data.slice(at + 8, at + 40) });
  }
  return out;
}

export async function slotHashFor(slot: bigint): Promise<Uint8Array | undefined> {
  return (await slotHashes()).find((e) => e.slot === slot)?.hash;
}

// ---------------------------------------------------------------------------
// Entropy instructions (processor at f26ae03)
// ---------------------------------------------------------------------------

/** `Open`: `[authority (w,s), payer (w,s), provider (w), var (w), system]`. Refused on mainnet. */
export async function openInstruction(
  authority: TransactionSigner,
  payer: TransactionSigner,
  id: bigint,
  provider: Address,
  commit: Uint8Array,
  isAuto: boolean,
  samples: bigint,
  endAt: bigint,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: authority.address, role: AccountRole.WRITABLE_SIGNER, signer: authority },
    { address: payer.address, role: AccountRole.WRITABLE_SIGNER, signer: payer },
    { address: provider, role: AccountRole.WRITABLE },
    { address: await entropyVarPda(authority.address, id), role: AccountRole.WRITABLE },
    { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
  ];
  return {
    programAddress: ENTROPY_PROGRAM_ADDRESS,
    accounts,
    data: encodeOpen({ id, commit, isAuto, samples, endAt }),
  };
}

/** `Sample`: `[signer (s), var (w), SlotHashes]`. Permissionless. */
export function sampleInstruction(
  signer: TransactionSigner,
  varAddress: Address,
): SignedInstruction {
  const accounts: SignedMetas = [
    { address: signer.address, role: AccountRole.READONLY_SIGNER, signer },
    { address: varAddress, role: AccountRole.WRITABLE },
    { address: SLOT_HASHES_SYSVAR, role: AccountRole.READONLY },
  ];
  return { programAddress: ENTROPY_PROGRAM_ADDRESS, accounts, data: encodeSample() };
}

/** `Reveal`: `[signer (s), var (w)]`. Permissionless; the seed must hash to the commit. */
export function revealInstruction(
  signer: TransactionSigner,
  varAddress: Address,
  seed: Uint8Array,
): SignedInstruction {
  const accounts: SignedMetas = [
    { address: signer.address, role: AccountRole.READONLY_SIGNER, signer },
    { address: varAddress, role: AccountRole.WRITABLE },
  ];
  return { programAddress: ENTROPY_PROGRAM_ADDRESS, accounts, data: encodeReveal(seed) };
}

/** `Next`: `[authority (s), var (w)]`. */
export function nextInstruction(
  authority: TransactionSigner,
  varAddress: Address,
  endAt: bigint,
): SignedInstruction {
  const accounts: SignedMetas = [
    { address: authority.address, role: AccountRole.READONLY_SIGNER, signer: authority },
    { address: varAddress, role: AccountRole.WRITABLE },
  ];
  return { programAddress: ENTROPY_PROGRAM_ADDRESS, accounts, data: encodeNext(endAt) };
}

/** `Close`: `[authority (w,s), var (w), system]`, three accounts (the processor's destructure). */
export function closeInstruction(
  authority: TransactionSigner,
  varAddress: Address,
): SignedInstruction {
  const accounts: SignedMetas = [
    { address: authority.address, role: AccountRole.WRITABLE_SIGNER, signer: authority },
    { address: varAddress, role: AccountRole.WRITABLE },
    { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
  ];
  return { programAddress: ENTROPY_PROGRAM_ADDRESS, accounts, data: encodeClose() };
}
