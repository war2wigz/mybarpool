/**
 * Hand-rolled Kit codecs for the admin, game, pool and draw instructions,
 * the six accounts and the sixteen events (build plan Steps 2–5). Discriminators are
 * read from `idl/mybarpool.json` at test time so the committed IDL is
 * exercised, not just diffed. Step 9 replaces this file with the
 * Codama-generated client.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  AccountRole,
  address,
  appendTransactionMessageInstructions,
  assertIsTransactionWithBlockhashLifetime,
  createDefaultRpcTransport,
  createSolanaRpcFromTransport,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  fixDecoderSize,
  fixEncoderSize,
  getAddressDecoder,
  getAddressEncoder,
  getArrayDecoder,
  getArrayEncoder,
  getBase58Encoder,
  getBase64Encoder,
  getBooleanDecoder,
  getBooleanEncoder,
  getBytesDecoder,
  getBytesEncoder,
  getEnumDecoder,
  getEnumEncoder,
  getI64Decoder,
  getI64Encoder,
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
  getU32Decoder,
  getU32Encoder,
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
  type InstructionWithSigners,
  type AccountMeta,
  type AccountSignerMeta,
  type ReadonlyUint8Array,
  type Rpc,
  type RpcSubscriptions,
  type RpcTransport,
  type Signature,
  type SolanaRpcApi,
  type SolanaRpcSubscriptionsApi,
  type TransactionSigner,
} from "@solana/kit";

import {
  counterSeeds,
  gameRecordSeeds,
  poolSeeds,
  sponsorshipSeeds,
  vaultSeeds,
  type GameKey,
} from "@mybarpool/shared";

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

/** PROGRAM §3.2: the six seeds `@mybarpool/shared` produces, in that order. */
export async function gamePda(key: GameKey, scheduledKickoff: bigint): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: gameRecordSeeds(key, scheduledKickoff),
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

