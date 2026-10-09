/**
 * The DESIGN §10.4 transaction helper over a mock RPC: the format rule, the headroom
 * arithmetic, the data-size limit, the fee conversion, legacy ComputeBudget instructions vs the
 * v1 config, a program error surfacing from simulation, blockhash-last ordering, the size
 * assertion, `networkFeeLamports`, `refreshBlockhash`, and `signAndSend`'s two paths.
 */
import {
  address,
  generateKeyPairSigner,
  type Address,
  type Instruction,
  type KeyPairSigner,
  type TransactionSendingSigner,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import {
  computeUnitLimitFor,
  createMyBarPoolClient,
  CU_MAX,
  DATA_SIZE_HEADROOM,
  isSdkError,
  LAMPORTS_PER_SIGNATURE,
  loadedAccountsDataSizeLimitFor,
  networkFeeLamports,
  prepareTransaction,
  PRIORITY_FEE_CAP_MICRO_LAMPORTS,
  PRIORITY_FEE_FLOOR_MICRO_LAMPORTS,
  priorityFeeLamportsFor,
  refreshBlockhash,
  SdkError,
  signAndSend,
  suggestPriorityFee,
  transactionFormatFor,
  type PreparedTransaction,
} from "../../src/index.js";
import { mockRpc, PROGRAM, type RpcCall } from "./fixtures.js";

const COMPUTE_BUDGET = address("ComputeBudget111111111111111111111111111111");
const MEMO = address("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const BLOCKHASH = {
  blockhash: "9GmRFaPKjHHNDMGRMSXwGEEEcYMaFfD5xVqaZ7oYSoUY",
  lastValidBlockHeight: 1_000n,
};

function memo(text: string): Instruction {
  return { programAddress: MEMO, accounts: [], data: new TextEncoder().encode(text) };
}

interface Sim {
  unitsConsumed?: bigint;
  loadedAccountsDataSize?: bigint | undefined;
  err?: unknown;
}

function rpcFor(sim: Sim, extra: Record<string, (...p: never[]) => unknown> = {}) {
  return mockRpc({
    simulateTransaction: () => ({
      context: { slot: 1n },
      value: {
        err: sim.err ?? null,
        logs: [],
        unitsConsumed: sim.unitsConsumed ?? 100_000n,
        loadedAccountsDataSize:
          "loadedAccountsDataSize" in sim ? sim.loadedAccountsDataSize : 10_000n,
        returnData: null,
        accounts: null,
        innerInstructions: null,
        replacementBlockhash: null,
      },
    }),
    getLatestBlockhash: () => ({ context: { slot: 1n }, value: BLOCKHASH }),
    getRecentPrioritizationFees: () => [],
    ...extra,
  });
}

let payer: KeyPairSigner;
beforeAll(async () => {
  payer = await generateKeyPairSigner();
});

describe("transactionFormatFor (ARCHITECTURE › Wallet acceptance)", () => {
  it("v1 iff the wallet lists 1", () => {
    expect(transactionFormatFor(["legacy", 0])).toBe("legacy");
    expect(transactionFormatFor([0, 1])).toBe("v1");
    expect(transactionFormatFor([1])).toBe("v1");
    expect(transactionFormatFor(new Set([0, 1]))).toBe("v1");
    expect(transactionFormatFor(new Set<number | "legacy">(["legacy"]))).toBe("legacy");
    expect(transactionFormatFor(undefined)).toBe("legacy");
    expect(transactionFormatFor([])).toBe("legacy");
  });
});

describe("the limit arithmetic (DESIGN §10.4)", () => {
  it("compute units: ceil(cu × 1.2) + 30,000, clamped at 1,400,000", () => {
    expect(computeUnitLimitFor(100_000)).toBe(150_000);
    expect(computeUnitLimitFor(279_629)).toBe(365_555); // the Step 8 readiness figure
    expect(computeUnitLimitFor(1)).toBe(30_002);
    expect(computeUnitLimitFor(2_000_000)).toBe(CU_MAX);
    expect(computeUnitLimitFor(1_141_667)).toBe(CU_MAX); // 1,370,001 + 30,000 > the max
  });
  it("data size: simulated + created + 8 KiB", () => {
    expect(loadedAccountsDataSizeLimitFor(10_000)).toBe(10_000 + DATA_SIZE_HEADROOM);
    expect(loadedAccountsDataSizeLimitFor(10_000, 1_442 + 74)).toBe(11_516 + DATA_SIZE_HEADROOM);
  });
  it("the fee conversion rounds up and is the same number either way", () => {
    expect(priorityFeeLamportsFor(50_000, 200_000)).toBe(10_000n); // the "~0.00001 SOL" figure
    expect(priorityFeeLamportsFor(1n, 1)).toBe(1n); // 1e-6 lamports rounds up to 1
    expect(priorityFeeLamportsFor(1_000, 150_000)).toBe(150n);
    expect(priorityFeeLamportsFor(0, 150_000)).toBe(0n);
    expect(priorityFeeLamportsFor(3, 333_334)).toBe(2n); // 1.000002 → 2
  });
  it("networkFeeLamports is signatures × 5,000 plus the priority total", () => {
    expect(LAMPORTS_PER_SIGNATURE).toBe(5_000n);
    expect(networkFeeLamports({ signatureCount: 2, priorityFeeLamports: 10_000n })).toEqual({
      baseFeeLamports: 10_000n,
      priorityFeeLamports: 10_000n,
      totalLamports: 20_000n,
    });
  });
});

describe("suggestPriorityFee", () => {
  it("is the clamped median of the recent fees over the writable accounts", async () => {
    const fees = [5n, 100_000n, 20_000n, 1n, 7_000n];
    const { rpc, calls } = mockRpc({
      getRecentPrioritizationFees: () =>
        fees.map((f, i) => ({ slot: BigInt(i), prioritizationFee: f })),
    });
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    expect(await suggestPriorityFee(client, [PROGRAM])).toBe(7_000n); // median of [1,5,7000,20000,100000]
    expect(calls[0]!.params).toEqual([[PROGRAM]]);
    const low = createMyBarPoolClient({
      rpc: mockRpc({ getRecentPrioritizationFees: () => [{ slot: 1n, prioritizationFee: 3n }] })
        .rpc,
      programAddress: PROGRAM,
    });
    expect(await suggestPriorityFee(low, [])).toBe(PRIORITY_FEE_FLOOR_MICRO_LAMPORTS);
    const high = createMyBarPoolClient({
      rpc: mockRpc({
        getRecentPrioritizationFees: () => [{ slot: 1n, prioritizationFee: 9_000_000n }],
      }).rpc,
      programAddress: PROGRAM,
    });
    expect(await suggestPriorityFee(high, [])).toBe(PRIORITY_FEE_CAP_MICRO_LAMPORTS);
    expect(await suggestPriorityFee(high, [], { capMicroLamports: 100n })).toBe(100n);
    const none = createMyBarPoolClient({
      rpc: mockRpc({ getRecentPrioritizationFees: () => [] }).rpc,
      programAddress: PROGRAM,
    });
    expect(await suggestPriorityFee(none, [])).toBe(PRIORITY_FEE_FLOOR_MICRO_LAMPORTS);
  });
});

function computeBudgetInstructions(p: PreparedTransaction) {
  return p.message.instructions.filter((i) => i.programAddress === COMPUTE_BUDGET);
}

describe("prepareTransaction (DESIGN §10.4, in order)", () => {
  it("v1: limits in the message config, no ComputeBudget instruction, the fee as a total", async () => {
    const { rpc, calls } = rpcFor({ unitsConsumed: 100_000n, loadedAccountsDataSize: 10_000n });
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    const p = await prepareTransaction(client, {
      feePayer: payer,
      instructions: [memo("hi")],
      format: "v1",
      priorityFee: 50_000n,
      createdAccountBytes: 1_442,
    });
    expect(p.format).toBe("v1");
    expect(p.message.version).toBe(1);
    expect(p.computeUnitsSimulated).toBe(100_000);
    expect(p.computeUnitLimit).toBe(150_000);
    expect(p.loadedAccountsDataSizeLimit).toBe(10_000 + 1_442 + DATA_SIZE_HEADROOM);
    expect(p.priorityFeeLamports).toBe(priorityFeeLamportsFor(50_000n, 150_000));
    expect(computeBudgetInstructions(p)).toHaveLength(0);
    expect(p.message.instructions).toHaveLength(1);
    expect(p.signatureCount).toBe(1);
    expect(p.baseFeeLamports).toBe(5_000n);
    expect(p.blockhash).toBe(BLOCKHASH.blockhash);
    expect(p.lastValidBlockHeight).toBe(1_000n);
    expect(p.sizeBytes).toBeGreaterThan(0);
    // Blockhash last: the final RPC call is getLatestBlockhash and nothing follows it.
    expect(calls.map((c) => c.method)).toEqual(["simulateTransaction", "getLatestBlockhash"]);
  });

  it("legacy: ComputeBudget instructions carry the limit, the data size and the price", async () => {
    const { rpc } = rpcFor({ unitsConsumed: 279_629n, loadedAccountsDataSize: 20_000n });
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    const p = await prepareTransaction(client, {
      feePayer: payer,
      instructions: [memo("hi")],
      format: "legacy",
      priorityFee: 1_000n,
    });
    expect(p.message.version).toBe("legacy");
    expect(p.computeUnitLimit).toBe(365_555);
    expect(p.loadedAccountsDataSizeLimit).toBe(20_000 + DATA_SIZE_HEADROOM);
    const cb = computeBudgetInstructions(p);
    expect(cb).toHaveLength(3); // SetComputeUnitLimit, SetLoadedAccountsDataSizeLimit, SetComputeUnitPrice
    const kinds = cb.map((i) => i.data![0]).sort();
    expect(kinds).toEqual([2, 3, 4]);
    expect(p.priorityFeeLamports).toBe(priorityFeeLamportsFor(1_000n, 365_555));
    expect(p.message.instructions.filter((i) => i.programAddress === MEMO)).toHaveLength(1);
  });

  it("without a priority fee no price instruction is added and the total is zero", async () => {
    const { rpc } = rpcFor({ unitsConsumed: 1_000n, loadedAccountsDataSize: undefined });
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    const p = await prepareTransaction(client, {
      feePayer: payer,
      instructions: [memo("x")],
      format: "legacy",
    });
    expect(computeBudgetInstructions(p)).toHaveLength(1); // the limit only
    expect(p.priorityFeeLamports).toBe(0n);
    expect(p.loadedAccountsDataSizeLimit).toBeUndefined();
    expect("loadedAccountsDataSizeLimit" in p).toBe(false);
  });

  it("the simulation's Custom(6004) surfaces as SimulationFailed with the program error name", async () => {
    const { rpc, calls } = rpcFor({ err: { InstructionError: [0, { Custom: 6004 }] } });
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    const e = await prepareTransaction(client, {
      feePayer: payer,
      instructions: [memo("x")],
      format: "v1",
    }).catch((x) => x);
    expect(isSdkError(e, "SimulationFailed")).toBe(true);
    expect((e as SdkError).details.programError).toBe("SalesClosed");
    expect((e as SdkError).details.programErrorCode).toBe(6004);
    expect(calls.map((c) => c.method)).toEqual(["simulateTransaction"]); // no blockhash fetched
    const anchor = await prepareTransaction(
      createMyBarPoolClient({
        rpc: rpcFor({ err: { InstructionError: [0, { Custom: 3010 }] } }).rpc,
        programAddress: PROGRAM,
      }),
      { feePayer: payer, instructions: [memo("x")], format: "v1" },
    ).catch((x) => x);
    expect(isSdkError(anchor, "SimulationFailed")).toBe(true);
    expect((anchor as SdkError).details.programError).toBeUndefined();
    expect((anchor as SdkError).details.programErrorCode).toBe(3010);
    const runtime = await prepareTransaction(
      createMyBarPoolClient({
        rpc: rpcFor({ err: "InvalidArgument" }).rpc,
        programAddress: PROGRAM,
      }),
      { feePayer: payer, instructions: [memo("x")], format: "v1" },
    ).catch((x) => x);
    expect(isSdkError(runtime, "SimulationFailed")).toBe(true);
    expect((runtime as SdkError).details.programErrorCode).toBeUndefined();
  });

  it("a message over the legacy limit is TransactionTooLarge with size and limit; the same under v1 passes", async () => {
    const big = memo("m".repeat(1_300));
    const { rpc } = rpcFor({});
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    const e = await prepareTransaction(client, {
      feePayer: payer,
      instructions: [big],
      format: "legacy",
    }).catch((x) => x);
    expect(isSdkError(e, "TransactionTooLarge")).toBe(true);
    expect((e as SdkError).details.limit).toBe(1232);
    expect((e as SdkError).details.sizeBytes).toBeGreaterThan(1232);
    const ok = await prepareTransaction(client, {
      feePayer: payer,
      instructions: [big],
      format: "v1",
    });
    expect(ok.sizeBytes).toBeGreaterThan(1232);
    expect(ok.sizeBytes).toBeLessThanOrEqual(4096);
  });

  it("refreshBlockhash re-stamps and changes nothing else", async () => {
    let n = 0;
    const { rpc } = rpcFor(
      {},
      {
        getLatestBlockhash: () => ({
          context: { slot: 1n },
          value:
            n++ === 0
              ? BLOCKHASH
              : {
                  blockhash: "4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi",
                  lastValidBlockHeight: 2_000n,
                },
        }),
      },
    );
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    const p = await prepareTransaction(client, {
      feePayer: payer,
      instructions: [memo("x")],
      format: "v1",
    });
    const r = await refreshBlockhash(client, p);
    expect(r.blockhash).not.toBe(p.blockhash);
    expect(r.lastValidBlockHeight).toBe(2_000n);
    expect(r.computeUnitLimit).toBe(p.computeUnitLimit);
    expect(r.sizeBytes).toBe(p.sizeBytes);
  });
});

describe("signAndSend", () => {
  function sendingSigner(onSend: (bytes: Uint8Array) => void): TransactionSendingSigner {
    return {
      address: payer.address,
      signAndSendTransactions: async (txs: readonly { messageBytes: Uint8Array }[]) => {
        const sig = new Uint8Array(64).fill(7);
        for (const t of txs) onSend(t.messageBytes as Uint8Array);
        return txs.map(() => sig);
      },
    } as unknown as TransactionSendingSigner;
  }

  async function prepared(
    rpc: ReturnType<typeof mockRpc>["rpc"],
    feePayer = payer as unknown as TransactionSendingSigner | KeyPairSigner,
  ) {
    const client = createMyBarPoolClient({ rpc, programAddress: PROGRAM });
    return {
      client,
      p: await prepareTransaction(client, { feePayer, instructions: [memo("x")], format: "v1" }),
    };
  }

  it("a wallet fee payer: signs and sends through the wallet, then polls to confirmed", async () => {
    let statuses = 0;
    const { rpc, calls } = rpcFor(
      {},
      {
        getSignatureStatuses: () => ({
          context: { slot: 1n },
          value: [
            statuses++ === 0
              ? null
              : { confirmationStatus: "confirmed", err: null, slot: 5n, confirmations: 1n },
          ],
        }),
        getEpochInfo: () => ({
          blockHeight: 10n,
          epoch: 0n,
          absoluteSlot: 1n,
          slotIndex: 0n,
          slotsInEpoch: 1n,
        }),
      },
    );
    const sent: Uint8Array[] = [];
    const wallet = sendingSigner((b) => sent.push(b));
    const { client, p } = await prepared(rpc, wallet);
    const sig = await signAndSend(client, p, { pollIntervalMs: 1 });
    expect(sent).toHaveLength(1);
    expect(typeof sig).toBe("string");
    const methods = calls
      .map((c) => c.method)
      .filter((m) => m !== "simulateTransaction" && m !== "getLatestBlockhash");
    expect(methods).toEqual(["getSignatureStatuses", "getEpochInfo", "getSignatureStatuses"]);
  });

  it("a wallet fee payer: past lastValidBlockHeight with no status is BlockhashExpired, never a second send", async () => {
    const { rpc } = rpcFor(
      {},
      {
        getSignatureStatuses: () => ({ context: { slot: 1n }, value: [null] }),
        getEpochInfo: () => ({
          blockHeight: 5_000n,
          epoch: 0n,
          absoluteSlot: 1n,
          slotIndex: 0n,
          slotsInEpoch: 1n,
        }),
      },
    );
    let sends = 0;
    const { client, p } = await prepared(
      rpc,
      sendingSigner(() => sends++),
    );
    const e = await signAndSend(client, p, { pollIntervalMs: 1 }).catch((x) => x);
    expect(isSdkError(e, "BlockhashExpired")).toBe(true);
    expect((e as SdkError).details.lastValidBlockHeight).toBe(1_000n);
    expect(sends).toBe(1);
  });

  it("a wallet fee payer: a failed status with a program code is ProgramError with its name", async () => {
    const { rpc } = rpcFor(
      {},
      {
        getSignatureStatuses: () => ({
          context: { slot: 1n },
          value: [
            {
              confirmationStatus: "confirmed",
              err: { InstructionError: [0, { Custom: 6017 }] },
              slot: 5n,
              confirmations: 1n,
            },
          ],
        }),
        getEpochInfo: () => ({
          blockHeight: 1n,
          epoch: 0n,
          absoluteSlot: 1n,
          slotIndex: 0n,
          slotsInEpoch: 1n,
        }),
      },
    );
    const { client, p } = await prepared(
      rpc,
      sendingSigner(() => {}),
    );
    const e = await signAndSend(client, p, { pollIntervalMs: 1 }).catch((x) => x);
    expect(isSdkError(e, "ProgramError")).toBe(true);
    expect((e as SdkError).details.programError).toBe("GateKeyNotSigner");
  });

  it("a keypair fee payer needs rpcSubscriptions", async () => {
    const { rpc } = rpcFor({});
    const { client, p } = await prepared(rpc, payer);
    const e = await signAndSend(client, p).catch((x) => x);
    expect(isSdkError(e, "NoSubscriptions")).toBe(true);
  });
});

void (0 as unknown as RpcCall | Address);
