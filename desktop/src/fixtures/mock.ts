/**
 * MOCK FIXTURES — SAMPLE DATA ONLY.
 *
 * Everything in this file is invented to mirror the supplied design reference
 * (reference/Synapse — telas.pdf). None of it was read from a repository,
 * vault, agent CLI or the Synapse runtime. Pages must render it behind the
 * "sample data" label and must never present it as verified or live.
 */
import type { AgentId } from "../components/ui";

export type StatusTone = "wait" | "work" | "done" | "fail" | "you";

export interface WaitingItem {
  kind: "approval" | "question" | "blocked";
  agent: AgentId;
  agentName: string;
  text: string;
  mono?: boolean;
  project: string;
}

export const MOCK_WAITING: WaitingItem[] = [
  { kind: "approval", agent: "cc", agentName: "claude-code", text: "pnpm add ioredis", mono: true, project: "atlas-api" },
  { kind: "question", agent: "cc", agentName: "claude-code", text: "Migrate legacy/auth.js too?", project: "atlas-api" },
  { kind: "blocked", agent: "cx", agentName: "codex", text: "Postgres unreachable · session stopped", project: "atlas-api" },
];

export interface ActiveTask {
  id: string;
  title: string;
  /** Invented request text shown in the task preview. */
  request: string;
  project: string;
  agents: { id: AgentId; name: string }[];
  state: string;
  tone: StatusTone;
}

const CC = { id: "cc", name: "claude-code" } as const;
const CX = { id: "cx", name: "codex" } as const;
const KM = { id: "km", name: "kimi" } as const;

export const MOCK_TASKS: ActiveTask[] = [
  { id: "move-auth", title: "Move auth to server-side sessions", request: "Replace stateless JWTs with server-side sessions and keep old tokens valid for 30 days.", project: "atlas-api", agents: [CC, CX], state: "Terminals · 1 question", tone: "you" },
  { id: "note-search", title: "Speed up note search", request: "Notes search is slow on large vaults; find where the time goes and fix it.", project: "atlas-api", agents: [CC, CX], state: "Paused · budget", tone: "you" },
  { id: "pagination", title: "Fix pagination on /notes", request: "The last page of /notes repeats the final item. Fix it and cover it with a test.", project: "atlas-api", agents: [CC], state: "Review · needs re-check", tone: "done" },
  { id: "translate-errors", title: "Translate error messages", request: "Translate user-facing API error messages into Portuguese.", project: "atlas-api", agents: [CC, KM], state: "Plan · blocked", tone: "fail" },
  { id: "onboarding-copy", title: "Draft onboarding copy", request: "Write the first-run onboarding copy for the web app.", project: "mochi-web", agents: [CC], state: "Context · not started", tone: "wait" },
];

export interface MockProject {
  name: string;
  path: string;
  line: string;
  stages: { label: string; tone: "ok" | "warn" | "you" | "wait"; text: string }[];
  repo: string;
  head: string;
  dirty: string;
  worktrees: string;
  wts: { id: string; task: string; agent: string; tone: StatusTone; state: string }[];
  vault?: { name: string; notes: string };
  tasks: { id: string; name: string; tone: StatusTone; state: string }[];
}