async function eventCpiAccounts(): Promise<{ address: Address; role: AccountRole.READONLY }[]> {
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
// Games (PROGRAM §2, §3.2, §4.2, §7)
// ---------------------------------------------------------------------------

export type { GameKey };

/** PROGRAM §3.2 `GameStatus`; the numeric value is the on-chain byte. */
export enum GameStatus {
  Scheduled = 0,
  Postponed = 1,
  Cancelled = 2,
  Suspended = 3,
  Final = 4,
}

export const gameKeyEncoder: Encoder<GameKey> = getStructEncoder([
  ["season", getU16Encoder()],
  ["week", getU8Encoder()],
  ["home", getU8Encoder()],
  ["away", getU8Encoder()],
]);

export const gameKeyDecoder: Decoder<GameKey> = getStructDecoder([
  ["season", getU16Decoder()],
  ["week", getU8Decoder()],
  ["home", getU8Decoder()],
  ["away", getU8Decoder()],
]);

export const gameStatusEncoder = getEnumEncoder(GameStatus) as Encoder<GameStatus>;
export const gameStatusDecoder = getEnumDecoder(GameStatus) as Decoder<GameStatus>;

export interface GameRecord {
  key: GameKey;
  scheduledKickoff: bigint;
  recordedKickoff: bigint;
  status: GameStatus;
  quartersPosted: number;
  homeScore: number[];
  awayScore: number[];
  postedAt: bigint[];
  finalHadOvertime: boolean;
  markedAt: bigint;
  bump: number;
  reserved: ReadonlyUint8Array;
}

export const gameRecordDecoder = getStructDecoder([
  ["key", gameKeyDecoder],
  ["scheduledKickoff", getI64Decoder()],
  ["recordedKickoff", getI64Decoder()],
  ["status", gameStatusDecoder],
  ["quartersPosted", getU8Decoder()],
  ["homeScore", getArrayDecoder(getU16Decoder(), { size: 4 })],
  ["awayScore", getArrayDecoder(getU16Decoder(), { size: 4 })],
  ["postedAt", getArrayDecoder(getI64Decoder(), { size: 4 })],
  ["finalHadOvertime", getBooleanDecoder()],
  ["markedAt", getI64Decoder()],
  ["bump", getU8Decoder()],
  ["reserved", fixDecoderSize(getBytesDecoder(), 64)],
]) as Decoder<GameRecord>;

export interface GameCreated {
  time: bigint;
  game: Address;
  key: GameKey;
  scheduledKickoff: bigint;
}
export const gameCreatedDecoder: Decoder<GameCreated> = getStructDecoder([
  ["time", getI64Decoder()],
  ["game", getAddressDecoder()],
  ["key", gameKeyDecoder],
  ["scheduledKickoff", getI64Decoder()],
]);

export interface KickoffUpdated {
  time: bigint;
  game: Address;
  old: bigint;
  new: bigint;
}
export const kickoffUpdatedDecoder: Decoder<KickoffUpdated> = getStructDecoder([
  ["time", getI64Decoder()],
  ["game", getAddressDecoder()],
  ["old", getI64Decoder()],
  ["new", getI64Decoder()],
]);

export interface ScoresPosted {
  time: bigint;
  game: Address;
  quarter: number;
  home: number;
  away: number;
  isFinal: boolean;
  hadOvertime: boolean;
}
export const scoresPostedDecoder: Decoder<ScoresPosted> = getStructDecoder([
  ["time", getI64Decoder()],
  ["game", getAddressDecoder()],
  ["quarter", getU8Decoder()],
  ["home", getU16Decoder()],
  ["away", getU16Decoder()],
  ["isFinal", getBooleanDecoder()],
  ["hadOvertime", getBooleanDecoder()],
]);

export interface GameMarked {
  time: bigint;
  game: Address;
  status: GameStatus;
}
export const gameMarkedDecoder: Decoder<GameMarked> = getStructDecoder([
  ["time", getI64Decoder()],
  ["game", getAddressDecoder()],
  ["status", gameStatusDecoder],
]);

/**
 * `create_game`: the keeper signs and pays. Unlike the admin instructions, the keeper is a
 * separate signer from the fee payer in the tests, so the signer is attached to the meta.
 */
export async function createGameInstruction(
  keeper: TransactionSigner,
  key: GameKey,
  scheduledKickoff: bigint,
): Promise<SignedInstruction> {
  return createGameInstructionAt(
    keeper,
    key,
    scheduledKickoff,
    await gamePda(key, scheduledKickoff),
  );
}

/**
 * `create_game` for a key `@mybarpool/shared` refuses to encode (home == away, a bad week), so
 * the suite can show the program rejects it too. The PDA is derived from raw seeds.
 */
export async function createGameInstructionUnchecked(
  keeper: TransactionSigner,
  key: GameKey,
  scheduledKickoff: bigint,
): Promise<SignedInstruction> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [
      utf8.encode("game"),
      getU16Encoder().encode(key.season),
      getU8Encoder().encode(key.week),
      getU8Encoder().encode(key.home),
      getU8Encoder().encode(key.away),
      getI64Encoder().encode(scheduledKickoff),
    ],
  });
  return createGameInstructionAt(keeper, key, scheduledKickoff, pda);
}

async function createGameInstructionAt(
  keeper: TransactionSigner,
  key: GameKey,
  scheduledKickoff: bigint,
  game: Address,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: keeper.address, role: AccountRole.WRITABLE_SIGNER, signer: keeper },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: game, role: AccountRole.WRITABLE },
    { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator(
      "create_game",
      getStructEncoder([
        ["key", gameKeyEncoder],
        ["scheduledKickoff", getI64Encoder()],
      ]).encode({ key, scheduledKickoff }),
    ),
  };
}

export async function updateKickoffInstruction(
  keeper: TransactionSigner,
  game: Address,
  newTime: bigint,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: keeper.address, role: AccountRole.READONLY_SIGNER, signer: keeper },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: game, role: AccountRole.WRITABLE },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("update_kickoff", getI64Encoder().encode(newTime)),
  };
}

export interface ScorePost {
  quarter: number;
  home: number;
  away: number;
  isFinal: boolean;
  hadOvertime: boolean;
}

const scorePostEncoder: Encoder<ScorePost> = getStructEncoder([
  ["quarter", getU8Encoder()],
  ["home", getU16Encoder()],
  ["away", getU16Encoder()],
  ["isFinal", getBooleanEncoder()],
  ["hadOvertime", getBooleanEncoder()],
]);

export async function postScoresInstruction(
  keeper: TransactionSigner,
  game: Address,
  post: ScorePost,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: keeper.address, role: AccountRole.READONLY_SIGNER, signer: keeper },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: game, role: AccountRole.WRITABLE },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("post_scores", scorePostEncoder.encode(post)),
  };
}

