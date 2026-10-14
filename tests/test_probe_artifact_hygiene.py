from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path, PurePosixPath

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

    def test_ci_upload_requires_successful_trusted_staging(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "gate.yml").read_text(encoding="utf-8")
        probe_step = workflow.split("- name: Run capability probes", 1)[1]
        probe_step = probe_step.split("- name: Stage validated capability summary", 1)[0]
        artifact_job = workflow.split("  capability-probe-artifact:", 1)[1]
        artifact_job = artifact_job.split("\n  packaging-smoke-macos:", 1)[0]
        stage = workflow.split("- name: Stage validated capability summary", 1)[1]
        stage = stage.split("- name: Upload capability probe artifacts", 1)[0]
        upload = workflow.split("- name: Upload capability probe artifacts", 1)[1]
        upload = upload.split("\n  packaging-smoke-macos:", 1)[0]

        self.assertNotIn("PROBE_CI_EXPORT_DIR", probe_step)
        self.assertIn("needs: capability-probes", artifact_job)
        self.assertIn("runs-on: ubuntu-latest", artifact_job)
        self.assertIn("id: stage_capability_summary", stage)
        self.assertIn("success()", stage)
        self.assertIn("needs.capability-probes.result == 'success'", stage)
        self.assertIn("sanitize_capability_probe_artifacts.py stage-success", stage)
        self.assertNotIn("docs/probes", upload)
        self.assertNotIn("**", upload)
        self.assertNotIn("if: always()", upload)
        self.assertIn("success()", upload)
        self.assertIn("needs.capability-probes.result == 'success'", upload)
        self.assertIn("steps.stage_capability_summary.outcome == 'success'", upload)
        self.assertIn("steps.stage_capability_summary.outputs.upload_ready == 'true'", upload)
        self.assertIn(
            "path: ${{ steps.stage_capability_summary.outputs.artifact_path }}",
            upload,
        )
        self.assertIn("if-no-files-found: error", upload)

    def test_ci_staging_ignores_precreated_paths_and_provider_content(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            test_root = Path(tmp)
            staging_parent = test_root / "trusted-staging"
            staging_parent.mkdir()
            precreated = staging_parent / "capability-probe-summary.json"
            dummy_private_value = "DUMMY_NONFUNCTIONAL_PRIVATE_VALUE"
            precreated.write_text(dummy_private_value, encoding="utf-8")
            unexpected = staging_parent / "unexpected-provider-state.txt"
            unexpected.write_text(dummy_private_value, encoding="utf-8")

            completed = subprocess.run(
                [
                    "python3", str(SANITIZER), "stage-success",
                    "--staging-parent", str(staging_parent),
                    "--run-date", "2099-01-01",
                    "--target-os", "linux",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            output = Path(completed.stdout.strip())
            self.assertNotEqual(output, precreated)
            self.assertNotIn(dummy_private_value, output.read_text(encoding="utf-8"))
            self.assertEqual([path.name for path in output.parent.iterdir()], [output.name])
            self.assertEqual(precreated.read_text(encoding="utf-8"), dummy_private_value)
            self.assertEqual(unexpected.read_text(encoding="utf-8"), dummy_private_value)

            precreated.unlink()
            symlink_target = test_root / "provider-controlled-summary.json"
            symlink_target.write_text(dummy_private_value, encoding="utf-8")
            precreated.symlink_to(symlink_target)
            symlink_attempt = subprocess.run(
                [
                    "python3", str(SANITIZER), "stage-success",
                    "--staging-parent", str(staging_parent),
                    "--run-date", "2099-01-01",
                    "--target-os", "linux",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(symlink_attempt.returncode, 0, symlink_attempt.stderr)
            symlink_output = Path(symlink_attempt.stdout.strip())
            self.assertFalse(symlink_output.is_symlink())
            self.assertEqual(symlink_target.read_text(encoding="utf-8"), dummy_private_value)

    def test_ci_staging_rejects_symlink_parent(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            test_root = Path(tmp)
            real_parent = test_root / "real"
            real_parent.mkdir()
            alias = test_root / "alias"
            alias.symlink_to(real_parent, target_is_directory=True)

            completed = subprocess.run(
                [
                    "python3", str(SANITIZER), "stage-success",
                    "--staging-parent", str(alias),
                    "--run-date", "2099-01-01",
                    "--target-os", "linux",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)

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
