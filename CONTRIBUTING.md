# Contributing to Ordelis

Thank you for your interest in Ordelis. This page tells you how to take part. The detailed development guide is [docs/development/README.md](docs/development/README.md); this page links to it instead of repeating it.

## What the project is today

Ordelis is a local-first, open-source desktop workspace for coordinating AI coding agents. It is in **early development**. The desktop app is an interface preview over sample data and cannot run agents. The repository also contains the command-line runtime inherited from [MCO](https://github.com/mco-org/mco), and tested safety foundations such as an execution gate that currently always answers no. There are no releases. The [README](README.md) has the full picture and [features](docs/product/features.md) lists every planned capability with its state.

## Find something to work on

- Browse the [open issues](https://github.com/RaioViajante/ordelis/issues). Issues labelled [`good first issue`](https://github.com/RaioViajante/ordelis/labels/good%20first%20issue) or [`help wanted`](https://github.com/RaioViajante/ordelis/labels/help%20wanted) are the best starting points. There may be none open at a given time.
- The [roadmap](docs/product/roadmap.md) orders the remaining work without dates.
- Documentation fixes are always welcome: unclear steps, broken links, and statements that no longer match the code.

## Propose a change

1. **Open an issue first** for anything beyond a typo or a broken link. Use the bug report or feature request form and say what the goal is and how you would know it is done. This avoids work on something that cannot be accepted.
2. Wait for a maintainer to agree on the direction before investing in a large change. Ordelis is maintained on a best-effort basis, so a reply may take time.
3. Work on a branch named for the kind of change and the issue, for example `docs/59-ordelis-readme-and-docs` or `feat/47-local-project-registry`.
4. Write [Conventional Commit](https://www.conventionalcommits.org/) messages, with the issue number at the end, for example `fix(security): close disclosure-lint bypasses (#58)`. The types used in the history are `feat`, `fix`, `docs`, `test`, `security`, `chore`, `ci`, `refactor` and `perf`.
5. Open a pull request using the [template](.github/pull_request_template.md). A draft pull request is a good way to ask for early feedback.

## Set up and run the checks

The [development guide](docs/development/README.md) covers the repository layout and every check: the Python runtime (Python 3.10+), the Node wrapper, the desktop frontend (Node 22+, pnpm) and the desktop native code (stable Rust). The [desktop commands](desktop/README.md#commands) are listed in its README.

- Run the checks for the parts you changed. A documentation-only change needs no build, but its links should resolve.
- You do not need to run every platform's build. CI runs the Python checks on every pull request and the desktop checks when desktop files change. The desktop has so far been built and run only on macOS.
- For a bug fix, add a test that fails before the fix and passes after it. If a test was already failing before your change, say so in the pull request.

## Pull request expectations

- **One concern per pull request**, with a small diff. Do not reformat or tidy code you were not asked to change.
- **Match the surrounding code**: its style, naming, comments and the libraries it already uses. Do not add a dependency without saying why.
- **Say what you did and why**, which checks you ran, and anything you are unsure about.
- **Update documentation** when behavior changes.
- Expect an independent review before merge. Respond to review on the same branch; do not force-push a branch others are using, and do not rewrite shared history.
- Contributions are accepted under the repository's [MIT License](LICENSE).

If you use a coding agent, the rules it should follow are in [CLAUDE.md](CLAUDE.md) (identical to `AGENTS.md`, and written in Chinese). The points above summarise them. You remain responsible for what you submit.

## Documentation

- Use the four status labels (**Working**, **Prototype**, **Foundation**, **Planned**) defined in [terminology](docs/product/terminology.md#status-labels). Never describe a planned capability as present, and never present sample data as working functionality.
- Give each topic one authoritative document and link to it. The [documentation index](docs/README.md) shows where each topic lives.
- Keep historical records as written, including the names Synapse and MCO, and add a short note where the old name would mislead.
- Do not invent commands, APIs, data formats, release dates or install instructions.

## Safety and authorization boundaries

Some rules are enforced by tests and are not style preferences. The full list is in the [development guide](docs/development/README.md#safety-restrictions-that-apply-to-every-change). In short:

- Do not weaken the [Execution Safety Gate](docs/implementation/execution-safety-gate.md). It denies by design, and execution stays disabled until a separate, reviewed design says otherwise.
- Do not add native desktop commands, filesystem, shell, network or plugin access casually. Each is a reviewed change; see [IPC authorization](docs/implementation/desktop-architecture.md#ipc-authorization).
- Keep the content security policy strict.
- Never commit, or paste into an issue, pull request or prompt, credentials, tokens, provider homes, shell snapshots, session transcripts, raw agent output or personal file paths. See [probe artifact hygiene](docs/guides/probe-artifact-hygiene.md).

## Report a security vulnerability

Do not open a public issue, pull request or discussion for a vulnerability. Report it privately as described in [SECURITY.md](SECURITY.md).

## MCO provenance and compatibility

Ordelis began from MCO and keeps its history, [MIT license and copyright notice](LICENSE). Much of the root directory is still MCO. Its names (the `mco` command, the `runtime` Python package, the npm package name `@tt-a1i/mco` and the `.mco/` state directory) are intentionally unchanged, so do not rename them as part of unrelated work. **Nothing is published from this repository.** Inherited guides that show `npx @tt-a1i/mco` refer to the upstream project. The [provenance record](docs/migration/ordelis.md#provenance-and-attribution) and the [identifier matrix](docs/migration/ordelis.md#identifier-migration-matrix) explain what is preserved and why. A problem that exists only in upstream MCO may belong in that project instead.

## Getting help

See [SUPPORT.md](SUPPORT.md).
