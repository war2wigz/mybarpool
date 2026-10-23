/**
 * The client object every function of the chain layer takes: a connection the caller pays
 * for, optional subscriptions, and the program address. The SDK ships with no endpoint and
 * no default connection (ARCHITECTURE › Open source and third-party clients).
 */
import type {
  Address,
  GetAccountInfoApi,
  GetEpochInfoApi,
  GetLatestBlockhashApi,
  GetMultipleAccountsApi,
  GetProgramAccountsApi,
  GetRecentPrioritizationFeesApi,
  GetSignatureStatusesApi,
  GetTransactionApi,
  Rpc,
  RpcSubscriptions,
  SendTransactionApi,
  SimulateTransactionApi,
  AccountNotificationsApi,
  LogsNotificationsApi,
  SignatureNotificationsApi,
  SlotNotificationsApi,
} from "@solana/kit";

import { MYBARPOOL_PROGRAM_ADDRESS } from "../generated/index.js";

/**
 * The one commitment the chain layer reads, simulates, subscribes and confirms at (the
 * finality pass, 2026-10-23). Under Alpenglow (mainnet since 2026-10-21) `finalized` is the
 * confirmed slot — a slot behind the tip, nothing slower — and `confirmed` is going away in a
 * later Agave release, so nothing here names it. On a TowerBFT-style local validator (Surfpool
 * before the Alpenglow gate) `finalized` trails by 31 slots; that is the validator's model, not
 * a reason to read two commitments.
 */
export const COMMITMENT = "finalized" as const;
export type Commitment = "confirmed" | "finalized";

/**
 * The RPC methods the chain layer calls — the minimal intersection, never `SolanaRpcApi`
 * whole, so a narrowed RPC (a proxy, a mock) type-checks.
 */
export type MyBarPoolRpcApi = GetAccountInfoApi &
  GetMultipleAccountsApi &
  GetProgramAccountsApi &
  SimulateTransactionApi &
  GetLatestBlockhashApi &
  GetRecentPrioritizationFeesApi &
  SendTransactionApi &
  GetSignatureStatusesApi &
  GetTransactionApi &
  GetEpochInfoApi;

/** The subscription methods the chain layer uses (`watch.ts` and `signAndSend`). */
export type MyBarPoolRpcSubscriptionsApi = AccountNotificationsApi &
  LogsNotificationsApi &
  SignatureNotificationsApi &
  SlotNotificationsApi;

/** What every chain-layer function takes. */
export interface MyBarPoolClient {
  /** The connection; the caller's, never the SDK's. */
  readonly rpc: Rpc<MyBarPoolRpcApi>;
  /** WebSocket subscriptions; required by `watch*` and by `signAndSend` with a keypair payer. */
  readonly rpcSubscriptions?: RpcSubscriptions<MyBarPoolRpcSubscriptionsApi>;
  /** The deployment to talk to; defaults to the published id. Check it against the repository. */
  readonly programAddress: Address;
}

/** Options for {@link createMyBarPoolClient}. */
export interface MyBarPoolClientOptions {
  /** The connection. */
  rpc: Rpc<MyBarPoolRpcApi>;
  /** WebSocket subscriptions, when the caller wants `watch*` or keypair confirmation. */
  rpcSubscriptions?: RpcSubscriptions<MyBarPoolRpcSubscriptionsApi>;
  /** Another deployment of the program (a localnet, a fork); defaults to the published id. */
  programAddress?: Address;
}

/**
 * Build a client. The program address defaults to `MYBARPOOL_PROGRAM_ADDRESS` and reaches
 * every generated call as `{ programAddress }`, so a localnet or a fork is one argument away.
 */
export function createMyBarPoolClient(options: MyBarPoolClientOptions): MyBarPoolClient {
  const client: MyBarPoolClient = {
    rpc: options.rpc,
    programAddress: options.programAddress ?? MYBARPOOL_PROGRAM_ADDRESS,
  };
  return options.rpcSubscriptions
    ? { ...client, rpcSubscriptions: options.rpcSubscriptions }
    : client;
}
