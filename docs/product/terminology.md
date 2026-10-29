# Terminology

**Status:** reference. Defines words used across the Ordelis documentation, including the four status labels that separate what exists from what is planned.
**Scope:** vocabulary only. It decides nothing about behavior.
**Related:** [Features](features.md) · [Architecture overview](../architecture/overview.md)

## Status labels

Every capability in these documents carries one of four labels. They are deliberately strict: a capability is only **Working** if you can use it today.

| Label | Meaning |
| --- | --- |
| **Working** | Implemented, covered by tests, and usable today. |
| **Prototype** | An interface over invented sample data. It changes nothing and starts nothing. |
| **Foundation** | Tested library or runtime code that is not connected to the desktop application. |
| **Planned** | Not implemented. Described to show intent, not to promise a delivery. |

A capability can have different labels in different places. For example, running a prompt through several agents is **Working** in the inherited command-line runtime and **Planned** in the desktop application.

## Product terms

**Agent.** A coding assistant that Ordelis coordinates, usually an installed command-line tool such as Claude Code, Codex or OpenCode. Ordelis does not provide a model of its own.

**Provider.** The vendor or tool behind an agent. The inherited runtime uses *provider* for the adapter that drives one agent CLI.

**Task.** A unit of work the user describes: a goal, constraints, and the context an agent may use.

**Plan.** A proposed breakdown of a task into stages, with an assignment of stages to agents. A plan is reviewed by the user before anything runs.

**Stage.** One step of a plan, assigned to one agent. Stages can run in sequence or in parallel, and can depend on each other.

**Orchestration.** Deciding what runs, in what order, with which agent, under which permissions. Orchestration is separate from the agents' own implementation.

**Execution.** Actually running an agent process. Execution is the part that needs the strongest controls.

**Authorization gate.** A point where nothing proceeds until the user explicitly approves. A gate is a human decision, not a configuration default.

**Worktree.** A separate Git working directory for the same repository, on its own branch. Ordelis intends to give each writing agent its own, so that parallel agents do not edit the same files.

**Integration.** Bringing an agent's reviewed changes into another branch, for example by merge or squash. In Ordelis, integration is always an explicit user action.

**Context.** The files, notes and history an agent is allowed to receive for a task. Context is selected, never assumed.

**Context package.** The set of context assigned to one task or one agent. Not a file format; the term names the idea.

**Session.** A continuing conversation or run with one agent. The inherited runtime has a *session* feature for persistent multi-turn agent sessions.

**Reviewer.** An agent, other than the one that did the work, that examines the result. Review output is evidence for the user, not a decision.

**Evidence.** Material the user can inspect to judge a result: diffs, test and build output, reviewer findings, recorded commands and exit statuses.

## Security terms

**Observation, planning, execution.** Three distinct modes. Observing reads state. Planning proposes work. Executing changes the world. The design keeps them separate, and moving from one to the next needs an explicit step.

**Fail closed.** When a required fact is missing, stale or in conflict, the answer is *no*.

**Sandbox.** An operating-system mechanism that confines a process. Ordelis does not provide one today. See [Security architecture](../architecture/security.md#what-does-not-exist-yet).

**Execution Safety Gate.** A library component that judges whether supplied evidence would be enough to consider executing a command. It currently always answers no. See [its design record](../implementation/execution-safety-gate.md).

## Origin terms

**MCO.** The open-source command-line orchestration layer from which Ordelis began. Its runtime remains in this repository under its original names. See [Origins](../migration/ordelis.md#provenance-and-attribution).

**Synapse.** The previous name of this project. Historical documents keep it.

**Sample data.** Invented content shown by the desktop prototype so that layouts can be reviewed. It is never real project, task or session data.
