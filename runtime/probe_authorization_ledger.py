"""Private durable single-use ledger for controlled probe authorizations."""

from __future__ import annotations

import hashlib
import json
import os
import re
import sqlite3
import stat
import time
from dataclasses import asdict
from pathlib import Path
from urllib.parse import quote

from .synapse_plan import PlanError, _overlaps

SCHEMA_VERSION = 1
MAX_LEDGER_ENTRIES = 100_000
MAX_LEDGER_BYTES = 64 * 1024 * 1024
_LEDGER_DIRECTORY = "probe-authorizations-v1"
_DATABASE_NAME = "ledger.sqlite3"
_INITIALIZED_NAME = "initialized-v1"
_EXPECTED_ENTRIES = {_DATABASE_NAME, _INITIALIZED_NAME, f"{_DATABASE_NAME}-journal"}
_TOKEN = re.compile(r"[A-Za-z0-9._:/@+=-]{1,128}")
_SHA256 = re.compile(r"[0-9a-f]{64}")
_OPEN_DIRECTORY = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0)
_OPEN_FILE = os.O_RDWR | getattr(os, "O_NOFOLLOW", 0)


class LedgerError(PlanError):
    pass


def database_path(state_root: Path) -> Path:
    return state_root / _LEDGER_DIRECTORY / _DATABASE_NAME