export async function markGameInstruction(
  admin: TransactionSigner,
  game: Address,
  newStatus: GameStatus,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: admin.address, role: AccountRole.READONLY_SIGNER, signer: admin },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: game, role: AccountRole.WRITABLE },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("mark_game", gameStatusEncoder.encode(newStatus)),
  };
}

/** The chain's clock, PROGRAM conventions: `unix_timestamp` is the i64 at byte 32 of the Clock sysvar. */
export const CLOCK_SYSVAR = address("SysvarC1ock11111111111111111111111111111111");
export async function chainNow(): Promise<bigint> {
  const data = await fetchAccountData(CLOCK_SYSVAR);
  if (!data || data.length < 40) throw new Error("Clock sysvar unreadable");
  return getI64Decoder().decode(data.subarray(32, 40));
}

// ---------------------------------------------------------------------------
// Pools (PROGRAM §3.3–§3.7, §4.3, §7)
// ---------------------------------------------------------------------------

/** Kit's encoder yields a `ReadonlyUint8Array`; the shared seed helpers take `Uint8Array`. */
const addrBytes = (a: Address): Uint8Array => Uint8Array.from(getAddressEncoder().encode(a));

export async function poolPda(game: Address, creator: Address, nonce: bigint): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: poolSeeds(addrBytes(game), addrBytes(creator), nonce),
  });
  return pda;
}

export async function vaultPda(pool: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: vaultSeeds(addrBytes(pool)),
  });
  return pda;
}

export async function counterPda(creator: Address, game: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: counterSeeds(addrBytes(creator), addrBytes(game)),
  });
  return pda;
}

export async function sponsorshipPda(pool: Address, wallet: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: sponsorshipSeeds(addrBytes(pool), addrBytes(wallet)),
  });
  return pda;
}

/** PROGRAM §3.3 `PoolStatus`; the numeric value is the on-chain byte. */
export enum PoolStatus {
  Open = 0,
  Locked = 1,
  Drawn = 2,
  Settled = 3,
  Returned = 4,
  Split = 5,
}

/** PROGRAM §3.3 `AccessType`. */
export enum AccessType {
  Public = 0,
  Link = 1,
  Allowlist = 2,
}

/** PROGRAM §1 `PayoutPreset`. */
export enum PayoutPreset {
  Standard = 0,
  Even = 1,
  FinalOnly = 2,
}

const poolStatusDecoder = getEnumDecoder(PoolStatus) as Decoder<PoolStatus>;
const accessTypeEncoder = getEnumEncoder(AccessType) as Encoder<AccessType>;
const accessTypeDecoder = getEnumDecoder(AccessType) as Decoder<AccessType>;
const payoutPresetEncoder = getEnumEncoder(PayoutPreset) as Encoder<PayoutPreset>;
const payoutPresetDecoder = getEnumDecoder(PayoutPreset) as Decoder<PayoutPreset>;

export interface CreatePoolParams {
  nonce: bigint;
  token: number;
  price: bigint;
  preset: PayoutPreset;
  accessType: AccessType;
  gateKey: Address;
  allowlistRoot: ReadonlyUint8Array;
  creatorAddonBps: number;
  integrator: Address;
  integratorBps: number;
  initialBoxes: number;
}

export const createPoolParamsEncoder: Encoder<CreatePoolParams> = getStructEncoder([
  ["nonce", getU64Encoder()],
  ["token", getU8Encoder()],
  ["price", getU64Encoder()],
  ["preset", payoutPresetEncoder],
  ["accessType", accessTypeEncoder],
  ["gateKey", getAddressEncoder()],
  ["allowlistRoot", fixEncoderSize(getBytesEncoder(), 32)],
  ["creatorAddonBps", getU16Encoder()],
  ["integrator", getAddressEncoder()],
  ["integratorBps", getU16Encoder()],
  ["initialBoxes", getU8Encoder()],
]);

/** A `CreatePoolParams` with the Step 4 defaults: Public, Standard, no gate, no integrator. */
export function poolParams(
  overrides: Partial<CreatePoolParams> & Pick<CreatePoolParams, "nonce" | "price">,
): CreatePoolParams {
  return {
    token: 0,
    preset: PayoutPreset.Standard,
    accessType: AccessType.Public,
    gateKey: DEFAULT_ADDRESS,
    allowlistRoot: new Uint8Array(32),
    creatorAddonBps: 0,
    integrator: DEFAULT_ADDRESS,
    integratorBps: 0,
    initialBoxes: 0,
    ...overrides,
  };
}

