# Agent integrations

**Status:** partly inherited. The command-line runtime has working adapters; the desktop has none, and Ordelis-specific capability knowledge is still being built. The intended behavior is a proposal for review.
**Scope:** how Ordelis finds, describes and talks to coding agents, and how it handles the fact that they differ. How an agent is run is specified in [Execution](execution.md).
**Related:** [Features B](../product/features.md#b-agent-integrations) · [Provider reference](../reference/providers.md) · [Runtime validation foundations](../guides/runtime-validation-foundations.md) · [MVP plan: capability audit](../implementation/synapse-mvp-plan.md#inherited-capability-audit)

## Goals

- Work with several agents, among them Claude Code, OpenAI Codex, OpenCode and Kimi, and with other compatible command-line agents, behind one orchestration contract.
- Know what each agent can actually do on this machine before relying on it.
- Never need the user's provider credentials.
- Treat a missing feature as a fact to report, not a gap to paper over.

## Intended behavior

**Discovery.** Ordelis looks for supported agent CLIs on the machine and reports which are installed and which version each one is. It does not install agents.

**Authentication state.** Ordelis can tell whether an agent appears ready to use, to the extent the agent's own tool reports it. Authentication stays entirely with the agent. Ordelis does not ask for, store or forward credentials, and it does not log them.

**Capability inspection.** For each agent, Ordelis records what it can do: which permission modes it supports, whether it can select a model or a reasoning level, whether output is structured, whether it can be cancelled cleanly, whether sessions can be resumed. The record says how each fact is known, using the four states from the [Execution Safety Gate](../implementation/execution-safety-gate.md#capability-knowledge-four-states):

| State | Meaning |
| --- | --- |
| Advertised | Someone says the control exists, for example in help text or an adapter declaration. |
| Observed | Ordelis watched a run and saw the behavior. |
| Independently enforced | Ordelis applied a mechanism the agent cannot disable and has evidence it is in force. |
| Unknown | No applicable, current evidence. |

Only the stronger states may satisfy a requirement that depends on them. Anything unknown blocks.

**Model and reasoning configuration.** Where an agent supports choosing a model or reasoning level, the user can choose one, and the plan shows the request. Whether the choice was applied is verified rather than assumed.

**Availability and environment.** Each agent lists what it needs: the executable, a minimum version, an operating system, and any account or entitlement the user must hold. A stage cannot be assigned to an agent whose needs are unmet.

**Unsupported features.** If a plan needs something an agent cannot provide, such as a read-only mode it cannot enforce, Ordelis rejects the assignment and says why. It does not fall back to a weaker behavior silently.

## User interactions

See which agents are installed, ready and capable of what; see why an agent is unavailable; choose agents and models for a stage; see exactly what was requested of an agent and what was verified.

## Architectural responsibilities

| Responsibility | Owner |
| --- | --- |
| Describing one agent's invocation, permissions and output | A provider adapter |
| Holding capability knowledge and its evidence | Capability inspection |
| Choosing an agent for a stage | [Orchestration](orchestration.md) |
| Running the agent | [Execution](execution.md) |

The adapter boundary is what keeps orchestration independent of any one agent. A new agent should need a new adapter and nothing else.

## Security implications

- An adapter's declaration of a capability is a claim by Ordelis's own code, not evidence about the user's installed version. It does not establish authentication, entitlement or model availability.
- An agent's statements about its own sandbox or permissions are not evidence of confinement.
- Replacing the binary behind a path changes what a past inspection means, so inspection results are tied to the exact executable they were taken from.
- Inspection that runs the agent is itself an execution. It is limited to commands that make no model call, and each is individually authorized (see the [probe guides](../guides/codex-zero-model-probes.md)).

## Constraints

- Providers do not expose the same capabilities. No common feature set is assumed.
- Ordelis cannot infer a paid or entitled provider from installed binaries and does not route by model name alone.
- The model catalog the inherited runtime builds is best effort. An incomplete catalog is not proof that a model is unavailable.

## Current implementation

- *Working, inherited CLI.* Adapters for Claude Code, Codex CLI, OpenCode and other agents, a configurable custom-agent path including an ACP transport, `mco doctor` for installation and authentication readiness, and `mco agent list`, `check` and `models`. The permission modes `read_only`, `write` and `yolo` are mapped to each provider's own controls, and strict mode rejects a policy the adapter cannot express. See [provider permissions](../contracts/provider-permissions-v0.1.x.md).
- *Foundation.* A closed evidence schema and evaluator that keep unverified requirements blocking ([guide](../guides/runtime-validation-foundations.md)); explicit Codex version and help inspection ([guide](../guides/codex-cli-capability-inspection.md)); a probe runner exercised only with fake executables.
- *Gaps recorded in the audit.* The Claude and OpenCode adapters currently ignore an invocation's model; the model catalog probes only some providers; there is no common reasoning-level control; there is no Kimi adapter.
- *Prototype.* The Agents page, which shows every fact as unknown and says nothing was checked.

## Open questions

- **Kimi.** Through OpenCode or through its own CLI, or both. The [decision gate](../implementation/synapse-mvp-plan.md#kimi-integration-decision-gate) requires a side-by-side local check of the read-only boundary, model selection, cancellation and usage reporting first.
- **Which agents are in a first execution milestone.** One provider at a time is the working assumption.
- **How to describe reasoning-level controls** when providers express them differently, without inventing a false common scale.
