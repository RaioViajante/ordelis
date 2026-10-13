/**
 * SYNTHETIC SESSION FIXTURES — SAMPLE DATA ONLY.
 *
 * Every record here is invented to mirror the supplied design reference. None of it was
 * recorded from a process, an agent CLI, a worktree or the Synapse runtime, and no record
 * corresponds to anything running. Ids are made-up labels, not process or provider ids.
 */
import type { AgentId } from "../components/ui";
import { findProject, findTask, type StatusTone } from "./mock";

export type SessionState = "running" | "paused" | "stopped" | "finished" | "blocked";

export const STATE_LABEL: Record<SessionState, string> = {
  running: "Running",
  paused: "Paused",
  stopped: "Stopped",
  finished: "Finished",
  blocked: "Blocked",
};
export const STATES = Object.keys(STATE_LABEL) as SessionState[];

export const STATE_TONE: Record<SessionState, StatusTone> = {
  running: "work",
  paused: "you",
  stopped: "fail",
  finished: "done",
  blocked: "fail",
};

export type EventKind = "start" | "command" | "output" | "error" | "question" | "stop" | "finish" | "note";

export interface SessionEvent {
  /** Seconds since the recorded start. Order is derived from this, not from array position. */
  t: number;
  kind: EventKind;
  /** Absent when the example has no supporting detail; the page then says so. */
  text?: string;
}

/** How a figure was obtained. `unknown` figures carry no value. */
export type Basis = "measured" | "estimated" | "reported" | "unknown";

export interface UsageFigure {
  label: string;
  value?: string;
  basis: Basis;
}

export type ResultAvailability = "available" | "partial" | "pending" | "none";

export interface SessionResult {
  availability: ResultAvailability;
  summary: string;
  /** What the result does not tell you. */
  limits: string;
}

export type HandoffStatus = "ready" | "partial" | "not-prepared";

export interface MockSession {
  id: string;
  agent: AgentId;
  agentName: string;
  taskId: string;
  task: string;
  project: string;
  state: SessionState;
  /** Short qualifier shown beside the state in lists, such as "timeout". */
  tag?: string;
  /** Why the session is in this state. */
  reason: string;
  /** Sample start, written as invented text. */
  started: string;
  /** Inert worktree id; null when none was created. */
  worktree: string | null;
  worktreeNote: string;
  events: SessionEvent[];
  result: SessionResult;
  handoff: { status: HandoffStatus; note: string };
  usage: UsageFigure[];
  /** Whether the agent's documentation advertises resume. Advertised is never verified here. */
  resume: "advertised" | "unknown";
  /** Always a statement that this record is invented. */
  provenance: string;
}

const CC = { agent: "cc", agentName: "claude-code" } as const;
const CX = { agent: "cx", agentName: "codex" } as const;
const KM = { agent: "km", agentName: "kimi" } as const;
const SYN = "Synthetic sample record, invented for this UI. It was not recorded from any process or CLI.";

