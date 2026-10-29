# Orchestration and planning

**Status:** planned. The intended behavior below is a proposal for review, not a decided design. A non-dispatching plan preview and the inherited command-line coordination features exist; the user-facing planning and approval flow does not.
**Scope:** how a task becomes a reviewed plan, how stages are assigned and ordered, and how the plan's state is tracked. Running the stages is specified in [Execution](execution.md). This document does not define data structures or protocols, because none have been decided.
**Related:** [Features C and D](../product/features.md#c-planning-and-task-delegation) · [Agents](agents.md) · [Worktrees](worktrees.md) · [Review](review.md)

## Goals

- Turn a natural-language task into a plan the user can read, question, change and approve before anything runs.
- Make the assignment of work to agents explicit, with a stated reason.
- Support work that must happen in order and work that can happen at the same time.
- Keep the plan, not the agents' own memory, as the record of what was intended.

## Intended behavior

**Task.** The user states a goal and constraints in plain language, and selects the [context](context.md) the task may use. The constraints are part of the task, not a footnote: they travel with every stage derived from it.

**Plan.** Ordelis proposes a decomposition into stages. For each stage the plan states what it should achieve, which agent should do it and why, which context it receives, which permissions it needs, and what it depends on. Because stages carry their own permissions, the plan is also where the user sees what authority would be requested before granting any of it.

**Ordering.** A stage may depend on others. Stages without dependencies between them can run in parallel; the others wait. Stages that write to the repository need [isolated worktrees](worktrees.md), which constrains how aggressively parallelism can be offered.

**Roles.** An agent can be given a role for a stage, such as implementer or reviewer. Role boundaries matter most for review: the agent that reviews a change should not be the one that wrote it (see [Review](review.md)).

**Approval.** Nothing in a plan runs until the user approves it. The user can approve the plan, change it, or reject it. Changing a plan after approval invalidates the approval for any stage that has not started, and the user is asked again.

**Replanning.** Feedback from the user, a failed stage or a reviewer's findings can lead to a revised plan. A revised plan is a new proposal and goes through approval again. It does not silently replace the plan that was approved.

**State.** At any time the user can see which stages are waiting, running, blocked on an authorization, finished, failed or cancelled, and why. Names for these states are not decided.

**Failure and retry.** A failed stage does not silently fail the task. The plan records the failure and offers the user options: retry, reassign, replan or stop. Retrying a stage that writes needs rules about repeatable effects, so retries are not automatic for writers.

## User interactions

Describe a task; select context; read the proposed plan; approve, edit or reject it; follow stage state; respond to authorization requests as they arise; give feedback that triggers replanning; stop the task.

## Architectural responsibilities

| Responsibility | Owner |
| --- | --- |
| Holding the task, plan and its approval | Orchestration |
| Deciding which agent can take a stage | Orchestration, using the capability knowledge in [Agents](agents.md) |
| Starting and stopping processes | [Execution](execution.md), never orchestration directly |
| Implementing a stage | The agent |

Orchestration decides *what* and *when*. It does not run processes, and it does not reach into an agent's internals.

## Security implications

- A plan is a request for authority. Showing the permissions a stage needs, before approval, is a security property and not a convenience.
- Text from the task, from context, and from agent output is untrusted input to planning. A note or an agent's answer must not be able to add a stage or widen a permission.
- Approval is bound to what the user saw. Anything that changes the plan afterwards needs a new approval.

## Constraints

- No plan runs without approval. There is no unattended mode in this design.
- Orchestration cannot raise an agent's permissions on its own.
- The plan must be reconstructible afterwards, so decisions and changes are recorded (see [Review](review.md#intended-behavior)).

## Current implementation

- *Foundation.* [`mco context plan`](../guides/private-execution-plan.md) builds one deterministic, non-dispatching plan from a task, selected context and policy. It marks settings it cannot verify as unknown and blocks on required unknowns. It has one step, not stages.
- *Working, inherited CLI.* Coordination by explicit user choice: `--chain` runs agents in sequence passing complete earlier answers through files; `--divide` splits scope between agents; `--debate` and `--synthesize` add read-only stages. These arrange prompts and scope. They do not plan, and they have no approval step. See the [workflow guide](../guides/workflows.md).
- *Prototype.* A task composer and a task preview in the desktop, over sample data.
- *Not present.* Plan generation, decomposition, dependency handling, replanning, approval.

## Open questions

- **Who proposes the plan.** A coordinating agent, a deterministic procedure, or the user with assistance. The [MVP plan](../implementation/synapse-mvp-plan.md) lists central-agent coordination as future work and does not choose.
- **How much of a plan the user must read** before approving a long one, without approval turning into a reflex.
- **Granularity of approval:** the whole plan, each stage, or each permission.
- **How retries interact with writing stages** and with the idempotency they would need.
