/**
 * The DESIGN §10.4 transaction helper, in the order the spec gives: build, simulate, set the
 * compute-unit and loaded-accounts-data-size limits with headroom, set the priority fee, assert
 * the size against the chosen format, fetch the blockhash last. `signAndSend` is what the app
 * calls when the wallet opens; it confirms at `confirmed` and never sends twice.
 */
import {
  appendTransactionMessageInstructions,
  assertIsTransactionMessageWithinSizeLimit,
  compileTransactionMessage,
  createTransactionMessage,
  estimateResourceLimitsFactory,
  getBase58Decoder,
  getSignatureFromTransaction,
  getSolanaErrorFromTransactionError,
  getTransactionMessageSize,
  getTransactionMessageSizeLimit,
  isSolanaError,
  isTransactionSendingSigner,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageComputeUnitLimit,
  setTransactionMessageComputeUnitPrice,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  setTransactionMessageLoadedAccountsDataSizeLimit,
  setTransactionMessagePriorityFeeLamports,
  signAndSendTransactionMessageWithSigners,
  signTransactionMessageWithSigners,
  SOLANA_ERROR__BLOCK_HEIGHT_EXCEEDED,
  SOLANA_ERROR__TRANSACTION_ERROR__BLOCKHASH_NOT_FOUND,
  SOLANA_ERROR__JSON_RPC__SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE,
  type Blockhash,
  type Instruction,
  type Signature,
  type TransactionMessage,
  type TransactionMessageWithFeePayerSigner,
  type TransactionSigner,
} from "@solana/kit";

import type { MyBarPoolClient } from "./client.js";
import { customErrorCode, programErrorName, SdkError } from "./errors.js";
import {
  computeUnitLimitFor,
  loadedAccountsDataSizeLimitFor,
  priorityFeeLamportsFor,
} from "./fees.js";

/** The wire format: `"v1"` (SIMD-0385, 4,096 bytes) or `"legacy"` (1,232 bytes). */
export type TransactionFormat = "v1" | "legacy";

/**
 * The format a wallet signs: `"v1"` when its `solana:signAndSendTransaction` (or
 * `solana:signTransaction`) feature lists `1` in `supportedTransactionVersions`, else
 * `"legacy"`. The keeper, with its own signer, passes `"v1"` directly.
 */
export function transactionFormatFor(
  supportedTransactionVersions:
    ReadonlyArray<number | "legacy"> | ReadonlySet<number | "legacy"> | undefined,
): TransactionFormat {
  if (!supportedTransactionVersions) return "legacy";
  const list = Array.isArray(supportedTransactionVersions)
    ? (supportedTransactionVersions as ReadonlyArray<number | "legacy">)
    : [...(supportedTransactionVersions as ReadonlySet<number | "legacy">)];
  return list.includes(1) ? "v1" : "legacy";
}

/** Input to {@link prepareTransaction}. */
export interface PrepareOptions {
  /** The fee payer: a wallet (`TransactionSendingSigner`) or a keypair. */
  feePayer: TransactionSigner;
  /** The instructions, in order; the ComputeBudget ones are the helper's to add. */
  instructions: readonly Instruction[];
  /** From {@link transactionFormatFor}. */
  format: TransactionFormat;
  /** Micro-lamports per compute unit (from `suggestPriorityFee` or the caller); `0n` for none. */
  priorityFee?: bigint;
  /** Bytes of accounts the instruction may create (the instruction helpers know; see each). */
  createdAccountBytes?: number;
  /**
   * Commitment of the blockhash fetched last (DESIGN §10.4: `confirmed`). A keeper that would
   * rather never see "Blockhash not found" from a node behind the tip passes `finalized`.
   */
  blockhashCommitment?: "confirmed" | "finalized";
}

/** A transaction ready for the wallet: the message with its lifetime and the numbers set on it. */
export interface PreparedTransaction {
  readonly message: TransactionMessage & TransactionMessageWithFeePayerSigner;
  readonly format: TransactionFormat;
  /** The compute-unit limit set on the message (`ceil(cu × 1.2) + 30,000`, clamped). */
  readonly computeUnitLimit: number;
  /** The simulation's compute units, before headroom. */
  readonly computeUnitsSimulated: number;
  /** The loaded-accounts-data-size limit set, when the simulation reported one. */
  readonly loadedAccountsDataSizeLimit?: number;
  /** The priority fee as a total in lamports (the same number in either format). */
  readonly priorityFeeLamports: bigint;
  /** `signatureCount × LAMPORTS_PER_SIGNATURE`. */
  readonly baseFeeLamports: bigint;
  /** Signatures the message needs. */
  readonly signatureCount: number;
  /** The serialized size, asserted against the format's limit. */
  readonly sizeBytes: number;
  readonly blockhash: Blockhash;
  readonly lastValidBlockHeight: bigint;
  /** The commitment the blockhash was fetched at; `refreshBlockhash` reuses it. */
  readonly blockhashCommitment: "confirmed" | "finalized";
}

