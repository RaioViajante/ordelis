# Documentation

Synapse is in early development. The desktop app is an interface preview over sample data, so most of what is documented here describes foundations and contracts rather than shipped features.

## Synapse desktop

- [Desktop architecture](./implementation/desktop-architecture.md): structure of the app and its [IPC authorization](./implementation/desktop-architecture.md#ipc-authorization).
- [MVP plan and phase exit gates](./implementation/synapse-mvp-plan.md#implementation-phases-and-exit-gates): the roadmap.
- [Visual verification](./implementation/desktop-visual-verification.md): how each screenshot was captured and what it does and does not prove.
- Page notes: [Context](./implementation/desktop-context-experience.md), [Sessions](./implementation/desktop-sessions-experience.md), [UI design inventory](./implementation/desktop-ui-design-inventory.md).

## Safety foundations (library only, not connected to the app)

- [Execution safety gate](./implementation/execution-safety-gate.md): currently always answers no.
- [Private authorization ledger](./implementation/private-authorization-ledger.md) and [controlled probe entry point](./implementation/controlled-probe-entry-point.md).
- [Interactive probe authorization](./guides/interactive-probe-authorization.md) and [private execution plan](./guides/private-execution-plan.md).

## Inherited from MCO

The MCO command-line runtime and its documentation remain in the repository.

- [Workflow guide](./guides/workflows.md), [CLI reference](./reference/cli.md), [configuration](./reference/configuration.md) and [providers](./reference/providers.md).
- [Contracts](./contracts/): CLI JSON, errors, install and invocation runtime.
- [Release notes](./releases/) and [archive](./archive/README.md).
