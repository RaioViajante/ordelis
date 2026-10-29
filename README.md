<div align="center">

# ordelis

**Your agents. In order.**

A local-first, open-source workspace for orchestrating AI coding agents.

<sub>Early development — not ready for production use.</sub>

</div>

## Overview

Ordelis is a desktop workspace for the work that surrounds coding agents. It is not a chatbot, not an AI model, and not a replacement for your editor. Agents such as Claude Code, Codex and OpenCode already write code. What is missing is a place that keeps their tasks, context, branches, reviews and approvals in order.

Today that work is scattered: a terminal per agent, sessions you cannot find again, context copied in by hand, and several agents sharing one working directory and one Git branch. Ordelis is meant to bring it into a single local workspace, where you decide what each agent may see and do, and where nothing reaches your main branch without your say-so.

## The vision

> This describes the intended experience. It is **not implemented**. See [current status](#current-status) for what exists.

1. **Describe** a task, with its goal and constraints.
2. **Choose** the context agents are allowed to see.
3. **Review** a proposed plan and its stages. Approve it, change it, or reject it.
4. **Assign** stages to agents, each working in its own isolated Git worktree.
5. **Follow** execution as it happens, and cancel it at any point.
6. **Inspect** the evidence: diffs, test results, and findings from an independent reviewing agent.
7. **Decide.** Request changes, reject, or approve. Integration is always your explicit action.

Everything is meant to run on your machine, with no Ordelis account and no Ordelis cloud service.

## Planned capabilities

- **Workspace and projects.** One desktop workspace for local projects, tasks, history and search. [Details](docs/product/features.md#a-workspace-and-projects)
- **Agent integrations.** Discover installed agent CLIs and orchestrate them behind provider-independent contracts, without assuming every agent can do the same things. [Details](docs/product/features.md#b-agent-integrations)
- **Planning and delegation.** Turn a task into a reviewable plan of sequential and parallel stages. [Details](docs/product/features.md#c-planning-and-task-delegation)
- **Orchestration and execution.** Controlled execution with permissions, human authorization gates, timeouts, cancellation, recovery and an event history, plus a terminal view per agent. [Details](docs/product/features.md#d-orchestration-and-execution)
- **Git and worktree isolation.** Parallel work in separate worktrees, per-agent diffs, conflict detection, and user-controlled integration. [Details](docs/product/features.md#e-git-and-worktree-isolation)
- **Context and sessions.** Explicit, inspectable context per task or agent, optional Obsidian notes you select, and continuity across sessions. [Details](docs/product/features.md#f-context-management)
- **Review and verification.** Independent reviewers, evidence-based completion reports, and an audit trail of your decisions. [Details](docs/product/features.md#i-review-and-verification)
- **Security and privacy.** Local-first, least privilege, no automatic trust elevation, human approval for destructive operations. [Details](docs/architecture/security.md)
- **Cross-platform and open source.** macOS, Windows and Linux are the intended targets; none beyond macOS has been verified. [Details](docs/architecture/cross-platform.md)

The complete inventory, with intended behavior and current state for every capability, is in [Features](docs/product/features.md).

## Current status

Ordelis is in early development. There are no releases, no installers and no downloads. The desktop application has not yet run a coding agent.

**Working**

- A desktop shell (Tauri 2, React, TypeScript) with navigation, and a native permission boundary that exposes a closed list of commands to the interface. It is built and tested in CI on macOS.
- The inherited [MCO](https://github.com/mco-org/mco) command-line runtime, which can dispatch a prompt to agent CLIs installed on your machine, run reviews, and manage persistent sessions from a terminal. It is not connected to the desktop app and is not governed by Ordelis's own safety design. See the [CLI reference](docs/reference/cli.md).

**Prototype, over sample data**

- Home with a task composer, Projects, Context, Sessions, Agents and Settings. Every project, task, note and session shown is invented, and actions such as "Preview task" change nothing and start nothing.

**Foundations, tested but not connected to the app**

- A local project registry in Rust (macOS only; no interface calls it yet).
- A read-only preview of selected Obsidian vault context, and a non-dispatching plan, both on the command line.
- An Execution Safety Gate that currently always answers *no*, and a private authorization ledger for controlled probes, exercised only with fake executables. These create no sandbox.

**Planned**

- Everything else in the vision above: running agents from the desktop, plan review and approval, worktree isolation, terminals, independent review, Windows and Linux support, and distribution.

macOS is the only platform on which the desktop app has been built and launched. See the [roadmap](docs/product/roadmap.md) for how the remaining work is ordered, and the [per-capability status](docs/product/features.md#implementation-status-at-a-glance) for detail.

## Documentation

The [documentation index](docs/README.md) lists everything, including the historical records.

- **Product.** [Vision](docs/product/vision.md) · [Features](docs/product/features.md) · [Roadmap](docs/product/roadmap.md) · [Terminology](docs/product/terminology.md)
- **Architecture.** [Overview](docs/architecture/overview.md) · [Orchestration](docs/architecture/orchestration.md) · [Agents](docs/architecture/agents.md) · [Execution](docs/architecture/execution.md) · [Worktrees](docs/architecture/worktrees.md) · [Context](docs/architecture/context.md) · [Sessions](docs/architecture/sessions.md) · [Review](docs/architecture/review.md)
- **Security.** [Security architecture](docs/architecture/security.md) · [Execution safety gate](docs/implementation/execution-safety-gate.md) · [Security policy](SECURITY.md)
- **Development.** [Contributing and development](docs/development/README.md) · [Desktop commands](desktop/README.md#commands)
- **Design.** [Design principles](docs/design/principles.md)

## Development

These are the commands the repository's CI runs. They assume a checkout; there is no published Ordelis package to install.

```sh
# Python runtime tests (Python 3.10+)
python3 -m unittest discover -s tests -p 'test_*.py'

# Desktop frontend (Node 22+, pnpm)
cd desktop
pnpm install --frozen-lockfile
pnpm lint && pnpm typecheck && pnpm test

# Desktop native tests (stable Rust toolchain), still from desktop/
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

[Contributing and development](docs/development/README.md) covers the repository layout, the inherited MCO package, the execution safety restrictions and how to report a security issue privately. Please report vulnerabilities through [SECURITY.md](SECURITY.md), not in public issues.

## Origins and license

Ordelis was previously named Synapse and began from [MCO](https://github.com/mco-org/mco), a CLI orchestration layer for AI coding agents by the mco maintainers, starting from its `v0.12.1` release. It is evolving into a separate product with its own desktop application. The inherited runtime, its documentation and its Git history remain in this repository, and several inherited names (the `mco` command, the `runtime` Python package) are intentionally unchanged. Thanks to the MCO authors; see the commit history for individual contributors. The provenance record is in the [migration decision record](docs/migration/ordelis.md#provenance-and-attribution).

Released under the MIT License. See [LICENSE](LICENSE), which carries the original copyright notice (Copyright (c) 2026 mco maintainers).
