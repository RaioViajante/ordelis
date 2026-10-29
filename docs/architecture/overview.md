# Architecture overview

**Status:** the *current structure* section is verified against the source. The *intended structure* is a direction and has open decisions, listed at the end.
**Scope:** the parts of Ordelis, how they relate, and which document is authoritative for each topic. Individual topics are specified in their own documents.
**Related:** [Vision](../product/vision.md) · [Features](../product/features.md) · [Desktop architecture](../implementation/desktop-architecture.md) · [Terminology](../product/terminology.md)

## Current structure

```text
   You
    │
    ▼
 ┌─────────────────────────── desktop/ ────────────────────────────┐
 │  Interface (React, TypeScript, in a webview)                     │
 │    Home · Projects · Sessions · Context · Agents · Settings      │
 │    mostly sample data; no network, files or processes            │
 │             │  a closed list of six reviewed native commands     │
 │             ▼                                                    │
 │  Native layer (Rust, Tauri 2)                                    │
 │    project registry (macOS) · IPC boundary · dev-only status     │
 └──────────────────────────────┬───────────────────────────────────┘
                                │ development builds only, one fixed question
                                ▼
 ┌─────────────────────────── runtime/ ────────────────────────────┐
 │  Python runtime                                                  │
 │    inherited MCO: adapters · invocation runtime · sessions · MCP │
 │    added since the fork: vault context · plans · safety gate     │
 └──────────────────────────────┬───────────────────────────────────┘
                                │ only when you run the CLI yourself
                                ▼
                    Agent CLIs installed on your machine
```

The desktop application and the Python runtime are **not integrated**. The one link between them exists in development builds only and asks the runtime to identify itself. Release builds cannot start Python. Running agents is something only the inherited command-line tool does today, outside the desktop application.

| Part | Where | What it is |
| --- | --- | --- |
| Interface | `desktop/src/` | A React and TypeScript prototype. Most pages show invented sample data. Agents runs no check, and Settings → Runtime shows the development-only identity query. |
| Native layer | `desktop/src-tauri/` | Rust code with an explicit permission manifest. It exposes six commands to the interface, one of them development-only. |
| Python runtime | `runtime/` | The inherited MCO runtime, plus context, planning and safety components added since. |
| Specifications | `docs/` | Contracts, guides, design records and, now, the product and architecture documents. |

## Intended structure

The intended shape keeps the same three layers and connects them deliberately:

1. **The interface never reaches the operating system directly.** It calls a short, closed list of typed native commands, each individually declared and tested.
2. **The native layer owns validation, path checks and process lifetime.** It speaks to the runtime over a narrow, versioned, local-only contract.
3. **The runtime coordinates agents,** but only after the safety design has authorized an execution. Orchestration and agent implementation stay separate: Ordelis decides what runs and under which limits, and each agent remains responsible for how it works.
4. **State written by the runtime lives in an explicit, private location,** not in the repository checkout or in browser storage.

This is the boundary proposed in the [desktop architecture](../implementation/desktop-architecture.md#future-rust-to-python-integration-boundaries-not-implemented). It is a proposal that has not been built.

## Where each topic is specified

| Topic | Authoritative document | Status |
| --- | --- | --- |
| Planning and delegation | [Orchestration](orchestration.md) | Planned |
| Agent integrations and capabilities | [Agents](agents.md) | Partly inherited |
| Running agents, permissions, terminals | [Execution](execution.md) | Planned; gate exists |
| Git worktrees and integration | [Worktrees](worktrees.md) | Planned |
| Context selection and provenance | [Context](context.md) | Partly working in the CLI |
| Sessions and continuity | [Sessions](sessions.md) | Partly working in the CLI |
| Review and verification | [Review](review.md) | Planned |
| Security model | [Security](security.md) | Partly implemented |
| Platforms and distribution | [Cross-platform](cross-platform.md) | macOS only |
| Desktop IPC and trust boundaries | [Desktop architecture](../implementation/desktop-architecture.md) | Implemented |
| Local project registration | [Desktop local projects](../implementation/desktop-local-projects.md) | Backend implemented |
| Runtime status bridge | [Desktop runtime bridge](../implementation/desktop-runtime-bridge.md) | Implemented, development-only |
| Execution Safety Gate | [Execution safety gate](../implementation/execution-safety-gate.md) | Implemented, always denies |
| Rename and provenance | [Migration record](../migration/ordelis.md) | Decision record |
| Inherited MCO interfaces | [Contracts](../contracts/) and [reference](../reference/cli.md) | Inherited |

When a topic has an existing design record, the new document links to it and adds only what is missing, rather than restating it.

## Open decisions

- **How the native layer reaches the runtime:** a bundled Python sidecar, or an installed `mco`. The desktop architecture document leaves this open and so does this one.
- **Whether orchestration stays in Python** or moves toward the native layer as it grows. Nothing has been decided.
- **Where private runtime state lives** on each operating system. macOS is documented for the project registry only.
