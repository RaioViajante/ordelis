// The UI's only native call: the development-only runtime identity query (issue #40).
// The command name is fixed and takes no arguments; nothing here can name another command, a path or an
// executable. Every answer is checked against the exact Rust contract in
// docs/implementation/desktop-runtime-bridge.md before the UI may show it.
import { invoke, isTauri } from "@tauri-apps/api/core";

const COMMAND = "get_runtime_status";

export const ERROR_CODES = [
  "runtime_not_installed",
  "runtime_unsupported_version",
  "runtime_timeout",
  "runtime_output_too_large",
  "runtime_output_malformed",
  "runtime_failed",
  "runtime_busy",
  "unsupported_platform",
] as const;
export type RuntimeErrorCode = (typeof ERROR_CODES)[number];

// Mirrors RuntimeStatus in src-tauri/src/runtime_bridge.rs.
export interface RuntimeStatus {
  schema_version: 1;
  provenance: { source: "python_runtime"; interpreter_kind: "dev_checkout"; observed_at_ms: number; duration_ms: number };
  runtime: { version: string; python: string };
  gate: { policy_version: number };
}

// "unreadable" covers anything outside the contract: a changed schema, an ACL rejection, an unknown error.
// Its contents are dropped, so no native text reaches the UI.
export type RuntimeResult =
  | { kind: "ok"; status: RuntimeStatus }
  | { kind: "error"; code: RuntimeErrorCode }
  | { kind: "unreadable" };

/** Whether the page runs inside the Tauri webview at all. It says nothing about whether the command is allowed. */
export function inDesktop(): boolean {
  return isTauri();
}

// One request at a time: a call while another is in flight shares its answer instead of reaching Rust,
// which would only answer runtime_busy. Cleared when it settles, so nothing is cached.
let pending: Promise<RuntimeResult> | null = null;

export function queryRuntimeStatus(): Promise<RuntimeResult> {
  pending ??= invoke(COMMAND)
    .then(readStatus, readError)
    .finally(() => {
      pending = null;
    });
  return pending;
}

type Obj = Record<string, unknown>;

function exact(value: unknown, keys: string[]): value is Obj {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const own = Object.keys(value).sort();
  return own.length === keys.length && [...keys].sort().every((k, i) => own[i] === k);
}

// Same rule as Rust: 1–32 characters of [A-Za-z0-9._+-], so a path or message cannot ride in an identity field.
const identity = (v: unknown) => typeof v === "string" && /^[A-Za-z0-9._+-]{1,32}$/.test(v);
const count = (v: unknown) => Number.isSafeInteger(v) && (v as number) >= 0;
// Positive and within what Date can represent.
const time = (v: unknown) => count(v) && (v as number) > 0 && (v as number) <= 8.64e15;
const u32 = (v: unknown) => Number.isInteger(v) && (v as number) >= 1 && (v as number) <= 0xffffffff;

export function readStatus(raw: unknown): RuntimeResult {
  if (!exact(raw, ["schema_version", "provenance", "runtime", "gate"]) || raw.schema_version !== 1) {
    return { kind: "unreadable" };
  }
  const { provenance: p, runtime: r, gate: g } = raw;
  const valid =
    exact(p, ["source", "interpreter_kind", "observed_at_ms", "duration_ms"]) &&
    p.source === "python_runtime" &&
    p.interpreter_kind === "dev_checkout" &&
    time(p.observed_at_ms) &&
    count(p.duration_ms) &&
    exact(r, ["version", "python"]) &&
    identity(r.version) &&
    identity(r.python) &&
    exact(g, ["policy_version"]) &&
    u32(g.policy_version);
  return valid ? { kind: "ok", status: raw as unknown as RuntimeStatus } : { kind: "unreadable" };
}

// The bridge rejects with { code, message }. Only a known code is kept; the message is never shown.
export function readError(raw: unknown): RuntimeResult {
  if (exact(raw, ["code", "message"]) && typeof raw.message === "string") {
    const code = ERROR_CODES.find((c) => c === raw.code);
    if (code) return { kind: "error", code };
  }
  return { kind: "unreadable" };
}