export interface Pool {
  game: Address;
  creator: Address;
  nonce: bigint;
  token: number;
  mint: Address;
  tokenProgram: Address;
  vault: Address;
  price: bigint;
  preset: PayoutPreset;
  accessType: AccessType;
  gateKey: Address;
  allowlistRoot: ReadonlyUint8Array;
  creatorAddonBps: number;
  integrator: Address;
  integratorBps: number;
  platformFee: bigint;
  creatorFee: bigint;
  integratorFee: bigint;
  status: PoolStatus;
  sold: number;
  owners: Address[];
  creatorBoxes: number;
  sponsoredTotal: bigint;
  sponsorCount: number;
  sponsorshipsOpen: number;
  var: Address;
  varEndAt: bigint;
  sampledSlot: bigint;
  sampledHash: ReadonlyUint8Array;
  varReplacements: number;
  drawn: boolean;
  homeAxis: number[];
  awayAxis: number[];
  prizePool: bigint;
  quarterPrize: bigint[];
  quartersSettled: number;
  winningBox: number[];
  feesPaid: boolean;
  unpaidPrizePool: bigint;
  returned: number;
  splitAmount: bigint;
  cancelledByAdmin: boolean;
  abandoned: boolean;
  createdAt: bigint;
  lockedAt: bigint;
  bump: number;
  vaultBump: number;
  reserved: ReadonlyUint8Array;
}

export const poolDecoder = getStructDecoder([
  ["game", getAddressDecoder()],
  ["creator", getAddressDecoder()],
  ["nonce", getU64Decoder()],
  ["token", getU8Decoder()],
  ["mint", getAddressDecoder()],
  ["tokenProgram", getAddressDecoder()],
  ["vault", getAddressDecoder()],
  ["price", getU64Decoder()],
  ["preset", payoutPresetDecoder],
  ["accessType", accessTypeDecoder],
  ["gateKey", getAddressDecoder()],
  ["allowlistRoot", fixDecoderSize(getBytesDecoder(), 32)],
  ["creatorAddonBps", getU16Decoder()],
  ["integrator", getAddressDecoder()],
  ["integratorBps", getU16Decoder()],
  ["platformFee", getU64Decoder()],
  ["creatorFee", getU64Decoder()],
  ["integratorFee", getU64Decoder()],
  ["status", poolStatusDecoder],
  ["sold", getU8Decoder()],
  ["owners", getArrayDecoder(getAddressDecoder(), { size: 25 })],
  ["creatorBoxes", getU8Decoder()],
  ["sponsoredTotal", getU64Decoder()],
  ["sponsorCount", getU16Decoder()],
  ["sponsorshipsOpen", getU16Decoder()],
  ["var", getAddressDecoder()],
  ["varEndAt", getU64Decoder()],
  ["sampledSlot", getU64Decoder()],
  ["sampledHash", fixDecoderSize(getBytesDecoder(), 32)],
  ["varReplacements", getU8Decoder()],
  ["drawn", getBooleanDecoder()],
  ["homeAxis", getArrayDecoder(getU8Decoder(), { size: 10 })],
  ["awayAxis", getArrayDecoder(getU8Decoder(), { size: 10 })],
  ["prizePool", getU64Decoder()],
  ["quarterPrize", getArrayDecoder(getU64Decoder(), { size: 4 })],
  ["quartersSettled", getU8Decoder()],
  ["winningBox", getArrayDecoder(getU8Decoder(), { size: 4 })],
  ["feesPaid", getBooleanDecoder()],
  ["unpaidPrizePool", getU64Decoder()],
  ["returned", getU32Decoder()],
  ["splitAmount", getU64Decoder()],
  ["cancelledByAdmin", getBooleanDecoder()],
  ["abandoned", getBooleanDecoder()],
  ["createdAt", getI64Decoder()],
  ["lockedAt", getI64Decoder()],
  ["bump", getU8Decoder()],
  ["vaultBump", getU8Decoder()],
  ["reserved", fixDecoderSize(getBytesDecoder(), 128)],
]) as Decoder<Pool>;

export interface CreatorCounter {
  creator: Address;
  game: Address;
  openCount: number;
  bump: number;
}
export const creatorCounterDecoder: Decoder<CreatorCounter> = getStructDecoder([
  ["creator", getAddressDecoder()],
  ["game", getAddressDecoder()],
  ["openCount", getU8Decoder()],
  ["bump", getU8Decoder()],
]);

