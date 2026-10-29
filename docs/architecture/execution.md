# Execution

**Status:** planned for the desktop. The Execution Safety Gate exists and always answers no; the inherited command-line runtime executes agents but is not governed by Ordelis's safety design. The intended behavior is a proposal for review.
**Scope:** how an approved stage is turned into a running agent process, under which limits, and how the user supervises it. Includes the planned terminal views. Planning is in [Orchestration](orchestration.md); security properties are collected in [Security](security.md).
**Related:** [Features D and H](../product/features.md#d-orchestration-and-execution) · [Execution safety gate](../implementation/execution-safety-gate.md) · [Agents](agents.md) · [Workflow guide](../guides/workflows.md)

## Goals

- Run an approved stage with exactly the authority it was granted and no more.
- Let the user see what is happening and stop it.
- Fail in ways that are visible, bounded and recoverable.
- Keep running agents separate from deciding what to run.

## Intended behavior

**Starting.** A stage starts only when an approved plan assigns it, its dependencies are met, a worktree is ready if it writes, and the safety design has authorized the specific command against the specific executable. If any of these cannot be shown, the stage does not start.

**Lifecycle.** Ordelis starts the process, tracks it, and ends it: normally, on timeout, on cancellation, or on failure. A process that cannot be accounted for, such as one that detached children, is treated as a problem to report, not ignored.

**Execution policy and permissions.** Each stage runs under a policy derived from the plan: what the agent may read, what it may write, whether it may use the network, and how long it may take. Permissions are per agent and per stage. An agent's own permission flags are used where the agent provides them, but see [Security](security.md) for why those alone are not treated as confinement.

**Authorization gates.** Some actions need a fresh human decision while a stage is running: widening permissions, a destructive operation, anything outside what the plan listed. The stage pauses at the gate. No timeout approves a gate on the user's behalf.

**Timeouts.** Each stage has a hard deadline, and a stall limit for output that stops progressing. Reaching either ends the stage as timed out, not as failed silently.

**Cancellation and termination.** The user can cancel a stage or the whole task. Cancellation first asks the agent to stop and then terminates the process and its descendants if it does not. The result records that the stop was requested by the user.

**Errors, recovery and resume.** A failure records what failed and what state the work was left in. Recovery then depends on the situation: resume the same session where the agent supports it, retry from a clean worktree, hand over to a different agent with a summary, or stop. Ordelis does not retry writing stages automatically.

**Observability.** Everything that matters is written to a structured event history: what was authorized and by whom, commands, exit statuses, state changes, cancellations and errors. Live progress is derived from that history. The format is not decided.

**Orchestration and agent implementation are separate.** Execution knows how to run a process under limits. It does not know how an agent plans or edits code, and no agent is trusted with decisions about its own limits.

## Terminal views

Each running stage has a terminal view for its agent.

- **Shows:** the output as it happens, logs and diagnostics, each command that was run, and exit statuses.
- **Cancellation** is available from the view.
- **Interactive input** is offered only where an agent supports it and only where it can be done safely. Typing into an agent's terminal is a form of authority, so interactive terminals carry their own permission, separate from the permission to run the stage.
- **Terminal output is untrusted text.** It is displayed as text and never interpreted as markup or as instructions to Ordelis.
- **Accessibility:** the view is keyboard operable and works with assistive technology, including announcing state changes politely rather than flooding.

Whether a full pseudo-terminal is feasible within the safety constraints is an open question; a read-only log view is the minimum.

## User interactions

Start an approved plan; watch stages; open an agent's terminal view; answer authorization gates; cancel a stage or a task; inspect the event history afterwards.

## Security implications

- Execution is the point where the other protections stop being theoretical. It is the last place a mistake can still be stopped.
- The Execution Safety Gate is deliberately not an enforcement mechanism. It says whether *supplied evidence* would be enough to consider running something. A trusted coordinator that applies confinement and produces authenticated evidence is separate work, and it must not be added by putting a branch in the gate. See the gate's [design](../implementation/execution-safety-gate.md).
- Agent output, terminal text and tool results can carry prompt injection. They never become policy.
- Hostile code running as the same user is out of scope for the gate and for the current design.

## Constraints

- No execution from the desktop exists, and none is permitted until the coordinator is designed, implemented and reviewed.
- Operating-system confinement differs on macOS, Windows and Linux, and none is implemented. See [Cross-platform](cross-platform.md).
- Process-group termination is weaker on Windows than on Unix in the inherited runtime.

## Current implementation

- *Working, inherited CLI.* `mco run` and `mco review` start the selected agents, in parallel or in a chain, with per-invocation hard deadlines, a stall timeout, a whole-run deadline, cancellation through the adapter, bounded parallelism, structured exit statuses (`complete`, `partial`, `failed`) and persisted artifacts. They have no authorization gates, and `--allow-paths` checks the requested scope without creating an operating-system sandbox.
- *Foundation.* The [gate](../implementation/execution-safety-gate.md), a [probe runner](../guides/codex-zero-model-probes.md) that runs only `--version` and help commands in a scratch directory (not an OS sandbox), [terminal authorization](../guides/interactive-probe-authorization.md), and a [ledger](../implementation/private-authorization-ledger.md) that makes an authorization single-use. All have been exercised with fake executables only.
- *Prototype.* No terminal or execution view exists in the desktop.

## Open questions

- **The trusted execution coordinator:** what it is, where it runs, and what it uses to confine a process on each operating system.
- **Authenticated evidence:** how Ordelis could trust a statement about a process's confinement without relying on the process.
- **Interactive terminals:** whether and how, and what permission model separates them from a stage's other permissions.
- **Event history:** format, retention and where it is stored privately.
- **Partial work after a crash:** what Ordelis can safely promise about the state of a worktree.