type Message = TransactionMessage & TransactionMessageWithFeePayerSigner;

/** Signatures the message needs: the compiled header's signer count. */
function signerCount(message: Message): number {
  const compiled =
    message.version === "legacy"
      ? compileTransactionMessage(message as Message & { version: "legacy" })
      : message.version === 0
        ? compileTransactionMessage(message as Message & { version: 0 })
        : compileTransactionMessage(message as Message & { version: 1 });
  return compiled.header.numSignerAccounts;
}

/** The six steps of DESIGN §10.4, blockhash last; nothing network-bound follows it. */
export async function prepareTransaction(
  client: MyBarPoolClient,
  options: PrepareOptions,
): Promise<PreparedTransaction> {
  const { feePayer, instructions, format } = options;
  const price = options.priorityFee ?? 0n;

  // 1. Build.
  const base = pipe(
    createTransactionMessage({ version: format === "v1" ? 1 : "legacy" }),
    (m) => setTransactionMessageFeePayerSigner(feePayer, m),
    (m) => appendTransactionMessageInstructions([...instructions], m),
  );

  // 2. Simulate (Kit sets the maximum limits and replaces the blockhash itself).
  let estimate: { computeUnitLimit: number; loadedAccountsDataSizeLimit?: number };
  try {
    estimate = await estimateResourceLimitsFactory({ rpc: client.rpc })(base, {
      commitment: "confirmed",
    });
  } catch (cause) {
    const code = customErrorCode(cause);
    const name = code !== undefined ? programErrorName(code) : undefined;
    throw new SdkError(
      "SimulationFailed",
      {
        ...(code !== undefined ? { programErrorCode: code } : {}),
        ...(name !== undefined ? { programError: name } : {}),
      },
      { cause },
    );
  }

  // 3. Limits with headroom.
  const computeUnitLimit = computeUnitLimitFor(estimate.computeUnitLimit);
  const loadedAccountsDataSizeLimit =
    estimate.loadedAccountsDataSizeLimit !== undefined
      ? loadedAccountsDataSizeLimitFor(
          // Kit's RPC transport parses integers as bigint; the estimator passes that through.
          Number(estimate.loadedAccountsDataSizeLimit),
          options.createdAccountBytes ?? 0,
        )
      : undefined;
  let message: Message = setTransactionMessageComputeUnitLimit(computeUnitLimit, base);
  if (loadedAccountsDataSizeLimit !== undefined) {
    message = setTransactionMessageLoadedAccountsDataSizeLimit(
      loadedAccountsDataSizeLimit,
      message,
    );
  }

  // 4. Priority fee: a total in lamports on v1, a price per unit on legacy; the same figure.
  const priorityFeeLamports = priorityFeeLamportsFor(price, computeUnitLimit);
  if (price > 0n) {
    message =
      format === "v1"
        ? setTransactionMessagePriorityFeeLamports(
            priorityFeeLamports,
            message as Message & { version: 1 },
          )
        : setTransactionMessageComputeUnitPrice(price, message as Message & { version: "legacy" });
  }

  // 5. Size, against the chosen format's limit.
  const sizeBytes = getTransactionMessageSize(message);
  try {
    assertIsTransactionMessageWithinSizeLimit(message);
  } catch (cause) {
    throw new SdkError(
      "TransactionTooLarge",
      { sizeBytes, limit: getTransactionMessageSizeLimit(message) },
      { cause },
    );
  }

  // 6. Blockhash last.
  const blockhashCommitment = options.blockhashCommitment ?? "confirmed";
  const { value } = await client.rpc.getLatestBlockhash({ commitment: blockhashCommitment }).send();
  const withLifetime = setTransactionMessageLifetimeUsingBlockhash(value, message);
  const signatureCount = signerCount(withLifetime);
  return {
    message: withLifetime,
    format,
    computeUnitLimit,
    computeUnitsSimulated: estimate.computeUnitLimit,
    ...(loadedAccountsDataSizeLimit !== undefined ? { loadedAccountsDataSizeLimit } : {}),
    priorityFeeLamports,
    baseFeeLamports: BigInt(signatureCount) * 5_000n,
    signatureCount,
    sizeBytes,
    blockhash: value.blockhash,
    lastValidBlockHeight: value.lastValidBlockHeight,
    blockhashCommitment,
  };
}