export interface Sponsorship {
  pool: Address;
  wallet: Address;
  amount: bigint;
  bump: number;
}
export const sponsorshipDecoder: Decoder<Sponsorship> = getStructDecoder([
  ["pool", getAddressDecoder()],
  ["wallet", getAddressDecoder()],
  ["amount", getU64Decoder()],
  ["bump", getU8Decoder()],
]);

export interface PoolCreated {
  time: bigint;
  pool: Address;
  game: Address;
  creator: Address;
  token: number;
  mint: Address;
  price: bigint;
  preset: PayoutPreset;
  accessType: AccessType;
  creatorAddonBps: number;
  integrator: Address;
  integratorBps: number;
  platformFee: bigint;
  creatorFee: bigint;
  integratorFee: bigint;
}
export const poolCreatedDecoder: Decoder<PoolCreated> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["game", getAddressDecoder()],
  ["creator", getAddressDecoder()],
  ["token", getU8Decoder()],
  ["mint", getAddressDecoder()],
  ["price", getU64Decoder()],
  ["preset", payoutPresetDecoder],
  ["accessType", accessTypeDecoder],
  ["creatorAddonBps", getU16Decoder()],
  ["integrator", getAddressDecoder()],
  ["integratorBps", getU16Decoder()],
  ["platformFee", getU64Decoder()],
  ["creatorFee", getU64Decoder()],
  ["integratorFee", getU64Decoder()],
]);

export interface BoxesBought {
  time: bigint;
  pool: Address;
  buyer: Address;
  /** 0-based indices in assignment order (Borsh Vec<u8>, u32 length prefix). */
  boxes: number[];
  count: number;
  soldAfter: number;
}
export const boxesBoughtDecoder: Decoder<BoxesBought> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["buyer", getAddressDecoder()],
  ["boxes", getArrayDecoder(getU8Decoder())],
  ["count", getU8Decoder()],
  ["soldAfter", getU8Decoder()],
]);

export interface PoolLocked {
  time: bigint;
  pool: Address;
  lockedAt: bigint;
}
export const poolLockedDecoder: Decoder<PoolLocked> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["lockedAt", getI64Decoder()],
]);

export interface Sponsored {
  time: bigint;
  pool: Address;
  sponsor: Address;
  amount: bigint;
  sponsoredTotal: bigint;
}
export const sponsoredDecoder: Decoder<Sponsored> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["sponsor", getAddressDecoder()],
  ["amount", getU64Decoder()],
  ["sponsoredTotal", getU64Decoder()],
]);

export interface GateKeyRotated {
  time: bigint;
  pool: Address;
}
export const gateKeyRotatedDecoder: Decoder<GateKeyRotated> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
]);

export const SLOT_HASHES_SYSVAR = address("SysvarS1otHashes111111111111111111111111111");
export const TOKEN_2022_PROGRAM = address("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/** The three optional token-path accounts; absent ones are the program id (Anchor's convention). */
export interface TokenPath {
  mint?: Address;
  tokenAccount?: Address;
  tokenProgram?: Address;
}

function optional(a: Address | undefined, writable = false) {
  return {
    address: a ?? PROGRAM_ID,
    role: a !== undefined && writable ? AccountRole.WRITABLE : AccountRole.READONLY,
  };
}

/** The pool-side addresses every post-creation instruction needs. */
export interface PoolRefs {
  pool: Address;
  game: Address;
  creator: Address;
}

export async function createPoolInstruction(
  creator: TransactionSigner,
  game: Address,
  params: CreatePoolParams,
  token: TokenPath = {},
): Promise<SignedInstruction> {
  const pool = await poolPda(game, creator.address, params.nonce);
  const accounts: SignedMetas = [
    { address: creator.address, role: AccountRole.WRITABLE_SIGNER, signer: creator },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: game, role: AccountRole.READONLY },
    { address: pool, role: AccountRole.WRITABLE },
    { address: await vaultPda(pool), role: AccountRole.WRITABLE },
    { address: await counterPda(creator.address, game), role: AccountRole.WRITABLE },
    { address: await overridePda(creator.address), role: AccountRole.READONLY },
    optional(token.mint),
    optional(token.tokenAccount, true),
    optional(token.tokenProgram),
    { address: SLOT_HASHES_SYSVAR, role: AccountRole.READONLY },
    { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("create_pool", createPoolParamsEncoder.encode(params)),
  };
}

export async function buyInstruction(
  buyer: TransactionSigner,
  refs: PoolRefs,
  count: number,
  token: TokenPath = {},
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: buyer.address, role: AccountRole.WRITABLE_SIGNER, signer: buyer },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: refs.game, role: AccountRole.READONLY },
    { address: refs.pool, role: AccountRole.WRITABLE },
    { address: await vaultPda(refs.pool), role: AccountRole.WRITABLE },
    { address: await counterPda(refs.creator, refs.game), role: AccountRole.WRITABLE },
    { address: await overridePda(refs.creator), role: AccountRole.READONLY },
    optional(token.mint),
    optional(token.tokenAccount, true),
    optional(token.tokenProgram),
    { address: SLOT_HASHES_SYSVAR, role: AccountRole.READONLY },
    { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("buy", getU8Encoder().encode(count)),
  };
}

