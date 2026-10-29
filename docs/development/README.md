# Contributing and development

**Status:** guide. Commands are taken from the repository's CI workflows, `RELEASING.md`, the pull request template and the desktop README. There is no separate `CONTRIBUTING.md`; this is the entry point.
**Scope:** how to get oriented in the repository, run the checks, and make a change that fits. It does not describe the product; see [Architecture](../architecture/overview.md) for that.
**Related:** [Desktop README](../../desktop/README.md) · [Security policy](../../SECURITY.md) · [Execution safety gate](../implementation/execution-safety-gate.md) · [Migration record](../migration/ordelis.md)

## Repository layout

| Path | What it is |
| --- | --- |
| `desktop/` | The Ordelis desktop application: a Tauri 2 shell (Rust in `src-tauri/`) with a React and TypeScript interface (`src/`). It has its own `pnpm` lockfile and is separate from the root npm package. |
| `runtime/` | The Python runtime. Mostly the inherited MCO package, plus modules added since the fork: vault context, private plans, probe authorization, and the Execution Safety Gate. |
| `tests/` | Python `unittest` suites for the runtime, and `tests/node/` for the Node wrapper. |
| `docs/` | Documentation. See the [index](../README.md). |
| `scripts/` | Release, packaging, gate and probe scripts, mostly inherited. |
| `.github/workflows/` | `gate.yml` (runtime, packaging and installer checks), `desktop.yml` (the desktop app on macOS), and the inherited `benchmark.yml`, `preview-package.yml` and `publish-npm.yml`. |
| `bin/`, `mco`, `skills/`, `reports/` | Inherited MCO launchers, bundled skill and report output. |

## The inherited MCO package

Much of the root directory is MCO, from which this project began. Its names are intentionally unchanged: the `mco` command, the `runtime` Python package, the npm package name `@tt-a1i/mco`, and the `.mco/` state directory. Do not rename them as part of unrelated work. The [migration record](../migration/ordelis.md#inherited-mco-preserved) explains which identifiers are preserved and why.

- Run the inherited CLI from a checkout with `./mco --help`.
- **Nothing is published from this repository.** The root package is marked private, a guard blocks npm publication, and a test fails if any workflow can publish. Inherited guides that show `npx @tt-a1i/mco@latest install` refer to the upstream project's package, not to an Ordelis release.
- Inherited documentation, such as the [workflow guide](../guides/workflows.md), the [CLI reference](../reference/cli.md) and [provider reference](../reference/providers.md), describes MCO and is kept as is.

## Running the checks

**Python runtime.** From the repository root, with Python 3.10 or newer:

```sh
python3 -m unittest discover -s tests -p 'test_*.py'
python3 -m unittest tests.test_execution_safety_gate -v   # one module
./scripts/run_runtime_gate_tests.sh                       # the runtime gate used by CI
```

**Node wrapper.** From the repository root:

```sh
npm ci
npm test
```

**Desktop frontend.** From `desktop/`, with Node 22 or newer and pnpm:

```sh
pnpm install --frozen-lockfile
pnpm lint && pnpm typecheck && pnpm test
pnpm build
```

**Desktop native code.** From `desktop/`, with a stable Rust toolchain:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

The desktop CI job also runs further checks: the same tests with the development runtime feature, a release-profile test run, checks that the generated permission files match the committed ones, a native-boundary verification, and a check that a release bundle holds no development runtime. See [`desktop.yml`](../../.github/workflows/desktop.yml) and [Visual verification](../implementation/desktop-visual-verification.md).

**Run the app.** `pnpm dev` serves the interface in a browser. `pnpm tauri dev` opens the desktop window. See [Desktop commands](../../desktop/README.md#commands).

## Making a change

The repository's [`CLAUDE.md`](../../CLAUDE.md) (identical to `AGENTS.md`, and written in Chinese) sets conventions for coding agents and is a good summary for anyone. In short:

- **Read before you write.** Look at how the code around you does the same thing, and match it.
- **Keep the diff small.** One concern per pull request. Do not reformat or tidy code you were not asked to change.
- **Test.** Write the failing test first for a bug fix. Run the existing tests before and after, and say so if something was already failing.
- **Say what you did and why,** and flag concerns.

The workflow in practice:

1. Open an issue that states the goal and the acceptance criteria.
2. Work on a branch named for the kind of change and the issue, for example `docs/59-ordelis-readme-and-docs`.
3. Open a pull request using the [template](../../.github/pull_request_template.md), including the validation you ran and a rollback note.
4. Expect an independent review before merge. Do not force-push shared branches or rewrite history.

## Safety restrictions that apply to every change

These are not style preferences. They protect users, and they are enforced by tests.

- **Do not weaken the Execution Safety Gate.** It denies by design, and execution stays disabled until a separate, reviewed design says otherwise. See the [gate record](../implementation/execution-safety-gate.md).
- **Do not add native commands casually.** A new command changes the manifest, the handler, the capability file and the tests together, and each is a reviewed change. The interface may not gain filesystem, shell, network or plugin access. See [IPC authorization](../implementation/desktop-architecture.md#ipc-authorization).
- **Keep the content security policy strict.** No remote fonts, scripts, images or inline styles in production.
- **Never present sample data or mockups as working functionality,** in the interface or in documentation.
- **Never commit provider homes, shell snapshots, session transcripts, raw provider output, credentials or private design exports.** See [Probe artifact hygiene](../guides/probe-artifact-hygiene.md). Do not paste such material into issues, pull requests or prompts.
- **Trust-origin values and other security vocabulary** in the safety gate are not renamed by branding work.

## Documentation conventions

- Use the four status labels (**Working**, **Prototype**, **Foundation**, **Planned**) from [Terminology](../product/terminology.md#status-labels), and never describe a planned capability as present.
- Give each topic one authoritative document and link to it instead of restating it.
- Keep historical records, including Synapse and MCO names, as written, and add a short note where the old name would mislead.
- Do not invent APIs, data formats, release dates or install commands.

## Reporting a security issue

Report suspected vulnerabilities privately, following [SECURITY.md](../../SECURITY.md). Do not open a public issue, pull request or discussion that describes one.
