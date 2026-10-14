from __future__ import annotations

import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROBE_SCRIPT = ROOT / "scripts" / "run_capability_probes.sh"

# The two credential-bearing shell snapshots stay byte-identical to main on this PR
# so that no rendered diff ever displays their contents (`.gitattributes` marks them
# `-diff`, but diff rendering takes attributes from the merge base, which does not
# have them yet). They are deleted by the stacked follow-up PR whose merge base
# includes `.gitattributes`. That follow-up empties this tuple; this test then
# asserts that nothing under a probe home is tracked at all.
PENDING_REMOVAL_PREFIXES = (
    "docs/probes/2026-02-26/.probe_codex_home/shell_snapshots/",
)
PROBE_HOME_ROOT = "docs/probes/2026-02-26/.probe_codex_home"

# Synthetic paths only: they never existed and carry no real material.
PROBE_HOME_PATHS = (
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
            if ("/.probe_" in p or "/shell_snapshots/" in p)
            and not p.startswith(PENDING_REMOVAL_PREFIXES)
        ]
        self.assertEqual(
            offenders, [],
            "probe-local home or shell snapshot artifacts must not be tracked "
            "outside the documented pending-removal set",
        )

    def test_only_pending_removal_files_remain_in_probe_home(self) -> None:
        # With PENDING_REMOVAL_PREFIXES emptied by the follow-up PR this asserts
        # that no file at all survives under the historical probe-local home.
        home = ROOT / "docs" / "probes" / "2026-02-26" / ".probe_codex_home"
        survivors = [p for p in home.rglob("*") if p.is_file()] if home.exists() else []
        for path in survivors:
            relative = path.relative_to(ROOT).as_posix()
            self.assertTrue(
                relative.startswith(PENDING_REMOVAL_PREFIXES),
                f"unexpected file tracked under probe home: {relative}",
            )

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

    def test_probe_home_paths_render_as_binary_in_diffs(self) -> None:
        # Deletions of these paths must never render line content in a diff.
        for path in (PROBE_HOME_PATHS[0], PROBE_HOME_PATHS[1]):
            with self.subTest(path=path):
                result = _git("check-attr", "diff", "--", path)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(
                    result.stdout.rstrip().endswith(": unset"),
                    f"{path} must be marked -diff in .gitattributes, got: {result.stdout.strip()}",
                )

    def test_capability_probe_script_prunes_environment_capturing_dirs(self) -> None:
        script = PROBE_SCRIPT.read_text(encoding="utf-8")
        self.assertIn(
            "-name shell_snapshots", script,
            "probe script must drop shell snapshots from the artifact set",
        )
        self.assertIn(
            "-name sessions", script,
            "probe script must drop session transcripts from the artifact set",
        )
        # The script must never point a provider at a home inside the repository;
        # that is how environment captures ended up in the artifact root before.
        assignments = re.search(
            r"^[ \t]*(export[ \t]+)?(HOME|CODEX_HOME)=", script, re.MULTILINE
        )
        self.assertIsNone(
            assignments,
            "probe script must not assign HOME/CODEX_HOME (would create a repo-local home)",
        )


if __name__ == "__main__":
    unittest.main()
