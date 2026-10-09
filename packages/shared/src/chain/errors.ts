/**
 * The chain layer's one error type, and the program-error name lookup. The SDK names the
 * error; the copy for each is the app's (DESIGN §10.10).
 */
import { isSolanaError, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM } from "@solana/kit";

import { MYBARPOOL_ERROR_NAMES } from "../generated/index.js";

/** What went wrong, as a closed set the app can switch on. */
export type SdkErrorCode =
  | "SimulationFailed"
  | "ProgramError"
  | "TokenAccountMissing"
  | "NotAllowlisted"
  | "InsufficientFunds"
  | "BlockhashExpired"
  | "TransactionTooLarge"
  | "NoSubscriptions"
  | "NotFound";

/** Details carried by an {@link SdkError}, by code. */
export interface SdkErrorDetails {
  /** PROGRAM §8 error name when the failure was the program's (`SimulationFailed`, `ProgramError`). */
  programError?: string;
  /** The raw custom error code, when the failure was a program's (`SimulationFailed`, `ProgramError`). */
  programErrorCode?: number;
  /** The serialized size that failed the assertion (`TransactionTooLarge`). */
  sizeBytes?: number;
  /** The format's limit (`TransactionTooLarge`). */
  limit?: number;
  /** The block height after which the blockhash was dead (`BlockhashExpired`). */
  lastValidBlockHeight?: bigint;
  /** The address that was missing (`TokenAccountMissing`, `NotFound`). */
  address?: string;
}

/** The chain layer's error: a code, details, and the underlying cause when there is one. */
export class SdkError extends Error {
  override readonly name = "SdkError";
  constructor(
    readonly code: SdkErrorCode,
    readonly details: SdkErrorDetails = {},
    options: { cause?: unknown; message?: string } = {},
  ) {
    super(options.message ?? describe(code, details), { cause: options.cause });
  }
}

function describe(code: SdkErrorCode, d: SdkErrorDetails): string {
  switch (code) {
    case "SimulationFailed":
      return d.programError
        ? `simulation failed: ${d.programError} (${d.programErrorCode})`
        : "simulation failed";
    case "ProgramError":
      return `program error: ${d.programError ?? d.programErrorCode}`;
    case "TokenAccountMissing":
      return `token account ${d.address ?? ""} does not exist`;
    case "NotAllowlisted":
      return "the buyer is not on the allowlist";
    case "InsufficientFunds":
      return "insufficient funds";
    case "BlockhashExpired":
      return `blockhash expired (block height ${d.lastValidBlockHeight})`;
    case "TransactionTooLarge":
      return `transaction is ${d.sizeBytes} bytes, the limit is ${d.limit}`;
    case "NoSubscriptions":
      return "the client has no rpcSubscriptions";
    case "NotFound":
      return `account ${d.address ?? ""} not found`;
  }
}

/** Whether `e` is an {@link SdkError} (with `code` when given). */
export function isSdkError(e: unknown, code?: SdkErrorCode): e is SdkError {
  return e instanceof SdkError && (code === undefined || e.code === code);
}

/**
 * The PROGRAM §8 name of a custom error code, or of a Kit `SolanaError` carrying one; `undefined`
 * for a code that is not the program's (Anchor's 2000–3999, another program's, `1`).
 */
export function programErrorName(codeOrError: number | unknown): string | undefined {
  const code = typeof codeOrError === "number" ? codeOrError : customErrorCode(codeOrError);
  return code === undefined ? undefined : MYBARPOOL_ERROR_NAMES[code];
}

/** The `Custom(n)` code inside a Kit error chain, if any. */
export function customErrorCode(error: unknown): number | undefined {
  let e: unknown = error;
  for (let depth = 0; depth < 6 && e !== undefined && e !== null; depth++) {
    if (isSolanaError(e, SOLANA_ERROR__INSTRUCTION_ERROR__CUSTOM)) return e.context.code;
    const fromErr = customFromInstructionError((e as { context?: { err?: unknown } }).context?.err);
    if (fromErr !== undefined) return fromErr;
    e = (e as { cause?: unknown }).cause;
  }
  return undefined;
}

/** `{ InstructionError: [index, { Custom: code }] }`, the RPC's shape for a preflight failure. */
export function customFromInstructionError(err: unknown): number | undefined {
  if (typeof err !== "object" || err === null || !("InstructionError" in err)) return undefined;
  const inner = (err as { InstructionError: [number, unknown] }).InstructionError[1];
  if (typeof inner === "object" && inner !== null && "Custom" in inner) {
    return Number((inner as { Custom: number | bigint }).Custom);
  }
  return undefined;
}
