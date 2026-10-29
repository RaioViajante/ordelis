# Ordelis migration: decision record

**Status:** accepted direction, partly implemented. This document is documentation only; the [current status](#current-status) lists what has been done. It records what was decided, what was verified, and what must happen before each later change. **One item is time-sensitive and is not part of the migration sequence: [SEC1](#security-exposure-probe-artifacts-in-public-git-history-sec1), the assessment of a possible security exposure in this public repository's git history. It was assessed on 2026-10-28 and is tracked as mitigated according to the maintainer's report, with confidential closure documentation pending.**

**Scope of the migration:** the product developed as **Synapse** (a fork of [MCO](https://github.com/mco-org/mco)) becomes **Ordelis** (`ordelis.app`). The repository has been renamed to `ordelis`; the application, packages and interface text keep the old name until N1 and the I-series PRs. Brand styling: `ordelis` as the visual wordmark, `Ordelis` in prose. This record covers the repository, desktop app, build metadata, ownership and release hygiene, and the planned website. It deliberately does not rename inherited MCO interfaces.

Contents: [Current status](#current-status) · [Accepted decisions](#accepted-decisions) · [Safety invariants](#safety-invariants) · [Security exposure (SEC1)](#security-exposure-probe-artifacts-in-public-git-history-sec1) · [Provenance](#provenance-and-attribution) · [Identifier matrix](#identifier-migration-matrix) · [Registry migration](#registry-migration-semantics-i2) · [Evidence provenance](#evidence-provenance) · [Platform assumptions](#platform-assumptions) · [Visibility](#repository-visibility-evaluation) · [Rename runbook and execution record](#repository-rename-runbook-and-execution-record) · [Prerequisites](#outstanding-prerequisites) · [Risk register](#risk-register) · [PR roadmap](#pr-roadmap)

## Current status

As of 2026-10-29. The rest of this document keeps the decisions, findings and plans as they were written, so a "Finding" column or a "prepared, not executed" label describes the state at that time. **This table is the current state.**

| ID | Status | Evidence |
| --- | --- | --- |
| **B1** | Done | Pull request #52 (merge commit `430ba6b`): this record. |
| **H1** | Done | Issue #53, pull request #54 (merge `8ad568f`). The inherited npm publish workflow cannot reach its publish job, the root package is `private`, and a test fails if any workflow can publish to npm. Follow-ups needed before the first real GitHub Release are tracked in issue #56. |
| **H2** | Done | Issue #55, pull request #57 (merge `a420a55`). `CODEOWNERS` names `@RaioViajante` (GitHub's validator reports 0 errors; it reported 5 before). `SECURITY.md` was introduced. The issue-template security link no longer points at an upstream project. The maintainer enabled private vulnerability reporting before the merge. |
| **R2** | Done on 2026-10-29 | The existing repository was renamed from `RaioViajante/synapse` to `RaioViajante/ordelis` (same repository id; no repository was created). See the [execution record](#repository-rename-runbook-and-execution-record). |
| **H3** | Implemented in draft PR #60; **not merged into `main`** | Issue #58 and draft pull request #60, under review. It covers the post-rename part of the roadmap's H3: the canonical security link, a private-first fallback in `SECURITY.md`, behavior-based security tests, and this status update. `SECURITY.md` itself was introduced by H2, which is merged. Until PR #60 merges, `main` still links to the security policy through the old repository name and `SECURITY.md` still has the issue-based fallback. |
| **R1** | Not done | Visibility is unchanged: the repository is public. |
| **SEC1** | Assessed; mitigated according to the maintainer's report | Assessed on 2026-10-28. The maintainer reports that the exposure is mitigated and that the responsible party has been contacted. This record has not verified that independently. Confidential closure documentation (contact date, confirmation status, residual risk) is pending, and the historical exposure in Git history is unchanged. The release and publication gate described in the SEC1 section is unchanged until that documentation exists. |
| **N1, I1, I2** | Not started | |
| **W1 onward, U1 onward, P1 onward** | Not started | |

**Security reporting today.**
- Private vulnerability reporting is enabled for the repository.
- The canonical security URL is `https://github.com/RaioViajante/ordelis/security/policy`. On `main`, the issue-template link still uses the old repository name and works through a redirect. Draft PR #60 switches it to the canonical URL.
- `SECURITY.md` (on `main` since H2) makes private reporting the primary route, promises no response time, and says the project is not production-ready. Its public issue-based fallback is replaced in draft PR #60, which points to the maintainer's GitHub profile instead; that change is not on `main` yet.
- `CODEOWNERS` routes review requests to `@RaioViajante`. It does not enforce reviews: no branch protection or required review is configured.

**Repository metadata.** The description is "A local-first, open-source workspace for orchestrating AI coding agents." The ten topics were already set. The homepage field was already `https://ordelis.app` before the rename; nothing about the website is deployed or connected.

## Accepted decisions

| # | Decision |
| --- | --- |
| D1 | **Structure.** Keep `desktop/` and `runtime/` in place and add `web/` beside them. No `apps/`, no Turborepo or Nx, no root pnpm workspace for structural symmetry. Lockfiles stay independent: the root is an npm package, `desktop/` has its own `pnpm-lock.yaml`, and `web/` will have its own. |
| D2 | **Single monorepo.** Temporarily making the repository private during development is being evaluated (see [Repository visibility](#repository-visibility-evaluation)). Until a visibility change is separately authorized and executed, private website exports and other private design assets are not committed to this repository, including on pull-request branches. Any strategy that departs from a single monorepo (for example, keeping private work in a separate repository) is **not** pre-approved: it requires an explicit, recorded amendment to this decision first. |
| D3 | **Rename.** Destination `RaioViajante/ordelis`, under the existing personal account. No new organization. Full history, issues, pull requests and attribution are preserved. The procedure is [prepared](#repository-rename-runbook-and-execution-record) and is not executed without explicit authorization. |
| D4 | **Bundle identifier.** Proposed `app.ordelis.desktop`, subject to the verification below. The existing local project registry is preserved through a separately tested migration or a documented safe fallback. Trust-origin values and other security-sensitive identifiers are not renamed as part of branding. |
| D5 | **Design sources.** The newer web export directories (`projeto/`, `preview/`, `imagens/`, `fonte/`) are the provisional canonical website sources. The desktop export is the visual reference for the Tauri application. The two exports are not merged, and prototype HTML is not production code. A final consistency check against the approved design precedes implementation. |
| D6 | **Design system and fonts.** Preserve the approved palette, symbol, typography and identity. The desktop app uses a system-native font stack with cross-platform fallbacks; JetBrains Mono for code and terminals. No Apple-proprietary font files are distributed. An approved vector master of the symbol is required before final application and installer icons are generated. |
| D7 | **MCO compatibility.** Inherited MCO package names, CLI compatibility, attribution and copyright notices stay unless a separately reviewed change requires otherwise. Original MCO foundations are clearly distinguished from Ordelis-specific work. |
| D8 | **Release workflow and ownership.** `publish-npm.yml` is reviewed and gated or disabled before any Ordelis release. `CODEOWNERS` is updated to the actual Ordelis maintainer without removing upstream attribution. Each is its own reviewable change. |
| D9 | **Cross-platform.** Targets are macOS, Windows and Linux. No platform is declared supported until verified. macOS-only dependencies and Unix assumptions are [documented](#platform-assumptions). Platform migration is not combined with cosmetic redesign. |
| D10 | **Quality.** Every increment has explicit acceptance criteria, local verification, CI evidence and an independent review before merge. No force pushes, history rewriting, unauthorized merges, deployments or release publication. |

### Verification of D4 against Tauri

Checked against the [Tauri v2 configuration reference](https://v2.tauri.app/reference/config/) and the [Tauri CLI 2.6.0 release notes](https://v2.tauri.app/release/tauri-cli/v2.6.0/):

- `app.ordelis.desktop` meets the documented character rule (alphanumerics, hyphens and periods only) and the reverse-domain form.
- Tauri CLI 2.6.0 added a warning that an identifier should not **end** in `.app`, because it collides with the macOS bundle extension. `app.ordelis.desktop` begins with `app` and does not end with it.
- The identifier must be unique and determines the webview data directory. This repository's own [local projects design](../implementation/desktop-local-projects.md) documents the macOS registry location, `~/Library/Application Support/<identifier>/`. The per-OS paths for Windows and Linux come from Tauri's path resolver and have **not** been checked in this repository.
- Tauri documents that the bundle `publisher` defaults to the second element of the identifier. For `app.ordelis.desktop` that is `ordelis`; for the current `dev.raioviajante.synapse` it is `raioviajante`.
- **Correction to an earlier audit statement:** the Windows WiX upgrade code is derived from the **product name**, not the identifier. Changing `productName` therefore changes the default upgrade code. This has no effect today because no Windows installer exists, but the product name must be final before the first Windows installer ships.
- Not yet verified: how NSIS derives its install identity, and whether `enableGTKAppId` should be set on Linux. Both belong to the packaging phase.

## Safety invariants

These hold for every change in this migration.

- The execution safety gate is not bypassed, weakened or removed. It currently always answers no and stays that way until execution is separately designed, reviewed and approved.
- The IPC authorization model (the Tauri app command manifest and capability file), the strict CSP (`default-src 'self'`; no remote fonts or scripts) and the dev-runtime release isolation are preserved. A change to any of them is its own reviewed change.
- Demo or sample data is never presented as live execution.
- No cloud service, mandatory account or telemetry is introduced by branding or redesign work.
- Trust-origin values (`synapse_observed`, `synapse_enforced` in `runtime/execution_safety_gate.py`) are not renamed by any branding change.

## Security exposure: probe artifacts in public git history (SEC1)

**Status (2026-10-29): assessed; mitigated according to the maintainer's report; confidential closure documentation pending.** The historical exposure in Git history is unchanged, and the repository is **public today** (read on 2026-10-28), so its entire history is available to anyone now.

The rest of this section is the policy as written before the assessment. Where it uses language that predates the assessment (for example "open" or "unknown"), read the state on 2026-10-28; the [current status](#current-status) is authoritative. No detail of the assessment appears in this document.

**What is known (metadata only).** This record's author did not open, scan or print the contents of any of these files, and this document deliberately names no file, path, hash or value for them.

- **Set A, highest priority:** 2 shell snapshots, removed by this fork's #49 work. A #49 commit subject describes them as credential-bearing. That description is the only evidence: whether any credential is real, valid or still active is **unknown**.
- **Set B:** the other 24 removed files: 2 session transcripts, 18 vendored skill files and 4 temporary launcher files. The repository's own `.gitignore` comment lists session transcripts among the state that probe homes can capture, so the transcripts are treated as potentially sensitive. The skill and launcher files are expected to be lower risk and are assessed in the same pass.
- **Set C:** the 68 files that are **still tracked** under `docs/probes/`: 34 JSON, 32 log, 1 YAML and 1 Markdown. 33 of them sit in directories named `raw`, which the repository's `.gitignore` comment describes as provider transcripts that "may embed environment details". That ignore rule prevents new additions but does not untrack these. Whether they were sanitized is not established here. **They need the same assessment**, and they are in the working tree today and not only in history.
- **Scope is history, not the tip.** Earlier versions of any file, and anything since deleted, are in the history.
- **Upstream carries the same objects.** The fork base commit exists in upstream's public repository (an API read of public metadata, 2026-10-28), and these files were in its tree.

**What a visibility change does not do.** It controls future access through GitHub. It does not revoke objects that were already fetched, mirrored, archived or cached, and it does nothing about upstream's copy. **Making the repository private is not containment for this issue** and is not a substitute for assessment. Nothing in R1 changes that.

**Required actions, in order**

1. **Contain now.** Nobody copies, quotes or forwards these artifacts, or excerpts of the history, into issues, pull requests, chat, logs or prompts. No public issue is opened about possible secrets. Discussion stays in private channels. Automated agents working in the repository do not read these artifacts unless the maintainer directs a controlled review. Nothing sensitive is added to the repository or to this record.
2. **Assess promptly.** The maintainer designates the assessor (by default the maintainer) and records a deadline in the private tracking item. The assessment starts before R1, W1 or any release, and is **not** deferred to open-source publication. Method: a private, offline working copy; a secret scanner run across the **full history** and across Sets A to C; and, for each finding, a record of the kind of credential or data, the provider and account it belongs to, whether it was ever valid and whether it is still valid. Whether any credential belongs to this project, to upstream's maintainers or to a third party is not established.
3. **Revoke or rotate.** Every confirmed **or reasonably suspected** exposed credential is revoked or rotated by its owner, together with any session or token derived from it. Certainty is not required. Where the provider allows, confirm that the old value no longer works.
4. **Notify upstream, confidentially, when warranted.** If any credential or sensitive data may belong to upstream's maintainers or to third parties, send a confidential notice through upstream's private security reporting channel. The channel must be confirmed before use; its existence is not assumed. The notice describes the class of exposure and not the contents. **Contacting upstream needs separate authorization.** It is never a public issue.
5. **Decide remediation beyond rotation separately.** Each of these needs its own explicit authorization and is not part of this record: rewriting or purging history (it conflicts with provenance requirement 4, cannot recall copies and does nothing for upstream's copy); untracking the remaining raw files at the tip (an ordinary reviewed change that does not remove them from history); asking GitHub to purge cached views (availability not verified); and a policy for future probe artifacts.
6. **Record the outcome privately.** The public record gets only a status line, for example "assessed; no active credentials" or "credentials rotated", with no detail.

**Tracking and dependency.** SEC1 is tracked in a **private** item outside the public issue tracker, for example a draft repository security advisory visible only to maintainers, or an equivalent private record. The maintainer confirms that such a mechanism is available; this was not verified. SEC1 has **no dependency** on H1 to H3, R1, R2 or any other roadmap item. It **blocks any release or open-source publication**. It does not wait for the pre-publication audit; that audit builds on SEC1's findings.

## Provenance and attribution

### Requirements

1. `LICENSE` remains verbatim, including `Copyright (c) 2026 mco maintainers`. Any additional copyright line for Ordelis work is a separate, explicit maintainer decision and is added beside the original, never in place of it.
2. The README keeps its "Origins and license" statement, updated to say Ordelis was formerly named Synapse and began from MCO `v0.12.1`.
3. Attribution appears in **two places**, introduced by N1 (not by this record):
   - A standalone `NOTICE` file at the repository root is the authoritative statement. It names Ordelis (formerly Synapse), the upstream project (MCO, `https://github.com/mco-org/mco`, `v0.12.1`), the upstream copyright line and license (`Copyright (c) 2026 mco maintainers`, MIT, with `LICENSE` as the full text), the fork base commit recorded below, a pointer to this document's [origin table](#original-mco-foundations-versus-ordelis-specific-work), a statement that upstream commit history is retained, and a pointer to `desktop/src/assets/fonts/FONT-LICENSES.txt` for bundled fonts.
   - The README keeps a short "Origins and license" section that links to `NOTICE` and does not duplicate it.
   - `NOTICE` is not in the root npm package's `files` allowlist today, and the app bundle currently ships only `FONT-LICENSES.txt` as a resource. If the npm package is ever published from this repository, or a desktop bundle ever ships MCO-derived code (for example a bundled runtime), `NOTICE` and `LICENSE` must ship with it. That is a P-series acceptance criterion.
4. Git history is never rewritten, so the upstream authors' commits remain intact.
5. Historical documents (`CHANGELOG.md`, `docs/releases/`, `docs/archive/`, `docs/implementation/` decision records, `README.zh-CN.md`) are not blindly renamed. Where the old name would mislead, add a short "formerly Synapse" note and keep the text.
6. GitHub does not show this repository as a fork (`isFork: false`), so the fork relationship cannot be preserved on GitHub itself. Provenance is carried by the history, `LICENSE` and the attribution in requirements 1 to 3.

### Original MCO foundations versus Ordelis-specific work

Fork base: commit `3cfce13` (`fix(ci): repair npm publishing and prepare v0.12.1 (#131)`, 2026-10-07, upstream). The first commit after it, `40313f0`, is the first non-upstream commit, and no commit after the base is by an upstream author.

**How this was determined.** `git diff --name-status -M 3cfce13 origin/main`, taken at `e0d2778` (before this record). Result: **151 files added, 11 modified, 26 deleted, 0 renamed.** The record's author computed this and it has not been independently reproduced; the command is the check. Each file has one of four statuses.

| Status | Files |
| --- | --- |
| **Inherited, unchanged** | `LICENSE`, `RELEASING.md`, `README.zh-CN.md`, `AGENTS.md` and `CLAUDE.md` (byte-identical to each other), root `package.json`, `package-lock.json`, `pyproject.toml`, the `mco` launcher, `bin/`, `skills/`, `reports/`; the MCO runtime (`runtime/` apart from the changes below, including `session/`, `acp/`, `adapters/` and `data/`) and its tests (including `tests/acp_conformance/` and `tests/node/`); `scripts/` apart from the changes below; `docs/contracts/` (5 files), `docs/reference/` (3), `docs/releases/` (8), `docs/archive/` (10), `docs/templates/` (1), `docs/superpowers/` (5), `docs/guides/workflows.md`, the 15 inherited `docs/assets/` brand and logo files (`mco-*`), and the 68 `docs/probes/` files that remain tracked (see [SEC1](#security-exposure-probe-artifacts-in-public-git-history-sec1)); in `.github/`: `CODEOWNERS`, `pull_request_template.md`, the issue templates, and the `benchmark`, `publish-npm` and `preview-package` workflows |
| **Inherited and modified** (11) | `README.md` (rewritten: +34 and −179 lines, so the original MCO README text is largely gone from the working tree and remains in history); `CHANGELOG.md` (+15, −0: entries appended); `.gitignore` (+14, −0: probe-home, shell-snapshot, raw-transcript and `.dev-python` ignores); `runtime/cli.py`; `scripts/run_capability_probes.sh`; `.github/workflows/gate.yml`; and five tests: `test_provider_validation_scripts.py`, `test_session_bugfixes.py`, `test_session_daemon.py`, `test_session_manager.py`, `test_session_nowait.py` |
| **Added by Synapse and Ordelis work** (151) | `desktop/` (81 files: the entire tree); in `runtime/`: `codex_capabilities.py`, `codex_probe_runner.py`, `execution_safety_gate.py`, `probe_authorization_ledger.py`, `probe_confirmation.py`, `runtime_validation.py`, `synapse_plan.py`, `vault_context.py`, `vault_mock_plan.py`; 13 test files; `scripts/sanitize_capability_probe_artifacts.py`; `.github/workflows/desktop.yml`; `.gitattributes` (absent at the base; marks probe-home paths `-diff`); and 45 files under `docs/`: `docs/README.md`, 11 in `docs/implementation/`, the 7 guides added under `docs/guides/` (capability inspection, zero-model probes, interactive probe authorization, private execution plan, probe artifact hygiene, runtime validation, vault context preview), and 26 in `docs/assets/` (the Synapse brand image and desktop screenshots). `web/` will also be Ordelis-specific. |
| **Removed from tracking** (26) | 26 of the 94 `docs/probes/` files present at the base, all inside a single probe-home directory, removed by this fork's #49 security work. By kind: 2 shell snapshots (described in a #49 commit subject as credential-bearing), 2 session transcripts, 18 vendored skill files and 4 temporary launcher files. They are upstream-origin and **remain in git history**, which is never rewritten (requirement 4). **This is a current exposure question, not a future one: see [SEC1](#security-exposure-probe-artifacts-in-public-git-history-sec1).** This record gives no file names, hashes or paths for them. |

## Identifier migration matrix

Each identifier is **Rename**, **Preserve**, **Migrate** (rename with backward compatibility) or **Audit first**. "PR" refers to the [roadmap](#pr-roadmap).

### User-facing

| Identifier | Where | Decision | PR |
| --- | --- | --- | --- |
| `productName` and window `title` `Synapse`, which also name the macOS bundle `Synapse.app` | `desktop/src-tauri/tauri.conf.json` | **Rename** to `Ordelis`. This changes the bundle path to `Ordelis.app`; see the next row. | N1 |
| **CI bundle paths** `bundle/macos/Synapse.app` | `.github/workflows/desktop.yml:120` and `:128` | **Must change in the same PR as `productName`**, or the Desktop job fails. This is a workflow edit inside N1's scope and is called out in N1's acceptance criteria. | N1 |
| Bundle path in usage and help text `<Synapse.app>` | `desktop/scripts/verify-release-isolation.sh:3` and `:15` | **Rename the text only.** `EXPECTED_EXECUTABLE` (line 9) is an I1 change, not N1. | N1 |
| `<title>Synapse</title>` and the package `description` | `desktop/index.html:6`, `desktop/package.json:6` | **Rename.** The package `name` is an I1 change. | N1 |
| UI copy: `aria-label="Synapse"` on the rail mark, "Set up Synapse", Agents lede and note, Continuity note | `desktop/src/app/Shell.tsx:62`, `desktop/src/pages/Home.tsx:161`, `desktop/src/pages/Agents.tsx:43` and `:157`, `desktop/src/components/Continuity.tsx:46` | **Rename.** | N1 |
| Frontend assertions on that copy | `desktop/src/app/Shell.test.tsx:80`, `desktop/src/app/Workspace.test.tsx:149` (`"Set up Synapse"`) and `:259` (`aria-label` `"Synapse"`) | **Update with the copy** in the same PR. | N1 |
| Native Rust user-facing strings | `desktop/src-tauri/src/projects.rs:21` (the folder-panel message), `projects.rs:64` (registry "newer version of Synapse" error), `desktop/src-tauri/src/lib.rs:86` (startup failure message) | **Rename.** A search of the Rust sources found no test asserting these strings; `cargo test --locked` in CI is the check. | N1 |
| Sample project named `synapse` at `~/code/synapse`, and the test that selects it | `desktop/src/fixtures/mock.ts:111-112`; `desktop/src/app/Context.test.tsx:82-83` | **Not branding: decide explicitly.** It is sample data and a test input. Rename it together with the test, or choose a neutral sample name. Never change it by blind find-and-replace. | N1 |
| `SynapseMark` component | `desktop/src/components/icons.tsx:6`, `desktop/src/app/Shell.tsx:2` and `:63` | **Preserve until the approved vector master exists.** It is replaced, not renamed in isolation, by the design-system and icon work. | U1, U-icons |
| Comments citing the former design reference (`reference/Synapse — telas.*`) and the old theme name | `icons.tsx:2`; the citation at the end of line 5 of `fixtures/mock.ts`; `assets/fonts/README.md:4`; `styles/index.css:4` and `:6`; `pages/Agents.tsx:5` | **Preserve** as provenance of the former design reference. The `index.css` header goes away when the theme is rewritten. | U1 |
| Cargo `description` ("Synapse desktop shell. Presents a local UI; …") | `desktop/src-tauri/Cargo.toml:4` | **Rename** the text. The package `name` is an I1 change. | N1 |
| Product-name statements in fixture comments ("…or the Synapse runtime") | `desktop/src/fixtures/mock.ts:6`, `desktop/src/fixtures/sessions.ts:5` | **Rename** the phrase. These are live statements about what the sample data is. In `mock.ts`, lines 5 and 6 are one sentence: the citation at the end of line 5 stays and only the phrase on line 6 changes. | N1 |
| Release-isolation *test* usage text `<dev-runtime Synapse.app>` | `desktop/scripts/test-verify-release-isolation.sh:3` | **Rename the text only.** | N1 |
| Product-name prose in the guides | `docs/guides/codex-cli-capability-inspection.md`, `docs/guides/codex-zero-model-probes.md`, `docs/guides/interactive-probe-authorization.md`, `docs/guides/private-execution-plan.md`, `docs/guides/runtime-validation-foundations.md`, `docs/guides/vault-context-preview.md` | **Rename the prose only**: no command, path, key or identifier changes. The guides describe security behavior, so each gets a careful read in review. | N1 |
| Demo config file name `synapse.json` | `docs/guides/private-execution-plan.md:41-42` (a documented example); `tests/test_synapse_plan.py:52` (a test fixture) | **Rename in the guide only.** No code requires the name: a search of `runtime/`, `desktop/`, `scripts/` and `bin/` found none, and only the guide and that one fixture use it. Keep the test fixture with its module. | N1 |
| Dated design records that open by citing their issue | `docs/implementation/desktop-context-experience.md`, `docs/implementation/desktop-sessions-experience.md`, `docs/implementation/desktop-ui-design-inventory.md`, `docs/implementation/execution-safety-gate.md`, `docs/implementation/private-authorization-ledger.md` (and the other `docs/implementation/` records named in this matrix) | **Preserve.** Add one "formerly Synapse" note where the old name would mislead. `desktop-ui-design-inventory.md` also names the reference files `Synapse — telas.*`, which stay. | N1 |
| Product-name prose and heading in the docs index | `docs/README.md:3` ("Synapse is in early development…") and `:5` (the heading "Synapse desktop") | **Rename in N1.** It is a living index. Leave the link target `./implementation/synapse-mvp-plan.md` at line 10 (the filename is preserved) and the migration entry at line 16, which names Synapse deliberately. | N1 |
| Legacy identifier and product references in the registry design record | `docs/implementation/desktop-local-projects.md`: the identifier `dev.raioviajante.synapse` and its data directory at `:28` and `:159`; header issue links (#45, #47) at `:3`; about a dozen product-name prose lines (for example `:107`, `:132`, `:300`, `:373`) | **Preserve as a dated design record**, with a "formerly Synapse" note (N1). **I2 updates lines 28 and 159** and the Storage section to describe the new identifier and data directory, keeps the legacy identifier documented as the import source, and links to the [import semantics](#registry-migration-semantics-i2) in this record. The UI strings quoted at `:300` and `:373` ("Remove from Synapse") describe copy that does not exist in the code today (a search of `desktop/src` found none), so they are not an N1 code coupling. | N1, I2 |
| Living how-to commands that embed the bundle path | `docs/implementation/desktop-visual-verification.md` (the build, open and verify steps around lines 160-181) | **Update** with the new bundle path. | N1 |
| Verification *records* that mention `Synapse.app` | `docs/implementation/desktop-visual-verification.md:5,54,62,74,82`, `desktop-architecture.md:61`, `desktop-runtime-bridge.md:97` | **Preserve** as historical records. Add a "formerly Synapse" note where the old name would mislead. | N1 |
| README and `desktop/README.md` | `README.md`, `desktop/README.md` | **Rename**, keeping the provenance statement. Add `NOTICE` (see Provenance). | N1 |
| CLI help text `Absolute external Synapse JSON config`; banner `=== Synapse: authorize ONE controlled Codex probe ===` | `runtime/cli.py:1305`, `runtime/probe_confirmation.py:89` | **Rename the text only.** A search of `tests/` found no test asserting either string, so the full unittest suite is the check. The banner is a human authorization prompt, so it gets careful review. | N1 |
| GitHub repo description, topics, social preview | Repository settings | **Update** after the rename | R2 |

### Compatibility-sensitive

| Identifier | Where | Decision | PR |
| --- | --- | --- | --- |
| Bundle identifier `dev.raioviajante.synapse` | `tauri.conf.json`; determines the app data directory holding `projects.json` | **Migrate** to `app.ordelis.desktop`, with a native-side, validated, copy-only import of the registry under the [semantics below](#registry-migration-semantics-i2). Fallback if the import is not approved: document that projects are re-added through the native folder picker. | I2 |
| Cargo package `synapse-desktop`, library `synapse_desktop_lib`, npm package `synapse-desktop`, executable `Contents/MacOS/synapse-desktop` | `desktop/src-tauri/Cargo.toml`, `main.rs`, `desktop/package.json`, `Cargo.lock` | **Rename** to `ordelis-desktop`, atomically with the scripts that hard-code the old names: `verify-release-isolation.sh` (`EXPECTED_EXECUTABLE`, line 9), `test-verify-release-isolation.sh`, `verify-native-boundary.mjs` (the build-directory pattern at line 95), and the `pgrep -x synapse-desktop` command in `docs/implementation/desktop-visual-verification.md:164`. These are security checks and must still pass, including their negative tests. The executable check stays **exact**: it expects `ordelis-desktop` only and must not accept both names, because accepting the old name would weaken the check. | I1 |
| `SYNAPSE_DEV_PYTHON`, `SYNAPSE_DEV_CHECKOUT` | `desktop/src-tauri/build.rs:19-20`, `desktop/src-tauri/src/runtime_bridge/process.rs:26` and `:28`, release-isolation needles at `verify-release-isolation.sh:40` | **Rename** the variables the build emits and the bridge reads to `ORDELIS_DEV_PYTHON` and `ORDELIS_DEV_CHECKOUT`. Compile-time and dev-only. **During the transition both the old and the new names stay in the release-isolation needle list**, so a bundle that embeds either spelling is rejected. The environment-scrub tests (`runtime_bridge/process/tests.rs:203`, the `SYNAPSE_TEST_SECRET` canary, and `:213`, the `SYNAPSE_` prefix assertion) assert that **both** `SYNAPSE_` and `ORDELIS_` prefixes, and a canary for each, do not reach the child process. The needle list in `docs/implementation/desktop-runtime-bridge.md:99` is updated to match. The old needles are removed only by a separately reviewed change; the recommendation is to keep them through the first Ordelis-named release. | I1 |
| `SYNAPSE_PROBE_AUTHORIZED`, `SYNAPSE_PROBE_ID`, `SYNAPSE_CONFIRM`, `SYNAPSE_TOKEN`, `SYNAPSE_UNLISTED`, `SYNAPSE_CANARY`, `SYNAPSE_PROJECTS_TEST_CHILD` | `tests/`, `desktop/src-tauri/src/projects/registry/tests.rs` | **Audit first.** They appear in tests only and, from their names, act as environment canaries or negative cases. Their exact purpose has not been read in detail. Add `ORDELIS_` twins; do not simply replace. | I1 |
| `synapse_observed`, `synapse_enforced` | `runtime/execution_safety_gate.py` (`ORIGINS`, `_TRUSTED_ORIGINS`) | **Preserve.** Trust-origin vocabulary of the safety gate. Renaming silently changes trust classification and may break stored evidence. If a rename is ever wanted, it is a dedicated security-reviewed change that accepts both spellings on read. | none |
| `runtime/synapse_plan.py` and its imports (`cli.py`, `codex_probe_runner.py`, `execution_safety_gate.py`, `probe_authorization_ledger.py`, `runtime_validation.py`; six test files) | `runtime/` | **Preserve.** Internal module, named in the import-boundary allowlist in `tests/test_runtime_status_bridge.py`. A later rename could ship a re-export shim. | none |
| Probe scratch prefix `synapse-codex-probe-` | `runtime/codex_probe_runner.py` | **Audit first.** Check for any cleanup glob keyed on the prefix before renaming, so old scratch directories are not orphaned. | I1 |
| Trust-origin literals in tests: `origin="synapse_enforced"` and `"synapse_observed"` | `tests/test_execution_safety_gate.py:36,48,50,56,198,247` | **Preserve**, together with the values in `runtime/execution_safety_gate.py`. | none |
| `SynapsePlanTests`, the `synapse.json` fixture, and `from runtime.synapse_plan import …` | `tests/test_synapse_plan.py:32,52`; imports in `tests/test_codex_probe_runner.py`, `tests/test_execution_safety_gate.py`, `tests/test_probe_confirmation.py`, `tests/test_runtime_validation.py` (the last contains only the import) | **Preserve** with the module. | none |
| Product-name comments in security-reviewed code | `runtime/synapse_plan.py:1` (docstring), `runtime/codex_probe_runner.py:51`, `runtime/execution_safety_gate.py:44`, `desktop/src-tauri/src/projects/folder.rs:1` | **Preserve.** Comment-only, and they sit in security-reviewed files, so editing them adds review and blame cost without changing behavior. Update them only alongside a functional change to the same lines. | none |
| Release-isolation test fixtures and the message they assert: default executable name `synapse-desktop`, and `expected 'synapse-desktop'` | `desktop/scripts/test-verify-release-isolation.sh:34,37,42,52,53,55,57` | **Rename atomically with `EXPECTED_EXECUTABLE`** in `verify-release-isolation.sh`, because line 52 asserts that script's own message. | I1 |
| Internal temp-directory prefixes and test canaries: `synapse-ipc-`, `synapse-projects-`, `synapse-bridge-`, `synapse-stderr-canary-`, `synapse-child-locked`, `synapse-child-ready`, `/nonexistent/synapse-test…`, and `synapse-dep-info-` in the boundary script; `synapse-boundary-inspector` and `synapse-frontend-boundary-` in the frontend boundary test; the CI scratch name `synapse-capability-probe-work-` | `desktop/src-tauri/src/lib.rs:500`, `projects/registry/tests.rs:51,596-597,798,1051,1123,1256,1264`, `runtime_bridge/process/tests.rs:11,171`, `desktop/scripts/verify-native-boundary.mjs:122`, `desktop/src/app/frontend-boundary.test.ts:39` and `:106`, `.github/workflows/gate.yml:89` | **Preserve.** Internal and cosmetic; renaming adds diff without changing behavior. Rename opportunistically only when a file is touched for another reason. `SYNAPSE_PROJECTS_TEST_CHILD` (`registry/tests.rs:593`) and the `synapse-child-*` markers look like one parent-child test protocol (not read in detail), so rename them together or not at all. | none |
| `/desktop/src-tauri/.dev-python` in `.gitignore` | `.gitignore:33` | **Preserve.** Contains no product name. | none |
| `Cargo.lock`, `pnpm-lock.yaml` entries | `desktop/` | Follow the package renames. Regenerate with `--locked` builds only. | I1 |
| Window `theme: "Dark"` and `backgroundColor: "#100e0c"` | `tauri.conf.json` | **Change** with the design system. The new design has light and dark themes. | U1 |

### Inherited MCO, preserved

| Identifier | Decision |
| --- | --- |
| Python package `runtime`, project name `mco` in `pyproject.toml`, console script `mco`, `bin/mco.js`, the `mco` launcher, npm package `@tt-a1i/mco`, `skills/mco-cli/`, `.mco/state.json` | **Preserve.** Any renaming needs its own tested package-migration plan with compatibility aliases. |
| `pyproject.toml` URLs to `mco-org/mco` | **Preserve.** They are correct attribution. |
| `LICENSE`, `README.zh-CN.md`, `CHANGELOG.md`, `docs/releases/`, `docs/archive/`, `docs/contracts/`, `docs/reference/`, the `mco-*` brand assets | **Preserve.** Append to the changelog; never rewrite. |
| `docs/implementation/synapse-mvp-plan.md` and similar historical filenames | **Preserve** the filename (it is referenced from code comments and other documents). Add a "formerly Synapse" note. |
| `RaioViajante/synapse` links in historical docs (10 files) | **Leave.** GitHub redirects renamed repositories. Update only living documents. |
| `docs/assets/brand/synapse-icon.png` and the current amber icon set | **Replace** only once the approved vector master exists (D6). Reuse `desktop/scripts/prepare-app-icon.swift`. |

### Release, ownership and reporting hygiene

| Item | Finding | Decision | PR |
| --- | --- | --- | --- |
| `.github/workflows/publish-npm.yml` | Publishes `@tt-a1i/mco` when a GitHub Release is published. Today it refuses to publish because no `NPM_TOKEN` secret exists (the repository has none), so safety rests on a missing secret alone. | **Gate or disable**; do not delete. Guard it so it can never run for this repository, and document why. | H1 |
| `.github/workflows/preview-package.yml` | Builds a `tt-a1i-mco-*.tgz` preview. It does not publish. | **Review** within H1. | H1 |
| `.github/CODEOWNERS` | Every path is owned by `@tt-a1i`, the upstream author. GitHub's documentation says a code owner needs explicit write access, and an owner without it is not assigned. The only collaborator on this repository is its owner, so the file assigns no reviewer today. | **Update** to the Ordelis maintainer (`@RaioViajante`). Keep the upstream author credited in `NOTICE`, not as review owner. Be aware that CODEOWNERS only *requests* review; GitHub documents enforcement as a separate branch-protection or ruleset setting ("Require review from Code Owners"). Whether that setting is available on this repository and plan has **not** been verified, and the repository has no branch protection or rulesets today. | H2 |
| `.github/ISSUE_TEMPLATE/config.yml` | The security-disclosure link points at `tt-a1i/mco/security/advisories/new`. Vulnerability reports about this project would reach upstream. No `SECURITY.md` exists. | **Repoint** it at this repository (H2, using today's URL), and **re-validate** it once visibility and name are final (H3). See the [security-reporting facts](#security-reporting-and-ownership-facts-checked-for-this-record) below. | H2, H3 |

### Security-reporting and ownership facts checked for this record

Checked against GitHub's documentation and read-only API queries. "Not established" means the pages read do not say, so no definite claim is made. These are the facts as checked on 2026-10-28; private vulnerability reporting has since been enabled and `SECURITY.md` introduced (see the [current status](#current-status)).

| Fact | Evidence | Status |
| --- | --- | --- |
| Private vulnerability reporting is a feature of **public** repositories | [GitHub docs](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository): "Owners and administrators of public repositories can allow security researchers to report vulnerabilities". Private repositories are not mentioned. | Verified for public; private **not established** |
| It is currently off for this repository | API read of the repository's private-vulnerability-reporting setting returned `enabled: false` | Verified |
| Repository security advisories on a **private** repository | [GitHub docs](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/about-repository-security-advisories) describe them for "maintainers of public repositories" and do not mention private ones | **Not established** |
| Whether the `…/security/advisories/new` URL redirects after a rename, or works after the repository goes private | Not covered by the pages read | **Not established**; check after R2 |
| A code owner needs explicit write access, otherwise no owner is assigned | [GitHub docs](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) | Verified |
| Code-owner review is *enforced* only by a branch-protection or ruleset setting | Same page; [protected-branches docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) | Verified |
| Branch protection and rulesets on a Free personal account, public or private | The protected-branches page read does not state plan or visibility availability for personal accounts | **Not established** |
| Current state: `main` unprotected, no rulesets, one collaborator (the owner), no `SECURITY.md` | API reads and the working tree | Verified |
| Whether a repository owner can approve their own pull request when they are the only collaborator | Not checked | **Not established**. It determines whether "required review" can be enforced with a single maintainer. |

**Ordering consequences.**

- While the repository is public, enabling private vulnerability reporting and repointing the issue-template link at this repository are both possible, so H2 does them with today's URL. This has no dependency on the rename.
- If the repository is later made private (R1), the documentation does not support private vulnerability reporting for it. The reporting channel must then be restated rather than assumed. H3 does this after R1 and R2 are decided, and may add a `SECURITY.md` that names the channel in a way that does not depend on visibility.
- The runbook's post-rename checks (below) include re-testing the advisory URL, so a broken redirect is caught rather than assumed away.
- Nothing in this list is executed by this PR.

### Registry migration semantics (I2)

The bundle identifier determines the app data directory, which holds the local project registry. These are the facts from source and the semantics I2 must meet.

**Facts (verified in source).**

- Registry files live in `<app_data_dir>`: `projects.json`, `projects.lock`, `projects.json.tmp`, and `projects.json.corrupt-<ms>[-<n>]` (`docs/implementation/desktop-local-projects.md`, Storage; constants in `registry.rs:17-20`).
- The registry and the folder panel are compiled **only on macOS** (`projects.rs:12-15`). Other targets return `UnsupportedPlatform` for every project command (`projects.rs`, non-macOS impl). The import is therefore macOS-only.
- Registry functions take the directory as a parameter. Only startup resolves it: `lib.rs:73` builds `Projects::new(app.path().app_data_dir().ok())`.
- The read protocol never creates, writes, renames, truncates or re-permissions anything, and rejects a directory that is a symlink, foreign-owned or group- or world-writable, and a file that is a symlink, a FIFO, not regular, not owned by the effective user or over 256 KiB.
- `remove` rewrites `projects.json` and keeps the file even when the list becomes empty (`registry.rs:111-123`). `reset` acts only on a corrupt registry, keeps it as `projects.json.corrupt-*`, and then removes `projects.json` (`registry.rs:127-149`). `projects.lock` is created by the first locked operation and never removed. `add` inspects the chosen folder **before** it locks (`registry.rs:83`), and `remove` validates the id **before** it locks (`:112-114`). Both then take the lock, creating the directory if needed (`:84`, `:115`), and only afterwards load the registry. A rejected folder or a malformed id therefore never reaches the lock. A directory holding only `projects.lock` is reached by an `add` that fails **after** the lock (the post-lock folder recheck, a clock error, or a commit that fails before the rename, which can also leave a stale `projects.json.tmp`), or by a `remove` of a well-formed but unknown id. The only code that removes `projects.json` is `reset` (`:145`), and only after it has kept the corrupt file as `projects.json.corrupt-*`. `commit` always renames a freshly written `projects.json` into place, including for an empty list (`registry.rs:359-383`). No existing test pins the "last project removed" case.
- Each record stores the project folder's `dev` and `ino` and its path, not anything tied to the data directory, so a record stays meaningful in a different data directory on the same volume.
- The identifier is the same for debug and release builds (`desktop-local-projects.md:28`), so both would import from the same legacy directory.
- The frontend uses no `localStorage`, `sessionStorage` or IndexedDB (searched `desktop/src`; one unrelated comment matched), so there is no browser-storage migration.

**Required semantics.**

1. **Native-side only, no new IPC surface.** The import runs in Rust at startup or inside an existing code path. It adds no command, no permission and no frontend input. The command list in `build.rs`, `capabilities/default.json`, the autogenerated permission files and the invoke handler stay as they are, and the existing test that keeps them equal continues to pass unchanged. The legacy directory is derived in Rust, as a sibling of the new data directory, from a fixed constant for the legacy identifier. It is never taken from the webview.
2. **Existing new registry takes precedence.** The import is skipped if the new data directory already contains `projects.json` or any `projects.json.corrupt-*`. `projects.json` counts as present whatever it is (a regular file, a symlink, a FIFO or a directory), so doubt means skip. These two artifacts are the **only** precedence signals. **`projects.lock` does not prevent import, and neither does a leftover `projects.json.tmp`**: a directory that holds only those is still eligible. `projects.json` is **authoritative whatever it holds**: a valid registry with projects, a valid registry whose project list is empty (what removing the last project leaves), a corrupt file and an unreadable one all skip the import. In the registry code, the loader's `Empty` outcome means the file is *absent* (the read protocol maps `ENOENT` to empty), not that it holds an empty list; an existing file with no projects is `Valid`. The import is whole-or-nothing into an empty registry and never merges.
3. **No restoration of intentionally removed projects.** Every commit writes `projects.json`, and the only operation that removes it, `reset`, first keeps a `projects.json.corrupt-*` backup. So a registry that ever held a project leaves one of the two precedence artifacts behind. Removing one project, removing the last project (the file stays with an empty list), resetting a corrupt registry, and the import's own commit all keep them. A lock-only directory records no committed registry: it is reachable through an `add` that fails after taking the lock, a `remove` of a well-formed unknown id, or a crash before the first commit, so it cannot be hiding a removal and stays eligible. This rests on the behavior in the facts above, and I2 adds a test for each sequence. One limit remains: a user who manually deletes the new registry files while the legacy directory is still in place gets a fresh import, and the app cannot prevent that. If the existing artifacts turn out not to guarantee the rest, a durable marker is added as an explicit, reviewed change to the file table, not silently.
4. **Validation through the existing loader.** The legacy registry is read only through the existing read protocol and strict parser. Only a valid version-1 result is imported. Empty, corrupt, newer-version, unavailable, symlinked, FIFO, foreign-owned or over-permissive input is skipped. The user then sees an empty list and the documented fallback. No legacy path or content is shown in an error, and no repair or `reset` is ever applied to the legacy directory.
5. **Copy, never move.** The legacy directory and its files are never modified, renamed, locked for writing or deleted. The parent of the new data directory is never created (an existing invariant).
6. **Same write guarantees.** The import commits through the existing locked, synced, atomic write path. Records are carried as validated, with `id` and `next_id` preserved. Per-record folder checks (missing or changed folder) continue to apply at the moment of use.
7. **Eligibility is decided under the new directory's lock, in a fixed order.**
   1. Read the legacy registry through the existing read protocol. It needs no lock and creates nothing. Anything other than a valid version-1 registry ends the procedure here, and the new data directory is neither created nor touched.
   2. Optionally pre-check the new directory without a lock and without creating anything. If `projects.json` or a `projects.json.corrupt-*` is present, stop. This is a fast path only and is not authoritative.
   3. Take the new directory's lock, creating the directory if absent exactly as the existing write path does. This creates `projects.lock`, which is why that file cannot be a precedence signal.
   4. **While holding the lock**, check eligibility again. If `projects.json` or any `projects.json.corrupt-*` is present, release the lock and skip. A lock-only directory passes this check.
   5. Commit the legacy records through the existing atomic write path, which also removes a stale `projects.json.tmp`, then release the lock.

   Because the decisive check in step 4 happens under the lock, two instances racing produce exactly one import and one skip, and an `add` that commits first wins: its `projects.json` makes the import skip. The scan for `projects.json.corrupt-*` needs a directory read that the existing read protocol does not perform. I2 adds it with the same trust checks first (the directory is not a symlink, is owned by the effective user and is not group- or world-writable) and treats any unexpected entry type as present.
8. **Scope.** macOS builds only. Other platforms keep `UnsupportedPlatform`.
9. **Fallback.** If the import is not approved or a legacy registry fails validation, the documented fallback is to re-add projects with the folder picker. Registry contents are paths and folder identities only, so nothing else is lost. Release notes say so.

**Tests I2 must add** (none exists today): import from a valid legacy fixture; skip when the new registry exists; the three removal and reset sequences above, each followed by a relaunch; a hostile legacy directory (symlink, foreign owner, group- or world-writable, FIFO, oversize, corrupt, newer version); the legacy directory unchanged byte for byte and in listing before and after; no new command in the manifest. Specific to the lock-only and concurrency rules:

- **Lock-only directory imports.** A new directory containing only `projects.lock` (the state left by a `remove` of an unknown id or a post-lock `add` failure) imports a valid legacy registry. The same holds with a stale `projects.json.tmp` added.
- **Each lock-only source.** Launch after a `remove` of a well-formed unknown id, and after an `add` whose commit fails before the rename (using the existing fault-injection steps), which also leaves a stale `projects.json.tmp`, against a legacy registry: both import.
- **Precedence signals.** `projects.json` present as a regular file, as a symlink, as a FIFO or as a directory each skips the import. A directory with only `projects.json.corrupt-*` skips it.
- **Valid empty registry.** An existing `projects.json` with an empty project list skips the import, and the legacy projects are not restored.
- **Concurrent importers.** Two instances started together against the same eligible directory produce exactly one commit: the registry equals the legacy records once, with no duplicates and no partial file, and the other instance skips.
- **Import against a concurrent `add`.** If the `add` commits first, the import skips and the added project survives. If the import commits first, the `add` appends to it. Neither overwrites the other.
- **No directory without legacy data.** With no valid legacy registry, the new data directory is not created.

## Evidence provenance

Every measured figure in this record was produced by its author (the implementation agent) on 2026-10-28 and has **not** been independently reproduced. A reviewer should re-derive the ones that matter.

| Figure or claim | Source and method | Caveats |
| --- | --- | --- |
| 813 tests passed, 4 skipped | `python3 -m unittest discover -s tests -t .`, run locally on macOS with Python 3.14.8 on a clean tree at `e0d2778`; about 175 s | One run on one machine. This is separate from CI: the `Gate` job's own test count was not extracted. |
| 101 tests | The five suites that read `docs/`: `tests.test_probe_artifact_hygiene`, `test_cli`, `test_skills_cli`, `test_vault_context`, `test_invocation_runtime` | Local runs by the author. |
| `Desktop` job about 14 minutes | `gh run view` on the `main` run 38505524133 (2026-10-14), rounded down to whole minutes | One run. |
| `Gate` job times | `gh run view` on one `main` run: macOS packaging 31 s, Windows installer smoke 78 s, Linux runtime gate 177 s. Four runs observed on this PR's branch: macOS 20–44 s, Windows 47–55 s, Linux 172–187 s | Computed from start and completion timestamps. A handful of runs, not a statistical sample. |
| Dollar figures | The author's arithmetic: measured minutes times the per-minute rates on GitHub's billing page | **Not** checked against the account, its plan, an invoice, per-job rounding, or the runner size behind `macos-latest`. |
| Repository state (stars, forks, collaborators, secrets, protection, private vulnerability reporting off, and similar) | Read-only `gh api` calls using the maintainer's token | Changes over time. Re-read at R1 and R2. The account plan could not be read. |
| File counts and statuses in the origin table | `git diff --name-status -M 3cfce13 origin/main` at `e0d2778` | Reproducible with that command. |
| `git grep` counts (63 tracked files mention `synapse`; 15 owner-name URL occurrences in 10 files; 18 owner-name occurrences in total) | `git grep -il synapse`, and the two owner-name patterns in the runbook's documentation URL audit, at `e0d2778` | Reproducible. The URL audit pattern is case-insensitive and broader than the first revision's. |
| Category counts for the removed and remaining probe files | `git diff --name-only --diff-filter=D` and `git ls-tree`, filtered by directory kind, at `e0d2778`; counts only, with names and contents deliberately not printed or read | Metadata only. Contents were not inspected, so nothing here says whether any file holds a credential. |
| Quotations from GitHub and Tauri documentation | Fetched through a summarizing web-fetch tool, which returns excerpts or paraphrase and not necessarily verbatim page text | Re-read the cited pages before relying on any quoted sentence. The "not established" rows are where the pages read were silent. |


## Platform assumptions

Documented here so new development stays portable. **No platform other than macOS has been built or verified for the desktop app.** CI builds it on `macos-latest` only (`desktop.yml`). Statements below are labelled as either *source* (read from the code) or *evidence* (a CI job or a run). Source-based expectations are not build evidence.

### macOS-only

- `rfd` (native folder panel) is a dependency only under `cfg(target_os = "macos")` in `desktop/src-tauri/Cargo.toml`.
- Desktop bundle targets are `app` and `dmg` (`tauri.conf.json`). `desktop/scripts/verify-release-isolation.sh` uses `PlistBuddy`, `otool`, `lipo` and `nm` and inspects `Info.plist` and `Contents/MacOS/`, so release isolation is verified on macOS bundles only. `desktop/scripts/prepare-app-icon.swift` is a Swift script (not read in detail here).
- The current UI copy and shortcuts assume macOS conventions in places. The design exports show macOS window chrome and `⌘` shortcuts. Windows and Linux need `Ctrl` mappings, native window controls and neutral copy such as "Stored on this Mac".

### Conditional-compilation boundaries in the desktop Rust code

These are read from the source (*source*), and they are deliberate. The unconditional `std::os::unix` and `libc` use is **inside modules that are compiled only on macOS** or inside test code.

| Boundary | Where | Effect on other targets |
| --- | --- | --- |
| Registry and folder modules | `desktop/src-tauri/src/projects.rs:12-15` (`#[cfg(target_os = "macos")] mod folder;` and `mod registry;`). Their `std::os::unix` and `libc` use (`getpwuid_r`, `geteuid`, `flock`, `O_NOFOLLOW`, `O_DIRECTORY`, modes `0o600` and `0o700`) lives here. | Not compiled. Every project command (`list`, `add`, `remove`, `reset`) returns `UnsupportedPlatform` ("Local projects are not supported on this platform."). |
| Native folder panel | `projects.rs`, `NativePicker::pick`: the `rfd` branch is `#[cfg(target_os = "macos")]`, with an explicit `#[cfg(not(target_os = "macos"))]` branch that returns no selection | A stub that cannot add a project. |
| Development runtime bridge | `desktop/src-tauri/src/runtime_bridge.rs:9`: `#[cfg(any(test, all(feature = "dev-runtime", target_os = "macos")))]` gates the process code in `runtime_bridge/process.rs`, which uses `std::os::unix` and `libc` (`geteuid`, `sysconf`, `fcntl`, `process_group`, `pre_exec`) | Not compiled in a default build, and the feature is development-only. It **is** compiled under `cfg(test)` on every target. |
| Test code | `desktop/src-tauri/src/lib.rs:613` (`std::os::unix::fs::PermissionsExt` inside `#[cfg(test)]`); registry tests use `mkfifo` and `PermissionsExt` | Unix-only. |
| `libc` dependency | `desktop/src-tauri/Cargo.toml:19` | Declared unconditionally. Whether it matters on Windows has not been checked. |

**What the source supports, and what it does not.**

- *Source expectation:* a **default, non-test** build is designed to compile on non-macOS targets, with project registration and the development bridge compiled out. This is an expectation from reading the code. It has **not** been confirmed by building.
- *Source expectation:* `cargo test` on a non-Unix target is expected not to compile, because test builds include `process.rs` and the unix test code above.
- *Not established:* whether the rest of the app (Tauri, the frontend, `build.rs`) builds, runs or behaves correctly on Windows or Linux. No Windows or Linux desktop job exists and none was run for this record.
- A Windows or Linux implementation of local project registration is a design task, not a stub: the registry's safety properties (refusing untrusted or foreign-owned directories, no symlink following, advisory locking) are implemented with the Unix primitives above, and equivalent protections would need to be designed and reviewed.

### Python runtime

- Already cross-platform in part: `runtime/platform.py` shims `os.getuid`, `os.killpg` and `.cmd` spawning for Windows, and `gate.yml` runs a Windows installer smoke job (`tests.test_platform_helpers`, `tests.test_acp_transport`, packaging and CLI smoke).
- Not portable: the session daemon and client (`runtime/session/daemon.py`, `client.py`) use `socket.AF_UNIX`. `runtime/probe_authorization_ledger.py` calls `os.geteuid()`, opens files relative to a directory descriptor (`dir_fd`) and enforces `0o700`/`0o600` modes, none of which has the same meaning on Windows (`os.geteuid` does not exist there). `runtime/vault_context.py` opens files with `os.O_NOFOLLOW`, which Windows does not define, and `runtime/codex_probe_runner.py` checks `0o700` scratch directories. Their Windows behavior is unverified.
- Process-group termination falls back to `terminate`/`kill` on Windows, with weaker guarantees than `killpg`.

### Evidence ledger

*Evidence* means a CI job or a run that exercised the thing. *Source* means read from the code only.

| Component | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Desktop app build and tests | **Evidence:** `Desktop` job on `macos-latest`, passing on `e0d2778` | None. Source: default build designed to compile (see above), `cargo test` expected not to. | None. Source: same as Windows for the build; Unix primitives exist, but the registry stays compiled out. |
| Desktop frontend (lint, typecheck, tests, build) | **Evidence:** the same `Desktop` job | None | None |
| Release-isolation and boundary verification | **Evidence:** `Desktop` job on a real bundle | Not applicable: macOS-only tooling | Not applicable |
| Python runtime, full unittest suite | **Evidence:** a full local run on this working tree (813 passed, 4 skipped; run by this record's author, see [Evidence provenance](#evidence-provenance)) | None | The `Runtime G/H gate` job runs on `ubuntu-latest`. Its coverage was not audited for this record. |
| Python CLI packaging and install | **Evidence:** `Packaging smoke (macOS)` | **Evidence, narrow:** `Installer smoke (Windows)` runs `tests.test_platform_helpers`, `tests.test_acp_transport`, `npm pack` and install, and `mco --help`, `--version`, `skills read` and `install --dry-run`. It does not exercise the session daemon, the authorization ledger, vault context or probe modules. | None beyond the gate job |
| Installers | `app` and `dmg` are configured. CI builds `--bundles app` only (`desktop.yml:102,117,125`), so there is no CI evidence for the `dmg`. | None configured | None configured |

No platform is declared supported on the strength of any row above (D9).

## Repository visibility evaluation

**Nothing is changed by this document. Visibility changes need a separate, explicit authorization.**

### Current state (read-only queries)

As read on 2026-10-28 by this record's author: public; `isFork: false`; 0 stars, 0 watchers, 0 forks; one collaborator (the owner); 5 open issues; no tags or releases; no other open pull request than this record's own while it is under review; no GitHub Pages, wiki, discussions, webhooks, Actions secrets or environments; no branch protection or rulesets; license MIT; no topics. The account plan was not readable with the available token scope.

### Consequences of public to private

Sources: [GitHub: setting repository visibility](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/managing-repository-settings/setting-repository-visibility) and [GitHub billing for Actions](https://docs.github.com/en/billing/managing-billing-for-your-products/managing-billing-for-github-actions/about-billing-for-github-actions).

| Area | Consequence | Impact here |
| --- | --- | --- |
| Public access | The page read describes the opposite direction (private to public: code visible to anyone). The expected inverse is that non-collaborators lose access to code, issues and pull requests and that existing links stop working for them. That is an inference, not a quoted statement. Anyone with an existing clone keeps that copy. | Low. No known downstream consumers: nothing is published to npm or any registry from this repository (no tags, releases or packages were found). |
| Objects already distributed | A visibility change controls future access through GitHub. It does **not** revoke Git objects that were already fetched, mirrored, archived or cached, and it has no effect on the same history in upstream's public repository. | **Making the repository private is not containment for [SEC1](#security-exposure-probe-artifacts-in-public-git-history-sec1).** The repository is public now, so the whole history, including the artifacts SEC1 covers, is available today. |
| Stars and watchers | Erased, per the page. | None (0 and 0). |
| Forks | Public forks are detached into a separate network and stay public. | None (0 forks). |
| Contributors | Only invited collaborators can read or push. | None today (owner only). |
| GitHub Pages | A published site is unpublished on Free plans. | None (no Pages). |
| Actions cost | Standard runners are free for public repositories. For private ones usage is billed against the plan's included allowance. The billing page lists GitHub Free as "$12 USD" of included compute per month, expressed as money and not minutes, with per-minute rates of $0.006 (Linux, 2-core), $0.010 (Windows, 2-core) and $0.062 (macOS, 3- or 4-core). Per-job rounding, and whether `macos-latest` matches the listed runner size, were not checked. | **Material, and an estimate.** The account plan could not be read, so the allowance is an assumption.<br>Measured on recent `main` runs: `Desktop` (macOS) ≈14 min, ≈$0.87 at the macOS rate; `Gate` ≈0.5 min macOS + 1.3 min Windows + 3 min Linux, ≈$0.06.<br>**Both workflows fire on both `push` and `pull_request`.** `gate.yml` has no path filter and `desktop.yml` filters on `desktop/**`, `runtime/**` and a few other paths on both events, so a pushed branch with an open PR runs each twice (this PR shows two `Gate` runs).<br>Estimated cost per push to a PR branch: ≈$0.13 without changes under `desktop/**` or `runtime/**`, and ≈$1.9 with them. At $12 that is on the order of 6 pushes per month with desktop-affecting changes, or about 95 docs-only pushes. Treat these as order-of-magnitude figures; they are the author's arithmetic from the rates on the billing page and were not checked against the account (see [Evidence provenance](#evidence-provenance)). |
| Branch protection, rulesets, required reviews | Availability on a private repository under a Free personal account is **not established**. The plans page lists some such tools among paid plans' private-repository features, but the branch-protection documentation read does not state plan or visibility availability for personal accounts. | Nothing is configured today (verified: `main` is unprotected, 0 rulesets), so nothing is lost now. If they are unavailable, D10's review requirement is a process rule only. If they are available, a single-collaborator repository may still be unable to satisfy a required review (not established). |
| Security features and reporting | The page read says custom Dependabot alert rules are disabled and code scanning is unavailable on a private repository unless GitHub Code Security is enabled, and that dependency graph and alerts stay on. Private vulnerability reporting is documented for public repositories only. | Low for Dependabot and scanning: nothing relies on them today (Dependabot security updates are disabled and secret scanning with push protection is enabled, per the API). For reporting, see the [security-reporting facts](#security-reporting-and-ownership-facts-checked-for-this-record). |
| Webhooks, integrations | None configured. | None. |
| Upstream attribution | Unaffected: the history and license are unchanged. | None. |
| Going public later | Code and Actions history and logs become visible to everyone; anyone can fork; stars and watchers reset; push rulesets are disabled. | **Pre-publication review is required**, including history, issue text, Actions logs and the contents of any private material committed while private. Nothing is committed that the maintainer would not want public. This repository is **already public**; the history question that raises is SEC1, which does not wait for this. |

### Recommendation

The audience-related costs are small because the repository has no audience yet (0 stars, watchers and forks). The real trade-off is CI cost, possible loss of review enforcement and a changed security-reporting channel, against the benefit of developing the private website and desktop design in one repository. The CI cost and the protection availability are unverified for this account.

Before authorizing, the maintainer should confirm: the GitHub plan; the Actions allowance and the cost estimate above; whether private-repository branch protection is available and needed; how security reports will be received while private; and the plan for a pre-publication history audit. If the change is authorized, make the repository private **before** the first private asset or private-derived implementation is committed, and perform the rename in the same maintenance window or immediately after.

SEC1 is independent of this decision and does not wait for it. Until then, the roadmap blocks `web/` and the desktop redesign (anything derived from the private designs), and allows only changes that contain no private material.

## Repository rename runbook and execution record

Destination `RaioViajante/ordelis`. Reference: [GitHub: renaming a repository](https://docs.github.com/en/repositories/creating-and-managing-repositories/renaming-a-repository).

**What GitHub documents.** The web URL, issues, stars and followers are redirected, and `git clone`, `fetch` and `push` against the old location keep working "as if made on the new location". GitHub Pages project URLs and actions hosted in the renamed repository are **not** redirected (this repository has no Pages site and hosts no action, and no workflow references `RaioViajante/synapse`). Creating a new repository with the old name breaks the redirect. **What the page does not say:** whether the REST API, `gh`, webhooks or the security-advisory URL follow the redirect, and it does not list pull requests explicitly. The checks below verify these instead of assuming them.

**Preconditions**

1. Explicit maintainer authorization for the rename.
2. H1 and H2 merged. H2 has no dependency on the rename (see the roadmap), so there is no circularity. H3 and the post-rename security-link check come after.
3. **Open pull requests are allowed and are not a blocker.** A rename preserves pull requests (D3), and their head branches live in the same repository. They are recorded in the baseline below and re-checked afterwards.
4. No workflow run is in progress at the moment of the rename (`gh run list --status in_progress` is empty), so no run straddles the rename.
5. The rename does not depend on the visibility decision. Visibility is a separate authorization (R1), scheduled first in the roadmap. Either order works technically.

**Baseline to capture before the rename** (read-only, saved outside the repository):

- `gh pr list --state all` with number, state, head ref, head SHA and base ref.
- The full ref listing from `git ls-remote origin`, including `refs/heads/*`, `refs/tags/*` and `refs/pull/*`, with SHAs.
- Issue counts by state, the commit count on `main`, and `gh repo view` output for name, visibility, fork flag, default branch, description and topics.
- The latest check status for `main` and for each open pull request head.
- Every local clone and worktree, with its `origin` URL.

**Steps (maintainer or authorized agent)**

1. Settings, General, repository name: `ordelis`.
2. In **each** local clone and worktree: `git remote set-url origin github-personal:RaioViajante/ordelis.git`, then `git fetch --all`. The `github-personal` host alias is a local SSH configuration; keep whichever alias that clone already uses.
3. Update the repository description and topics. Keep "forked from MCO" or equivalent attribution in the description.
4. **Do not edit documentation links in this step.** Verify them with the documentation URL audit below. Any change it shows to be necessary goes to N1; none is expected, because the three READMEs contain no repository URLs.
5. **Never create a repository named `synapse`** under this account.

**Post-rename checks.** Each must pass or be recorded with its result.

| Check | How | Expected |
| --- | --- | --- |
| Repository state | `gh repo view RaioViajante/ordelis` | New name; visibility, fork flag, default branch and counts match the baseline |
| Web redirect | `curl -sI` on the old repository URL | A redirect to the new URL |
| Git through the old URL | `git ls-remote` on the old HTTPS and SSH URLs | Works and lists the same SHAs |
| API through the old name | `gh api repos/RaioViajante/synapse` | **Record the result**; not documented on the page read |
| Refs | `git ls-remote` on the new URL compared with the baseline | Every head, tag and `refs/pull/*` entry present with an identical SHA |
| Pull requests | `gh pr list --state all` compared with the baseline | Same numbers, states, head and base refs, head SHAs, comments and review requests |
| Pushing to a PR branch | An ordinary push (no force) to an open PR branch, only when there is real work to push | Accepted, and the PR updates |
| CI | The next ordinary push, or a re-run on an open PR head | `Gate` passes; `Desktop` passes when its paths change; no workflow still references the old name (`git grep RaioViajante/synapse` over `.github`) |
| Remotes | `git remote -v` and `git fetch --all` in every clone and worktree | New URL; no redirect warnings |
| Security link | Request the advisory URL under the new name | **Record the result**; H3 acts on it |
| Documentation URLs | `git grep -niE -e 'raioviajante[/:]synapse' -e 'synapse\.git' -e 'github\.com[:/]+raioviajante' -e 'github-personal' -- . ':!docs/migration/ordelis.md'` (case-insensitive; covers HTTPS, SSH-style `git@github.com:` and `ssh://` forms, a `.git` suffix, raw and API URLs, and the local SSH alias), then `curl -sI` on each distinct HTTPS URL | At `e0d2778` this broadened pattern finds the same **15 occurrences in 10** `docs/implementation/*.md` files, all HTTPS issue links, covering 11 issues (#1, #17, #19, #24, #26, #28, #30, #36, #40, #45, #47). No SSH-style or `.git` form exists in tracked files, and `README.md`, `desktop/README.md` and `docs/README.md` contain none. **A second check**, `git grep -nioE 'raioviajante' -- . ':!docs/migration/ordelis.md'`, finds **18** occurrences: those 15 plus the three bundle-identifier references (`desktop/src-tauri/tauri.conf.json:5` and `docs/implementation/desktop-local-projects.md:28` and `:159`), which I2 handles. Each issue link must redirect and stay in the preserved records. **Any other hit** (a clone URL, badge, install instruction or raw-file link) is a living-document defect, and N1 fixes it. |
| Counts | Issues, pull requests, commits on `main` | Equal to the baseline |

Text that earlier bot comments wrote with the old name stays as history and needs no action.

**Rollback:** rename back in Settings. Redirects follow the rename in either direction, but any links created under the new name need updating, and the post-rename checks are repeated. No data is deleted by a rename. If a repository named `synapse` is created by accident, treat it as an incident: it breaks the redirect.

### Execution record (R2)

The runbook above was executed on 2026-10-29 (about 02:52 UTC) with a REST update of the existing repository's name. The measurements below were taken by the record's author and have not been independently reproduced.

- **Before the rename**, these were verified: repository identity, `main` (`a420a55`), a clean working tree, no active workflow runs, no open pull requests, a free destination name, CODEOWNERS (0 errors), private vulnerability reporting, `SECURITY.md` and the security link. A baseline of settings, permissions, labels, issues, pull requests and every ref was recorded.
- **After the rename**, the same inventory was regenerated and compared. Everything is identical (30 issues, numbered up to #56, 27 pull requests, 12 branches, 27 pull-request refs, 0 tags, 0 releases, settings, permissions, labels), apart from the repository description, which was then changed on request. `main` is still `a420a55`, and no commit was created.
- **Local remote:** `origin` was switched to the new name, keeping the same protocol and host alias.
- **Redirects that are reliable:** the repository, pull requests, commits, trees, labels, and Git over SSH and HTTPS. The old security-policy link followed one redirect to a 200 in 24 of 24 samples, with the policy content arriving every time.
- **Old-name issue URLs depend on the client.** For browser-like requests, 30 of 30 redirected to the same issue number in each of the last two of three measured rounds (29 of 30 in the first). For a plain HTTP client (`Accept: */*`), only 4 to 7 of 30 did, and the failing set changed between rounds. The failing response is a bare text 404 from GitHub, and its cause is not documented.
- **Decision:** historical links to old-name issues are **not** rewritten. Browsers, which is how the rendered documents are read, are served correctly. No link checker exists in this repository. The behavior for plain clients is unexplained and may be temporary. A rewrite needs a separate justification, for example a persistent failure in browsers or the introduction of a link checker.
- **Documentation URL audit:** 18 occurrences in 12 files before H3, as expected (15 historical issue links in 10 `docs/implementation/*.md` files, the `config.yml` link and two test anchors). After H3 the `config.yml` link and the test anchors no longer use the old name, so the only old-name references left are the 15 historical issue links (count them with `raioviajante[/:]synapse`). The broad owner-name pattern above now also matches new-name URLs, such as the canonical security URL and the maintainer profile link in `SECURITY.md`, so it is no longer a measure of old-name references.

## Outstanding prerequisites

These are recorded, **not** assumed complete.

| Prerequisite | Needed by | Status |
| --- | --- | --- |
| **Private-content gate:** either R1 (the visibility change) is executed, **or** an explicit, recorded amendment to D2 approves a named alternative strategy | W1, U1 and later (anything derived from the private designs) | **Open.** R1 needs its own authorization. Any alternative that departs from a single monorepo needs the D2 amendment first. Neither is assumed. |
| GitHub plan, the Actions allowance and cost estimate, availability of private-repository branch protection, and how security reports will be received while private | R1 | **Open** |
| Approved vector master of the Ordelis symbol | Icons, favicon, installers, social images (U-series, P-series) | **Open.** The provisional reconstruction is not used for final assets. |
| Final consistency check of the canonical design against the approved design: cross-platform language, shared layout grid, responsive layouts, latest storytelling | W1 | **Open** |
| Confirm which foundations and components board is authoritative for the desktop app (the desktop export's or the web export's system board) | U1 | **Open** |
| Updates and telemetry statement for the website privacy page | W-series | **Open.** The app currently sends nothing and its CSP allows only IPC. |
| Decision on the sample project named `synapse` in the fixtures and the test that selects it | N1 | **Open** (see the identifier matrix) |
| Maintainer contact or channel to name in `SECURITY.md` | H3 | **Proposed in draft PR #60, not merged.** `SECURITY.md` would point to the contact details on the maintainer's GitHub profile, with no vulnerability details in a first message. The maintainer confirms that choice or supplies a dedicated address. |
| Final product-name check before the first Windows installer (the WiX upgrade code follows `productName`) | P-series | **Open** |
| **SEC1: security assessment of the probe artifacts in public history** (see the [section above](#security-exposure-probe-artifacts-in-public-git-history-sec1)). Not tied to publication. | Blocks any release or open-source publication (the gate is unchanged) | **Assessed on 2026-10-28; mitigated according to the maintainer's report, not independently verified.** Confidential closure documentation (contact date, confirmation status, residual risk) is pending. Historical Git data is unchanged. This record initiates no further outreach. |
| Plan for a pre-publication audit if the repository is later made public again (history, issue text, Actions logs, private material committed while private). It builds on SEC1's findings and does not replace SEC1. | Open-source publication | **Open** |

## Risk register

| # | Risk | Likelihood | Impact | Mitigation | Owner PR |
| --- | --- | --- | --- | --- | --- |
| 1 | Private design or website material lands in a public repository, including on a PR branch | Medium | High | Do not commit exports; the private-content gate must be satisfied first; review each diff for private material | W1, U-series |
| 2 | App data orphaned by an identifier change | Medium | Medium | Native-side, validated, copy-only import with the semantics above; documented re-add fallback | I2 |
| 3 | Safety-gate vocabulary or verification scripts renamed carelessly | Low | High | Trust-origin values preserved; verification scripts updated atomically with their negative tests; old and new development-environment names both kept in the isolation needles | I1 |
| 4 | Accidental npm publish under the upstream scope | Low today (no `NPM_TOKEN` exists), grows if one is added | High | Hard guard on `publish-npm.yml` | H1 |
| 5 | Security reports reach upstream, or no channel exists after the repository goes private | Medium | Medium | H2 repoints the link now while the repository is public; H3 re-validates the channel after R1 and R2; private vulnerability reporting is documented for public repositories only | H2, H3 |
| 6 | Attribution erosion | Low | High | Provenance requirements; `LICENSE` untouched; standalone `NOTICE` plus a README link | B1, N1 |
| 7 | Private-repository CI cost and possible loss of review enforcement | Medium | Medium | Both unverified for this account; confirm the plan first; the estimate counts `push` and `pull_request` triggers; keep path filters; review may be a process rule only | R1 |
| 8 | Windows or Linux claimed before verification | Medium | High | No support claims; download surfaces stay "not yet available"; the evidence ledger separates source expectations from evidence | P-series |
| 9 | A Windows or Linux project-registry port weakens the Unix-hardened guarantees | Medium | High | Local projects return `UnsupportedPlatform` off macOS by design; a port needs a separate reviewed design, not a stub | P-series |
| 10 | Redesign copies prototype HTML that violates the CSP (remote fonts, inline styles) | Medium | Medium | Rebuild components in React and Tailwind; keep `font-src 'self'`; CSP checks stay in CI | U-series |
| 11 | Security-sensitive screens (approve, run, merge, discard, terminal) look functional | Medium | High | Inert, labelled fixtures only; no new IPC commands; execution stays disabled | U-series |
| 12 | Flaky tests (issue #44) mask rename regressions | Low | Medium | Triage reruns explicitly; do not hide failures | all |
| 13 | A rename PR is large and unreviewable | Medium | Medium | One concern per PR as listed below | all |
| 14 | A new repository is created under the old name | Low | Medium | Runbook step 5 | R2 |
| 15 | Provisional icon art leaks into releases | Low | Medium | Block icon and installer work on the approved vector master | U-series, P-series |
| 16 | Renaming `productName` breaks the Desktop CI job, which hard-codes `bundle/macos/Synapse.app` | High if missed | Medium | N1 includes the `desktop.yml` paths and its acceptance requires the Desktop job to pass | N1 |
| 17 | The registry import restores projects the user deliberately removed, or fails to import because of a lock-only directory | Medium | Medium | Precedence keyed only on `projects.json` and `projects.json.corrupt-*`; the decisive check runs under the lock; a test for each remove, reset, lock-only and concurrency sequence | I2 |
| 18 | A blind find-and-replace changes sample data, test inputs or security vocabulary | Medium | High | The identifier matrix classifies each site; N1 ends with a grep audit against an explicit allowlist of preserved items | N1, I1 |
| 19 | A source-based platform expectation is mistaken for build evidence (this record's first revision did exactly that for Windows) | Medium | Medium | The evidence ledger; the first P1 step adds Windows and Linux compile jobs and replaces expectations with results | P1 |
| 20 | **Probe artifacts in this public repository's git history may expose credentials or sensitive environment data** (Sets A to C in SEC1). Upstream's public history carries the same objects. | Assessed on 2026-10-28; mitigation reported by the maintainer, not independently verified | **High** at assessment; mitigated according to the maintainer's report, closure documentation pending | **SEC1.** Confidential closure documentation is pending (contact date, confirmation status, residual risk). Historical Git data is unchanged, so no public text names the artifacts. Any further credential action goes through its owner, and any notice to upstream uses a private channel and needs separate authorization. Making the repository private does not contain this. | SEC1 |

## PR roadmap

One concern per pull request. Every PR carries: acceptance criteria, local verification, CI evidence, an independent review before merge, and a rollback note. None changes the safety gate's behavior. Order follows the maintainer's sequence (record, release and ownership hygiene, visibility and rename, branding, compatibility-sensitive identifiers and data, website, desktop design, cross-platform). Adjustments: SEC1 runs independently and in parallel from the start; H3 follows R1 and R2 so the security channel is validated against the final visibility and name; W1 and U1 wait for the private-content gate; N1 contains a workflow edit, as a separate commit.

| ID | Scope | Acceptance criteria | Verification | Rollback | Depends on |
| --- | --- | --- | --- | --- | --- |
| **B1** | This record. Docs only. | Adds `docs/migration/ordelis.md` and one index link. No code, workflow or asset changes. No private material. | Diff touches only the two docs files; links resolve; tables well-formed; `Gate` green; review of the amended head by someone other than its author | Revert the commit | none |
| **SEC1** | **Private security tracking and assessment** of the probe artifacts in public git history (the [section above](#security-exposure-probe-artifacts-in-public-git-history-sec1)). Not a pull request: a private tracking item plus maintainer actions. | A private tracking item exists and is not a public issue. Sets A to C are assessed for credentials and sensitive environment data in a controlled, private, full-history review. Every confirmed or reasonably suspected exposed credential is revoked or rotated by its owner. A confidential notice goes to upstream through its private security channel when warranted. The public record holds only a status line. | The private record; provider-side confirmation of revocation where possible; no sensitive content in any public place | Not applicable: assessment and rotation are not rolled back. Any remedial history change is a separate authorized decision. | **None.** It starts on authorization, independent of H1 to H3, R1 and R2. **Blocks** any release or open-source publication. R1 does not close it. |
| **H1** | Gate or disable `publish-npm.yml`; review `preview-package.yml` (its PR comment names the upstream package) | The workflow cannot run for this repository, with a comment explaining why. The upstream package name is untouched. `Gate` still green. | Inspect with a dry trigger; no publish step reachable; `Gate` green | Revert | B1 |
| **H2** | `CODEOWNERS` to `@RaioViajante`, keeping a header comment that credits the upstream MCO maintainers; repoint the issue-template security link at this repository using today's URL; **maintainer action:** optionally enable private vulnerability reporting while the repository is public | No path is owned by the upstream author. The link targets this repository. No claim is made about enforcement. | Review the file diffs; read the PVR setting back through the API | Revert; disable the setting | B1. **No dependency on R2.** |
| **R1** | **Maintainer action.** Visibility change, if authorized | Consequences and cost estimate re-confirmed at execution time; plan known; SEC1's status recorded (R1 neither contains nor closes SEC1) | `gh repo view` shows visibility; CI still runs | Make public again (note the consequences above) | Separate authorization |
| **R2** | **Maintainer action.** Rename to `RaioViajante/ordelis` per the runbook | Every post-rename check passes or is recorded, including the documentation URL audit; any needed documentation change is assigned to N1 | The runbook's baseline and post-rename checks | Rename back; repeat the checks | H1, H2, authorization. Open PRs do not block. |
| **H3** | Re-validate the security-reporting channel after R1 and R2; add `SECURITY.md` naming it; fix the issue-template link if the redirect failed | The channel is stated truthfully for the final visibility (private vulnerability reporting is public-only per GitHub's documentation) | The advisory-URL result from the runbook; read-back of settings | Revert | H2, R1 and R2 decided |
| **N1** | User-facing branding exactly as in the identifier matrix's user-facing rows: `productName` and title, **`desktop.yml` bundle paths** (own commit), usage text, `index.html` and package `description`, UI copy, the frontend assertions on that copy, native Rust strings, living how-to commands, READMEs, CLI help text and probe banner; add `NOTICE` and link it from the README; resolve the sample-project fixture decision | **The `Desktop` job passes with the `Ordelis.app` path.** A grep audit finds no `Synapse` on user-facing surfaces except the matrix's preserved items. Compatibility-sensitive identifiers (`synapse_observed`, `synapse_enforced`, `synapse_plan`, the executable and crate names, the environment names) are untouched. `NOTICE` exists and the README links to it. After R2, the documentation URL audit in the runbook shows no hit outside the 15 historical issue links. | `pnpm lint && pnpm typecheck && pnpm test`; `cargo test --locked`; the Python `unittest` suite; `Gate` and `Desktop` green; the grep audit output attached | Revert | R2 |
| **I1** | Atomic rename of the Cargo and npm packages, library, executable, `ORDELIS_DEV_*` build variables and the scripts that hard-code them | `verify-release-isolation` and `verify-native-boundary` pass on a real build, and their negative tests pass. The executable check is exact. **The isolation needle list contains both old and new development-environment names**, and the scrub tests cover both prefixes. | `test-verify-*.sh`; `pnpm tauri build --bundles app` followed by `verify-release-isolation.sh`; `cargo test --locked` | Revert | N1 |
| **I2** | Bundle identifier `app.ordelis.desktop` plus the registry import under the [semantics above](#registry-migration-semantics-i2), or the documented fallback | Every listed test passes, including a relaunch after each remove and reset sequence, the lock-only cases, and the concurrent-importer and import-versus-`add` cases. The legacy directory is unchanged. The command manifest, capabilities and permissions are unchanged. | New registry tests; `cargo test --locked`; a manual launch with a pre-existing legacy registry on macOS | Revert; the legacy data directory is untouched | I1 |
| **W1** | `web/` scaffold: Next.js with static export, TypeScript, own pnpm lockfile and path-filtered CI | Builds independently of `desktop/`; no private exports committed; no deployment | Build in CI; `Desktop` unaffected | Remove `web/` | Private-content gate; design consistency check |
| **W2 onward** | Website increments: layout grid, hero, motion, supporting pages, accessibility and reduced-motion checks | Responsive at the documented breakpoints; reduced-motion honoured; placeholders left visibly unfilled; no platform or download claims; no deploy | Ported browser checks; manual keyboard pass | Revert | W1 |
| **U1** | Desktop design system: tokens, light and dark themes, system-native font stack, JetBrains Mono for code | CSP unchanged; no remote fonts; contrast checks pass in both themes; no Apple font files; unused bundled fonts and their license text reconciled | Tests; contrast check; `Desktop` green | Revert | Private-content gate; N1 |
| **U2 onward** | Incremental desktop UI: shell and navigation, then screens as labelled fixtures | Existing tests pass; no new IPC commands; sample-data labelling intact; execution stays disabled | Tests; visual verification per `desktop-visual-verification.md` | Revert per PR | U1 |
| **U-icons** | Application and installer icons | Generated only from the approved vector master | Icon pipeline output inspected | Revert | Approved master |
| **P1 onward** | Cross-platform. **First step: add Windows and Linux compile jobs (default build, and tests separately) and record the real results, replacing this record's source-based expectations with evidence.** Then: a reviewed design for Windows and Linux project registration (until then they keep `UnsupportedPlatform`), CI jobs, packaging, signing, release metadata, and shipping `NOTICE` and `LICENSE` with any bundle that contains MCO-derived code | A platform is documented as verified only after its build, packaging and security behavior pass in CI and by hand. Download surfaces stay unavailable until real metadata exists. | Per-platform CI and manual checks | Revert per PR | I2; relates to issue #38 |

**N1 may need to be split.** N1 is already broad: application strings and tests, native Rust strings, the `desktop.yml` bundle path, usage text, `NOTICE`, the guides' prose, CLI help and the probe banner, and the URL audit. If it becomes too broad to review as one pull request, it may be split into smaller ones, for example (a) application identity: `productName`, window title, UI copy, native strings, the frontend and Rust assertions, and the `desktop.yml` and verification-script paths; (b) documentation: READMEs, guides, `NOTICE`, "formerly Synapse" notes and the living how-to commands; (c) runtime text: CLI help and the probe banner. Two constraints hold for any split: the `productName` change and the `desktop.yml` bundle paths must land in the same pull request, or the Desktop job fails; and each UI string must change in the same pull request as the frontend assertions that check it. Each part keeps CI green on its own and has its own acceptance criteria and independent review. This record neither performs nor schedules the split.

Existing open issues that interact with this plan: #38 (release packaging assurance, anchors P1), #44 (flaky ledger consumers), #39 (runtime import hygiene), #23 (session readiness race).

## Explicitly out of scope

Application changes; renaming any inherited MCO interface; enabling agent execution; deploying the website or connecting `ordelis.app`; publishing any release; telemetry or accounts; committing private design exports; creating a new organization.

## Revisions

- **First revision.** The initial record.
- **Second revision.** Corrections made after a review of the first revision: the Windows compilation statement was replaced by the actual conditional-compilation boundaries and an evidence ledger; the identifier matrix and N1 scope were completed with the CI, test, Rust-string and fixture couplings; the H2 and R2 ordering was resolved by splitting out H3; the rename runbook no longer treats open pull requests as a blocker and gained baseline and post-rename checks; registry migration semantics were specified; visibility, billing and protection claims were qualified, with both CI triggers counted; the privacy prerequisite was reconciled with D2; and attribution was specified as a standalone `NOTICE` plus a README link.
- **Third revision.** A focused correction after a second review: the registry-import specification no longer contradicts itself (`projects.lock` is not a precedence signal, and eligibility is decided under the lock in a fixed order); lock-only and concurrency tests were added to I2; the identifier matrix now names every tracked file that mentions `synapse`; the origin table is complete by status and records the 26 files removed from tracking; the runbook's documentation step now matches where repository URLs actually are, with a concrete audit; and every measured figure is attributed in a new Evidence provenance section.
- **Fourth revision.** A final narrowly scoped correction after a third review: a new SEC1 section and risk-register entry track the probe artifacts in public history as a current exposure, with containment, assessment, rotation and confidential-notification expectations and no sensitive detail; the registry text now gives the actual lock ordering, the real sources of a lock-only directory, and states that an existing `projects.json` is authoritative even when its project list is empty; the matrix covers `docs/README.md` and the registry design record and fixes the fixture-comment rows; the documentation URL audit is case-insensitive and covers SSH-style forms; the origin table lists the remaining inherited files; and the possible split of N1 is recorded.
- **Fifth revision (H3).** Brought the record up to date after H1, H2 and the rename: a current-status section, the SEC1 status line (assessed, mitigated according to the maintainer's report, closure documentation pending, no sensitive detail), and an execution record for the rename, including the old-name issue redirect measurements and the decision not to rewrite historical links. No decision, plan or gate was changed.
- **Fifth revision, correction pass.** Removed the remaining contradictory SEC1 status wording from the open-item table and the risk register, separated what is implemented in draft PR #60 from what is on `main`, and corrected the number of issues that existed at the rename to 30.
