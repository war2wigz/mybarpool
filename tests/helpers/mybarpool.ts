/**
 * Hand-rolled Kit codecs for the four admin instructions, the two accounts and
 * the three events (build plan Step 2). Discriminators are read from
 * `idl/mybarpool.json` at test time so the committed IDL is exercised, not
 * just diffed. Step 9 replaces this file with the Codama-generated client.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  AccountRole,
  address,
  appendTransactionMessageInstruction,
  assertIsTransactionWithBlockhashLifetime,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  fixDecoderSize,
  getAddressDecoder,
  getAddressEncoder,
  getArrayDecoder,
  getArrayEncoder,
  getBase58Encoder,
  getBase64Encoder,
  getBooleanDecoder,
  getBooleanEncoder,
  getBytesDecoder,
  getI64Decoder,
  getOptionEncoder,
  getProgramDerivedAddress,
  getSignatureFromTransaction,
  getStructDecoder,
  getStructEncoder,
  getU16Decoder,
  getU16Encoder,
  getU64Decoder,
  getU64Encoder,
  getU8Decoder,
  getU8Encoder,
  getUtf8Encoder,
  isSolanaError,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM,
  SOLANA_ERROR__JSON_RPC__SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE,
  type Address,
  type Decoder,
  type Encoder,
  type Instruction,
  type ReadonlyUint8Array,
  type Rpc,
  type RpcSubscriptions,
  type Signature,
  type SolanaRpcApi,
  type SolanaRpcSubscriptionsApi,
  type TransactionSigner,
} from "@solana/kit";

import { LOCALNET_URL } from "../../scripts/localnet.js";

// ---------------------------------------------------------------------------
// IDL
// ---------------------------------------------------------------------------

interface IdlNamed {
  name: string;
  discriminator: number[];
}
interface Idl {
  address: string;
  instructions: IdlNamed[];
  accounts: IdlNamed[];
  events: IdlNamed[];
  errors: { code: number; name: string }[];
  constants: { name: string; value: string }[];
  types: { name: string }[];
}

export const IDL: Idl = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../idl/mybarpool.json", import.meta.url)), "utf8"),
) as Idl;

function discriminator(list: IdlNamed[], name: string): Uint8Array {
  const entry = list.find((e) => e.name === name);
  if (!entry) throw new Error(`${name} not in idl/mybarpool.json`);
  return Uint8Array.from(entry.discriminator);
}

export const DISCRIMINATORS = {
  instruction: (name: string) => discriminator(IDL.instructions, name),
  account: (name: string) => discriminator(IDL.accounts, name),
  event: (name: string) => discriminator(IDL.events, name),
};

/** The id `anchor test` deployed, from Anchor.toml (CI syncs it to a fresh keypair). */
export function declaredProgramId(): Address {
  const toml = readFileSync(fileURLToPath(new URL("../../Anchor.toml", import.meta.url)), "utf8");
  const match = /^mybarpool\s*=\s*"([1-9A-HJ-NP-Za-km-z]{32,44})"/m.exec(toml);
  if (!match?.[1]) throw new Error("programs.localnet.mybarpool not found in Anchor.toml");
  return address(match[1]);
}

export const PROGRAM_ID = declaredProgramId();
export const SYSTEM_PROGRAM = address("11111111111111111111111111111111");
export const BPF_LOADER_UPGRADEABLE = address("BPFLoaderUpgradeab1e11111111111111111111111");
export const TOKEN_PROGRAM = address("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
/** ORE on mainnet (ARCHITECTURE › Solana program); Surfpool fetches it on first touch. */
export const ORE_MINT = address("oreoU2P8bN6jkk3jbaiVxYnG1dCXcYxwhwyK9jSybcp");
export const DEFAULT_ADDRESS = SYSTEM_PROGRAM;

/** Anchor custom error base; `errors[i]` is `6000 + i`. */
export const ERROR_CODE_OFFSET = 6000;
export function errorCode(name: string): number {
  const entry = IDL.errors.find((e) => e.name === name);
  if (!entry) throw new Error(`${name} not in idl errors`);
  return entry.code;
}

// ---------------------------------------------------------------------------
// PDAs
// ---------------------------------------------------------------------------

const utf8 = getUtf8Encoder();

export async function configPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [utf8.encode("config")],
  });
  return pda;
}