/** Re-stamp a prepared transaction with a fresh blockhash (the one retry after `BlockhashExpired`). */
export async function refreshBlockhash(
  client: MyBarPoolClient,
  prepared: PreparedTransaction,
): Promise<PreparedTransaction> {
  const { value } = await client.rpc
    .getLatestBlockhash({ commitment: prepared.blockhashCommitment })
    .send();
  return {
    ...prepared,
    message: setTransactionMessageLifetimeUsingBlockhash(value, prepared.message),
    blockhash: value.blockhash,
    lastValidBlockHeight: value.lastValidBlockHeight,
  };
}

/** Options for {@link signAndSend}. */
export interface SignAndSendOptions {
  abortSignal?: AbortSignal;
  /** How often the wallet path polls `getSignatureStatuses` (ms). */
  pollIntervalMs?: number;
}

/**
 * Sign and send a prepared transaction, confirming at `confirmed`. A wallet fee payer
 * (`TransactionSendingSigner`) signs with every partial signer first (a gate key) and then
 * sends; confirmation polls `getSignatureStatuses` bounded by `lastValidBlockHeight` — past it
 * with no status the transaction was never processed and `SdkError("BlockhashExpired")` is
 * thrown, never a second send. A keypair fee payer signs here and goes through
 * `sendAndConfirmTransactionFactory` (needs `client.rpcSubscriptions`). A program custom code
 * is rethrown as `SdkError("ProgramError", { programError })`.
 */
export async function signAndSend(
  client: MyBarPoolClient,
  prepared: PreparedTransaction,
  options: SignAndSendOptions = {},
): Promise<Signature> {
  const message = prepared.message;
  try {
    if (isTransactionSendingSigner(message.feePayer)) {
      const bytes = await signAndSendTransactionMessageWithSigners(message as never, {
        ...(options.abortSignal ? { abortSignal: options.abortSignal } : {}),
      });
      const signature = bytesToSignature(bytes);
      await confirmByPolling(client, signature, prepared.lastValidBlockHeight, options);
      return signature;
    }
    if (!client.rpcSubscriptions) throw new SdkError("NoSubscriptions");
    const signed = await signTransactionMessageWithSigners(message as never);
    const send = sendAndConfirmTransactionFactory({
      rpc: client.rpc as never,
      rpcSubscriptions: client.rpcSubscriptions as never,
    });
    await send(signed as never, {
      commitment: "confirmed",
      ...(options.abortSignal ? { abortSignal: options.abortSignal } : {}),
    });
    return getSignatureFromTransaction(signed as never);
  } catch (cause) {
    throw classify(cause, prepared.lastValidBlockHeight);
  }
}

/** A signature is the base58 of its 64 bytes; Kit's sending signer returns the bytes. */
function bytesToSignature(bytes: Uint8Array): Signature {
  return getBase58Decoder().decode(bytes) as Signature;
}

async function confirmByPolling(
  client: MyBarPoolClient,
  signature: Signature,
  lastValidBlockHeight: bigint,
  options: SignAndSendOptions,
): Promise<void> {
  const interval = options.pollIntervalMs ?? 1_000;
  for (;;) {
    options.abortSignal?.throwIfAborted();
    const { value } = await client.rpc.getSignatureStatuses([signature]).send();
    const status = value[0];
    if (status) {
      if (status.err) throw getSolanaErrorFromTransactionError(status.err as never);
      if (status.confirmationStatus === "confirmed" || status.confirmationStatus === "finalized")
        return;
    }
    const epoch = await client.rpc.getEpochInfo({ commitment: "confirmed" }).send();
    if (epoch.blockHeight > lastValidBlockHeight) {
      throw new SdkError("BlockhashExpired", { lastValidBlockHeight });
    }
    await new Promise((r) => setTimeout(r, interval));
  }
}

/** Preflight's "Blockhash not found": the transaction was never sent; a refresh and one retry. */
function isBlockhashNotFoundAtPreflight(cause: unknown): boolean {
  return (
    isSolanaError(cause, SOLANA_ERROR__JSON_RPC__SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE) &&
    isSolanaError(cause.cause, SOLANA_ERROR__TRANSACTION_ERROR__BLOCKHASH_NOT_FOUND)
  );
}

function classify(cause: unknown, lastValidBlockHeight: bigint): unknown {
  if (cause instanceof SdkError) return cause;
  if (
    isSolanaError(cause, SOLANA_ERROR__BLOCK_HEIGHT_EXCEEDED) ||
    isBlockhashNotFoundAtPreflight(cause)
  ) {
    return new SdkError("BlockhashExpired", { lastValidBlockHeight }, { cause });
  }
  const code = customErrorCode(cause);
  if (code !== undefined) {
    const name = programErrorName(code);
    return new SdkError(
      "ProgramError",
      { programErrorCode: code, ...(name ? { programError: name } : {}) },
      { cause },
    );
  }
  if (
    isSolanaError(cause, SOLANA_ERROR__JSON_RPC__SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE)
  ) {
    return new SdkError("SimulationFailed", {}, { cause });
  }
  return cause;
}
