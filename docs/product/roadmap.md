# Roadmap

**Status:** planning document. The phases are categories of work, not releases. There are no delivery dates and no committed version numbers, and the order can change.
**Scope:** how the remaining work is grouped and what it depends on. It is derived from the repository's merged pull requests, the issue tracker, and the [migration decision record](../migration/ordelis.md#pr-roadmap), which remains the authority on the rename and branding sequence.
**Related:** [Features](features.md) · [Vision](vision.md) · [Milestone plan from the original audit](../implementation/synapse-mvp-plan.md#implementation-phases-and-exit-gates)

Where a phase has no issue yet, none is linked. Issues are created when a phase is about to start.

## Phases

### 1. Existing safety and desktop foundations

**Largely done.** What exists is a tested base, not a product.

- The MCO capability audit and MVP plan: [#1](https://github.com/RaioViajante/ordelis/issues/1).
- Private execution plans, runtime-validation foundations, Codex capability inspection and zero-model-call probes: [#3](https://github.com/RaioViajante/ordelis/issues/3), [#5](https://github.com/RaioViajante/ordelis/issues/5), [#7](https://github.com/RaioViajante/ordelis/issues/7), [#11](https://github.com/RaioViajante/ordelis/issues/11), [#13](https://github.com/RaioViajante/ordelis/issues/13), [#15](https://github.com/RaioViajante/ordelis/issues/15).
- The authorization ledger and the Execution Safety Gate: [#17](https://github.com/RaioViajante/ordelis/issues/17), [#19](https://github.com/RaioViajante/ordelis/issues/19).
- The desktop UI foundation and its Home, Projects, Context and Sessions prototypes: [#24](https://github.com/RaioViajante/ordelis/issues/24), [#26](https://github.com/RaioViajante/ordelis/issues/26), [#28](https://github.com/RaioViajante/ordelis/issues/28), [#30](https://github.com/RaioViajante/ordelis/issues/30).
- The native IPC boundary and the development-only runtime bridge: [#32](https://github.com/RaioViajante/ordelis/issues/32), [#36](https://github.com/RaioViajante/ordelis/issues/36), [#40](https://github.com/RaioViajante/ordelis/issues/40), [#42](https://github.com/RaioViajante/ordelis/issues/42).
- Local project registration, backend only: the design ([#45](https://github.com/RaioViajante/ordelis/issues/45)) and the Rust registry with its four authorized commands ([#47](https://github.com/RaioViajante/ordelis/issues/47), implemented by [PR #48](https://github.com/RaioViajante/ordelis/pull/48)). The issue was kept open for review after that PR merged, so its state does not describe the implementation. The Projects interface that would call the backend is separate work with no issue of its own yet, and nothing in the desktop calls the commands.
- Repository hygiene: [#49](https://github.com/RaioViajante/ordelis/issues/49) (probe artifacts), [#53](https://github.com/RaioViajante/ordelis/issues/53) (npm publication blocked), [#55](https://github.com/RaioViajante/ordelis/issues/55) (ownership and security reporting).

### 2. Ordelis branding and repository migration

**In progress.** The repository has been renamed to `RaioViajante/ordelis`, and the decision record, release hardening and ownership corrections are merged. Remaining work follows the [migration PR roadmap](../migration/ordelis.md#pr-roadmap): post-rename security documentation ([#58](https://github.com/RaioViajante/ordelis/issues/58)), user-facing renaming, atomic renaming of internal identifiers, and the bundle identifier with its project-registry import. Inherited MCO names stay unless a separately reviewed change says otherwise.

### 3. Desktop UI redesign

**Not started.** A design system with light and dark themes, then the screens rebuilt as clearly labelled sample-data prototypes. It waits on the approved vector master of the Ordelis symbol and on the migration record's private-content gate, so the existing interface keeps its previous styling for now. No screen in this phase connects to execution. See [Design principles](../design/principles.md).

### 4. Provider discovery and capability integration

**Not started in the desktop.** The inherited CLI already detects installed agents and checks readiness, and the Codex inspection and probe foundations exist. This phase connects real discovery to the Agents page, and records what each provider actually supports, one provider at a time, starting with read-only checks. The Kimi decision gate is part of it. See [Agents](../architecture/agents.md).

### 5. Task planning and approvals

**Not started.** Turning a task into a reviewable plan, and the approve, change and reject interactions. Depends on phase 4 for knowing which agents can be assigned. See [Orchestration](../architecture/orchestration.md).

### 6. Safe agent execution

**Not started, and deliberately gated.** The safety gate answers no today and no enforcement mechanism exists. This phase needs a separately designed and reviewed trusted execution coordinator, enforced confinement, and authenticated evidence before any agent runs from the desktop. It is the phase with the strictest review requirements. See [Execution](../architecture/execution.md) and [Security](../architecture/security.md).

### 7. Git worktree orchestration

**Not started.** Per-agent worktrees, diffs, conflict detection and cleanup. The original audit placed this after provider validation and requires tests for isolation, ownership boundaries, conflict handling and cleanup before parallel writers. See [Worktrees](../architecture/worktrees.md).

### 8. Independent review and integration

**Not started.** Reviewer agents, evidence reports, the request-changes, reject and approve decisions, and user-controlled integration with an audit trail. Depends on phases 6 and 7. See [Review](../architecture/review.md).

### 9. Context and session continuity

**Partly foundation.** Vault preview and a non-dispatching plan exist, as do inherited sessions. Context packages, provenance in the interface, session handoffs and recovery are planned. Related issues: [#23](https://github.com/RaioViajante/ordelis/issues/23) (session readiness race) and [#39](https://github.com/RaioViajante/ordelis/issues/39) (runtime import hygiene). See [Context](../architecture/context.md) and [Sessions](../architecture/sessions.md).

### 10. Cross-platform verification and distribution

**Not started.** The first step is to build and test the desktop on Windows and Linux and replace the migration record's source-based expectations with results. Then project registration for those systems (which needs its own reviewed design), packaging, signing and release metadata. Related issues: [#38](https://github.com/RaioViajante/ordelis/issues/38) (release packaging assurance) and [#56](https://github.com/RaioViajante/ordelis/issues/56) (release hardening). See [Cross-platform](../architecture/cross-platform.md).

## What gates a first release

A first release, or any statement that Ordelis is ready, is blocked by more than feature work:

- A security assessment of the repository's history, tracked privately and not described here. See the [migration record](../migration/ordelis.md#security-exposure-probe-artifacts-in-public-git-history-sec1).
- The release-hardening follow-ups in [#56](https://github.com/RaioViajante/ordelis/issues/56), including replacing the deliberately failing npm publication guard with a separately reviewed release workflow.
- Release packaging assurance, [#38](https://github.com/RaioViajante/ordelis/issues/38).
- The approved vector master for final application icons.
- Verification of every platform before it is described as supported.

The `ordelis.app` domain is planned for a website that has not been deployed, and no page links to it as a live site.

## Finding current issues

Issues are cited above as context for a phase. This document does not state whether any of them is open or closed, because that changes faster than the document does, and an issue's state is not evidence of implementation state. For the current list, query GitHub directly:

- [Open issues](https://github.com/RaioViajante/ordelis/issues?q=is%3Aissue+is%3Aopen) in the web interface.
- `gh issue list --repo RaioViajante/ordelis --state open` from a terminal.
