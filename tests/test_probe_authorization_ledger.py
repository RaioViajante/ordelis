from __future__ import annotations

import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from runtime import probe_authorization_ledger as ledger
ROOT = Path(__file__).resolve().parent.parent
CHILD = r'''
import json, os, sys
from pathlib import Path
from runtime import probe_authorization_ledger as ledger

values = json.loads(sys.argv[1])
try:
    ledger.consume_authorization(
        state_root=values["state_root"], authorization_id=values["authorization_id"],
        specification_sha256=values["specification_sha256"], expires_at=values["expires_at"],
        repo_root=Path(values["repo_root"]), protected_roots=tuple(Path(p) for p in values["protected_roots"]),
        scratch_parent=Path(values["scratch_parent"]))
    result = "consumed"
except ledger.LedgerError as exc:
    result = str(exc)
if values.get("crash") and result == "consumed":
    os._exit(91)
print(result)
'''


class LedgerCase(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(os.path.realpath(self.temp.name))
        self.repo = self.root / "repo"
        self.vault = self.root / "vault"
        self.scratch = self.root / "scratch"
        self.state = self.root / "state"
        for path in (self.repo, self.vault, self.scratch, self.state):
            path.mkdir(mode=0o700)
        self.state.chmod(0o700)
        self.counter = 0

    def values(self, **overrides) -> dict[str, object]:
        self.counter += 1
        values: dict[str, object] = {
            "state_root": str(self.state),
            "authorization_id": f"authorization-{self.counter}",
            "specification_sha256": "a" * 64,
            "expires_at": time.time() + 300,
            "repo_root": self.repo,
            "protected_roots": (self.vault,),
            "scratch_parent": self.scratch,
        }
        values.update(overrides)
        return values

    def consume(self, **overrides) -> None:
        ledger.consume_authorization(**self.values(**overrides))

    @staticmethod
    def child_values(values: dict[str, object], *, crash: bool = False) -> str:
        payload = dict(values, crash=crash)
        payload["repo_root"] = str(payload["repo_root"])
        payload["protected_roots"] = [str(path) for path in payload["protected_roots"]]
        payload["scratch_parent"] = str(payload["scratch_parent"])
        return json.dumps(payload)

    def child(self, values: dict[str, object], *, crash: bool = False) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, "-c", CHILD, self.child_values(values, crash=crash)], cwd=ROOT,
            text=True, capture_output=True, check=False)


class ConsumptionTests(LedgerCase):
    def test_first_consumption_succeeds_and_same_process_reuse_fails(self) -> None:
        values = self.values()
        ledger.consume_authorization(**values)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_reused"):
            ledger.consume_authorization(**values)

    def test_reuse_in_another_process_fails(self) -> None:
        values = self.values()
        ledger.consume_authorization(**values)
        result = self.child(values)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "authorization_reused")

    def test_two_processes_competing_for_one_identity_have_exactly_one_winner(self) -> None:
        values = self.values()
        argv = [sys.executable, "-c", CHILD, self.child_values(values)]
        processes = [subprocess.Popen(argv, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE) for _ in range(2)]
        results = [process.communicate(timeout=15) for process in processes]
        self.assertEqual([process.returncode for process in processes], [0, 0], results)
        self.assertEqual(sorted(stdout.strip() for stdout, _ in results),
                         ["authorization_reused", "consumed"])

    def test_crash_after_commit_does_not_make_the_identity_reusable(self) -> None:
        values = self.values()
        crashed = self.child(values, crash=True)
        self.assertEqual(crashed.returncode, 91, crashed.stderr)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_reused"):
            ledger.consume_authorization(**values)

    def test_different_specification_for_the_same_identity_is_denied(self) -> None:
        values = self.values()
        ledger.consume_authorization(**values)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_specification_mismatch"):
            ledger.consume_authorization(**dict(values, specification_sha256="b" * 64))

    def test_commit_failure_fails_closed_without_recording_a_false_success(self) -> None:
        values = self.values()
        with patch.object(ledger, "_commit", side_effect=sqlite3.OperationalError("synthetic")):
            with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_unavailable"):
                ledger.consume_authorization(**values)
        ledger.consume_authorization(**values)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_reused"):
            ledger.consume_authorization(**values)

    def test_clock_rollback_does_not_remove_a_consumed_identity(self) -> None:
        values = self.values()
        ledger.consume_authorization(**values)
        with patch.object(ledger.time, "time", return_value=1.0):
            with self.assertRaisesRegex(ledger.LedgerError, "authorization_reused"):
                ledger.consume_authorization(**values)


