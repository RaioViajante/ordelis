# Git worktree isolation and integration

**Status:** planned. Nothing in this document is implemented. The intended behavior is a proposal for review.
**Scope:** how Ordelis isolates parallel agent work using Git worktrees, tracks what each agent changed, and brings approved work into another branch under the user's control.
**Related:** [Features E](../product/features.md#e-git-and-worktree-isolation) · [Orchestration](orchestration.md) · [Review](review.md) · [Execution](execution.md) · [Workflow guide: scope and parallel writing](../guides/workflows.md#scope-and-parallel-writing)

## Goals

- Let several agents write at the same time without sharing a working directory.
- Keep the user's own working directory and the main branch untouched until the user decides otherwise.
- Make every change attributable to the agent and stage that produced it.
- Make integration a deliberate act, and make rejecting work cheap and safe.

## Intended behavior

**Repository inspection.** Before anything else, Ordelis reads the state of the repository: current branch, uncommitted changes, existing worktrees and the branches involved. A task starts from a known base, and if the repository is in a state that makes isolation unsafe (for example, unresolved conflicts), the user is told rather than worked around.

**Branch-aware tasks.** A task records the branch it started from and the branch its results are intended for.

**Isolated worktrees.** Each stage that writes runs in its own Git worktree on its own branch. The worktree is created by Ordelis for the stage, so that parallel implementers cannot overwrite one another or the user's checkout.

**Creation and cleanup.** Ordelis creates worktrees when a stage is approved to start and removes them when their work is integrated or discarded. Cleanup never removes a worktree that holds changes the user has not seen or decided about. Failed and cancelled stages leave their worktree available for inspection until the user discards it.

**Tracking changes per agent.** Changes are tracked at the worktree level, so the diff of an agent's work is the diff of its branch against its base. Stages that need to touch the same files are flagged during planning, because isolation prevents overwrites but not logical conflicts.

**Diffs and conflict detection.** The user can read each agent's diff. Before integration, Ordelis checks whether the changes from several agents conflict with each other or with the target branch, and shows conflicts instead of resolving them silently.

**Integration preparation and integration.** Preparing integration assembles the approved branches and shows the combined result. Merging or squashing into another branch happens only when the user confirms that specific action. **No agent merges code into the main branch on its own, and Ordelis does not either without the user's authorization.**

**Main-branch protection.** The main branch is never an agent's working branch. Writes to it are limited to the user-confirmed integration step.

**Rollback and rejection.** Rejecting a result discards the worktree and its branch without touching anything else. After an integration, the user can see what to revert; Ordelis does not rewrite history to undo it.

## User interactions

See which worktree belongs to which stage; read per-agent diffs; see conflict reports; reject a result; prepare, confirm and perform an integration; discard leftovers.

## Architectural responsibilities

| Responsibility | Owner |
| --- | --- |
| Creating, listing and removing worktrees | Worktree management |
| Deciding which stages may run in parallel | [Orchestration](orchestration.md), using the files each stage may touch |
| Confining an agent to its worktree | [Execution](execution.md), and not by convention alone |
| Reporting the result of an agent's work | [Review](review.md) |

## Security implications

- A worktree is a separate directory, not a security boundary. A process with access to the filesystem can still reach other paths. Confinement is a separate property that [Execution](execution.md) must provide; this document does not claim it.
- Git operations can run code through hooks and configuration. Treating a worktree's repository configuration as untrusted is part of the design to be done.
- Destructive operations such as removing a worktree with unseen changes, force operations and history rewrites need explicit human approval.
- Branch names, file names and commit messages come from agents and are untrusted text.

## Constraints

- Ordelis works with the user's existing Git repository and does not require a hosting service.
- Windows and Linux paths, permissions and symlink behavior differ from macOS and are unverified here.
- Isolation cannot prevent two agents from making changes that are individually fine and jointly wrong. Review and integration checks exist for that.

## Current implementation

- *Planned.* Nothing in this document exists in the runtime or the desktop.
- *Relevant today.* The inherited runtime explicitly does not create or manage worktrees and does not prohibit parallel writers. Users are advised to partition ownership by path and use distinct task IDs. The original audit made worktree isolation a precondition for parallel code writers: [phase 5](../implementation/synapse-mvp-plan.md#implementation-phases-and-exit-gates).
- *Prototype.* The Projects and Sessions pages mention worktrees as part of sample data only.

## Open questions

- **Where worktrees live on disk,** and how that interacts with the user's own layout and tooling.
- **Handling of submodules, large repositories and untracked files** inside a worktree.
- **How much conflict detection is possible before integration** without performing it.
- **Whether integration produces a pull request** when the repository has a remote, or stays a local merge. Neither is decided.
- **Hooks and configuration:** what Ordelis must neutralize in an agent's worktree.
