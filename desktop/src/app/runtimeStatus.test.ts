import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ERROR_CODES, queryRuntimeStatus, readError, readStatus } from "./runtimeStatus";

// The real Rust shape (src-tauri/src/runtime_bridge.rs); mocks exist only in tests.
const valid = () => ({
  schema_version: 1,
  provenance: { source: "python_runtime", interpreter_kind: "dev_checkout", observed_at_ms: 1_776_000_000_000, duration_ms: 84 },
  runtime: { version: "0.12.1", python: "3.14.7" },
  gate: { policy_version: 1 },
});
type Status = ReturnType<typeof valid>;

afterEach(() => clearMocks());

describe("runtime status adapter: IPC call", () => {
  it("invokes only get_runtime_status, with an empty payload", async () => {
    const calls: [string, unknown][] = [];
    mockIPC((cmd, payload) => {
      calls.push([cmd, payload]);
      return valid();
    });
    await expect(queryRuntimeStatus()).resolves.toEqual({ kind: "ok", status: valid() });
    expect(calls).toEqual([["get_runtime_status", {}]]);
  });

  it("shares one in-flight request between overlapping callers, then asks again", async () => {
    let calls = 0;
    let answer!: (v: unknown) => void;
    mockIPC(() => {
      calls++;
      return new Promise((r) => (answer = r));
    });
    const a = queryRuntimeStatus();
    const b = queryRuntimeStatus();
    expect(b).toBe(a);
    await vi.waitFor(() => expect(calls).toBe(1));
    answer(valid());
    await a;
    mockIPC(() => {
      calls++;
      return valid();
    });
    await queryRuntimeStatus();
    expect(calls).toBe(2);
  });

  it.each(ERROR_CODES)("maps the native error %s to its code and drops the message", async (code) => {
    mockIPC(() => {
      throw { code, message: "Development runtime is unavailable." };
    });
    await expect(queryRuntimeStatus()).resolves.toEqual({ kind: "error", code });
  });

  it("never carries native text, such as a path, into the result", async () => {
    const secret = "/Users/someone/.pyenv/versions/3.14.7/bin/python3 HOME=/Users/someone";
    for (const thrown of [
      { code: "runtime_failed", message: secret },
      { code: "runtime_crashed", message: secret },
      { code: "runtime_failed", message: secret, detail: secret },
      `get_runtime_status not allowed. ${secret}`,
      new Error(secret),
    ]) {
      mockIPC(() => {
        throw thrown;
      });
      const result = await queryRuntimeStatus();
      expect(JSON.stringify(result)).not.toContain("/Users");
    }
  });
});

describe("runtime status adapter: response validation", () => {
  const broken: [string, (s: Status) => unknown][] = [
    ["an unexpected top-level field", (s) => ({ ...s, path: "/opt/python" })],
    ["an unexpected provenance field", (s) => ({ ...s, provenance: { ...s.provenance, interpreter: "/opt/python" } })],
    ["an unexpected runtime field", (s) => ({ ...s, runtime: { ...s.runtime, providers: [] } })],
    ["an unexpected gate field", (s) => ({ ...s, gate: { ...s.gate, authorized: true } })],
    ["an unexpected field sorting after every expected key", (s) => ({ ...s, gate: { policy_version: 1, zz: 1 } })],
    ["schema_version 2", (s) => ({ ...s, schema_version: 2 })],
    ["schema_version as a string", (s) => ({ ...s, schema_version: "1" })],
    ["a missing gate", (s) => ({ schema_version: s.schema_version, provenance: s.provenance, runtime: s.runtime })],
    ["a missing runtime version", (s) => ({ ...s, runtime: { python: s.runtime.python } })],
    ["a missing duration", (s) => ({ ...s, provenance: { ...s.provenance, duration_ms: undefined } })],
    ["another source", (s) => ({ ...s, provenance: { ...s.provenance, source: "sample" } })],
    ["another interpreter kind", (s) => ({ ...s, provenance: { ...s.provenance, interpreter_kind: "bundled" } })],
    ["a numeric version", (s) => ({ ...s, runtime: { ...s.runtime, version: 12 } })],
    ["an empty version", (s) => ({ ...s, runtime: { ...s.runtime, version: "" } })],
    ["a version with a path", (s) => ({ ...s, runtime: { ...s.runtime, version: "/usr/bin/python3" } })],
    ["a version with spaces", (s) => ({ ...s, runtime: { ...s.runtime, python: "3.14.7 (main)" } })],
    ["a 33-character version", (s) => ({ ...s, runtime: { ...s.runtime, python: "3".repeat(33) } })],
    ["a negative timestamp", (s) => ({ ...s, provenance: { ...s.provenance, observed_at_ms: -1 } })],
    ["a zero timestamp", (s) => ({ ...s, provenance: { ...s.provenance, observed_at_ms: 0 } })],
    ["a fractional timestamp", (s) => ({ ...s, provenance: { ...s.provenance, observed_at_ms: 1.5 } })],
    ["a timestamp past what Date holds", (s) => ({ ...s, provenance: { ...s.provenance, observed_at_ms: 8.64e15 + 1 } })],
    ["a string timestamp", (s) => ({ ...s, provenance: { ...s.provenance, observed_at_ms: "1776000000000" } })],
    ["a negative duration", (s) => ({ ...s, provenance: { ...s.provenance, duration_ms: -5 } })],
    ["a NaN duration", (s) => ({ ...s, provenance: { ...s.provenance, duration_ms: NaN } })],
    ["policy_version 0", (s) => ({ ...s, gate: { policy_version: 0 } })],
    ["a fractional policy_version", (s) => ({ ...s, gate: { policy_version: 1.5 } })],
    ["policy_version above u32", (s) => ({ ...s, gate: { policy_version: 2 ** 32 } })],
    ["policy_version as a string", (s) => ({ ...s, gate: { policy_version: "1" } })],
    ["an array instead of the gate", (s) => ({ ...s, gate: [1] })],
    ["null", () => null],
    ["a string", () => "ok"],
  ];

  it.each(broken)("rejects %s instead of showing it", (_name, mutate) => {
    expect(readStatus(mutate(valid()))).toEqual({ kind: "unreadable" });
  });

  it("accepts the documented contract, including the identity-string boundaries", () => {
    const edge = { ...valid(), runtime: { version: "a".repeat(32), python: "3.10.0+rc1-x_y" } };
    expect(readStatus(edge)).toEqual({ kind: "ok", status: edge });
    expect(readStatus({ ...valid(), gate: { policy_version: 2 ** 32 - 1 } }).kind).toBe("ok");
    expect(readStatus({ ...valid(), provenance: { ...valid().provenance, duration_ms: 0 } }).kind).toBe("ok");
  });

  it("only maps the exact { code, message } error shape", () => {
    expect(readError({ code: "runtime_busy", message: "x" })).toEqual({ kind: "error", code: "runtime_busy" });
    expect(readError({ code: "runtime_busy" })).toEqual({ kind: "unreadable" });
    expect(readError({ code: "runtime_busy", message: 1 })).toEqual({ kind: "unreadable" });
    expect(readError({ code: "toString", message: "x" })).toEqual({ kind: "unreadable" });
    expect(readError(undefined)).toEqual({ kind: "unreadable" });
  });
});
