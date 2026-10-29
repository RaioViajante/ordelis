# Features

**Status:** inventory. The *intent* sections describe planned behavior and are not commitments. The *today* sections describe what the repository contains and are checked against the source.
**Scope:** every capability group the product is meant to cover, and the current state of each. The mechanisms and open questions for each group are in the [architecture documents](../architecture/overview.md).
**Related:** [Vision](vision.md) · [Roadmap](roadmap.md) · [Terminology](terminology.md), which defines the four status labels used below.

Nothing here is a release commitment, a delivery date or a version number.

## Implementation status at a glance

| Group | State today | Specification |
| --- | --- | --- |
| [A. Workspace and projects](#a-workspace-and-projects) | **Prototype** pages over sample data; **Foundation** project registry (macOS) | [Overview](../architecture/overview.md) |
| [B. Agent integrations](#b-agent-integrations) | **Working** in the inherited CLI; nothing in the desktop | [Agents](../architecture/agents.md) |
| [C. Planning and task delegation](#c-planning-and-task-delegation) | **Prototype** task preview; **Foundation** non-dispatching plan | [Orchestration](../architecture/orchestration.md) |
| [D. Orchestration and execution](#d-orchestration-and-execution) | **Working** in the inherited CLI; **Foundation** safety gate; nothing in the desktop | [Execution](../architecture/execution.md) |
| [E. Git and worktree isolation](#e-git-and-worktree-isolation) | **Planned** | [Worktrees](../architecture/worktrees.md) |
| [F. Context management](#f-context-management) | **Working** vault preview in the CLI; **Prototype** Context page | [Context](../architecture/context.md) |
| [G. Sessions and continuity](#g-sessions-and-continuity) | **Working** in the inherited CLI; **Prototype** Sessions page | [Sessions](../architecture/sessions.md) |
| [H. Integrated terminal](#h-integrated-terminal) | **Planned** | [Execution](../architecture/execution.md#terminal-views) |
| [I. Review and verification](#i-review-and-verification) | **Working** raw multi-agent review in the CLI; rest **Planned** | [Review](../architecture/review.md) |
| [J. Security and privacy](#j-security-and-privacy) | Partly **Working** (desktop boundary); the rest **Foundation** or **Planned** | [Security](../architecture/security.md) |
| [K. Cross-platform distribution](#k-cross-platform-distribution) | macOS desktop build only; no releases | [Cross-platform](../architecture/cross-platform.md) |
| [L. Open-source ecosystem](#l-open-source-ecosystem) | Repository, license, security policy **Working**; the rest **Planned** | [Development](../development/README.md) |

"Inherited CLI" means the MCO command-line runtime that remains in this repository. It runs from a terminal, is not connected to the desktop application, and is not governed by Ordelis's own safety design.

## A. Workspace and projects

**Intent**

- A unified desktop workspace.
- Local project registration and organization, across multiple repositories and workspaces.
- Project-specific configuration.
- Repository state and Git awareness.
- A task composer and a task overview, with task history and status.
- Search and navigation, and keyboard-driven workflows.
- Light and dark themes, and cross-platform UI conventions.

**Today**

- *Prototype.* Home (with a task composer), Projects, Sessions, Context, Agents and Settings, all over invented sample data and marked as such. Pages switch with `⌘`/Ctrl plus 1 to 6. The Context page searches sample notes. The interface is dark only.
- *Foundation.* A Rust registry that lets the user pick a folder through a native panel and remembers it, with list, add, remove and reset commands. macOS only, and no interface calls it yet. See [local project registration](../implementation/desktop-local-projects.md).
- *Planned.* Git awareness, real task history, project configuration, light theme, and Windows and Linux conventions.

## B. Agent integrations

**Intent**

- Claude Code, OpenAI Codex, OpenCode, Kimi, and other compatible command-line agents.
- Discovery of installed agents and inspection of their CLI versions.
- Awareness of authentication state, without Ordelis collecting credentials.
- Inspection of provider capabilities, with model and reasoning configuration where a provider supports it.
- Per-agent availability and environment requirements.
- Provider-independent orchestration contracts, with explicit handling of unsupported features.

Providers do not expose the same capabilities. Ordelis does not assume they do.

**Today**

- *Working, inherited CLI.* Built-in adapters, including Claude Code, Codex CLI and OpenCode, and a way to configure additional agents. `mco doctor` checks installation and authentication readiness, and `mco agent list`, `check` and `models` inspect agents. See the [provider reference](../reference/providers.md), whose adapter count is out of date; the [MVP plan](../implementation/synapse-mvp-plan.md#inherited-capability-audit) treats the source registry as authoritative.
- *Foundation.* An explicit Codex inspection that records version and advertised options without running a task: [guide](../guides/codex-cli-capability-inspection.md).
- *Not present.* A Kimi adapter. Two options are documented and neither is validated: the [Kimi decision gate](../implementation/synapse-mvp-plan.md#kimi-integration-decision-gate).
- *Prototype.* The Agents page states that nothing was checked and shows every fact as unknown.

## C. Planning and task delegation

**Intent**

- Natural-language task descriptions with explicit goals and constraints.
- Plan generation, then plan review and approval by the user.
- Task decomposition, assignment of stages to agents, sequential and parallel stages, and task dependencies.
- Agent roles and responsibilities.
- Replanning after feedback.
- Clear execution state, with defined failure and retry behavior.

**Today**

- *Prototype.* The task composer and a task preview page. "Preview task" changes nothing.
- *Foundation.* `mco context plan` produces one deterministic, private, non-dispatching plan from a task, selected context and policy, and reports unknown settings as unknown. It is not stage decomposition. See the [guide](../guides/private-execution-plan.md).
- *Planned.* Everything else in this group.

## D. Orchestration and execution

**Intent**

- Coordinated multi-agent workflows with controlled process execution and agent lifecycle management.
- Execution policies and per-agent permissions.
- Human authorization gates.
- Execution timeouts, cancellation and termination.
- Error handling, and recovery and resume behavior.
- Observability: a structured event history and live progress.
- A clear separation between orchestration and agent implementation.

**Today**

- *Working, inherited CLI.* `mco run` dispatches one prompt to several chosen agents in parallel, or in a chain, with per-invocation deadlines, a stall timeout, a whole-run deadline, cancellation, three execution modes (`read_only`, `write`, `yolo`) translated to each provider's own controls, and per-invocation status and artifacts. It has no approval gates, and `--allow-paths` validates the requested scope without creating an operating-system sandbox. See the [workflow guide](../guides/workflows.md) and [provider permissions](../reference/providers.md).
- *Foundation.* The [Execution Safety Gate](../implementation/execution-safety-gate.md), which always answers no, and a controlled [probe runner](../guides/codex-zero-model-probes.md) with [interactive authorization](../guides/interactive-probe-authorization.md) and a [private ledger](../implementation/private-authorization-ledger.md). These have been exercised only with fake executables and have run no real provider.
- *Desktop.* The application does not start agents. Its single development-only link to the runtime asks one fixed question, "what are you?"; see the [runtime bridge](../implementation/desktop-runtime-bridge.md).
- *Planned.* Execution from the desktop, authorization gates, recovery.

## E. Git and worktree isolation

**Intent**

- Repository inspection and branch-aware tasks.
- Isolated Git worktrees, created and cleaned up by Ordelis, so agents implement in parallel without sharing a working directory.
- Tracking of changes per agent, Git diffs, conflict detection and integration preparation.
- Explicit, user-controlled merge or squash, with main-branch protection.
- Safe rollback and rejection workflows.

No agent merges into the main branch without the user's authorization.

**Today**

- *Planned.* Nothing in this group is implemented. The inherited runtime states that it does not create or manage worktrees and does not prohibit parallel writers; see the [workflow guide](../guides/workflows.md#scope-and-parallel-writing). The Projects and Sessions prototypes mention worktrees only as sample data.

## F. Context management

**Intent**

- Explicit context selection from project files and documentation, with scoped file access.
- Context packages per task or agent, and reusable project context.
- Optional Obsidian integration that uses selected vault notes, never unrestricted vault ingestion.
- Session history as context.
- Visibility and provenance of everything an agent receives, and control over what each agent gets.
- Local-first handling.

**Today**

- *Working, inherited CLI.* `mco context preview` reads an explicitly given external vault and JSON index and prints bounded excerpts with source paths and truncation flags. It never modifies the vault and starts no agent. See the [guide](../guides/vault-context-preview.md).
- *Foundation.* A mock-only bridge that hands selected excerpts to a fake agent in tests.
- *Prototype.* The Context page: searchable sample notes, provenance and a shared selection.
- *Planned.* Context packages, file scoping, session history as context.

## G. Sessions and continuity

**Intent**

- Session discovery and management, and agent session history.
- Task-to-session relationships.
- Resume and continuation where provider capabilities allow.
- Conversation and execution history, session status, and persistent task metadata.
- Session summaries and handoffs, context continuity, and recovery from interruptions.

**Today**

- *Working, inherited CLI.* `mco session` starts, lists, sends to, stops, resumes and shows the history of persistent agent sessions. By default it stores them inside the repository, which is unsuitable for private state, and it uses a Unix-domain socket whose behavior on Windows is unverified. An open issue, [#23](https://github.com/RaioViajante/ordelis/issues/23), concerns a readiness race when a session starts or resumes.
- *Prototype.* The Sessions page and a session detail view over sample data.
- *Planned.* Task-to-session relations, handoffs, and cross-provider continuity. Existing sessions are provider sessions, not a shared memory.

## H. Integrated terminal

**Intent**

- Per-agent terminal views showing execution output, logs and diagnostics.
- Command visibility and exit statuses, and cancellation.
- Interactive terminal capabilities where feasible, with terminal permissions clearly separated from other permissions.
- Accessibility and keyboard interaction.

**Today**

- *Planned.* There is no terminal in the desktop application.

## I. Review and verification

**Intent**

- Independent reviewing agents, kept separate from the agents that implement.
- Code diff inspection, and test and build results.
- Reviewer findings with severity, and evidence-based completion reports.
- A user-facing review summary, with request changes, reject and approve.
- Explicit integration confirmation, and an audit trail of decisions.

**Today**

- *Working, inherited CLI.* `mco review` runs a read-only review across chosen agents and returns every raw answer with its status. `--debate` and `--synthesize` add read-only stages. The runtime deliberately does not interpret answers into findings or consensus; the structured findings schema and diff-only review were removed from it.
- *Planned.* Independent reviewer roles, severity, evidence reports, decisions and an audit trail.

## J. Security and privacy

**Intent**

- Local-first architecture, with no mandatory Ordelis account and no mandatory cloud backend.
- Least-privilege principles and permission-controlled execution.
- Secure desktop IPC boundaries and context access restrictions.
- Defense against unintended file modification and no automatic elevation of trust.
- A clear distinction between observation, planning and execution.
- Safe handling of provider authentication and credential hygiene.
- Human approval for destructive operations.
- Reproducible, auditable behavior.

**Today**

- *Working.* A closed, tested list of native commands exposed to the interface, a strict content security policy with no remote content, and a release build that cannot start Python. The application sends nothing over the network.
- *Foundation.* The Execution Safety Gate, the authorization ledger and artifact-hygiene rules.
- *Not present.* A sandbox, enforced confinement, or authenticated evidence. The documents say so wherever it matters.

[Security architecture](../architecture/security.md) separates what exists from what is still being designed.

## K. Cross-platform distribution

**Intent**

- macOS, Windows and Linux targets, with platform-native window and filesystem behavior and portable configuration.
- Installer packaging, release verification, versioning and updates.
- Accessibility, and platform-specific security guarantees stated honestly.

**Today**

- The desktop application is built and tested on macOS in CI, as an `.app` bundle. No installer is published, no release exists, and no download is offered.
- No Windows or Linux desktop build has been run. The inherited Python wrapper has a narrow Windows installer smoke test, which says nothing about the desktop application.

Intended platforms are not tested platforms. See [Cross-platform](../architecture/cross-platform.md).

## L. Open-source ecosystem

**Intent**

- Open-source development with transparent architecture and technical documentation.
- Community contributions, contribution guidelines, and an issue and pull-request workflow.
- Extensible provider integrations.
- A development roadmap, security reporting, release notes and a changelog.

**Today**

- *Working.* MIT license, a public repository, issue and pull-request templates, a `CODEOWNERS` file, a [security policy](../../SECURITY.md) with private reporting, a [changelog](../../CHANGELOG.md), and a [roadmap](roadmap.md). The inherited adapter contract is the starting point for provider extensibility.
- *Planned.* Ordelis release notes, once releases exist.

Not planned: a plugin marketplace, cloud synchronization, or other product features that have not been approved.
