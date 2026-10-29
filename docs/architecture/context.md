# Context management

**Status:** partly working. A read-only vault preview exists on the command line and a context prototype exists in the desktop over sample data. Context packages, file scoping and the rest are planned. The intended behavior is a proposal for review.
**Scope:** how the user decides what an agent may see, how that is made visible, and how local-first handling is kept. Optional Obsidian integration is covered here.
**Related:** [Features F](../product/features.md#f-context-management) · [Vault context preview](../guides/vault-context-preview.md) · [Private execution plan](../guides/private-execution-plan.md) · [Desktop context experience](../implementation/desktop-context-experience.md) · [Sessions](sessions.md)

## Goals

- An agent receives what the user chose and nothing else.
- The user can see, before and after, exactly what an agent was given and where it came from.
- Private material stays on the machine and out of the repository.

## Intended behavior

**Explicit selection.** Context is selected by the user for a task. Ordelis may suggest, but nothing is attached by default and nothing is scanned in advance.

**Sources.** Project files and documentation; session history from earlier work; and optionally notes from an Obsidian vault. For a vault, the user selects notes or sections. Ordelis does not ingest a vault wholesale, does not write to it, and treats the vault as read-only reference data.

**Scoped file access.** Context that refers to files grants only those files. It is separate from the permission to read or write the repository during execution, and the two are shown separately.

**Context packages.** The context for a task, or for one agent within it, is a package assembled before the plan is approved. Different agents in the same task may receive different packages. The user can reuse a package for related work in the same project.

**Visibility and provenance.** For every item the user can see its source, which excerpt was taken, whether it was truncated, and which agents received it. After a stage runs, the record of what was sent remains available.

**Control per agent.** The user can change what each agent receives. Removing an item from a package before approval takes effect before anything runs.

**Local-first handling.** Context is read locally and handed to a locally run agent. Ordelis sends it nowhere itself. What a provider's tool does with it afterwards is governed by that provider and the user's account with them, which Ordelis cannot see or control, and the interface says so.

## User interactions

Search and browse sources; select items; see budgets and truncation; see provenance; review the package as part of plan approval; adjust per agent; review what was sent afterwards.

## Security implications

- Context is data, not instructions. A note or file that says "ignore your constraints" is text to be passed along as reference material, delimited so it cannot close its own wrapper, and never a source of policy.
- Sensitive material is a user decision. Ordelis cannot know that an excerpt contains a secret, so it makes the contents visible before they are shared rather than promising to detect them.
- Context packages and history are private state. They belong outside the repository checkout, and ignoring a path in Git is not a privacy boundary.
- Paths are validated. The inherited private-path rules reject configuration, vault, index and state paths that overlap the checkout.

## Constraints

- Per-note and total size budgets are required, so that selection stays understandable and agents' limits are respected.
- Provider tools differ in how they accept context, and some surfaces cannot be controlled by Ordelis at all.
- Symlinks, invalid text encodings and paths outside the chosen roots need defined handling. The vault preview's tests already pin its behavior for symlinks, UTF-8, traversal and exclusion; the desktop's rules for files are not yet designed.

## Current implementation

- *Working, inherited CLI.* [`mco context preview`](../guides/vault-context-preview.md) takes an explicit external vault directory and JSON index, selects project and topic notes, optionally headings, and prints bounded UTF-8 excerpts with source paths and truncation flags, plus a proposed agent and a no-dispatch policy. It modifies nothing and starts no agent. Its tests cover traversal, symlinks, exclusion, determinism and the absence of writes.
- *Foundation.* [`mco context plan`](../guides/private-execution-plan.md) adds a versioned external configuration and one non-dispatching plan. A mock-only bridge passes selected excerpts to a fake agent in tests, in a delimited, provenance-bearing block.
- *Prototype.* The Context page: searchable sample notes, provenance and a selection shared with Home. It reads no real file or vault. See [Context experience](../implementation/desktop-context-experience.md).
- *Planned.* Real file and vault selection in the desktop, context packages, scoped file access, session history as context, reuse.

## Open questions

- **How context reaches each agent,** given that the surfaces differ by provider and some cannot be restricted.
- **How the user is warned about sensitive content** without Ordelis pretending to detect it.
- **Where private context and its history are stored** per operating system.
- **Whether a context package is versioned** so that a past run's context can be reproduced exactly.
