/**
 * Surfpool cheatcodes for the localnet tests (ARCHITECTURE › Environments).
 *
 * Thin, typed wrappers over Surfpool's `surfnet_*` JSON-RPC methods. Later
 * steps use these to walk a pool through kickoff, the four quarters and the
 * 30-day reclaim without waiting on a clock, to give test wallets SKR and ORE
 * without owning any, to place an Entropy `Var` in any state, and to set our
 * own accounts' fields by name through the registered IDL.
 *
 * Method and parameter shapes follow Surfpool 1.6.0's
 * `crates/core/src/rpc/surfnet_cheatcodes.rs`. Each wrapper is exercised by a
 * test the first time a step needs it; until then it is a typed call, not a
 * promise about Surfpool's behaviour.
 */

export const LOCALNET_URL = process.env["ANCHOR_PROVIDER_URL"] ?? "http://127.0.0.1:8899";

/** Regolith Entropy, forked from mainnet on first use (PROGRAM §1). */
export const ENTROPY_PROGRAM = "3jSkUuYBoJzQPMEzTvkDFXCZUBksPamrVhrnHR9igu2X";

type JsonRpcResponse<T> =
  | { jsonrpc: "2.0"; id: number; result: T }
  | { jsonrpc: "2.0"; id: number; error: { code: number; message: string; data?: unknown } };

/** `surfnet_timeTravel` target: one of these, camelCase per Surfpool's serde. */
export type TimeTravel =
  { absoluteEpoch: number } | { absoluteSlot: number } | { absoluteTimestamp: number };

/** Fields of `surfnet_setAccount`'s update; every field is optional. */
export interface AccountUpdate {
  lamports?: number;
  /** Account data as a hex string (Surfpool 1.6.0 rejects base64: "Invalid hex data provided"). */
  data?: string;
  owner?: string;
  executable?: boolean;
  rentEpoch?: number;
}

/** Fields of `surfnet_setTokenAccount`'s update; every field is optional. */
export interface TokenAccountUpdate {
  amount?: number;
  delegate?: string | null;
  state?: "initialized" | "frozen" | "uninitialized";
  delegatedAmount?: number;
  closeAuthority?: string | null;
}

export interface EpochInfo {
  absoluteSlot: number;
  blockHeight: number;
  epoch: number;
  slotIndex: number;
  slotsInEpoch: number;
  transactionCount: number | null;
}

export interface Scenario {
  id: string;
  name: string;
  description: string;
  overrides: unknown[];
  tags: string[];
}

export class Localnet {
  #id = 0;

  constructor(readonly url: string = LOCALNET_URL) {}

  async call<T>(method: string, params: unknown[] = []): Promise<T> {
    const id = ++this.#id;
    const response = await fetch(this.url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
    });
    if (!response.ok) {
      throw new Error(`${method}: HTTP ${response.status}`);
    }
    const body = (await response.json()) as JsonRpcResponse<T>;
    if ("error" in body) {
      // Surfpool puts the reason in `data` ("Cannot travel to past timestamp: …"); surface it.
      const detail = body.error.data === undefined ? "" : ` (${JSON.stringify(body.error.data)})`;
      throw new Error(`${method}: ${body.error.code} ${body.error.message}${detail}`);
    }
    return body.result;
  }

  /** Jump the clock. Slots and epochs are the native units; timestamps are Unix seconds. */
  timeTravel(to: TimeTravel): Promise<EpochInfo> {
    return this.call("surfnet_timeTravel", [to]);
  }

  pauseClock(): Promise<EpochInfo> {
    return this.call("surfnet_pauseClock");
  }

  resumeClock(): Promise<EpochInfo> {
    return this.call("surfnet_resumeClock");
  }

  /** Overwrite any account's fields; the Entropy `Var` has no IDL, so this is raw bytes. */
  setAccount(address: string, update: AccountUpdate): Promise<void> {
    return this.call("surfnet_setAccount", [address, update]);
  }

  /** Give `owner` a token account for `mint` with the given state (creates it if missing). */
  setTokenAccount(
    owner: string,
    mint: string,
    update: TokenAccountUpdate,
    tokenProgram?: string,
  ): Promise<void> {
    const params: unknown[] = [owner, mint, update];
    if (tokenProgram !== undefined) params.push(tokenProgram);
    return this.call("surfnet_setTokenAccount", params);
  }

  /** Register our IDL so `registerScenario` can set account fields by name. */
  registerIdl(idl: unknown, slot?: number): Promise<void> {
    return this.call("surfnet_registerIdl", slot === undefined ? [idl] : [idl, slot]);
  }

  registerScenario(scenario: Scenario, slot?: number): Promise<void> {
    return this.call(
      "surfnet_registerScenario",
      slot === undefined ? [scenario] : [scenario, slot],
    );
  }

  /** Compute-unit and account-size profile of a base64-encoded transaction. */
  profileTransaction(transactionBase64: string, tag?: string): Promise<unknown> {
    return this.call(
      "surfnet_profileTransaction",
      tag === undefined ? [transactionBase64] : [transactionBase64, tag],
    );
  }

  surfnetInfo(): Promise<unknown> {
    return this.call("surfnet_getSurfnetInfo");
  }
}
