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
  title: string;
  project: string;
  agents: { id: AgentId; name: string }[];
  state: string;
  tone: StatusTone;
}

const CC = { id: "cc", name: "claude-code" } as const;
const CX = { id: "cx", name: "codex" } as const;
const KM = { id: "km", name: "kimi" } as const;

export const MOCK_TASKS: ActiveTask[] = [
  { title: "Move auth to server-side sessions", project: "atlas-api", agents: [CC, CX], state: "Terminals · 1 question", tone: "you" },
  { title: "Speed up note search", project: "atlas-api", agents: [CC, CX], state: "Paused · budget", tone: "you" },
  { title: "Fix pagination on /notes", project: "atlas-api", agents: [CC], state: "Review · needs re-check", tone: "done" },
  { title: "Translate error messages", project: "atlas-api", agents: [CC, KM], state: "Plan · blocked", tone: "fail" },
  { title: "Draft onboarding copy", project: "mochi-web", agents: [CC], state: "Context · not started", tone: "wait" },
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
  tasks: { name: string; tone: StatusTone; state: string }[];
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
      { name: "Move auth to server-side sessions", tone: "you", state: "terminals" },
      { name: "Fix pagination on /notes", tone: "done", state: "review" },
      { name: "Translate error messages", tone: "fail", state: "blocked" },
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
    tasks: [{ name: "Draft onboarding copy", tone: "wait", state: "not started" }],
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
    line: "no vault · 2 tasks",
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

export const MOCK_CONTEXT = {
  vaultName: "Cérebro",
  notes: "1,284 notes · 4 linked to atlas-api",
  path: "~/Obsidian/Cérebro",
  index: "1,284 notes · 23:10",
  stored: "~/.local/share/synapse",
  results: [
    { src: "your notes", where: "Security / Tokens", sub: "Refresh", tok: "1,200 of 1,900 tok", trimmed: true, text: "Every refresh token works once; presenting a spent one revokes the whole family…" },
    { src: "your notes", where: "Journal / October 2", tok: "380 tok", text: "The iOS client resends the refresh token in parallel when it returns from the background…" },
  ],
  history: { where: "Decision · Oct 12, 23:27", tok: "64 tok", text: "“Accept old JWTs for 30 days” — decided by you in Move auth to sessions." },
};
