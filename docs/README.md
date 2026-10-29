# Documentation

Ordelis is in early development. The desktop app is an interface preview over sample data, so most of what is documented here describes intent, foundations and contracts rather than shipped features. Each document says which parts exist. The status labels (**Working**, **Prototype**, **Foundation**, **Planned**) are defined in [Terminology](./product/terminology.md#status-labels).

Documents are grouped below by what they answer. Product and architecture documents describe the whole planned system. Implementation records, guides and the inherited MCO material describe what was built and decided, and keep the names they were written under, including Synapse.

## Product

- [Vision](./product/vision.md): why Ordelis exists, what it is not, and the intended experience.
- [Features](./product/features.md): every planned capability group and its state today.
- [Roadmap](./product/roadmap.md): the phases of remaining work, with real issues and no dates.
- [Terminology](./product/terminology.md): status labels and product vocabulary.

## Architecture

- [Overview](./architecture/overview.md): the parts, how they relate, and which document owns each topic.
- [Orchestration and planning](./architecture/orchestration.md), [agent integrations](./architecture/agents.md) and [execution](./architecture/execution.md), including terminal views.
- [Git worktree isolation](./architecture/worktrees.md), [context management](./architecture/context.md), [sessions](./architecture/sessions.md) and [review and verification](./architecture/review.md).
- [Security architecture](./architecture/security.md) and [cross-platform targets](./architecture/cross-platform.md).

## Design

- [Design principles](./design/principles.md): identity, color, typography, accessibility and motion.

## Development

- [Contributing](../CONTRIBUTING.md): how to find work, propose a change and open a pull request.
- [Contributing and development](./development/README.md): layout, checks, conventions and safety restrictions.
- [Support](../SUPPORT.md): where to get help and what to expect.
- [Code of Conduct](../CODE_OF_CONDUCT.md): expected behavior, and how to report a concern privately.
- [Release status](../RELEASING.md): Ordelis has no approved release or published installation package, and what gates one.
- [Desktop commands](../desktop/README.md#commands).
- [Security policy](../SECURITY.md): how to report a vulnerability privately.

## Desktop application

- [Desktop architecture](./implementation/desktop-architecture.md): structure of the app and its [IPC authorization](./implementation/desktop-architecture.md#ipc-authorization).
- [Development-only runtime bridge](./implementation/desktop-runtime-bridge.md): the one fixed Rust-to-Python status query, its contract and limits.
- [Local project registration](./implementation/desktop-local-projects.md): design for adding folders to Projects. The Rust backend is implemented; the Projects interface for it is not.
- [MVP plan and phase exit gates](./implementation/synapse-mvp-plan.md#implementation-phases-and-exit-gates): the original capability audit and milestone plan.
- [Visual verification](./implementation/desktop-visual-verification.md): how each screenshot was captured and what it does and does not prove.
- Page notes: [Context](./implementation/desktop-context-experience.md), [Sessions](./implementation/desktop-sessions-experience.md), [UI design inventory](./implementation/desktop-ui-design-inventory.md).

## Ordelis migration

- [Ordelis migration decision record](./migration/ordelis.md): the Synapse to Ordelis rename, decisions, identifier matrix, risks and PR roadmap. Most of it is a decision record written at the time of each decision; its [current status](./migration/ordelis.md#current-status) table is the authoritative statement of what is done.

## Safety foundations (library only, not connected to the app)

- [Execution safety gate](./implementation/execution-safety-gate.md): currently always answers no.
- [Private authorization ledger](./implementation/private-authorization-ledger.md) and [controlled probe entry point](./implementation/controlled-probe-entry-point.md).
- [Interactive probe authorization](./guides/interactive-probe-authorization.md) and [private execution plan](./guides/private-execution-plan.md).
- [Probe artifact hygiene](./guides/probe-artifact-hygiene.md): why probe-local homes and raw transcripts are never tracked, and what that does and does not protect.

## Inherited from MCO

The MCO command-line runtime and its documentation remain in the repository.

- [Workflow guide](./guides/workflows.md), [CLI reference](./reference/cli.md), [configuration](./reference/configuration.md) and [providers](./reference/providers.md).
- [Contracts](./contracts/): CLI JSON, errors, install and invocation runtime.
- [Release notes](./releases/) and [archive](./archive/README.md), which includes the [original MCO Chinese README and npm release guide](./archive/mco-v0.12.1/README.md). They describe MCO, not Ordelis.