export async function overridePda(wallet: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [utf8.encode("override"), getAddressEncoder().encode(wallet)],
  });
  return pda;
}

export async function programDataPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: BPF_LOADER_UPGRADEABLE,
    seeds: [getAddressEncoder().encode(PROGRAM_ID)],
  });
  return pda;
}

export async function eventAuthorityPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [utf8.encode("__event_authority")],
  });
  return pda;
}

// ---------------------------------------------------------------------------
// Types and codecs (PROGRAM §3.1, §3.6, §4.1, §7)
// ---------------------------------------------------------------------------

export interface TokenRule {
  enabled: boolean;
  mint: Address;
  tokenProgram: Address;
  decimals: number;
  minPrice: bigint;
  step: bigint;
  maxPrice: bigint;
  maxSponsorship: bigint;
}

export const tokenRuleEncoder: Encoder<TokenRule> = getStructEncoder([
  ["enabled", getBooleanEncoder()],
  ["mint", getAddressEncoder()],
  ["tokenProgram", getAddressEncoder()],
  ["decimals", getU8Encoder()],
  ["minPrice", getU64Encoder()],
  ["step", getU64Encoder()],
  ["maxPrice", getU64Encoder()],
  ["maxSponsorship", getU64Encoder()],
]);

export const tokenRuleDecoder: Decoder<TokenRule> = getStructDecoder([
  ["enabled", getBooleanDecoder()],
  ["mint", getAddressDecoder()],
  ["tokenProgram", getAddressDecoder()],
  ["decimals", getU8Decoder()],
  ["minPrice", getU64Decoder()],
  ["step", getU64Decoder()],
  ["maxPrice", getU64Decoder()],
  ["maxSponsorship", getU64Decoder()],
]);

export interface ConfigFields {
  scoreAuthority: Address;
  entropyProvider: Address;
  feeWallet: Address;
  platformBps: number;
  creatorBps: number;
  addonBudgetBps: number;
  defaultPreset: number;
  maxOpenPools: number;
  maxOwnBoxes: number;
  preseasonEnabled: boolean;
  paused: boolean;
  tokens: [TokenRule, TokenRule, TokenRule];
}

export type InitializeParams = ConfigFields;

export interface PlatformConfig extends ConfigFields {
  admin: Address;
  bump: number;
  reserved: ReadonlyUint8Array;
}

const configFieldEncoders = [
  ["scoreAuthority", getAddressEncoder()],
  ["entropyProvider", getAddressEncoder()],
  ["feeWallet", getAddressEncoder()],
  ["platformBps", getU16Encoder()],
  ["creatorBps", getU16Encoder()],
  ["addonBudgetBps", getU16Encoder()],
  ["defaultPreset", getU8Encoder()],
  ["maxOpenPools", getU8Encoder()],
  ["maxOwnBoxes", getU8Encoder()],
  ["preseasonEnabled", getBooleanEncoder()],
  ["paused", getBooleanEncoder()],
  ["tokens", getArrayEncoder(tokenRuleEncoder, { size: 3 })],
] as const;

const configFieldDecoders = [
  ["scoreAuthority", getAddressDecoder()],
  ["entropyProvider", getAddressDecoder()],
  ["feeWallet", getAddressDecoder()],
  ["platformBps", getU16Decoder()],
  ["creatorBps", getU16Decoder()],
  ["addonBudgetBps", getU16Decoder()],
  ["defaultPreset", getU8Decoder()],
  ["maxOpenPools", getU8Decoder()],
  ["maxOwnBoxes", getU8Decoder()],
  ["preseasonEnabled", getBooleanDecoder()],
  ["paused", getBooleanDecoder()],
  ["tokens", getArrayDecoder(tokenRuleDecoder, { size: 3 })],
] as const;

export const initializeParamsEncoder = getStructEncoder([
  ...configFieldEncoders,
]) as Encoder<InitializeParams>;

