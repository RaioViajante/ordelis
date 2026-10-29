# Security architecture

**Status:** partly implemented. The desktop's IPC boundary and the fail-closed safety foundations exist. Confinement, authenticated evidence and execution controls do not. The intended properties are a proposal for review.
**Scope:** the security model across the product: what is protected, what already exists, what is still being designed, and what is explicitly not claimed. It collects properties that are specified in more detail elsewhere; it does not replace those records. To report a vulnerability, use the [security policy](../../SECURITY.md).
**Related:** [Features J](../product/features.md#j-security-and-privacy) · [Execution safety gate](../implementation/execution-safety-gate.md) · [Desktop architecture: trust boundaries](../implementation/desktop-architecture.md#tauri-and-webview-trust-boundaries) · [Execution](execution.md) · [Cross-platform](cross-platform.md)

## Principles

- **Local first.** There is no mandatory Ordelis account and no mandatory cloud backend. The desktop application sends nothing over the network, and its content security policy allows only local resources.
- **Least privilege.** Every capability is granted individually and narrowly. A new native command is a reviewed change to a manifest, a handler, a capability file and tests together.
- **Observation, planning and execution are different modes.** Reading state, proposing work and changing the world are separated, and moving between them needs an explicit step.
- **No automatic elevation of trust.** A successful earlier action, an agent's claim about itself, or a note's contents never raises anyone's permissions.
- **Fail closed.** Missing, stale, conflicting or unverifiable evidence means *no*.
- **Human approval for destructive operations** and for integration into protected branches.
- **Reproducible and auditable.** What ran, with which authority, and what the user decided can be reconstructed.
- **Credentials stay with their owners.** Provider authentication stays with each provider's own tool. Ordelis does not collect, store or log credentials.

## What exists today

| Property | State | Source |
| --- | --- | --- |
| The interface can call exactly six reviewed native commands, checked by Tauri's access control with an explicit app manifest | Implemented and tested | [IPC authorization](../implementation/desktop-architecture.md#ipc-authorization) |
| Strict content security policy: no remote scripts, fonts or images, no inline styles in production | Implemented and tested | [Trust boundaries](../implementation/desktop-architecture.md#tauri-and-webview-trust-boundaries) |
| The interface has no filesystem, shell, network or plugin permission | Implemented and tested | [Capabilities](../implementation/desktop-architecture.md#security-capabilities-and-permissions) |
| A release build cannot start Python; the one runtime query is development-only, fixed and argument-free | Implemented, verified on a real bundle in CI | [Runtime bridge](../implementation/desktop-runtime-bridge.md) |
| Project registration takes no path from the interface, refuses unsafe directories and symlinks, and stores only paths and folder identities | Implemented in the backend (macOS), not yet reachable from the interface | [Local projects](../implementation/desktop-local-projects.md#threat-model) |
| An Execution Safety Gate that fails closed and currently always denies | Implemented as a library, not connected to the app | [Gate](../implementation/execution-safety-gate.md) |
| Single-use, replay-resistant authorization for controlled probes, bound to the exact specification | Implemented as a library, tested with fake executables | [Ledger](../implementation/private-authorization-ledger.md) |
| New probe artifacts and provider homes are kept out of the repository | Implemented as ignore rules, checks and tests. Material committed earlier is handled separately, under the migration record. | [Artifact hygiene](../guides/probe-artifact-hygiene.md) |
| npm publication from this repository is blocked | Implemented, with known follow-ups | [#56](https://github.com/RaioViajante/ordelis/issues/56) |

## What does not exist yet

State this plainly wherever it matters: none of the following has been built.

- **No sandbox and no confinement.** Neither the gate nor the inherited `--allow-paths` option confines a process. They check, they do not enforce.
- **No authenticated evidence.** Evidence about a provider process is unauthenticated caller input today.
- **No trusted execution coordinator.** The component that would apply confinement, produce authenticated evidence and own an allow decision is future, separately reviewed work.
- **No equivalent guarantees across operating systems.** The implemented protections are macOS-oriented, and Windows and Linux are unverified.
- **No execution from the desktop at all.**
- **No defense against hostile code running as the same user.** The design does not claim one.

## Intended properties

These are what the finished system is meant to provide. They are goals under design, not features.

- **Permission-controlled execution** with per-agent, per-stage permissions shown to the user before they are granted. See [Execution](execution.md).
- **Context access restrictions:** an agent receives only the selected context, and file access is scoped. See [Context](context.md).
- **Defense against unintended modification:** agents write only in isolated worktrees, and integration is user-confirmed. See [Worktrees](worktrees.md).
- **Secure desktop boundaries** that stay closed as the product grows: no general shell, filesystem or process API ever reaches the interface.
- **Safe provider authentication handling:** readiness is read from the provider's own tool, and no secrets pass through Ordelis.
- **Untrusted text everywhere it matters:** notes, agent output, terminal text and file names are data. They are rendered as text and never become policy.
- **An audit trail** of authorizations and decisions. See [Review](review.md#intended-behavior).

## Trust boundaries

```text
 User ──── decisions and approvals ────────────────────────────┐
  │                                                            │
  ▼                                                            ▼
 Interface (untrusted content may reach it)        Orchestration (holds the plan)
  │  closed list of typed commands                             │
  ▼                                                            ▼
 Native layer (validates; owns paths and processes) ──► Execution (applies limits)
                                                               │
                                                               ▼
                                         Agent process (trusted only as far as its limits hold)
```

The agent process is the least trusted component that is still given real authority. Everything the other components do exists to bound it.

## Constraints

- A statement made by a process about its own confinement is never accepted as evidence.
- Ignoring a path in Git is not a privacy boundary.
- A user interface confirmation protects the user from a misclick. It is not authentication, and it does not protect a command from a caller.

## Open questions

- **How confinement is applied on each operating system,** and what evidence of it Ordelis can obtain from outside the confined process.
- **Where private state is stored,** and with what permissions, on Windows and Linux.
- **Hooks and configuration in agent-written repositories,** which can run code.
- **A threat model for the future coordinator,** which does not exist yet. The gate's [threat model](../implementation/execution-safety-gate.md#threat-model) is the starting point.
- **Signing and update integrity,** which are not configured. See [Cross-platform](cross-platform.md).