export const MOCK_SESSIONS: MockSession[] = [
  {
    id: "a3f2", ...CC, taskId: "move-auth", task: "Move auth to server-side sessions", project: "atlas-api",
    state: "paused", tag: "question", reason: "waiting on your answer to a question", started: "Oct 12 · 23:30 (sample)",
    worktree: "a3f2", worktreeNote: "kept · sample",
    events: [
      { t: 0, kind: "start", text: "session recorded as started" },
      { t: 95, kind: "command", text: "grep -rn \"jwt.verify\" src/" },
      { t: 210, kind: "output", text: "found 14 call sites across 6 files" },
      { t: 640, kind: "question", text: "Migrate legacy/auth.js too?" },
      { t: 641, kind: "stop" },
    ],
    result: { availability: "partial", summary: "2 files edited so far, no tests run.", limits: "Work stopped at a question, so this is not a finished result and nothing has been checked." },
    handoff: { status: "partial", note: "a summary of the first 640 s could be prepared; the open question would not be part of it" },
    usage: [
      { label: "Ran", value: "10:41", basis: "measured" },
      { label: "Tokens", value: "~22k", basis: "estimated" },
      { label: "Cost", basis: "unknown" },
    ],
    resume: "advertised", provenance: SYN,
  },
  {
    id: "b71c", ...CX, taskId: "move-auth", task: "Move auth to server-side sessions", project: "atlas-api",
    state: "stopped", tag: "timeout", reason: "time limit reached (20:00) · exit 130", started: "Oct 12 · 23:52 (sample)",
    worktree: "b71c", worktreeNote: "kept · sample",
    events: [
      { t: 1200, kind: "stop", text: "time limit 20:00 reached · SIGINT sent" },
      { t: 0, kind: "command", text: "pnpm vitest run test/auth/integration" },
      { t: 60, kind: "error", text: "ECONNREFUSED 127.0.0.1:5432 (retry 1/5)" },
      { t: 300, kind: "error", text: "ECONNREFUSED 127.0.0.1:5432 (retry 5/5)" },
      { t: 301, kind: "note", text: "retrying with backoff…" },
      { t: 1201, kind: "finish", text: "exited 130 after 1.9 s" },
    ],
    result: { availability: "partial", summary: "3 files produced · 11 unit tests passed.", limits: "Integration tests never ran because the database was unreachable, so they are untested, not passing." },
    handoff: { status: "ready", note: "a summary, the diff and the notes could be handed to a new run" },
    usage: [
      { label: "Ran", value: "20:00 of 20:00", basis: "measured" },
      { label: "Exit code", value: "130", basis: "reported" },
      { label: "Tokens", value: "~31k", basis: "estimated" },
      { label: "Cost", basis: "unknown" },
    ],
    resume: "advertised", provenance: SYN,
  },
  {
    id: "e81a", ...CC, taskId: "note-search", task: "Speed up note search", project: "atlas-api",
    state: "running", reason: "shown as running in the sample only — no process exists", started: "Oct 13 · 00:04 (sample)",
    worktree: "e81a", worktreeNote: "sample",
    events: [
      { t: 0, kind: "start", text: "session recorded as started" },
      { t: 120, kind: "command", text: "node scripts/profile-search.js" },
      { t: 340, kind: "output", text: "index lookup dominates: 71% of 2.4 s" },
    ],
    result: { availability: "pending", summary: "No result yet.", limits: "A running session has produced nothing to review. This sample never advances." },
    handoff: { status: "not-prepared", note: "nothing to hand over until the session ends" },
    usage: [
      { label: "Ran", value: "14 min", basis: "estimated" },
      { label: "Tokens", basis: "unknown" },
      { label: "Cost", basis: "unknown" },
    ],
    resume: "unknown", provenance: SYN,
  },
  {
    id: "9c1e", ...CC, taskId: "pagination", task: "Fix pagination on /notes", project: "atlas-api",
    state: "finished", tag: "exit 0", reason: "exited normally · exit 0", started: "Oct 12 · 21:10 (sample)",
    worktree: "9c1e", worktreeNote: "kept · approved, not applied · sample",
    events: [
      { t: 0, kind: "start", text: "session recorded as started" },
      { t: 180, kind: "command", text: "pnpm vitest run test/notes/pagination" },
      { t: 181, kind: "error", text: "expected 20 items, received 21 (last page repeats)" },
      { t: 600, kind: "output", text: "fixed off-by-one in page offset" },
      { t: 640, kind: "output", text: "4 tests passed" },
      { t: 641, kind: "finish", text: "exited 0" },
    ],
    result: { availability: "available", summary: "1 file changed · 4 tests passed.", limits: "Only the recorded tests ran. Nothing was applied to the project." },
    handoff: { status: "ready", note: "a summary and the diff could be handed to a new run" },
    usage: [
      { label: "Ran", value: "10:41", basis: "measured" },
      { label: "Exit code", value: "0", basis: "reported" },
      { label: "Tokens", value: "18.2k", basis: "reported" },
      { label: "Cost", basis: "unknown" },
    ],
    resume: "advertised", provenance: SYN,
  },
  {
    id: "c5d0", ...KM, taskId: "translate-errors", task: "Translate error messages", project: "atlas-api",
    state: "blocked", tag: "authorization", reason: "needs an authorization that was not granted", started: "Oct 12 · 22:15 (sample)",
    worktree: null, worktreeNote: "none · not created",
    events: [
      { t: 0, kind: "note", text: "start requested" },
      { t: 1, kind: "stop", text: "start refused until access is authorized" },
      { t: 2, kind: "note" },
    ],
    result: { availability: "none", summary: "Nothing was produced.", limits: "The session never began work, so there are no files, tests or output." },
    handoff: { status: "not-prepared", note: "no work to summarize" },
    usage: [
      { label: "Ran", value: "0:00", basis: "measured" },
      { label: "Tokens", basis: "unknown" },
      { label: "Cost", basis: "unknown" },
    ],
    resume: "unknown", provenance: SYN,
  },
];


/** Chronological order by recorded offset; ties keep their recorded order. */
export function orderedEvents(events: SessionEvent[]): SessionEvent[] {
  return events.map((e, i) => ({ e, i })).sort((a, b) => a.e.t - b.e.t || a.i - b.i).map(({ e }) => e);
}

export function offsetLabel(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `+${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export interface SessionFilter {
  state: SessionState | "all";
  project: string | "all";
  query: string;
}

/** State and project are exact; each whitespace-separated search term must match task, agent or id. */
export function filterSessions(sessions: MockSession[], f: SessionFilter): MockSession[] {
  const terms = f.query.toLowerCase().split(/\s+/).filter(Boolean);
  return sessions.filter((s) => {
    if (f.state !== "all" && s.state !== f.state) return false;
    if (f.project !== "all" && s.project !== f.project) return false;
    const hay = `${s.task} ${s.agentName} ${s.id}`.toLowerCase();
    return terms.every((t) => hay.includes(t));
  });
}

/** A record whose task or project does not exist is dropped rather than shown with dead links. */
export const usableSessions = (): MockSession[] => {
  const seen = new Set<string>();
  return MOCK_SESSIONS.filter((s) => {
    const ok = !!s.id && !seen.has(s.id) && !!findProject(s.project) && !!findTask(s.taskId) && !!s.provenance.trim();
    if (ok) seen.add(s.id);
    return ok;
  });
};

export const findSession = (id: string | null | undefined) => usableSessions().find((s) => s.id === id);

export const sessionsForTask = (taskId: string) => usableSessions().filter((s) => s.taskId === taskId);