export async function sponsorInstruction(
  sponsor: TransactionSigner,
  refs: PoolRefs,
  amount: bigint,
  token: TokenPath = {},
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: sponsor.address, role: AccountRole.WRITABLE_SIGNER, signer: sponsor },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: refs.game, role: AccountRole.READONLY },
    { address: refs.pool, role: AccountRole.WRITABLE },
    { address: await vaultPda(refs.pool), role: AccountRole.WRITABLE },
    { address: await sponsorshipPda(refs.pool, sponsor.address), role: AccountRole.WRITABLE },
    optional(token.mint),
    optional(token.tokenAccount, true),
    optional(token.tokenProgram),
    { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("sponsor", getU64Encoder().encode(amount)),
  };
}

export async function rotateGateKeyInstruction(
  creator: TransactionSigner,
  pool: Address,
  newKey: Address,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: creator.address, role: AccountRole.READONLY_SIGNER, signer: creator },
    { address: pool, role: AccountRole.WRITABLE },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("rotate_gate_key", getAddressEncoder().encode(newKey)),
  };
}

/** Permissionless: no signer among the accounts; the fee payer is whoever sends it. */
export async function closeCounterInstruction(
  creator: Address,
  game: Address,
  feeWallet: Address,
): Promise<Instruction> {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: await counterPda(creator, game), role: AccountRole.WRITABLE },
      { address: await configPda(), role: AccountRole.READONLY },
      { address: feeWallet, role: AccountRole.WRITABLE },
    ],
    data: withDiscriminator("close_counter", new Uint8Array(0)),
  };
}

// ---------------------------------------------------------------------------
// Step 5: the draw (PROGRAM §4.4). Four discriminator-only instructions.
// ---------------------------------------------------------------------------

export const ENTROPY_PROGRAM_ADDRESS = address("3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X");

const bytes32 = () => fixDecoderSize(getBytesDecoder(), 32);
const digits10 = () => getArrayDecoder(getU8Decoder(), { size: 10 });

export interface VarSet {
  time: bigint;
  pool: Address;
  var: Address;
  endAt: bigint;
}
export const varSetDecoder: Decoder<VarSet> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["var", getAddressDecoder()],
  ["endAt", getU64Decoder()],
]);

export interface VarSampled {
  time: bigint;
  pool: Address;
  var: Address;
  sampler: Address;
  slot: bigint;
  endAt: bigint;
  slotHash: ReadonlyUint8Array;
}
export const varSampledDecoder: Decoder<VarSampled> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["var", getAddressDecoder()],
  ["sampler", getAddressDecoder()],
  ["slot", getU64Decoder()],
  ["endAt", getU64Decoder()],
  ["slotHash", bytes32()],
]);

export interface VarReplaced {
  time: bigint;
  pool: Address;
  oldVar: Address;
  newVar: Address;
  endAt: bigint;
  replacements: number;
}
export const varReplacedDecoder: Decoder<VarReplaced> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["oldVar", getAddressDecoder()],
  ["newVar", getAddressDecoder()],
  ["endAt", getU64Decoder()],
  ["replacements", getU8Decoder()],
]);

export interface DigitsDrawn {
  time: bigint;
  pool: Address;
  var: Address;
  value: ReadonlyUint8Array;
  homeAxis: number[];
  awayAxis: number[];
}
export const digitsDrawnDecoder: Decoder<DigitsDrawn> = getStructDecoder([
  ["time", getI64Decoder()],
  ["pool", getAddressDecoder()],
  ["var", getAddressDecoder()],
  ["value", bytes32()],
  ["homeAxis", digits10()],
  ["awayAxis", digits10()],
]);

