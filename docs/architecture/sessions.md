# Sessions and continuity

**Status:** partly working. Persistent agent sessions exist in the inherited command-line runtime and a Sessions prototype exists in the desktop over sample data. Task-to-session relationships, handoffs and recovery are planned. The intended behavior is a proposal for review.
**Scope:** how Ordelis keeps track of agent sessions, relates them to tasks, and carries work across interruptions. How context is selected is in [Context](context.md).
**Related:** [Features G](../product/features.md#g-sessions-and-continuity) · [Execution](execution.md) · [Desktop sessions experience](../implementation/desktop-sessions-experience.md) · [CLI reference](../reference/cli.md)

## Goals

- Find, inspect and resume earlier agent work.
- Know which task and stage a session belongs to.
- Survive crashes and restarts without losing the thread of the work.
- Never pretend that continuity exists where a provider cannot supply it.

## Intended behavior

**Discovery and management.** Ordelis lists sessions it started, with their agent, status and history. It manages their lifecycle: start, stop, and clean up.

**Task relationship.** A session belongs to a stage of a task, and a task shows its sessions. This lets the user move from a result to the conversation that produced it, and back.

**History.** The conversation and execution history of a session is available to read. It is private state, stored outside the repository.

**Status.** A session is active, idle, finished, failed, stopped or interrupted, and the interface says which. Names are not decided.

**Resume and continuation.** Resuming is offered when the agent's own tool supports it. It is not emulated. When a provider cannot resume, the user is told, and the alternative is a handoff.

**Summaries and handoffs.** When work moves to another session or agent, Ordelis can carry a summary and the relevant context across. A handoff is visible and reviewable, because a summary is a lossy copy and the user should be able to see what was lost.

**Session history as context.** The user may choose to include earlier session material in a new task's [context](context.md). It follows the same selection and provenance rules as any other source.

**Recovery from interruptions.** After a crash, a restart or a lost connection, Ordelis reconciles what it recorded with what is actually running, shows the result, and offers the safe options. It does not assume an interrupted writing stage completed.

**Persistent task metadata.** Task, plan, stage and decision records persist across restarts so that continuity does not depend on any agent's memory.

## User interactions

Browse and filter sessions; open a session's history; resume or stop one; follow the link between a task, its stages and their sessions; review a handoff before it is used; inspect what was recovered after an interruption.

## Security implications

- Session history can contain anything an agent saw or printed, including secrets the user pasted. It is stored privately and treated as sensitive.
- A resumed session carries its earlier permissions only if the user re-grants them. Resuming does not restore authority silently.
- Summaries written by an agent are untrusted text when they are fed to another agent.
- Cleanup of session data is a destructive operation and is confirmed.

## Constraints

- Sessions are provider sessions. They are not a shared memory across providers, and Ordelis does not claim they are.
- What can be resumed, and how faithfully, depends on the provider and its version.
- Local storage locations and the inter-process mechanism differ by operating system.

## Current implementation

- *Working, inherited CLI.* `mco session` starts, lists, sends to, broadcasts to, stops, resumes, cancels and shows the history and queue of persistent sessions, backed by a local daemon. It records history and logs. By default it writes them inside the repository's `.mco/sessions`, which the [MVP plan](../implementation/synapse-mvp-plan.md#inherited-capability-audit) flags as unsuitable for private state. The daemon uses a Unix-domain socket and its behavior on Windows is unverified. A known race when a session starts or resumes is tracked in [#23](https://github.com/RaioViajante/ordelis/issues/23).
- *Prototype.* The Sessions page, a session detail view and a continuity panel, all over invented data. They resume nothing.
- *Planned.* Everything that relates sessions to tasks, handoffs, summaries, recovery and persistent task metadata.

## Open questions

- **Where session data and task metadata live,** and in what form, for each operating system.
- **How a handoff summary is produced and checked** so that loss is visible.
- **Retention:** how long history is kept and how the user clears it.
- **How reliable resumption is** across provider versions, which only real provider validation can establish.
