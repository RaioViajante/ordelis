# Contributing to Ordelis

Thank you for your interest in Ordelis. Bug reports, documentation fixes, ideas and code are all welcome. This page covers the essentials; the [development guide](docs/development/README.md) has the detail.

## What the project is today

Ordelis is a local-first, open-source desktop workspace for coordinating AI coding agents. It is in **early development**. The desktop app is an interface preview over sample data and cannot run agents. The repository also contains the command-line runtime inherited from [MCO](https://github.com/mco-org/mco), and tested safety foundations such as an execution gate that currently always answers no. There are no releases. The [README](README.md) has the full picture and [features](docs/product/features.md) lists every planned capability with its state.

## Find something to work on

- Browse the [open issues](https://github.com/RaioViajante/ordelis/issues). Those labelled [`good first issue`](https://github.com/RaioViajante/ordelis/labels/good%20first%20issue) or [`help wanted`](https://github.com/RaioViajante/ordelis/labels/help%20wanted) are good starting points. There may be none at a given time.
- The [roadmap](docs/product/roadmap.md) orders the remaining work without dates.
- Documentation fixes are always welcome: unclear steps, broken links, and statements that no longer match the code.

## Propose a change

- **Small fixes and well-scoped improvements** (a typo, a broken link, a documented bug with an obvious fix) can go straight to a pull request.
- **For anything larger or less obvious, opening an issue first is encouraged.** Say what you want to achieve and how you would know it is done. It lets a maintainer confirm the direction before you invest time.
- Name your branch for the kind of change and, if there is one, the issue, for example `docs/59-ordelis-readme-and-docs` or `feat/47-local-project-registry`.
- Write [Conventional Commit](https://www.conventionalcommits.org/) messages, with the issue number at the end when there is one, for example `fix(security): close disclosure-lint bypasses (#58)`. The history uses `feat`, `fix`, `docs`, `test`, `security`, `chore`, `ci`, `refactor` and `perf`.
- Open a pull request using the [template](.github/pull_request_template.md). A draft pull request is a good way to ask for early feedback.

Ordelis is maintained on a best-effort basis, so replies may take time.

## Set up and run the checks

The [development guide](docs/development/README.md) covers the repository layout and every check: the Python runtime (Python 3.10+), the Node wrapper, the desktop frontend (Node 22+, pnpm) and the desktop native code (stable Rust). The [desktop commands](desktop/README.md#commands) are in its README.

- Run the checks for the parts you changed. A documentation-only change needs no build, but its links should resolve.
- You do not need to run every platform's build. CI runs the Python checks on every pull request and the desktop checks when desktop files change. The desktop has so far been built and run only on macOS.
- For a bug fix, add a test that fails before the fix and passes after it. If a test was already failing before your change, say so in the pull request.

## Pull requests

- Keep each pull request to **one concern**, with a small diff. Please do not reformat code you were not asked to change.
- Match the surrounding code: its style, naming and the libraries it already uses. If you add a dependency, say why.
- Say what you did, why, which checks you ran, and anything you are unsure about.
- Update the documentation when behavior changes.
- Expect an independent review before merge. Respond on the same branch, and please do not force-push a branch others are using.

## Licensing

Ordelis is released under the [MIT License](LICENSE). Contributions submitted for inclusion in the project are covered by the same terms. There is no contributor license agreement and no copyright assignment.

## Documentation

- Use the four status labels (**Working**, **Prototype**, **Foundation**, **Planned**) from [terminology](docs/product/terminology.md#status-labels). Never describe a planned capability as present, or sample data as working functionality.
- Give each topic one authoritative document and link to it. The [documentation index](docs/README.md) shows where each topic lives.
- Keep historical records as written, including the names Synapse and MCO, and add a short note where the old name would mislead.
- Do not invent commands, APIs, data formats, release dates or install instructions.

## Safety and authorization boundaries

Some rules are enforced by tests and are not style preferences. The full list is in the [development guide](docs/development/README.md#safety-restrictions-that-apply-to-every-change). In short:

- Do not weaken the [Execution Safety Gate](docs/implementation/execution-safety-gate.md). It denies by design, and execution stays disabled until a separate, reviewed design says otherwise.
- Do not add native desktop commands, or filesystem, shell, network or plugin access, casually. Each is a reviewed change; see [IPC authorization](docs/implementation/desktop-architecture.md#ipc-authorization).
- Keep the content security policy strict.
- Never commit, or paste into an issue, pull request or prompt, credentials, tokens, provider homes, shell snapshots, session transcripts, raw agent output or personal file paths. See [probe artifact hygiene](docs/guides/probe-artifact-hygiene.md).

## Report a security vulnerability

Do not open a public issue, pull request or discussion for a vulnerability. Report it privately as described in [SECURITY.md](SECURITY.md).

## MCO provenance and compatibility

Ordelis began from MCO and keeps its history and [copyright notice](LICENSE). Much of the root directory is still MCO. Its names (the `mco` command, the `runtime` Python package, the npm package name `@tt-a1i/mco` and the `.mco/` state directory) are intentionally unchanged, so please do not rename them as part of unrelated work. **Nothing is published from this repository.** Inherited guides that show `npx @tt-a1i/mco` refer to the upstream project. The [provenance record](docs/migration/ordelis.md#provenance-and-attribution) and the [identifier matrix](docs/migration/ordelis.md#identifier-migration-matrix) explain what is preserved and why. A problem that exists only in upstream MCO may belong in that project instead.

## Coding agents

If you use a coding agent, the repository's instructions for agents are in [`CLAUDE.md`](CLAUDE.md), which is identical to `AGENTS.md`. **They are written in Chinese, and you do not need to read them to contribute.** Their rules are the same ones described in this guide and in [Making a change](docs/development/README.md#making-a-change): read the surrounding code first, keep the diff small, test, and explain what you did. You remain responsible for what you submit.

## Getting help

See [SUPPORT.md](SUPPORT.md).
