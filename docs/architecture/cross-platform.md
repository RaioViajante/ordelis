# Cross-platform targets and distribution

**Status:** macOS is the only platform on which the desktop application has been built and launched. Windows and Linux are intended targets and are unverified. Nothing has been released.
**Scope:** which platforms Ordelis aims at, what has actually been verified, what differs between platforms, and what distribution would require. The detailed evidence ledger lives in the migration record and is linked rather than copied.
**Related:** [Features K](../product/features.md#k-cross-platform-distribution) · [Migration record: platform assumptions](../migration/ordelis.md#platform-assumptions) · [Desktop architecture: platform support](../implementation/desktop-architecture.md#platform-support) · [Security](security.md)

## Goals

- Run on macOS, Windows and Linux, behaving natively on each.
- State support only for what has been built, run and checked.
- Keep configuration portable where that is safe, and platform-specific where it must be.

## Intended behavior

**Targets.** macOS, Windows and Linux.

**Platform-native behavior.** Window controls, menus, keyboard shortcuts, file dialogs, paths and permissions follow each platform's conventions. Copy that assumes one system, such as "this Mac" or `⌘`, is replaced by neutral wording or platform mappings. Today the interface does not detect the operating system: its page shortcuts accept either `⌘` or Ctrl on every system, and Settings lists both labels (`⌘1 · Ctrl+1`). Labels that match the platform are planned.

**Portable configuration.** User-facing settings and project definitions are meant to be portable across machines where paths allow. Anything tied to a platform's security model, such as private state directories and file permissions, is not.

**Installer packaging.** Each platform gets its own package. The bundle identity, product name and upgrade identity must be settled before the first Windows installer ships, because they affect upgrades.

**Release verification.** Each release is verified per platform: build, package, and the security checks that apply, including verifying that a release bundle contains no development runtime. A platform is described as supported only after that passes in CI and by hand.

**Versioning and updates.** Versioning and any update mechanism are not designed. Updates are a security-sensitive feature and need their own integrity design before they exist.

**Accessibility.** Keyboard operation and assistive-technology support are required on every platform and are verified per platform.

**Platform-specific security guarantees.** Where a protection is stronger on one system, the documentation says so. A guarantee is never stated for a platform it was not verified on.

## What has been verified

Evidence is kept in two groups, because they do not mean the same thing. **CI evidence** is what the repository's own workflows ran, on the runner named. **Local verification** was run by hand on one developer machine and is not repeatable by a reader from the repository alone.

### CI evidence

| Runner | Workflow job | What it runs |
| --- | --- | --- |
| macOS (`macos-latest`) | `Desktop` ([`desktop.yml`](../../.github/workflows/desktop.yml)) | Frontend lint, type check, tests and build; the native Rust tests (default, development-runtime and release profiles); the targeted Python test `tests.test_runtime_status_bridge`; debug and release `.app` builds; native-boundary and release-isolation verification. |
| macOS (`macos-latest`) | `Packaging smoke (macOS)` ([`gate.yml`](../../.github/workflows/gate.yml)) | `npm test`, skill-format and version-alignment checks, and an npm packaging and install smoke test of the inherited CLI wrapper. It does not run the Python test suite. |
| Ubuntu (`ubuntu-latest`) | `Runtime G/H gate` | The **complete Python `unittest` suite** (on Python 3.11), the Node tests, skill and version checks, and the npm packaging smoke test. This is the only CI job that runs the full Python suite. |
| Windows (`windows-latest`) | `Installer smoke (Windows)` | `npm test`, the targeted Python tests `tests.test_platform_helpers` and `tests.test_acp_transport`, skill and version checks, and an npm pack, install and `mco` smoke run (`--help`, `--version`, `skills read`, `install --dry-run`). |

What this does and does not show:

- The desktop application is built and tested in CI on **macOS only**. No Windows or Linux desktop job exists.
- The Windows job exercises the CLI wrapper's packaging and two targeted Python modules. It does not run the session daemon, the authorization ledger, vault context or probe modules, and it says nothing about the desktop application.
- The Ubuntu job runs the full Python suite for the inherited runtime, but it does not build or test the desktop application.
- CI builds the macOS `.app` bundle only. No disk image or installer is built or published, and no job signs or notarizes anything.

### Local verification

| Machine | What was run | Not CI |
| --- | --- | --- |
| macOS developer machine | The complete Python suite (Python 3.14.8; 852 tests, 4 skipped), the desktop frontend checks and the native Rust tests, and launching the application for visual verification | Yes |
| Windows, Linux | Nothing | |

Local runs are recorded to show that the work was exercised, not as platform support. A platform is described as supported only when the build, packaging and security checks pass in CI and by hand on that platform.

## Known platform differences

- **Project registration** is compiled only on macOS. On other targets every project command returns an unsupported-platform error. A Windows or Linux version is a design task, because its protections use Unix primitives (ownership checks, symlink refusal, advisory locks) that need equivalent, separately reviewed designs.
- **The development runtime bridge** is macOS-only and development-only.
- **Release isolation tooling** relies on macOS utilities and inspects macOS bundles only.
- **The Python runtime** is partly portable. Some modules assume Unix features, such as Unix-domain sockets, `geteuid`, directory file descriptors and `O_NOFOLLOW`, whose Windows behavior is unverified. Process-group termination is weaker on Windows.
- **Fonts and window chrome** need a pass on each system. The current interface uses bundled fonts and the native macOS title bar.

## User interactions

None directly. The user-visible effect is what the documentation and download surfaces say about support. They state that support is unverified for Windows and Linux, and no download exists.

## Security implications

- Confinement mechanisms differ by operating system, so equivalent security cannot be assumed across them. See [Security](security.md#what-does-not-exist-yet).
- Code signing and notarization are not configured. Until they exist and are verified, nothing should be described as a trustworthy installer.
- Permission and ownership checks on private state do not translate directly between systems.

## Constraints

- The desktop uses Tauri 2, which targets all three desktop systems, but the project has not exercised the other two.
- Windows and Linux CI jobs for the desktop do not exist yet. Adding compile jobs and recording real results is the first planned step in phase 10 of the [roadmap](../product/roadmap.md#10-cross-platform-verification-and-distribution).

## Current implementation

- *Working.* A macOS CI job that lints, type-checks, tests and builds the interface, runs the native tests, builds app bundles, and verifies a release bundle holds no development runtime.
- *Not present.* A Windows or Linux desktop build, any installer, a signed artifact, an update mechanism, a published release.

## Open questions

- **Packaging formats** on each system, and signing and notarization arrangements.
- **Private state locations** on Windows and Linux.
- **How updates are delivered and verified,** or whether there are any.
- **Whether the Python runtime ships inside the application** and how its platform differences are handled. See the open decision in the [overview](overview.md#open-decisions).
- **Linux variety:** which distributions and display servers are in scope.
