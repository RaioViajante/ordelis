from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path, PurePosixPath

from runtime.contracts import PROVIDER_IDS

ROOT = Path(__file__).resolve().parent.parent
SANITIZER = ROOT / "scripts" / "sanitize_capability_probe_artifacts.py"

# The base PR allowlists exactly the two retained files so it produces no deletion
# section for them. This removal PR deletes both, so no probe-home path is allowed.
# GitHub ignores `-diff` for hosted deletions; the attribute protects local Git only.
PENDING_REMOVAL_PATHS: tuple[str, ...] = ()
PROBE_HOME_ROOT = "docs/probes/2026-02-26/.probe_codex_home"

# Synthetic paths only: they never existed and carry no real material.
PROBE_HOME_PATHS = (
    ".probe_codex_home/auth.json",
    "shell_snapshots/snapshot.sh",
    "docs/probes/2099-01-01/.probe_codex_home/shell_snapshots/snapshot.sh",
    "docs/probes/2099-01-01/.probe_codex_home/sessions/rollout.jsonl",
    "docs/probes/2099-01-01/.probe_codex_home/auth.json",
    "nested/elsewhere/shell_snapshots/snapshot.sh",
    "docs/probes/2099-01-01/claude/C0/raw/stdout.log",
)
COMMITTABLE_EVIDENCE_PATHS = (
    "docs/probes/2099-01-01/claude/C0/result.json",
    "docs/probes/2099-01-01/claude/C0/parsed.json",
    "docs/probes/2099-01-01/summary.md",
    "docs/probes/2099-01-01/lock-summary.yaml",
)


def _git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )


def _is_forbidden_probe_path(path: str) -> bool:
    parts = PurePosixPath(path).parts
    return "shell_snapshots" in parts or any(
        part.startswith(".probe_") and part.endswith("_home")
        for part in parts
    )


class ProbeArtifactHygieneTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        probe = _git("rev-parse", "--is-inside-work-tree")
        if probe.returncode != 0 or probe.stdout.strip() != "true":
            raise unittest.SkipTest("git worktree unavailable; hygiene checks need git metadata")

    def test_no_probe_home_or_shell_snapshot_paths_are_tracked(self) -> None:
        tracked = _git("ls-files")
        self.assertEqual(tracked.returncode, 0, tracked.stderr)
        paths = [line for line in tracked.stdout.splitlines() if line]
        offenders = [
            p for p in paths
            if _is_forbidden_probe_path(p) and p not in PENDING_REMOVAL_PATHS
        ]
        self.assertEqual(
            offenders, [],
            "probe-local home or shell snapshot artifacts must not be tracked "
            "outside the documented pending-removal set",
        )

    def test_only_pending_removal_files_remain_in_probe_home(self) -> None:
        # With PENDING_REMOVAL_PATHS emptied by the follow-up PR this asserts
        # that no file at all survives under the historical probe-local home.
        home = ROOT / "docs" / "probes" / "2026-02-26" / ".probe_codex_home"
        survivors = {
            path.relative_to(ROOT).as_posix()
            for path in home.rglob("*")
            if path.is_file()
        } if home.exists() else set()
        self.assertEqual(survivors, set(PENDING_REMOVAL_PATHS))

    def test_sensitive_probe_paths_are_ignored(self) -> None:
        for path in PROBE_HOME_PATHS:
            with self.subTest(path=path):
                result = _git("check-ignore", "-q", "--", path)
                self.assertEqual(
                    result.returncode, 0,
                    f"{path} must be covered by .gitignore so it cannot be staged",
                )

    def test_curated_probe_evidence_stays_committable(self) -> None:
        for path in COMMITTABLE_EVIDENCE_PATHS:
            with self.subTest(path=path):
                result = _git("check-ignore", "-q", "--", path)
                self.assertEqual(
                    result.returncode, 1,
                    f"{path} is curated probe evidence and must not be ignored",
                )

    def test_probe_home_paths_disable_local_git_text_diffs(self) -> None:
        # GitHub-hosted diffs ignore this attribute; this assertion is local only.
        for path in (PROBE_HOME_PATHS[0], PROBE_HOME_PATHS[1]):
            with self.subTest(path=path):
                result = _git("check-attr", "diff", "--", path)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(
                    result.stdout.rstrip().endswith(": unset"),
                    f"{path} must be marked -diff in .gitattributes, got: {result.stdout.strip()}",
                )

    def test_ci_upload_is_one_status_only_file_outside_the_checkout(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "gate.yml").read_text(encoding="utf-8")
        upload = workflow.split("- name: Upload capability probe artifacts", 1)[1]
        upload = upload.split("\n  packaging-smoke-macos:", 1)[0]

        self.assertNotIn("docs/probes", upload)
        self.assertNotIn("**", upload)
        self.assertIn(
            "path: ${{ runner.temp }}/synapse-capability-probe-export-${{ github.run_id }}-${{ github.run_attempt }}/capability-probe-summary.json",
            upload,
        )

    def test_ci_export_ignores_unexpected_files_and_rejects_invalid_status(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            test_root = Path(tmp)
            artifact_parent = test_root / "probe-work"
            run_root = artifact_parent / "2099-01-01"
            output = test_root / "export" / "capability-probe-summary.json"
            dummy_private_value = "DUMMY_NONFUNCTIONAL_PRIVATE_VALUE"
            for provider in PROVIDER_IDS:
                for capability in ("C0", "C1", "C2"):
                    result = run_root / provider / capability / "result.json"
                    result.parent.mkdir(parents=True, exist_ok=True)
                    result.write_text(
                        json.dumps(
                            {
                                "provider": provider,
                                "capability": capability,
                                "os": "linux",
                                "status": "PASS",
                                "provider_version": dummy_private_value,
                                "unexpected": dummy_private_value,
                            },
                        ),
                        encoding="utf-8",
                    )
            unexpected = run_root / "unexpected-provider-state" / "private.txt"
            unexpected.parent.mkdir()
            unexpected.write_text(dummy_private_value, encoding="utf-8")

            completed = subprocess.run(
                [
                    "python3", str(SANITIZER), "export",
                    "--run-root", str(run_root),
                    "--output", str(output),
                    "--run-date", "2099-01-01",
                    "--target-os", "linux",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertNotIn(dummy_private_value, output.read_text(encoding="utf-8"))
            self.assertEqual([path.name for path in output.parent.iterdir()], [output.name])

            mutated = run_root / PROVIDER_IDS[0] / "C0" / "result.json"
            payload = json.loads(mutated.read_text(encoding="utf-8"))
            payload["status"] = dummy_private_value
            mutated.write_text(json.dumps(payload), encoding="utf-8")
            rejected_output = test_root / "rejected" / "summary.json"
            rejected = subprocess.run(
                [
                    "python3", str(SANITIZER), "export",
                    "--run-root", str(run_root),
                    "--output", str(rejected_output),
                    "--run-date", "2099-01-01",
                    "--target-os", "linux",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(rejected.returncode, 0)
            self.assertFalse(rejected_output.exists())

    def test_cleanup_never_follows_sensitive_directory_symlinks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            test_root = Path(tmp)
            artifact_parent = test_root / "probe-work"
            run_root = artifact_parent / "2099-01-01"
            external = test_root / "external"
            victim = external / "must-remain.txt"
            victim.parent.mkdir()
            victim.write_text("DUMMY_SAFE_VALUE", encoding="utf-8")
            nested = run_root / "provider"
            nested.mkdir(parents=True)
            (nested / "sessions").symlink_to(external, target_is_directory=True)
            snapshot = nested / "shell_snapshots" / "snapshot.sh"
            snapshot.parent.mkdir()
            snapshot.write_text("DUMMY_SAFE_VALUE", encoding="utf-8")

            completed = subprocess.run(
                [
                    "python3", str(SANITIZER), "cleanup",
                    "--run-root", str(run_root),
                    "--artifact-parent", str(artifact_parent),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertTrue(victim.exists())
            self.assertFalse((nested / "sessions").exists())
            self.assertFalse(snapshot.parent.exists())


if __name__ == "__main__":
    unittest.main()