export const MOCK_PROJECTS: MockProject[] = [
  {
    name: "atlas-api",
    path: "~/code/atlas-api",
    line: "4 tasks · 2 sessions running",
    stages: [
      { label: "Workspace", tone: "ok", text: "3 worktrees" },
      { label: "Context", tone: "ok", text: "vault · 4 notes linked" },
      { label: "Sessions", tone: "you", text: "1 running · 1 paused" },
      { label: "Changes", tone: "warn", text: "1 approved, not applied" },
    ],
    repo: "git · origin/main",
    head: "main @ 9b04e7d",
    dirty: "3 files",
    worktrees: "3 active",
    wts: [
      { id: "a3f2", task: "Move auth to sessions", agent: "claude-code", tone: "work", state: "running" },
      { id: "b71c", task: "Move auth to sessions", agent: "codex", tone: "fail", state: "stopped · kept" },
      { id: "9c1e", task: "Fix pagination on /notes", agent: "claude-code", tone: "you", state: "approved · not applied" },
    ],
    vault: { name: "Cérebro", notes: "Projects/Atlas, Decisions/Auth 2026, Security, #backend" },
    tasks: [
      { id: "move-auth", name: "Move auth to server-side sessions", tone: "you", state: "terminals" },
      { id: "note-search", name: "Speed up note search", tone: "you", state: "paused" },
      { id: "pagination", name: "Fix pagination on /notes", tone: "done", state: "review" },
      { id: "translate-errors", name: "Translate error messages", tone: "fail", state: "blocked" },
    ],
  },
  {
    name: "mochi-web",
    path: "~/code/mochi-web",
    line: "1 task · not started",
    stages: [
      { label: "Workspace", tone: "ok", text: "no worktrees" },
      { label: "Context", tone: "wait", text: "no vault" },
      { label: "Sessions", tone: "wait", text: "none" },
      { label: "Changes", tone: "wait", text: "none" },
    ],
    repo: "git · origin/main",
    head: "main @ 41c9d02",
    dirty: "clean",
    worktrees: "none",
    wts: [],
    tasks: [{ id: "onboarding-copy", name: "Draft onboarding copy", tone: "wait", state: "not started" }],
  },
  {
    name: "synapse",
    path: "~/code/synapse",
    line: "no tasks yet",
    stages: [
      { label: "Workspace", tone: "ok", text: "no worktrees" },
      { label: "Context", tone: "wait", text: "no vault" },
      { label: "Sessions", tone: "wait", text: "none" },
      { label: "Changes", tone: "wait", text: "none" },
    ],
    repo: "git · origin/main",
    head: "main @ 099c889",
    dirty: "clean",
    worktrees: "none",
    wts: [],
    tasks: [],
  },
  {
    name: "ensaios",
    path: "~/escrita/ensaios",
    line: "no vault · no tasks yet",
    stages: [
      { label: "Workspace", tone: "wait", text: "plain folder" },
      { label: "Context", tone: "wait", text: "no vault" },
      { label: "Sessions", tone: "wait", text: "none" },
      { label: "Changes", tone: "wait", text: "none" },
    ],
    repo: "plain folder · no git",
    head: "—",
    dirty: "—",
    worktrees: "none",
    wts: [],
    tasks: [],
  },
];

export interface MockSession {
  id: string;
  agent: AgentId;
  agentName: string;
  task: string;
  project: string;
  state: string;
  tone: StatusTone;
  wt: string;
}

export const MOCK_SESSIONS: MockSession[] = [
  { id: "a3f2", agent: "cc", agentName: "claude-code", task: "Move auth to server-side sessions", project: "atlas-api", state: "paused · question", tone: "you", wt: "a3f2" },
  { id: "b71c", agent: "cx", agentName: "codex", task: "Move auth to server-side sessions", project: "atlas-api", state: "stopped · timeout", tone: "fail", wt: "b71c" },
  { id: "e81a", agent: "cc", agentName: "claude-code", task: "Speed up note search", project: "atlas-api", state: "running", tone: "work", wt: "e81a" },
  { id: "9c1e", agent: "cc", agentName: "claude-code", task: "Fix pagination on /notes", project: "atlas-api", state: "finished · exit 0", tone: "done", wt: "9c1e" },
];

export type ContextSource = "notes" | "history" | "suggestion";

export interface ContextEntry {
  id: string;
  source: ContextSource;
  project: string;
  title: string;
  where: string;
  /** Full synthetic text. The page shows a deliberately shortened excerpt of it. */
  text: string;
  /** Where this record comes from. Always a statement that it is invented. */
  provenance: string;
  /** Set when the record exists but cannot be used; the value is the reason shown. */
  unavailable?: string;
}

export const SOURCE_LABEL: Record<ContextSource, string> = {
  notes: "your notes",
  history: "private history",
  suggestion: "pending suggestion",
};

