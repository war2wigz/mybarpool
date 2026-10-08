/**
 * Entropy on the localnet (Steps 5 and 5b): preloading the platform's
 * deployment (the fork fixture) at `ENTROPY_PROGRAM` as an upgradeable-loader
 * program, the `Var` PDA, planting a `Var` through `surfnet_setAccount` (the
 * adversarial states), reading one back, and raw instruction builders for
 * `Open`, `Sample`, `Reveal`, `Next` and `Close` so the tests drive the real
 * bytecode. Data encoders come from `@mybarpool/shared`.
 */
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

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
  BPF_LOADER_UPGRADEABLE,
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

// ---------------------------------------------------------------------------
// The platform's deployment, preloaded from the fork fixture (Step 5b)
// ---------------------------------------------------------------------------

/**
 * `war2wigz/entropy` at this commit is what `ENTROPY_PROGRAM` runs: the fixture below is its
 * `solana-verify build --arch v3` output, byte for byte, and `ci.yml`'s `entropy-fork-hash` job
 * rebuilds the fork at the same commit and compares. The fixture is named by the first seven
 * characters.
 */
export const ENTROPY_FORK_COMMIT = "170b7ddaee53b7a55d134d614c8ce080e72b4d4f";
/** SHA-256 of the fixture file (`programs/mybarpool/tests/fixtures/entropy-<fork7>.so`). */
export const ENTROPY_FORK_ELF_SHA256 =
  "2ec504ac1a71ff0f0f515c70a541522a5c58ef126aebb893dbf6ad7245d65fe8";
/** `solana-verify get-executable-hash`: the same bytes with trailing zeros stripped. */
export const ENTROPY_FORK_EXECUTABLE_HASH =
  "dae042011c5d881edd8287a61694d60cf19471fb68c7fccd45f8965734bf53f1";
/** Regolith's deployed bytecode, `verify.osec.io`'s `on_chain_hash` for `3jSk…` (Step 5). */
export const REGOLITH_ENTROPY_EXECUTABLE_HASH =
  "b64ffdfef7bb05839fbe0d24196697cc20de6bc72081031bc0523699181b18b1";

export function entropyForkFixturePath(): string {
  return fileURLToPath(
    new URL(
      `../../programs/mybarpool/tests/fixtures/entropy-${ENTROPY_FORK_COMMIT.slice(0, 7)}.so`,
      import.meta.url,
    ),
  );
}

export function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

export function stripTrailingZeros(bytes: Uint8Array): Uint8Array {
  let end = bytes.length;
  while (end > 0 && bytes[end - 1] === 0) end--;
  return bytes.subarray(0, end);
}

/** `surfnet_setAccount` takes the data as hex, not base64 (Surfpool 1.6.0). */
export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** The ProgramData address of an upgradeable program: `find_program_address([program], loader)`. */
export async function programDataAddress(program: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: BPF_LOADER_UPGRADEABLE,
    seeds: [enc.encode(program)],
  });
  return pda;
}

/**
 * Place the fork's ELF at `ENTROPY_PROGRAM` as an upgradeable-loader program, through two raw
 * `surfnet_setAccount` writes (the address holds no account on mainnet, so Surfpool's first
 * touch fetches nothing and the writes stand). Layouts are `UpgradeableLoaderState`'s:
 *   ProgramData: u32 tag 3, u64 slot (0), Option<Pubkey> upgrade authority as 1 + 32 bytes,
 *                then the ELF; owner the loader; not executable.
 *   Program:     u32 tag 2, the ProgramData address (36 bytes); owner the loader; executable.
 * The upgrade authority is a placeholder — the system program id — since nothing on the localnet
 * upgrades it; the real one is the Squads multisig at deployment. Idempotent: skips when the
 * program account already exists and is executable.
 */
export async function preloadEntropyFork(): Promise<{ preloaded: boolean; programData: Address }> {
  const programData = await programDataAddress(ENTROPY_PROGRAM_ADDRESS);
  const existing = await rpc.getAccountInfo(ENTROPY_PROGRAM_ADDRESS, { encoding: "base64" }).send();
  if (existing.value?.executable) return { preloaded: false, programData };

  const elf = new Uint8Array(readFileSync(entropyForkFixturePath()));
  if (sha256Hex(elf) !== ENTROPY_FORK_ELF_SHA256) {
    throw new Error("the fork fixture does not match ENTROPY_FORK_ELF_SHA256");
  }
  const data = new Uint8Array(45 + elf.length);
  data.set([3, 0, 0, 0], 0); // UpgradeableLoaderState::ProgramData
  // slot: 0u64 at 4..12
  data[12] = 1; // Some(upgrade_authority)
  data.set(enc.encode(SYSTEM_PROGRAM), 13); // placeholder authority
  data.set(elf, 45);
  await localnet.setAccount(programData, {
    lamports: Number(await rpc.getMinimumBalanceForRentExemption(BigInt(data.length)).send()),
    data: hex(data),
    owner: BPF_LOADER_UPGRADEABLE,
    executable: false,
  });

  const program = new Uint8Array(36);
  program.set([2, 0, 0, 0], 0); // UpgradeableLoaderState::Program
  program.set(enc.encode(programData), 4);
  await localnet.setAccount(ENTROPY_PROGRAM_ADDRESS, {
    lamports: Number(await rpc.getMinimumBalanceForRentExemption(36n).send()),
    data: hex(program),
    owner: BPF_LOADER_UPGRADEABLE,
    executable: true,
  });
  return { preloaded: true, programData };
}

/** The ELF of an upgradeable program as deployed: ProgramData bytes 45.., trailing zeros stripped. */
export async function deployedElf(program: Address): Promise<Uint8Array> {
  const data = await fetchAccountData(await programDataAddress(program));
  if (data === null) throw new Error(`${program}: no ProgramData`);
  return stripTrailingZeros(data.subarray(45));
}

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

/**
 * `Open`: `[authority (w,s), payer (w,s), provider (w), var (w), system]`. Refused by Regolith's
 * deployment; accepted by the platform's (Step 5b). The provider does not sign.
 */
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