class InitializationRecoveryTests(LedgerCase):
    def ledger_directory(self) -> Path:
        return ledger.database_path(self.state).parent

    def test_interrupted_first_use_states_are_recoverable_without_losing_identities(self) -> None:
        directory = self.ledger_directory()
        directory.mkdir(mode=0o700)  # crash after mkdir, before the database file
        self.consume(authorization_id="after-mkdir")
        marker = directory / ledger._INITIALIZED_NAME
        marker.unlink()  # crash after the schema-only commit, before the marker
        self.assertEqual(sqlite3.connect(ledger.database_path(self.state)).execute(
            "SELECT count(*) FROM consumptions").fetchone(), (1,))
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_incomplete"):
            self.consume(authorization_id="rows-without-marker")

    def test_marker_creation_failure_is_retryable_and_never_consumes(self) -> None:
        values = self.values()
        with patch.object(ledger, "_create_initialized_marker", side_effect=ledger.LedgerError("synthetic")):
            with self.assertRaises(ledger.LedgerError):
                ledger.consume_authorization(**values)
        ledger.consume_authorization(**values)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_reused"):
            ledger.consume_authorization(**values)

    def test_schema_only_database_without_marker_is_completed(self) -> None:
        values = self.values()
        with patch.object(ledger, "_create_initialized_marker", side_effect=ledger.LedgerError("synthetic")):
            with self.assertRaises(ledger.LedgerError):
                ledger.consume_authorization(**values)
        self.assertFalse((self.ledger_directory() / ledger._INITIALIZED_NAME).exists())
        ledger.check_authorization_available(**values)

    def test_truncated_database_with_marker_cannot_reset_consumed_identities(self) -> None:
        values = self.values()
        ledger.consume_authorization(**values)
        database = ledger.database_path(self.state)
        database.write_bytes(b"")
        database.chmod(0o600)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_schema"):
            ledger.consume_authorization(**values)

    def test_concurrent_first_use_has_one_winner_per_identity_and_loses_nothing(self) -> None:
        shared = self.values(authorization_id="shared")
        distinct = [self.values(authorization_id=f"distinct-{index}") for index in range(3)]
        argvs = [[sys.executable, "-c", CHILD, self.child_values(values)]
                 for values in (shared, shared, shared, *distinct)]
        processes = [subprocess.Popen(argv, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE) for argv in argvs]
        results = [process.communicate(timeout=30) for process in processes]
        self.assertEqual([process.returncode for process in processes], [0] * 6, results)
        outcomes = sorted(stdout.strip() for stdout, _ in results)
        self.assertEqual(outcomes, ["authorization_reused"] * 2 + ["consumed"] * 4)
        connection = sqlite3.connect(ledger.database_path(self.state))
        try:
            self.assertEqual(connection.execute("SELECT count(*) FROM consumptions").fetchone(), (4,))
        finally:
            connection.close()