export const platformConfigDecoder = getStructDecoder([
  ["admin", getAddressDecoder()],
  ...configFieldDecoders,
  ["bump", getU8Decoder()],
  ["reserved", fixDecoderSize(getBytesDecoder(), 256)],
]) as Decoder<PlatformConfig>;

export interface UpdateConfigParams {
  admin?: Address;
  scoreAuthority?: Address;
  entropyProvider?: Address;
  feeWallet?: Address;
  platformBps?: number;
  creatorBps?: number;
  addonBudgetBps?: number;
  defaultPreset?: number;
  maxOpenPools?: number;
  maxOwnBoxes?: number;
  preseasonEnabled?: boolean;
  paused?: boolean;
  tokens?: [TokenRule | undefined, TokenRule | undefined, TokenRule | undefined];
}

/** Anchor `Option<T>` is a 1-byte tag, Kit's `getOptionEncoder` default. */
const opt = <T>(inner: Encoder<T>) => getOptionEncoder(inner);

const updateConfigParamsEncoder = getStructEncoder([
  ["admin", opt(getAddressEncoder())],
  ["scoreAuthority", opt(getAddressEncoder())],
  ["entropyProvider", opt(getAddressEncoder())],
  ["feeWallet", opt(getAddressEncoder())],
  ["platformBps", opt(getU16Encoder())],
  ["creatorBps", opt(getU16Encoder())],
  ["addonBudgetBps", opt(getU16Encoder())],
  ["defaultPreset", opt(getU8Encoder())],
  ["maxOpenPools", opt(getU8Encoder())],
  ["maxOwnBoxes", opt(getU8Encoder())],
  ["preseasonEnabled", opt(getBooleanEncoder())],
  ["paused", opt(getBooleanEncoder())],
  ["tokens", getArrayEncoder(opt(tokenRuleEncoder), { size: 3 })],
]);

function encodeUpdateConfigParams(p: UpdateConfigParams): ReadonlyUint8Array {
  const tokens = p.tokens ?? [undefined, undefined, undefined];
  return updateConfigParamsEncoder.encode({
    admin: p.admin ?? null,
    scoreAuthority: p.scoreAuthority ?? null,
    entropyProvider: p.entropyProvider ?? null,
    feeWallet: p.feeWallet ?? null,
    platformBps: p.platformBps ?? null,
    creatorBps: p.creatorBps ?? null,
    addonBudgetBps: p.addonBudgetBps ?? null,
    defaultPreset: p.defaultPreset ?? null,
    maxOpenPools: p.maxOpenPools ?? null,
    maxOwnBoxes: p.maxOwnBoxes ?? null,
    preseasonEnabled: p.preseasonEnabled ?? null,
    paused: p.paused ?? null,
    tokens: tokens.map((t) => t ?? null),
  });
}

export interface WalletOverride {
  wallet: Address;
  maxOpenPools: number;
  maxOwnBoxes: number;
  bump: number;
}

export const walletOverrideDecoder: Decoder<WalletOverride> = getStructDecoder([
  ["wallet", getAddressDecoder()],
  ["maxOpenPools", getU8Decoder()],
  ["maxOwnBoxes", getU8Decoder()],
  ["bump", getU8Decoder()],
]);

export interface ConfigUpdated extends ConfigFields {
  time: bigint;
  admin: Address;
}

export const configUpdatedDecoder = getStructDecoder([
  ["time", getI64Decoder()],
  ["admin", getAddressDecoder()],
  ...configFieldDecoders,
]) as Decoder<ConfigUpdated>;

export interface OverrideEvent {
  time: bigint;
  wallet: Address;
  maxOpenPools: number;
  maxOwnBoxes: number;
}

export const overrideEventDecoder: Decoder<OverrideEvent> = getStructDecoder([
  ["time", getI64Decoder()],
  ["wallet", getAddressDecoder()],
  ["maxOpenPools", getU8Decoder()],
  ["maxOwnBoxes", getU8Decoder()],
]);

/** Strip and check an 8-byte Anchor discriminator before decoding `body`. */
export function decodeAccount<T>(name: string, data: Uint8Array, body: Decoder<T>): T {
  const disc = DISCRIMINATORS.account(name);
  if (!bytesEqual(data.subarray(0, 8), disc)) throw new Error(`${name}: discriminator mismatch`);
  return body.decode(data.subarray(8));
}

