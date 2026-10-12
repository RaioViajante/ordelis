from __future__ import annotations

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from runtime.cli import main
from runtime.synapse_plan import PlanError, build_plan, load_private_config
from tests.test_invocation_runtime import DeterministicFakeAdapter


class EffortFakeAdapter(DeterministicFakeAdapter):
    def supported_effort_values(self) -> list[str]:
        return ["medium"]


class NoModelFakeAdapter(DeterministicFakeAdapter):
    def supported_model_keys(self) -> list[str]:
        return []


class NoPermissionFakeAdapter(DeterministicFakeAdapter):
    def supported_permission_keys(self) -> list[str]:
        return []


class SynapsePlanTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.private = self.root / "private"
        self.private.mkdir()
        self.vault = self.private / "vault"
        self.vault.mkdir()
        (self.vault / "selected.md").write_text("# Work\nRelevant note.\n# Other\nExcluded.\n", encoding="utf-8")
        (self.vault / "unrelated.md").write_bytes(b"\xff")
        self.index = self.private / "index.json"
        self.index.write_text(json.dumps({
            "projects": {"demo": [{"path": "selected.md", "heading": "Work"}]},
            "topics": {"other": [{"path": "unrelated.md"}]},
        }), encoding="utf-8")
        self.state = self.private / "state"
        self.state.mkdir()
        self.config = self.private / "synapse.json"
        self.raw = {
            "schema_version": 1,
            "vault": {"root": str(self.vault), "index": str(self.index), "read_only": True},
            "context": {"per_note_bytes": 64, "total_excerpt_bytes": 128},
            "execution": {"mode": "read_only", "hard_timeout_seconds": 60,
                          "max_parallelism": 1, "max_retries": 0},
            "state": {"root": str(self.state)},
        }
        self.write_config()

    def write_config(self) -> None:
        self.config.write_text(json.dumps(self.raw), encoding="utf-8")

    def plan(self, **changes: object) -> dict[str, object]:
        options = dict(config_path=self.config, repo_root=self.repo, task="Summarize work.",
                       project="demo", topic=None, agent="pi", model="synthetic-model",
                       effort=None, adapter=DeterministicFakeAdapter())
        options.update(changes)
        return build_plan(**options)

    def test_synthetic_plan_is_deterministic_bounded_and_blocked(self) -> None:
        before = {p.name: p.read_bytes() for p in self.vault.iterdir()}
        with patch("subprocess.Popen", side_effect=AssertionError("provider launched")), \
             patch("subprocess.run", side_effect=AssertionError("provider launched")):
            first = self.plan()
            second = self.plan()
        self.assertEqual(first, second)
        self.assertEqual(first["context"]["excerpts"], [{
            "path": "selected.md", "heading": "Work", "excerpt": "Relevant note.\n",
            "missing_heading": False, "truncated": False,
        }])
        self.assertEqual(first["context"]["vault_root"], str(self.vault.resolve()))
        self.assertNotIn("unrelated.md", json.dumps(first))
        self.assertEqual(first["requested_settings"]["model"], "synthetic-model")
        self.assertEqual(first["capability_evidence"]["adapter_declaration"]["model_keys"], ["model"])
        self.assertEqual(first["capability_evidence"]["local_cli"]["model"], "not_verified")
        self.assertIsNone(first["effective_settings"]["model"])
        self.assertEqual(first["validation"]["status"], "blocked")
        self.assertIn("model_not_verified_against_local_cli", first["validation"]["blocking_reasons"])
        self.assertFalse(first["execution"]["dispatch"])
        self.assertFalse(first["execution"]["persist"])
        self.assertEqual(first["execution"]["max_parallelism"], 1)
        self.assertEqual(first["execution"]["max_retries"], 0)
        self.assertEqual(first["execution"]["hard_timeout_seconds"], 60)
        self.assertEqual(first["execution"]["state_root"], str(self.state.resolve()))
        self.assertEqual({p.name: p.read_bytes() for p in self.vault.iterdir()}, before)
        self.assertEqual(list(self.state.iterdir()), [])
        self.assertEqual(list(self.repo.iterdir()), [])

    def test_required_unsupported_settings_reject_and_unknown_effort_blocks(self) -> None:
        for changed in ({"adapter": NoModelFakeAdapter()},
                        {"adapter": NoPermissionFakeAdapter()},
                        {"effort": "high"}):
            with self.subTest(changed=changed), self.assertRaisesRegex(PlanError, "does not support"):
                self.plan(**changed)
        unknown = self.plan(effort="medium", adapter=EffortFakeAdapter())
        self.assertEqual(unknown["requested_settings"]["effort"], "medium")
        self.assertEqual(unknown["capability_evidence"]["adapter_declaration"]["effort_values"], ["medium"])
        self.assertEqual(unknown["capability_evidence"]["local_cli"]["effort"], "not_verified")
        self.assertIsNone(unknown["effective_settings"]["effort"])
        self.assertIn("effort_not_verified_against_local_cli", unknown["validation"]["blocking_reasons"])

    def test_rejects_invalid_configuration_and_limits(self) -> None:
        bad_cases = (
            ("schema_version", 2),
            ("vault.read_only", False),
            ("context.per_note_bytes", 0),
            ("context.per_note_bytes", True),
            ("context.total_excerpt_bytes", 8193),
            ("execution.mode", "write"),
            ("execution.hard_timeout_seconds", 0),
            ("execution.max_parallelism", 0),
            ("execution.max_parallelism", False),
            ("execution.max_retries", 1),
            ("state.root", str(self.repo / "state")),
            ("vault.index", "index.json"),
        )
        for path, value in bad_cases:
            with self.subTest(path=path):
                root = self.raw
                keys = path.split(".")
                owner = root if len(keys) == 1 else root[keys[0]]
                previous = owner[keys[-1]]
                owner[keys[-1]] = value
                if path == "state.root":
                    (self.repo / "state").mkdir(exist_ok=True)
                self.write_config()
                with self.assertRaises(PlanError):
                    self.plan()
                owner[keys[-1]] = previous
        self.write_config()
        self.raw["unexpected"] = True
        self.write_config()
        with self.assertRaises(PlanError):
            self.plan()

    def test_rejects_repo_local_and_symlinked_private_paths(self) -> None:
        local_config = self.repo / "config.json"
        local_config.write_text(json.dumps(self.raw), encoding="utf-8")
        with self.assertRaisesRegex(PlanError, "overlaps"):
            self.plan(config_path=local_config)
        alias = self.private / "alias.json"
        alias.symlink_to(self.config)
        with self.assertRaisesRegex(PlanError, "non-symlink"):
            self.plan(config_path=alias)
        state_alias = self.private / "state-alias"
        state_alias.symlink_to(self.state, target_is_directory=True)
        self.raw["state"]["root"] = str(state_alias)
        self.write_config()
        with self.assertRaisesRegex(PlanError, "non-symlink"):
            self.plan()
        self.raw["state"]["root"] = str(self.root)
        self.write_config()
        with self.assertRaisesRegex(PlanError, "overlaps"):
            self.plan()
        self.raw["state"]["root"] = str(self.vault)
        self.write_config()
        with self.assertRaisesRegex(PlanError, "read-only vault"):
            self.plan()

    def test_malformed_oversize_and_duplicate_config_rejected(self) -> None:
        for raw in (b"\xff", b"{", b" " * 65537,
                    b'{"schema_version":1,"schema_version":1}'):
            with self.subTest(raw=raw[:10]):
                self.config.write_bytes(raw)
                with self.assertRaises(PlanError):
                    load_private_config(self.config, self.repo)

    def test_task_and_output_limits(self) -> None:
        for task in ("", "x" * 4097, "\ud800"):
            with self.subTest(task=task[:8]), self.assertRaises(PlanError):
                self.plan(task=task)
        (self.vault / "selected.md").write_text("# Work\n" + "x" * 200, encoding="utf-8")
        excerpt = self.plan()["context"]["excerpts"][0]
        self.assertEqual(len(excerpt["excerpt"].encode("utf-8")), 64)
        self.assertTrue(excerpt["truncated"])

    def test_cli_plan_and_existing_preview_stay_non_dispatching(self) -> None:
        base = ["context", "plan", "--config", str(self.config), "--repo", str(self.repo),
                "--project", "demo", "--task", "Summarize work.", "--agent", "pi",
                "--model", "synthetic-model", "--json"]
        outputs = []
        with patch("subprocess.Popen", side_effect=AssertionError("provider launched")), \
             patch("subprocess.run", side_effect=AssertionError("provider launched")), \
             patch("runtime.cli.run_invocation_workflow", side_effect=AssertionError("dispatched")):
            for _ in range(2):
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    self.assertEqual(main(base), 0)
                outputs.append(output.getvalue())
            old = io.StringIO()
            with contextlib.redirect_stdout(old):
                self.assertEqual(main(["context", "preview", "--vault", str(self.vault),
                                       "--index", str(self.index), "--project", "demo", "--agent", "pi",
                                       "--model", "synthetic-model", "--effort", "medium", "--json"]), 0)
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(json.loads(outputs[0])["validation"]["status"], "blocked")
        self.assertFalse(json.loads(old.getvalue())["execution"]["policy"]["dispatch"])


if __name__ == "__main__":
    unittest.main()