/** `set_var`: `score_authority (s)`, `config`, `pool (w)`, `var`, event CPI. */
export async function setVarInstruction(
  keeper: TransactionSigner,
  pool: Address,
  varAddress: Address,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: keeper.address, role: AccountRole.READONLY_SIGNER, signer: keeper },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: pool, role: AccountRole.WRITABLE },
    { address: varAddress, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("set_var", new Uint8Array(0)),
  };
}

/** `sample_var` (permissionless): `sampler (s)`, `pool (w)`, `var (w)`, SlotHashes, Entropy, event CPI. */
export async function sampleVarInstruction(
  sampler: TransactionSigner,
  pool: Address,
  varAddress: Address,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: sampler.address, role: AccountRole.READONLY_SIGNER, signer: sampler },
    { address: pool, role: AccountRole.WRITABLE },
    { address: varAddress, role: AccountRole.WRITABLE },
    { address: SLOT_HASHES_SYSVAR, role: AccountRole.READONLY },
    { address: ENTROPY_PROGRAM_ADDRESS, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("sample_var", new Uint8Array(0)),
  };
}

/** `draw`: `score_authority (s)`, `config`, `pool (w)`, `var`, event CPI. */
export async function drawInstruction(
  keeper: TransactionSigner,
  pool: Address,
  varAddress: Address,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: keeper.address, role: AccountRole.READONLY_SIGNER, signer: keeper },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: pool, role: AccountRole.WRITABLE },
    { address: varAddress, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("draw", new Uint8Array(0)),
  };
}

/** `replace_var`: `admin (s)`, `config`, `pool (w)`, `new_var`, event CPI. */
export async function replaceVarInstruction(
  admin: TransactionSigner,
  pool: Address,
  newVar: Address,
): Promise<SignedInstruction> {
  const accounts: SignedMetas = [
    { address: admin.address, role: AccountRole.READONLY_SIGNER, signer: admin },
    { address: await configPda(), role: AccountRole.READONLY },
    { address: pool, role: AccountRole.WRITABLE },
    { address: newVar, role: AccountRole.READONLY },
    ...(await eventCpiAccounts()),
  ];
  return {
    programAddress: PROGRAM_ID,
    accounts,
    data: withDiscriminator("replace_var", new Uint8Array(0)),
  };
}

// ---------------------------------------------------------------------------
// Sending and reading back
// ---------------------------------------------------------------------------

/**
 * Surfpool answers a failed remote fetch ("Failed to fetch accounts from remote: …") with a
 * JSON-RPC error that has no `data`; Kit's `getSolanaErrorFromJsonRpcError` then throws
 * `TypeError: Cannot destructure property 'err' of 'data'` and the message is lost. The
 * transport sees the raw response first and rethrows such an error with the RPC `message`,
 * so a CI log says what happened (Step 3 audit M1 b). Errors with `data` (program errors,
 * preflight failures) pass through to Kit untouched.
 */
export class RpcErrorWithoutData extends Error {
  constructor(
    readonly code: number | undefined,
    message: string,
  ) {
    super(`RPC error ${code ?? "?"} without data: ${message}`);
    this.name = "RpcErrorWithoutData";
  }
}

const defaultTransport = createDefaultRpcTransport({ url: LOCALNET_URL });
const transport: RpcTransport = async (...args) => {
  const response = await defaultTransport<unknown>(...args);
  const error = (response as { error?: { code?: number; message?: string; data?: unknown } }).error;
  if (error !== undefined && error.data === undefined) {
    throw new RpcErrorWithoutData(error.code, error.message ?? "(no message)");
  }
  return response as never;
};
export const rpc: Rpc<SolanaRpcApi> = createSolanaRpcFromTransport(transport);
export const rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi> =
  createSolanaRpcSubscriptions(LOCALNET_URL.replace(/^http/, "ws").replace(/:8899$/, ":8900"));
const sendAndConfirm = sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions });

/** An instruction whose signer metas carry their `TransactionSigner`, so `send` needs no extra signers. */
export type SignedInstruction = Instruction & InstructionWithSigners;
export type SignedMetas = (AccountMeta | AccountSignerMeta)[];

type Instructions = Instruction | SignedInstruction | (Instruction | SignedInstruction)[];

