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

**Platform-native behavior.** Window controls, menus, keyboard shortcuts, file dialogs, paths and permissions follow each platform's conventions. Copy that assumes one system, such as "this Mac" or `⌘`, is replaced by neutral wording or platform mappings. The interface already switches `⌘` for Ctrl on other systems.

**Portable configuration.** User-facing settings and project definitions are meant to be portable across machines where paths allow. Anything tied to a platform's security model, such as private state directories and file permissions, is not.

**Installer packaging.** Each platform gets its own package. The bundle identity, product name and upgrade identity must be settled before the first Windows installer ships, because they affect upgrades.

**Release verification.** Each release is verified per platform: build, package, and the security checks that apply, including verifying that a release bundle contains no development runtime. A platform is described as supported only after that passes in CI and by hand.

**Versioning and updates.** Versioning and any update mechanism are not designed. Updates are a security-sensitive feature and need their own integrity design before they exist.

**Accessibility.** Keyboard operation and assistive-technology support are required on every platform and are verified per platform.

**Platform-specific security guarantees.** Where a protection is stronger on one system, the documentation says so. A guarantee is never stated for a platform it was not verified on.

## What has been verified

| | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Desktop app builds and tests pass in CI | Yes | No | No |
| Desktop app launched and inspected | Yes | No | No |
| Release-bundle isolation verified | Yes (macOS bundles only) | Not applicable yet | Not applicable yet |
| Desktop installer or disk image published | No | No | No |
| Python runtime test suite | Yes (CI and local) | Narrow smoke test of the CLI wrapper only | Runtime gate runs in CI; coverage not audited |

"Built" and "tested" mean the repository's own CI jobs. See the [evidence ledger](../migration/ordelis.md#evidence-ledger) for what each job covers. Source-based expectations about Windows and Linux in that record are expectations, not results.

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