def authorization_specification_digest(authorization: object, repo_root: Path,
                                       protected_roots: tuple[Path, ...],
                                       commands: tuple[tuple[str, ...], ...]) -> str:
    spec = asdict(authorization)
    spec.update({"protected_roots": [str(root) for root in protected_roots],
                 "checkout": str(repo_root), "commands": [list(args) for args in commands]})
    return hashlib.sha256(json.dumps(spec, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def _fail(reason: str, exc: BaseException | None = None) -> None:
    if exc is None:
        raise LedgerError(reason)
    raise LedgerError(reason) from exc


def _validate_identifier(value: str, label: str, pattern: re.Pattern[str]) -> None:
    if not isinstance(value, str) or pattern.fullmatch(value) is None:
        _fail(label)


def _private_directory(path: Path, label: str) -> os.stat_result:
    try:
        found = os.lstat(path)
    except OSError as exc:
        _fail(f"{label}_unavailable", exc)
    if not stat.S_ISDIR(found.st_mode) or path.is_symlink():
        _fail(f"{label}_unsafe")
    if found.st_uid != os.geteuid():
        _fail(f"{label}_unsafe_owner")
    if stat.S_IMODE(found.st_mode) != 0o700:
        _fail(f"{label}_not_private")
    return found


def _private_file(path: Path, label: str) -> os.stat_result:
    try:
        found = os.lstat(path)
    except OSError as exc:
        _fail(f"{label}_unavailable", exc)
    if not stat.S_ISREG(found.st_mode) or path.is_symlink() or found.st_nlink != 1:
        _fail(f"{label}_unsafe")
    if found.st_uid != os.geteuid() or stat.S_IMODE(found.st_mode) != 0o600:
        _fail(f"{label}_unsafe")
    return found


def _open_checked_directory(path: Path, expected: os.stat_result, label: str) -> int:
    descriptor: int | None = None
    try:
        descriptor = os.open(path, _OPEN_DIRECTORY)
        opened = os.fstat(descriptor)
    except OSError as exc:
        if descriptor is not None:
            os.close(descriptor)
        _fail(f"{label}_unavailable", exc)
    if not os.path.samestat(expected, opened):
        os.close(descriptor)
        _fail(f"{label}_changed")
    return descriptor


def _validate_state_root(raw: str, repo_root: Path, protected_roots: tuple[Path, ...],
                         scratch_parent: Path) -> tuple[Path, int]:
    if not isinstance(raw, str) or not raw or not os.path.isabs(raw) or os.path.realpath(raw) != raw:
        _fail("state_root_unsafe")
    root = Path(raw)
    expected = _private_directory(root, "state_root")
    try:
        resolved = root.resolve(strict=True)
        if any(_overlaps(resolved, other) for other in (repo_root, scratch_parent, *protected_roots)):
            _fail("state_root_overlap")
    except LedgerError:
        raise
    except OSError as exc:
        _fail("state_root_unavailable", exc)
    return resolved, _open_checked_directory(resolved, expected, "state_root")


def validate_state_root(raw: str, repo_root: Path, protected_roots: tuple[Path, ...],
                        scratch_parent: Path) -> Path:
    root, descriptor = _validate_state_root(raw, repo_root, protected_roots, scratch_parent)
    os.close(descriptor)
    return root


def _directory_entries(directory: Path) -> set[str]:
    try:
        entries = {entry.name for entry in os.scandir(directory)}
    except OSError as exc:
        _fail("authorization_ledger_unavailable", exc)
    if not entries <= _EXPECTED_ENTRIES:
        _fail("authorization_ledger_unexpected_files")
    return entries


def _open_ledger_directory(root: Path, root_fd: int, *, initialize: bool) -> tuple[Path, int] | None:
    directory = root / _LEDGER_DIRECTORY
    if not initialize:
        try:
            os.lstat(directory)
        except FileNotFoundError:
            return None
        except OSError as exc:
            _fail("authorization_ledger_unavailable", exc)
        expected = _private_directory(directory, "authorization_ledger")
        return directory, _open_checked_directory(directory, expected, "authorization_ledger")
    try:
        os.mkdir(_LEDGER_DIRECTORY, 0o700, dir_fd=root_fd)
        os.fsync(root_fd)
    except FileExistsError:
        pass
    except OSError as exc:
        _fail("authorization_ledger_unavailable", exc)
    expected = _private_directory(directory, "authorization_ledger")
    return directory, _open_checked_directory(directory, expected, "authorization_ledger")


def _create_database_file(directory_fd: int) -> None:
    flags = _OPEN_FILE | os.O_CREAT | os.O_EXCL
    descriptor: int | None = None
    try:
        descriptor = os.open(_DATABASE_NAME, flags, 0o600, dir_fd=directory_fd)
        os.fsync(descriptor)
        os.fsync(directory_fd)
    except FileExistsError:
        pass
    except OSError as exc:
        _fail("authorization_ledger_unavailable", exc)
    finally:
        if descriptor is not None:
            os.close(descriptor)


def _create_initialized_marker(directory_fd: int) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    descriptor: int | None = None
    try:
        descriptor = os.open(_INITIALIZED_NAME, flags, 0o600, dir_fd=directory_fd)
        os.write(descriptor, b"1\n")
        os.fsync(descriptor)
        os.fsync(directory_fd)
    except FileExistsError:
        pass
    except OSError as exc:
        _fail("authorization_ledger_unavailable", exc)
    finally:
        if descriptor is not None:
            os.close(descriptor)


def _connect(database: Path) -> sqlite3.Connection:
    uri = f"file:{quote(str(database), safe='/')}?mode=rw"
    connection = sqlite3.connect(uri, uri=True, timeout=5, isolation_level=None)
    try:
        connection.execute("PRAGMA busy_timeout = 5000")
        connection.execute("PRAGMA trusted_schema = OFF")
        if connection.execute("PRAGMA journal_mode = DELETE").fetchone() != ("delete",):
            _fail("authorization_ledger_unavailable")
        connection.execute("PRAGMA synchronous = FULL")
        page_size = connection.execute("PRAGMA page_size").fetchone()
        if not page_size or type(page_size[0]) is not int or page_size[0] <= 0:
            _fail("authorization_ledger_corrupt")
        maximum_pages = MAX_LEDGER_BYTES // page_size[0]
        applied = connection.execute(f"PRAGMA max_page_count = {maximum_pages}").fetchone()
        if not applied or type(applied[0]) is not int or applied[0] > maximum_pages:
            _fail("authorization_ledger_full")
        return connection
    except BaseException:
        connection.close()
        raise


def _begin(connection: sqlite3.Connection) -> None:
    connection.execute("BEGIN IMMEDIATE")


def _commit(connection: sqlite3.Connection) -> None:
    connection.execute("COMMIT")


def _rollback(connection: sqlite3.Connection) -> None:
    if connection.in_transaction:
        try:
            connection.execute("ROLLBACK")
        except sqlite3.Error:
            pass


def _initialize_schema(connection: sqlite3.Connection) -> None:
    """Create the schema in an empty database; only called before the marker exists, so no rows can be lost."""
    version = connection.execute("PRAGMA user_version").fetchone()
    objects = connection.execute("SELECT count(*) FROM sqlite_master").fetchone()
    if version != (0,) or objects != (0,):
        return
    connection.execute(
        "CREATE TABLE consumptions ("
        "authorization_id TEXT PRIMARY KEY, "
        "specification_sha256 TEXT NOT NULL, "
        "consumed_at INTEGER NOT NULL, "
        "expires_at INTEGER NOT NULL"
        ") WITHOUT ROWID")
    connection.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")


def _validate_schema(connection: sqlite3.Connection) -> None:
    try:
        check = connection.execute("PRAGMA quick_check").fetchone()
        if check != ("ok",):
            _fail("authorization_ledger_corrupt")
        version = connection.execute("PRAGMA user_version").fetchone()
        if version != (SCHEMA_VERSION,):
            _fail("authorization_ledger_schema")
        objects = connection.execute(
            "SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name"
        ).fetchall()
        if objects != [("table", "consumptions")]:
            _fail("authorization_ledger_schema")
        columns = connection.execute("PRAGMA table_info(consumptions)").fetchall()
        summary = [(row[1], row[2], row[3], row[5]) for row in columns]
        expected = [
            ("authorization_id", "TEXT", 1, 1),
            ("specification_sha256", "TEXT", 1, 0),
            ("consumed_at", "INTEGER", 1, 0),
            ("expires_at", "INTEGER", 1, 0),
        ]
        if summary != expected:
            _fail("authorization_ledger_schema")
    except LedgerError:
        raise
    except sqlite3.DatabaseError as exc:
        _fail("authorization_ledger_corrupt", exc)


def _existing(connection: sqlite3.Connection, authorization_id: str,
              specification_sha256: str) -> None:
    row = connection.execute(
        "SELECT specification_sha256 FROM consumptions WHERE authorization_id = ?",
        (authorization_id,)).fetchone()
    if row is None:
        return
    if row[0] == specification_sha256:
        _fail("authorization_reused")
    _fail("authorization_specification_mismatch")


def _validate_database_identity(database: Path, before: os.stat_result) -> None:
    after = _private_file(database, "authorization_ledger")
    if not os.path.samestat(before, after):
        _fail("authorization_ledger_changed")


def _marker_present(directory: Path) -> bool:
    return _INITIALIZED_NAME in _directory_entries(directory)


def _operate(*, state_root: str, authorization_id: str, specification_sha256: str,
             expires_at: float, repo_root: Path, protected_roots: tuple[Path, ...],
             scratch_parent: Path, consume: bool) -> None:
    _validate_identifier(authorization_id, "invalid_authorization_id", _TOKEN)
    _validate_identifier(specification_sha256, "invalid_specification_sha256", _SHA256)
    if type(expires_at) not in (int, float):
        _fail("invalid_authorization_expiry")
    root, root_fd = _validate_state_root(state_root, repo_root, protected_roots, scratch_parent)
    ledger_fd: int | None = None
    connection: sqlite3.Connection | None = None
    try:
        opened = _open_ledger_directory(root, root_fd, initialize=consume)
        if opened is None:
            return
        directory, ledger_fd = opened
        entries = _directory_entries(directory)
        database = directory / _DATABASE_NAME
        marker = directory / _INITIALIZED_NAME
        # Invariant: rows are inserted only after the marker exists, so "no marker" means "no consumptions"
        # and any process may finish an interrupted first use under SQLite's write lock.
        if _INITIALIZED_NAME not in entries:
            if not consume:
                return
            if _DATABASE_NAME not in entries:
                _create_database_file(ledger_fd)
        else:
            _private_file(marker, "authorization_ledger_marker")
            if _DATABASE_NAME not in entries:
                _fail("authorization_ledger_missing")
        before = _private_file(database, "authorization_ledger")
        if before.st_size > MAX_LEDGER_BYTES:
            _fail("authorization_ledger_full")
        if f"{_DATABASE_NAME}-journal" in _directory_entries(directory):
            _private_file(directory / f"{_DATABASE_NAME}-journal", "authorization_ledger_journal")
        connection = _connect(database)
        if _INITIALIZED_NAME not in entries:
            _begin(connection)
            if not _marker_present(directory):
                _initialize_schema(connection)
                _validate_schema(connection)
                if connection.execute("SELECT count(*) FROM consumptions").fetchone() != (0,):
                    _fail("authorization_ledger_incomplete")
                _validate_database_identity(database, before)
                _commit(connection)
                _create_initialized_marker(ledger_fd)
            else:
                _rollback(connection)
            _private_file(marker, "authorization_ledger_marker")
        _begin(connection)
        _validate_schema(connection)
        _existing(connection, authorization_id, specification_sha256)
        count = connection.execute("SELECT count(*) FROM consumptions").fetchone()
        if not count or type(count[0]) is not int:
            _fail("authorization_ledger_corrupt")
        if count[0] >= MAX_LEDGER_ENTRIES:
            _fail("authorization_ledger_full")
        if not consume:
            _rollback(connection)
            return
        connection.execute(
            "INSERT INTO consumptions "
            "(authorization_id, specification_sha256, consumed_at, expires_at) VALUES (?, ?, ?, ?)",
            (authorization_id, specification_sha256, int(time.time()), int(expires_at)))
        _validate_database_identity(database, before)
        _commit(connection)
        os.fsync(ledger_fd)
        _validate_database_identity(database, before)
        if _directory_entries(directory) - {_DATABASE_NAME, _INITIALIZED_NAME}:
            _fail("authorization_ledger_unexpected_files")
    except LedgerError:
        if connection is not None:
            _rollback(connection)
        raise
    except sqlite3.DatabaseError as exc:
        if connection is not None:
            _rollback(connection)
        message = str(exc).lower()
        if "not a database" in message or "malformed" in message:
            reason = "authorization_ledger_corrupt"
        elif "full" in message:
            reason = "authorization_ledger_full"
        else:
            reason = "authorization_ledger_unavailable"
        _fail(reason, exc)
    except OSError as exc:
        if connection is not None:
            _rollback(connection)
        _fail("authorization_ledger_unavailable", exc)
    finally:
        if connection is not None:
            connection.close()
        if ledger_fd is not None:
            os.close(ledger_fd)
        os.close(root_fd)


def check_authorization_available(*, state_root: str, authorization_id: str,
                                  specification_sha256: str, expires_at: float,
                                  repo_root: Path, protected_roots: tuple[Path, ...],
                                  scratch_parent: Path) -> None:
    _operate(state_root=state_root, authorization_id=authorization_id,
             specification_sha256=specification_sha256, expires_at=expires_at,
             repo_root=repo_root, protected_roots=protected_roots,
             scratch_parent=scratch_parent, consume=False)


def consume_authorization(*, state_root: str, authorization_id: str,
                          specification_sha256: str, expires_at: float,
                          repo_root: Path, protected_roots: tuple[Path, ...],
                          scratch_parent: Path) -> None:
    _operate(state_root=state_root, authorization_id=authorization_id,
             specification_sha256=specification_sha256, expires_at=expires_at,
             repo_root=repo_root, protected_roots=protected_roots,
             scratch_parent=scratch_parent, consume=True)
