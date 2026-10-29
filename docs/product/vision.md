# Vision

**Status:** direction. This describes the intended product. Almost none of it is implemented; [Features](features.md#implementation-status-at-a-glance) says what is.
**Scope:** why Ordelis exists, what it is and is not, and the experience it aims for. Mechanisms live in the [architecture documents](../architecture/overview.md).
**Related:** [Roadmap](roadmap.md) · [Design principles](../design/principles.md) · [Terminology](terminology.md)

## The problem

Coding agents have become capable on their own. Using several of them well has not become easier. A developer who works with more than one agent typically ends up with:

- a terminal per agent, and no shared view of what each is doing;
- sessions that cannot be found, compared or resumed later;
- context pasted in by hand, with no record of what an agent was actually shown;
- several agents editing one working directory on one branch, so their changes collide;
- results that are hard to verify, and a merge that happens because it was convenient rather than because it was reviewed.

None of these is a model problem. They are coordination problems, and they sit around the agents rather than inside them.

## What Ordelis is

Ordelis is a local-first desktop workspace that coordinates coding agents: their tasks, execution environments, context, reviews and approvals. It is the layer between the developer and the agents they already use.

It is meant to let a developer **describe a task, coordinate multiple coding agents, supervise execution, review evidence and control integration** from one place on their own machine.

## What Ordelis is not

- **Not a chatbot or a model.** It ships no model and answers no questions itself.
- **Not an editor or IDE.** It does not replace where you read and write code.
- **Not an autonomous system.** Agents act under permissions the user grants, and consequential steps wait for the user.
- **Not a hosted service.** There is no mandatory account and no mandatory cloud backend.
- **Not a marketplace.** No plugin marketplace, cloud synchronization or similar product has been approved, and none is planned here.

## The intended experience

1. **Describe.** The user writes a task in natural language and states its goal and constraints.
2. **Choose context.** The user selects what agents may see: project files, documentation, and optionally notes from an Obsidian vault. Nothing is ingested wholesale.
3. **Review a plan.** Ordelis proposes how to decompose the task into stages, which agent should take each, and which stages can run in parallel. The user approves, edits or rejects the plan. Feedback leads to a revised plan.
4. **Delegate.** Each writing stage runs in its own Git worktree, so parallel agents cannot overwrite one another or the user's working directory.
5. **Supervise.** The user watches execution live, sees each command and exit status, and can cancel at any point. Actions beyond what was authorized wait for an explicit decision.
6. **Verify.** A separate reviewing agent examines the result. The user sees the diff, test and build output, and reviewer findings with severity, together in an evidence-based report.
7. **Decide.** The user requests changes, rejects, or approves. Integration into another branch, by merge or squash, happens only when the user confirms it. The decisions are recorded.

## Principles the product is held to

- **The user holds the decisions.** Plans, permissions and integration are approved by a person. No agent merges into the main branch on its own authority.
- **Local first.** Projects, context, history and credentials stay on the user's machine. Provider authentication stays with each provider's own tool.
- **Explicit context.** What an agent receives is chosen, visible, and traceable afterwards.
- **Least privilege.** Every agent gets the narrowest permissions the stage needs, and trust is never raised automatically.
- **Honest state.** The interface shows what is real and what is sample or planned. Demonstration data is never presented as live execution.
- **Providers differ.** Ordelis does not assume every agent offers the same features. A feature a provider lacks is reported as unsupported, not emulated silently.
- **Auditable.** What ran, with what authority, and what the user decided can be reconstructed.

## Current position

The vision is far ahead of the implementation. The repository today contains a desktop interface prototype that is mostly sample data, several tested safety and context foundations, and the inherited MCO command-line runtime. Running agents from the desktop is not implemented, and the safety design that must precede it is still being built. The [roadmap](roadmap.md) orders the remaining work without dates.

## Open questions

- Which agents ship first. A candidate set is Claude Code, Codex and OpenCode; Kimi has no adapter and a [decision gate](../implementation/synapse-mvp-plan.md#kimi-integration-decision-gate) is open.
- Whether a central coordinating agent is part of the first execution milestone or a later one. The [MVP plan](../implementation/synapse-mvp-plan.md) treats it as future work.
