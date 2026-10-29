# Review and verification

**Status:** planned. A raw multi-agent review exists in the inherited command-line runtime. Independent reviewer roles, evidence reports, decisions and the audit trail do not exist. The intended behavior is a proposal for review.
**Scope:** how the result of agent work is examined, how evidence is presented, and how the user decides. How changes are isolated and integrated is in [Worktrees](worktrees.md).
**Related:** [Features I](../product/features.md#i-review-and-verification) · [Orchestration](orchestration.md) · [Execution](execution.md) · [Workflow guide](../guides/workflows.md)

## Goals

- Never treat an agent's report of its own success as evidence.
- Separate the agent that did the work from the one that checks it.
- Put verifiable material in front of the user, and let the decision be theirs.
- Keep a record of what was decided and on what basis.

## Intended behavior

**Independent reviewers.** A review stage uses a different agent from the stage under review, and ideally a different provider, so that one model's blind spots are less likely to be shared. A reviewer has read-only access to the result. It cannot change what it reviews.

**Evidence.** The material for a decision is gathered by Ordelis, not reported by the agents:

- the diff of the work, from the [worktree](worktrees.md);
- the output of tests and builds that Ordelis ran, with commands and exit statuses;
- the reviewer's findings, each with a severity and a reference to what it concerns;
- the record of what each agent was authorized to do and did.

**Completion reports.** A report says what was asked, what changed, what was verified and by what, what failed, and what was not checked. Statements are tied to evidence the user can open. A claim with no evidence is shown as a claim.

**User-facing summary.** The summary is written for a person making a decision: the result, the risks that matter, and where to look. It does not replace the evidence.

**Decisions.** The user can request changes, reject, or approve.

- *Request changes* returns feedback to [orchestration](orchestration.md) as a revised plan or a follow-up stage.
- *Reject* discards the work without integrating it.
- *Approve* marks the work as acceptable. It does not integrate anything.

**Integration confirmation.** Integration is a separate, explicit step after approval. Approving a review never merges.

**Audit trail.** Each decision is recorded: who decided, what they saw, when, and the outcome. The trail is append-only from the user's point of view.

## User interactions

Read the report and the evidence; open any diff, log or finding; compare reviewer opinions; decide; confirm integration; look back at past decisions.

## Architectural responsibilities

| Responsibility | Owner |
| --- | --- |
| Running tests and builds, and recording the results | [Execution](execution.md) |
| Producing findings | A reviewing agent |
| Assembling evidence and the report | Review |
| Deciding | The user |

## Security implications

- Reviewer output is untrusted text. It is evidence for a human, and it never grants permission or approves anything.
- A reviewer that shares the author's provider shares its failure modes. Independence is a risk reduction, not a guarantee, and the interface does not call it a guarantee.
- Evidence must be gathered from outside the agents. An agent that can write to the test output or the report can forge it.
- The audit trail must resist accidental loss. Resistance to a hostile same-user process is out of scope, as elsewhere in the design.

## Constraints

- A review depends on the quality of the checks that exist in the project. Ordelis can run what the project has and can report that nothing covers a change; it cannot make weak tests strong.
- Reviewers see only the context they are given.
- Severity labels come from the reviewer. Ordelis presents them as the reviewer's opinion.

## Current implementation

- *Working, inherited CLI.* `mco review` runs a read-only review across chosen agents in parallel and returns each raw answer with its operational status. `--debate` adds a read-only stage over earlier answers, and `--synthesize` adds a synthesis from one chosen agent. Earlier answers are passed as untrusted reference material.
- *Deliberately absent from the runtime.* The inherited runtime removed its structured findings schema, consensus, SARIF and diff-only review, so it makes no semantic claims about answers. Ordelis's reviewer findings and severity would be a new layer, not a restoration.
- *Planned.* Independent reviewer assignment, evidence reports, the three decisions, integration confirmation and the audit trail.

## Open questions

- **A findings format.** Severity scales and references are not designed, and the inherited runtime removed its own.
- **How independent is independent enough,** and how Ordelis reports the independence it actually achieved.
- **How tests and builds are run safely** in a worktree whose contents came from an agent.
- **Where the audit trail lives** and how long it is kept.