class StateValidationTests(LedgerCase):
    def test_missing_state_root_fails_closed(self) -> None:
        with self.assertRaisesRegex(ledger.LedgerError, "state_root"):
            self.consume(state_root=str(self.root / "missing"))

    def test_state_root_permissions_must_be_private(self) -> None:
        self.state.chmod(0o755)
        with self.assertRaisesRegex(ledger.LedgerError, "state_root_not_private"):
            self.consume()

    def test_symlinked_state_root_is_refused(self) -> None:
        target = self.root / "other-state"
        target.mkdir(mode=0o700)
        target.chmod(0o700)
        link = self.root / "state-link"
        link.symlink_to(target)
        with self.assertRaisesRegex(ledger.LedgerError, "state_root"):
            self.consume(state_root=str(link))

    def test_state_root_must_not_overlap_checkout_scratch_or_protected_roots(self) -> None:
        candidates = (self.repo, self.scratch, self.vault, self.root)
        for candidate in candidates:
            candidate.chmod(0o700)
            with self.subTest(candidate=candidate), self.assertRaisesRegex(ledger.LedgerError, "state_root_overlap"):
                self.consume(state_root=str(candidate))

    def test_missing_database_in_initialized_directory_fails_closed(self) -> None:
        self.consume()
        ledger.database_path(self.state).unlink()
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_missing"):
            self.consume()

    def test_corrupt_database_fails_closed(self) -> None:
        self.consume()
        database = ledger.database_path(self.state)
        database.write_bytes(b"not sqlite")
        database.chmod(0o600)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_corrupt"):
            self.consume()

    def test_symlinked_database_is_refused(self) -> None:
        self.consume()
        database = ledger.database_path(self.state)
        database.unlink()
        target = self.root / "other.sqlite3"
        target.write_bytes(b"")
        target.chmod(0o600)
        database.symlink_to(target)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_unsafe"):
            self.consume()

    def test_database_permissions_must_remain_private(self) -> None:
        self.consume()
        database = ledger.database_path(self.state)
        database.chmod(0o644)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_unsafe"):
            self.consume()

    def test_unexpected_file_and_schema_version_are_refused(self) -> None:
        self.consume()
        unexpected = ledger.database_path(self.state).parent / "unexpected"
        unexpected.write_text("x")
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_unexpected_files"):
            self.consume()
        unexpected.unlink()
        connection = sqlite3.connect(ledger.database_path(self.state))
        try:
            connection.execute("PRAGMA user_version = 99")
            connection.commit()
        finally:
            connection.close()
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_schema"):
            self.consume()

    def test_repeated_initialization_preserves_existing_consumptions(self) -> None:
        first = self.values(authorization_id="first")
        ledger.consume_authorization(**first)
        second = self.values(authorization_id="second")
        ledger.consume_authorization(**second)
        with self.assertRaisesRegex(ledger.LedgerError, "authorization_reused"):
            ledger.consume_authorization(**first)


class RetentionAndPrivacyTests(LedgerCase):
    def test_storage_limit_fails_closed_without_pruning_consumed_ids(self) -> None:
        first = self.values(authorization_id="first")
        second = self.values(authorization_id="second")
        with patch.object(ledger, "MAX_LEDGER_ENTRIES", 1):
            ledger.consume_authorization(**first)
            with self.assertRaisesRegex(ledger.LedgerError, "authorization_ledger_full"):
                ledger.consume_authorization(**second)
            with self.assertRaisesRegex(ledger.LedgerError, "authorization_reused"):
                ledger.consume_authorization(**first)

    def test_ledger_persists_only_bounded_identifiers_digests_and_timestamps(self) -> None:
        secret = "VAULT-CONTENT-SECRET-5519"
        values = self.values(authorization_id="private-id")
        ledger.consume_authorization(**values)
        database = ledger.database_path(self.state)
        raw = database.read_bytes()
        for forbidden in (secret, str(self.repo), str(self.vault), str(self.scratch), str(self.state)):
            self.assertNotIn(forbidden.encode(), raw)
        connection = sqlite3.connect(database)
        try:
            rows = connection.execute("SELECT * FROM consumptions").fetchall()
        finally:
            connection.close()
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0][0:2], ("private-id", "a" * 64))

    def test_ledger_module_cannot_resolve_or_execute_codex(self) -> None:
        source = (ROOT / "runtime" / "probe_authorization_ledger.py").read_text()
        for forbidden in ("subprocess", "shutil.which", "executable_path", "run_controlled_probe"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main()
