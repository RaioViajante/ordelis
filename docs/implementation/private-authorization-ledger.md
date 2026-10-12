# Private cross-process authorization ledger

Status: accepted for issue [#17](https://github.com/RaioViajante/synapse/issues/17).

## Decision

The controlled probe entry point uses a private SQLite database in an explicitly configured state root.
It does not fall back to the checkout, the current directory, a vault, a scratch directory, a user project,
or a platform default. The standard-library `sqlite3` module is sufficient; no dependency is added.

The alternatives were rejected for this increment:

| Design | Concurrent processes | Crash and filesystem failure | Recovery |
| --- | --- | --- | --- |
| SQLite with `BEGIN IMMEDIATE` and a unique primary key | One writer commits an authorization identity; later contenders observe the committed row. SQLite supplies locking instead of a Python-only mutex. | A committed transaction is durable with `synchronous=FULL`; an interrupted transaction is rolled back or recovered from its journal. Corruption, locking errors and uncertain commit results fail closed. | Schema and integrity checks are deterministic. Backup and restore are not implemented (see below). |
| One exclusive-create file per identity | `O_CREAT | O_EXCL` can select one winner on a supported local filesystem. | File creation is small, but safely binding metadata, syncing the containing directory, detecting partial files, validating every pathname and enumerating a large directory add a bespoke recovery protocol. | Permanent tombstones create unbounded directory fan-out, and partial-file recovery needs another trusted marker. |
| Append-only journal | Appends alone do not define one winner; cooperating writers still need a cross-process lock and duplicate scan. | Torn or reordered tail writes need framing, checksums, sync rules and truncation recovery. | Replay time and file size grow forever, while compaction reintroduces a transactional database problem. |

SQLite is the smallest reliable option for supported local filesystems. The guarantee assumes ordinary local
filesystems on which SQLite rollback-journal locking, atomic rename/write behavior and `fsync` are reliable.
Synapse does not claim atomic consumption on NFS, SMB, cloud-synchronized folders, FUSE mounts with weaker
semantics, removable media with unsafe caching, or arbitrary network shares.

## Private state and filesystem rules

`ProbeAuthorization.state_root` is an absolute, existing, canonical directory supplied by the caller. It
must be owned by the effective user, have mode `0700`, contain no symlinked path component, and be disjoint
by filesystem identity from the checkout, scratch parent and every protected root (including vaults and real
projects). The executable must also be outside it. Validation failure is denial; there is no fallback.

Synapse creates `probe-authorizations-v1/` beneath that root with mode `0700` and stores
`ledger.sqlite3` plus an initialization-complete marker with mode `0600`. Only those files and a valid SQLite rollback journal are accepted in the
ledger directory. Symlinks, hard-linked database files, unexpected file types, foreign ownership, broader
permissions, unexpected files, a missing database in an existing ledger directory, corrupt data and unknown
schema versions all fail closed. Checks are repeated around database use. They reduce accidental substitution
but do not make pathnames safe from hostile code running as the same user.

First use is a two-step, retryable initialization that never involves a consumed identity. Under SQLite's
write lock a process creates the schema in an empty database, commits it, and only then creates the
`initialized-v1` marker (`O_EXCL`, synced). Identities are inserted in a later transaction, so the invariant is:
**no marker means no consumptions**. Any contender may therefore finish an interrupted first use: a crash after
`mkdir`, after the database file, or after the schema commit leaves a state the next call completes, and a
failed commit or marker write consumes nothing. Once the marker exists the schema is validated strictly, so a
truncated, emptied or replaced database is refused (`authorization_ledger_schema`/`_corrupt`) instead of being
re-initialized. Rows without a marker mean tampering or corruption and are refused
(`authorization_ledger_incomplete`). Repeated initialization against a complete database is idempotent.

## Identity, binding and consumption

The authorization identity is the validated `probe_id`. Its database primary key is permanent within this
ledger. The row also stores a SHA-256 digest of the complete immutable `ProbeAuthorization`, the canonical
checkout and protected-root identities used by confirmation, and the fixed allowlisted command vectors. The
database never stores those paths or the specification itself. Reusing an identity with the same digest is
reported as replay; using it with a different digest is reported as an identity/specification conflict.

The order is:

1. Validate the authorization, time window, executable, paths and ledger availability.
2. Display the exact specification on the controlling terminal and require the digest-bound phrase.
3. Revalidate the specification and consume the process-local confirmation nonce.
4. Open an immediate SQLite transaction, recheck schema and integrity, check the permanent row limit, insert
   the identity and specification digest, and durably commit.
5. Only after the commit may the runner create its scratch directory or spawn the first fake executable.

Decline, interruption, expiry, validation failure or failed/uncertain database consumption launches nothing.
Once the commit succeeds, command failure, cancellation, scratch failure, runner exception or process crash
does not remove the row. A crash immediately after commit therefore spends the authorization. There is no
rollback API.

Expiry is checked before confirmation, after confirmation and before consumption. Wall-clock rollback can
make a fresh authorization not yet valid, but it cannot remove a committed primary key. `consumed_at` and
`expires_at` are troubleshooting metadata only and are never used to delete or revive rows.

SQLite transactions are the sole authority for same-thread, cross-thread and cross-process consumption. The
old process-local consumed-ID set is removed because it cannot establish cross-process correctness and would
create a second, inconsistent capacity limit. The bounded process-local confirmation-nonce registry remains:
it proves that this process completed the terminal step, while SQLite proves permanent single use among
cooperating processes. Neither boundary resists hostile same-user code that can replace Synapse or invoke an
executable directly.

## Schema, retention and recovery

Schema version 1 has one `consumptions` table:

- `authorization_id`: the `probe_id`, primary key;
- `specification_sha256`: digest binding the exact confirmed specification;
- `consumed_at`: integer wall-clock time for troubleshooting;
- `expires_at`: the authorization expiry for troubleshooting.

No credential, prompt, terminal answer, vault content, provider output, command output, environment value or
raw path is stored. Evidence remains unauthenticated and dispatch remains unauthorized.

Permanent single use and automatic pruning are incompatible without a separately trusted epoch or identity
namespace: deleting an old row would make that identity valid again. This increment therefore never prunes.
It enforces fixed row and database-byte limits and fails closed for every new identity when full; already
recorded identities remain denied. A future bounded-history design needs an explicit epoch carried by
authorizations and a trusted migration that preserves the retired epochs.

Backup and restore are **deferred**: this increment implements, tests and supports neither. Copying
`ledger.sqlite3` and its rollback journal with file tools is not a crash-consistent backup (the pair can be torn
by a concurrent writer, and a journal copied without its database state is meaningless), and Synapse does not
describe it as one. A future increment must specify a SQLite-aware, quiesced snapshot, verify it, and define
restore. Until then, restoring any older copy can lose tombstones and permit replay, so rollback is not a safe
recovery operation. Corruption, a missing initialized database or a full ledger requires explicit operator
recovery. Deleting the ledger is an authorization-state reset and can make old identities reusable;
it is never automatic and must be treated like issuing a new trust epoch. Synapse provides no reset command in
this increment.

## Concurrency and durability guarantees

- Another process's transient rollback journal is normal. Journal inspection tolerates one that disappears,
  and no directory check runs after a commit, so a recorded consumption is never reported as a failure because
  of a peer's journal (an earlier revision could spend an identity yet deny the run, or reject a still-unspent one).
- A process killed mid-transaction leaves nothing consumed; SQLite rolls back a hot journal on the next open.
  A process killed after commit leaves the identity spent.
- The connection requests `synchronous=FULL` and `fullfsync=ON`; on macOS plain `fsync` does not guarantee a
  flush to stable storage, so power-loss durability depends on the drive honoring `F_FULLFSYNC`. The marker
  and directory syncs use plain `fsync`; losing the marker fails closed (`authorization_ledger_incomplete`) and
  never revives an identity. Crash tests kill a real process; power loss, disk-full and I/O faults are covered
  only by fail-closed error mapping and mocks, not by fault injection.
- Database replacement between validation and opening is detected best-effort by file identity checks around
  the transaction; it is not prevented against hostile code running as the same user.
