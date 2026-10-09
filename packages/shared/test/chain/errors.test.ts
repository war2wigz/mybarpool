/** `programErrorName`, `MYBARPOOL_ERROR_NAMES` against the IDL file, `SdkError`. */
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { MYBARPOOL_ERROR_NAMES } from "../../src/generated/index.js";
import {
  customFromInstructionError,
  isSdkError,
  programErrorName,
  SdkError,
} from "../../src/index.js";

describe("program error names (PROGRAM §8)", () => {
  it("MYBARPOOL_ERROR_NAMES has 65 entries matching idl/mybarpool.json", () => {
    const idl = JSON.parse(
      readFileSync(new URL("../../../../idl/mybarpool.json", import.meta.url), "utf8"),
    ) as { errors: Array<{ code: number; name: string }> };
    expect(idl.errors).toHaveLength(65);
    expect(Object.keys(MYBARPOOL_ERROR_NAMES)).toHaveLength(65);
    for (const e of idl.errors) expect(MYBARPOOL_ERROR_NAMES[e.code]).toBe(e.name);
  });

  it("programErrorName maps a code, a preflight error shape, and nothing else", () => {
    expect(programErrorName(6018)).toBe("AllowlistProofInvalid");
    expect(programErrorName(6004)).toBe("SalesClosed");
    expect(programErrorName(1)).toBeUndefined();
    expect(programErrorName(3010)).toBeUndefined();
    const preflight = { context: { err: { InstructionError: [1, { Custom: 6017 }] } } };
    expect(programErrorName(preflight)).toBe("GateKeyNotSigner");
    const nested = { cause: { cause: preflight } };
    expect(programErrorName(nested)).toBe("GateKeyNotSigner");
    expect(programErrorName({})).toBeUndefined();
    expect(
      customFromInstructionError({ InstructionError: [0, "InvalidArgument"] }),
    ).toBeUndefined();
  });

  it("SdkError carries its code and details and describes itself", () => {
    const e = new SdkError("TransactionTooLarge", { sizeBytes: 1300, limit: 1232 });
    expect(isSdkError(e)).toBe(true);
    expect(isSdkError(e, "TransactionTooLarge")).toBe(true);
    expect(isSdkError(e, "NotFound")).toBe(false);
    expect(isSdkError(new Error("x"))).toBe(false);
    expect(e.message).toBe("transaction is 1300 bytes, the limit is 1232");
    expect(
      new SdkError("SimulationFailed", { programError: "SalesClosed", programErrorCode: 6004 })
        .message,
    ).toBe("simulation failed: SalesClosed (6004)");
    expect(new SdkError("NoSubscriptions").message).toBe("the client has no rpcSubscriptions");
    expect(new SdkError("BlockhashExpired", { lastValidBlockHeight: 5n }).message).toContain("5");
    for (const code of [
      "ProgramError",
      "TokenAccountMissing",
      "NotAllowlisted",
      "InsufficientFunds",
      "NotFound",
      "SimulationFailed",
    ] as const) {
      expect(new SdkError(code).message.length).toBeGreaterThan(0);
    }
  });
});