const NOTE = "Synthetic sample note, invented for this UI. Not read from a vault.";
const HIST = "Synthetic sample decision, invented for this UI. No task ever recorded it.";
const SUG = "Synthetic sample draft, invented for this UI. Nothing generated it.";

export const MOCK_CONTEXT_ENTRIES: ContextEntry[] = [
  {
    id: "tokens", source: "notes", project: "atlas-api", title: "Security / Tokens", where: "Refresh", provenance: NOTE,
    text: "Every refresh token works once. When used, the server issues a new pair and marks the old one spent. If a spent token comes back, the whole family is revoked. Clients must store the newest refresh token before they discard the previous one, otherwise a crash between the two steps locks the user out. Access tokens live for fifteen minutes and are never stored server-side. Revocation lists are keyed by family id and kept for the refresh lifetime plus one day. Device names shown to users come from the client at login and are never trusted for anything else.",
  },
  {
    id: "auth-2026", source: "notes", project: "atlas-api", title: "Decisions / Auth 2026", where: "Expiry", provenance: NOTE,
    text: "Sessions expire after 30 days of inactivity. Old JWTs stay valid for the same 30 days so that signed-in clients are not logged out by the migration.",
  },
  {
    id: "journal", source: "notes", project: "atlas-api", title: "Journal / October 2", where: "Journal", provenance: NOTE,
    text: "The iOS client resends the refresh token in parallel when it returns from the background. Worth remembering when a reuse alarm fires.",
  },
  {
    id: "runbook", source: "notes", project: "atlas-api", title: "Runbooks / Old deploys", where: "Archive", provenance: NOTE,
    text: "Retired deploy steps.", unavailable: "this sample record is marked unavailable",
  },
  {
    id: "hist-jwt", source: "history", project: "atlas-api", title: "Decision · Oct 12", where: "Move auth to sessions", provenance: HIST,
    text: "“Accept old JWTs for 30 days” — decided by you in Move auth to sessions.",
  },
  {
    id: "hist-prisma", source: "history", project: "atlas-api", title: "Decision · Prisma upgrade", where: "Oct 8", provenance: HIST,
    text: "Pin Prisma to the current major until the migration of the notes table is finished.",
  },
  {
    id: "hist-copy", source: "history", project: "mochi-web", title: "Decision · Tone of voice", where: "Oct 3", provenance: HIST,
    text: "Onboarding copy stays short and avoids exclamation marks.",
  },
  {
    id: "sug-rotation", source: "suggestion", project: "atlas-api", title: "Security / Rotation", where: "Draft", provenance: SUG,
    text: "Draft: refresh tokens rotate on every use, and reuse revokes the whole family.",
  },
  {
    id: "sug-ios", source: "suggestion", project: "atlas-api", title: "Journal / iOS resend", where: "Draft", provenance: SUG,
    text: "Draft: the iOS client may send the same refresh token twice when it resumes.",
  },
];

/** Records that are malformed (missing text, unknown source, duplicate id) are dropped, never shown. */
export function usableEntries(entries: ContextEntry[]): ContextEntry[] {
  const seen = new Set<string>();
  return entries.filter((e) => {
    const ok =
      !!e.id &&
      !seen.has(e.id) &&
      (e.source in SOURCE_LABEL) &&
      !!findProject(e.project) &&
      !!e.title?.trim() &&
      !!e.text?.trim() &&
      !!e.provenance?.trim();
    if (ok) seen.add(e.id);
    return ok;
  });
}

export const entriesFor = (project: string | null | undefined) =>
  project ? usableEntries(MOCK_CONTEXT_ENTRIES).filter((e) => e.project === project) : [];

/** Drafts and unavailable records can be read but never become context. */
export const isSelectable = (e: ContextEntry) => e.source !== "suggestion" && !e.unavailable;

export const findProject = (name: string | null | undefined) => MOCK_PROJECTS.find((p) => p.name === name);
export const findTask = (id: string | undefined) => MOCK_TASKS.find((t) => t.id === id);