/** One transaction with `instructions` in order (a single instruction, or a list such as
 * `[setComputeUnitLimit(400_000), sampleVar]`). */
export async function send(
  payer: TransactionSigner,
  instructions: Instructions,
): Promise<Signature> {
  const build = async () => {
    const { value: blockhash } = await rpc.getLatestBlockhash().send();
    const message = pipe(
      createTransactionMessage({ version: 0 }),
      (m) => setTransactionMessageFeePayerSigner(payer, m),
      (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
      (m) =>
        appendTransactionMessageInstructions(
          Array.isArray(instructions) ? instructions : [instructions],
          m,
        ),
    );
    const tx = await signTransactionMessageWithSigners(message);
    assertIsTransactionWithBlockhashLifetime(tx);
    return tx;
  };
  // Any instruction that touches an account Surfpool has not seen makes it ask mainnet, and
  // the public endpoint stalls on some of those (Step 3 audit M1); a stall is never the
  // answer a test is after, so it is retried here, and only it. Surfpool's stall surfaces at
  // simulation, before the transaction is accepted, so a re-send cannot double-process; all
  // the same, a retry first asks whether the signature is already known and, if it is, waits
  // for it instead of re-sending (Step 4 audit L2). A stall can outlive the blockhash (150
  // slots is 30 s at 200 ms slots once Surfpool produces blocks on the clock, Step 5), so an
  // unknown signature is re-signed over a fresh blockhash.
  let tx = await build();
  let attempt = 0;
  await withRetry(
    async () => {
      if (attempt++ > 0) {
        const { value } = await rpc.getSignatureStatuses([getSignatureFromTransaction(tx)]).send();
        const status = value[0];
        if (status !== null && status !== undefined) {
          if (status.err) throw new Error(`transaction failed: ${JSON.stringify(status.err)}`);
          return;
        }
        tx = await build();
      }
      await sendAndConfirm(tx, { commitment: "confirmed" });
    },
    { onlyRpcStalls: true },
  );
  return getSignatureFromTransaction(tx);
}

/**
 * Retry a remote-touching call that failed for a reason other than a program error (an RPC
 * stall on Surfpool's first fetch of an account from mainnet). Program errors (an Anchor custom
 * code) are never retried; they are the test's answer (Step 3 audit M1 c).
 */
export async function withRetry<T>(
  fn: () => Promise<T>,
  {
    attempts = 3,
    delayMs = 2_000,
    onlyRpcStalls = false,
  }: { attempts?: number; delayMs?: number; onlyRpcStalls?: boolean } = {},
): Promise<T> {
  let last: unknown;
  for (let attempt = 1; attempt <= attempts; attempt++) {
    try {
      return await fn();
    } catch (error) {
      if (hasCustomErrorCode(error)) throw error;
      if (onlyRpcStalls && !(error instanceof RpcErrorWithoutData)) throw error;
      last = error;
      if (attempt < attempts) await new Promise((r) => setTimeout(r, delayMs));
    }
  }
  throw last;
}

function hasCustomErrorCode(error: unknown): boolean {
  try {
    customErrorCode(error);
    return true;
  } catch {
    return false;
  }
}

/** Send and return the Anchor custom error code the program failed with, or `null` on success. */
export async function sendExpectingError(
  payer: TransactionSigner,
  instructions: Instructions,
): Promise<number | null> {
  try {
    await send(payer, instructions);
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

export const COMPUTE_BUDGET_PROGRAM = address("ComputeBudget111111111111111111111111111111");

/** `ComputeBudgetInstruction::SetComputeUnitLimit(units)`: `[2] ‖ u32 LE`. */
export function setComputeUnitLimit(units: number): Instruction {
  const data = new Uint8Array(5);
  data[0] = 2;
  data.set(getU32Encoder().encode(units), 1);
  return { programAddress: COMPUTE_BUDGET_PROGRAM, accounts: [], data };
}

/** `meta.computeUnitsConsumed` of a confirmed transaction. */
export async function computeUnitsConsumed(signature: Signature): Promise<bigint> {
  const tx = await rpc
    .getTransaction(signature, {
      maxSupportedTransactionVersion: 0,
      commitment: "confirmed",
      encoding: "json",
    })
    .send();
  if (!tx?.meta) throw new Error("transaction not found");
  const consumed = tx.meta.computeUnitsConsumed;
  if (consumed === undefined || consumed === null) throw new Error("no computeUnitsConsumed");
  return BigInt(consumed);
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
