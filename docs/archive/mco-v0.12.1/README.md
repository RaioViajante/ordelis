# Inherited MCO documents (v0.12.1)

Documents inherited from the upstream [MCO](https://github.com/mco-org/mco) project at the fork base (`v0.12.1`, commit `3cfce13`). They describe MCO, not Ordelis, and are archived here for provenance and attribution. They were previously at the repository root, where they could be mistaken for Ordelis documentation.

**Ordelis is a separate early-development project. It has no approved release and no published installation package. `npx @tt-a1i/mco` installs upstream's package, not Ordelis.** For the current release status, see the root [RELEASING.md](../../../RELEASING.md).

Contents:
- [`README.zh-CN.md`](./README.zh-CN.md): MCO's Simplified Chinese README. Ordelis has no Chinese translation yet.
- [`RELEASING.md`](./RELEASING.md): MCO's guide for publishing `@tt-a1i/mco` to npm. Do not use it to release Ordelis.

## What changed in the archived copies

The files were moved with `git mv`, so their history, including the upstream authors' commits, follows them. Their text is unchanged except for:

- **Both files:** a warning notice added at the top.
- **`README.zh-CN.md`:** its relative links were repointed for the new location (`LICENSE`, `CHANGELOG.md` and the `docs/` pages it references), and its "English" language switch, which pointed at the repository `README.md` (now the Ordelis README), points to the [upstream MCO README](https://github.com/mco-org/mco/blob/main/README.md).
- **`RELEASING.md`:** it keeps the "not applicable" notice that was added when npm publication was disabled (issue #53), below the new warning.

`LICENSE` and the upstream copyright notice are untouched. See the [migration record](../../migration/ordelis.md#provenance-and-attribution) for provenance and attribution.
