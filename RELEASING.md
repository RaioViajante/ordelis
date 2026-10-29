# Releasing Ordelis

**Status:** Ordelis has no approved production release and no published installation package. There are no releases or tags, and nothing is published from this repository.

This page used to hold the upstream MCO guide for publishing `@tt-a1i/mco` to npm. That guide describes a different project and is archived at [docs/archive/mco-v0.12.1/RELEASING.md](docs/archive/mco-v0.12.1/RELEASING.md) for provenance only.

## Do not use the inherited MCO instructions

- Do not follow the archived guide to release Ordelis. The root package is MCO's package under the upstream maintainer's npm scope, it is marked `private`, and Ordelis must never publish it.
- `npx @tt-a1i/mco` installs upstream's package, not Ordelis. No document in this repository describes an Ordelis installation method, because none exists.
- `.github/workflows/publish-npm.yml` is inherited and disabled. A guard job always fails and the other jobs depend on it, so a published GitHub Release cannot reach a publish step. `tests/test_publication_guard.py` fails if any workflow can publish to npm.

## What exists and what is missing

Ordelis does not yet have a release process. The desktop app is an interface preview over sample data, it has only been built and run on macOS, and there are no installers or downloads. When a release process is designed, it will use its own, separately reviewed workflow, and this page will be replaced by the real guide. This page does not define that process.

## Where the work is tracked

- [What gates a first release](docs/product/roadmap.md#what-gates-a-first-release) in the roadmap is the authoritative list.
- [#56](https://github.com/RaioViajante/ordelis/issues/56): release-hardening follow-ups, including replacing the deliberately failing publication guard before the first real release.
- [#38](https://github.com/RaioViajante/ordelis/issues/38): release packaging and distribution assurance.
- The [migration record](docs/migration/ordelis.md#current-status) holds the current status of the release and publication gate and of the security assessment that gates a release.
- Vulnerabilities are reported privately, as described in [SECURITY.md](SECURITY.md).