function bytesEqual(a: ReadonlyUint8Array, b: ReadonlyUint8Array): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

// ---------------------------------------------------------------------------
// Instructions (PROGRAM §4.1); absent optional accounts are the program id.
// The admin is always the fee payer in these tests, so the payer's signature
// covers the signer meta and no per-account signer is attached.
// ---------------------------------------------------------------------------

function withDiscriminator(name: string, body: ReadonlyUint8Array): Uint8Array {
  const disc = DISCRIMINATORS.instruction(name);
  const out = new Uint8Array(8 + body.length);
  out.set(disc, 0);
  out.set(body, 8);
  return out;
}

async function eventCpiAccounts() {
  return [
    { address: await eventAuthorityPda(), role: AccountRole.READONLY },
    { address: PROGRAM_ID, role: AccountRole.READONLY },
  ];
}

export async function initializeInstruction(
  admin: TransactionSigner,
  params: InitializeParams,
  mints: { mint1?: Address; mint2?: Address } = {},
): Promise<Instruction> {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: admin.address, role: AccountRole.WRITABLE_SIGNER },
      { address: await configPda(), role: AccountRole.WRITABLE },
      { address: await programDataPda(), role: AccountRole.READONLY },
      { address: mints.mint1 ?? PROGRAM_ID, role: AccountRole.READONLY },
      { address: mints.mint2 ?? PROGRAM_ID, role: AccountRole.READONLY },
      { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
      ...(await eventCpiAccounts()),
    ],
    data: withDiscriminator("initialize", initializeParamsEncoder.encode(params)),
  };
}

export async function updateConfigInstruction(
  admin: TransactionSigner,
  params: UpdateConfigParams,
  mints: { mint1?: Address; mint2?: Address } = {},
): Promise<Instruction> {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: admin.address, role: AccountRole.READONLY_SIGNER },
      { address: await configPda(), role: AccountRole.WRITABLE },
      { address: mints.mint1 ?? PROGRAM_ID, role: AccountRole.READONLY },
      { address: mints.mint2 ?? PROGRAM_ID, role: AccountRole.READONLY },
      ...(await eventCpiAccounts()),
    ],
    data: withDiscriminator("update_config", encodeUpdateConfigParams(params)),
  };
}

const setOverrideArgs = getStructEncoder([
  ["wallet", getAddressEncoder()],
  ["maxOpenPools", getU8Encoder()],
  ["maxOwnBoxes", getU8Encoder()],
]);

export async function setWalletOverrideInstruction(
  admin: TransactionSigner,
  wallet: Address,
  maxOpenPools: number,
  maxOwnBoxes: number,
): Promise<Instruction> {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: admin.address, role: AccountRole.WRITABLE_SIGNER },
      { address: await configPda(), role: AccountRole.READONLY },
      { address: await overridePda(wallet), role: AccountRole.WRITABLE },
      { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
      ...(await eventCpiAccounts()),
    ],
    data: withDiscriminator(
      "set_wallet_override",
      setOverrideArgs.encode({ wallet, maxOpenPools, maxOwnBoxes }),
    ),
  };
}

export async function closeWalletOverrideInstruction(
  admin: TransactionSigner,
  wallet: Address,
): Promise<Instruction> {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: admin.address, role: AccountRole.WRITABLE_SIGNER },
      { address: await configPda(), role: AccountRole.READONLY },
      { address: await overridePda(wallet), role: AccountRole.WRITABLE },
      ...(await eventCpiAccounts()),
    ],
    data: withDiscriminator("close_wallet_override", getAddressEncoder().encode(wallet)),
  };
}

// ---------------------------------------------------------------------------
// Sending and reading back
// ---------------------------------------------------------------------------

export const rpc: Rpc<SolanaRpcApi> = createSolanaRpc(LOCALNET_URL);
export const rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi> =
  createSolanaRpcSubscriptions(LOCALNET_URL.replace(/^http/, "ws").replace(/:8899$/, ":8900"));
const sendAndConfirm = sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions });

export async function send(payer: TransactionSigner, instruction: Instruction): Promise<Signature> {
  const { value: blockhash } = await rpc.getLatestBlockhash().send();
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(payer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
    (m) => appendTransactionMessageInstruction(instruction, m),
  );
  const tx = await signTransactionMessageWithSigners(message);
  assertIsTransactionWithBlockhashLifetime(tx);
  await sendAndConfirm(tx, { commitment: "confirmed" });
  return getSignatureFromTransaction(tx);
}

/** Send and return the Anchor custom error code the program failed with, or `null` on success. */
export async function sendExpectingError(
  payer: TransactionSigner,
  instruction: Instruction,
): Promise<number | null> {
  try {
    await send(payer, instruction);
    return null;
  } catch (error) {
    return customErrorCode(error);
  }
}

function customErrorCode(error: unknown): number {
  let e: unknown = error;
  for (let depth = 0; depth < 5 && e !== undefined; depth++) {
    if (isSolanaError(e, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM)) return e.context.code;
    if (isSolanaError(e, SOLANA_ERROR__JSON_RPC__SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE)) {
      // Preflight reports { InstructionError: [index, { Custom: code }] }.
      const err = (e.context as { err?: unknown }).err;
      const code = customFromInstructionError(err);
      if (code !== undefined) return code;
    }
    e = (e as { cause?: unknown }).cause;
  }
  throw error;
}

function customFromInstructionError(err: unknown): number | undefined {
  if (typeof err !== "object" || err === null || !("InstructionError" in err)) return undefined;
  const [, inner] = (err as { InstructionError: [number, unknown] }).InstructionError;
  if (typeof inner === "object" && inner !== null && "Custom" in inner) {
    return Number((inner as { Custom: number }).Custom);
  }
  return undefined;
}

export async function fetchAccountData(addr: Address): Promise<Uint8Array | null> {
  const { value } = await rpc
    .getAccountInfo(addr, { encoding: "base64", commitment: "confirmed" })
    .send();
  if (!value) return null;
  return new Uint8Array(getBase64Encoder().encode(value.data[0]));
}

export async function fetchLamports(addr: Address): Promise<bigint> {
  const { value } = await rpc.getBalance(addr, { commitment: "confirmed" }).send();
  return value;
}

/**
 * The event bodies the transaction emitted through `emit_cpi!`: every inner
 * instruction whose program is ours and whose data begins with Anchor's
 * event-CPI tag, with the tag stripped (leaving event discriminator + body).
 */
export async function emittedEvents(signature: Signature): Promise<Uint8Array[]> {
  const tx = await rpc
    .getTransaction(signature, {
      maxSupportedTransactionVersion: 0,
      commitment: "confirmed",
      encoding: "json",
    })
    .send();
  if (!tx?.meta) throw new Error("transaction not found");
  const keys = [
    ...tx.transaction.message.accountKeys,
    ...(tx.meta.loadedAddresses?.writable ?? []),
    ...(tx.meta.loadedAddresses?.readonly ?? []),
  ];
  const base58 = getBase58Encoder();
  const out: Uint8Array[] = [];
  for (const group of tx.meta.innerInstructions ?? []) {
    for (const ix of group.instructions) {
      if (keys[ix.programIdIndex] !== PROGRAM_ID) continue;
      const data = new Uint8Array(base58.encode(ix.data));
      if (data.length >= 16 && bytesEqual(data.subarray(0, 8), EVENT_IX_TAG_LE))
        out.push(data.subarray(8));
    }
  }
  return out;
}

/** `anchor_lang::event::EVENT_IX_TAG` (0x1d9acb512ea545e4) little-endian. */
export const EVENT_IX_TAG_LE = Uint8Array.from([0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d]);

export function decodeEvent<T>(name: string, payload: Uint8Array, body: Decoder<T>): T {
  if (!bytesEqual(payload.subarray(0, 8), DISCRIMINATORS.event(name)))
    throw new Error(`${name}: not this event`);
  return body.decode(payload.subarray(8));
}

export function eventName(payload: Uint8Array): string | undefined {
  return IDL.events.find((e) =>
    bytesEqual(payload.subarray(0, 8), Uint8Array.from(e.discriminator)),
  )?.name;
}
