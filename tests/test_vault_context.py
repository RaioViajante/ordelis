from __future__ import annotations

import contextlib
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from runtime.cli import main
from runtime.vault_context import MAX_INDEX_BYTES, MAX_NOTE_BYTES, VaultContextError, preview


class VaultContextTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.vault = self.root / "vault"
        self.vault.mkdir()
        (self.vault / "project.md").write_text("# Overview\nintro\n## Plan\nalpha\n## Other\nsecret\n", encoding="utf-8")
        (self.vault / "unrelated.md").write_text("unrelated", encoding="utf-8")
        self.index = self.root / "index.json"
        self.write_index({"projects": {"demo": [{"path": "project.md", "heading": "Plan"}]},
                          "topics": {"other": [{"path": "unrelated.md"}]}})

    def write_index(self, value: object) -> None:
        self.index.write_text(json.dumps(value), encoding="utf-8")

    def run_preview(self, **kwargs: object) -> dict[str, object]:
        return preview(self.vault, self.index, "demo", None, "codex", "example-model", "medium", **kwargs)

    def test_retrieves_only_selected_heading_and_is_deterministic(self) -> None:
        (self.vault / "unrelated.md").write_bytes(b"\xff")
        with patch("subprocess.Popen", side_effect=AssertionError("provider launched")):
            first = self.run_preview()
            second = self.run_preview()
        self.assertEqual(first, second)
        self.assertEqual(first["context"]["excerpts"], [{
            "path": "project.md", "heading": "Plan", "excerpt": "alpha\n",
            "missing_heading": False, "truncated": False,
        }])
        self.assertEqual(first["execution"]["policy"]["dispatch"], False)
        self.assertEqual(first["execution"]["proposed_model"], "example-model")
        self.assertEqual(first["execution"]["proposed_effort"], "medium")
        self.assertFalse(first["execution"]["model_validated"])
        self.assertFalse(first["execution"]["effort_validated"])
        self.assertEqual(first["context"]["kind"], "reference_data")

    def test_missing_heading_and_truncation_are_explicit(self) -> None:
        self.write_index({"projects": {"demo": [
            {"path": "project.md", "heading": "Absent"},
            {"path": "project.md", "heading": "Plan"}]}, "topics": {}})
        excerpts = self.run_preview(per_note_bytes=3)["context"]["excerpts"]
        self.assertEqual(excerpts[0]["excerpt"], "")
        self.assertTrue(excerpts[0]["missing_heading"])
        self.assertEqual(excerpts[1]["excerpt"], "alp")
        self.assertTrue(excerpts[1]["truncated"])

    def test_heading_ignores_fenced_code_and_keeps_nested_section(self) -> None:
        (self.vault / "project.md").write_text(
            "~~~markdown\n## Plan\nfalse heading\n~~~\n"
            "## Plan ###\nactual\n### Detail\nnested\n## Next\nexcluded\n", encoding="utf-8")
        excerpt = self.run_preview()["context"]["excerpts"][0]
        self.assertEqual(excerpt["excerpt"], "actual\n### Detail\nnested\n")
        self.assertFalse(excerpt["missing_heading"])

    def test_total_budget_and_deduplication(self) -> None:
        self.write_index({"projects": {"demo": [{"path": "project.md"}]},
                          "topics": {"same": [{"path": "project.md"}, {"path": "unrelated.md"}]}})
        result = preview(self.vault, self.index, "demo", "same", "codex", "m", "low", 20, 5)
        self.assertEqual(len(result["context"]["excerpts"]), 2)
        self.assertEqual(result["context"]["total_excerpt_bytes"], 5)
        self.assertTrue(all(item["truncated"] for item in result["context"]["excerpts"]))

    def test_rejects_unsafe_paths_and_malformed_index(self) -> None:
        for path in ("../secret.md", "/tmp/secret.md", "a/./note.md", "a//note.md", "C:\\secret.md"):
            with self.subTest(path=path):
                self.write_index({"projects": {"demo": [{"path": path}]}, "topics": {}})
                with self.assertRaises(VaultContextError):
                    self.run_preview()
        for raw in (b"\xff", b"{", b'{"projects":{},"projects":{},"topics":{}}',
                    b'{"projects":[],"topics":{}}', b"[" * 1100 + b"0" + b"]" * 1100,
                    b'{"projects":{"demo":[{"path":"\\ud800.md"}]},"topics":{}}',
                    b"[" + b"9" * 5000 + b"]"):
            with self.subTest(raw=raw):
                self.index.write_bytes(raw)
                with self.assertRaises(VaultContextError):
                    self.run_preview()

    def test_rejects_symlink_escape_and_invalid_utf8(self) -> None:
        outside = self.root / "outside.md"
        outside.write_text("private", encoding="utf-8")
        (self.vault / "link.md").symlink_to(outside)
        self.write_index({"projects": {"demo": [{"path": "link.md"}]}, "topics": {}})
        with self.assertRaises(VaultContextError):
            self.run_preview()
        (self.vault / "linked_dir").symlink_to(self.root, target_is_directory=True)
        self.write_index({"projects": {"demo": [{"path": "linked_dir/outside.md"}]}, "topics": {}})
        with self.assertRaises(VaultContextError):
            self.run_preview()
        (self.vault / "bad.md").write_bytes(b"\xff")
        self.write_index({"projects": {"demo": [{"path": "bad.md"}]}, "topics": {}})
        with self.assertRaisesRegex(VaultContextError, "UTF-8"):
            self.run_preview()

    def test_rejects_invalid_limits_and_oversize_note(self) -> None:
        for per_note, total in ((0, 10), (2049, 10), (10, 0), (10, 8193), (True, 10)):
            with self.subTest(per_note=per_note, total=total):
                with self.assertRaises(VaultContextError):
                    self.run_preview(per_note_bytes=per_note, total_bytes=total)
        (self.vault / "project.md").write_bytes(b"a" * 65537)
        with self.assertRaisesRegex(VaultContextError, "exceeds"):
            self.run_preview()

    def test_bounded_reads_at_size_limit(self) -> None:
        self.index.write_bytes(b" " * (MAX_INDEX_BYTES + 1))
        with patch("runtime.vault_context._read_note", side_effect=AssertionError("note read")):
            with self.assertRaisesRegex(VaultContextError, "exceeds"):
                self.run_preview()
        self.write_index({"projects": {"demo": [{"path": "project.md"}]}, "topics": {}})
        (self.vault / "project.md").write_bytes(b"a" * MAX_NOTE_BYTES)
        result = self.run_preview()
        self.assertTrue(result["context"]["excerpts"][0]["truncated"])
        (self.vault / "project.md").write_bytes(b"a" * (MAX_NOTE_BYTES + 1))
        with self.assertRaisesRegex(VaultContextError, "exceeds"):
            self.run_preview()

    def test_utf8_budget_keeps_complete_characters(self) -> None:
        (self.vault / "project.md").write_text("😀x", encoding="utf-8")
        self.write_index({"projects": {"demo": [{"path": "project.md"}]}, "topics": {}})
        short = self.run_preview(per_note_bytes=3)
        self.assertEqual(short["context"]["excerpts"][0]["excerpt"], "")
        self.assertTrue(short["context"]["excerpts"][0]["truncated"])
        exact = self.run_preview(per_note_bytes=4)
        self.assertEqual(exact["context"]["excerpts"][0]["excerpt"], "😀")
        self.assertEqual(exact["context"]["total_excerpt_bytes"], 4)
        self.assertTrue(exact["context"]["excerpts"][0]["truncated"])

    def test_cli_opens_files_read_only_and_leaves_vault_unchanged(self) -> None:
        before = {path.name: path.read_bytes() for path in self.vault.iterdir()}
        original_open = os.open
        access_modes = []

        def checked_open(path: object, flags: int, *args: object, **kwargs: object) -> int:
            access_modes.append(flags & os.O_ACCMODE)
            return original_open(path, flags, *args, **kwargs)

        args = ["context", "preview", "--vault", str(self.vault), "--index", str(self.index),
                "--project", "demo", "--agent", "codex", "--model", "example-model", "--effort", "medium"]
        with patch("runtime.vault_context.os.open", side_effect=checked_open), \
                patch("subprocess.Popen", side_effect=AssertionError("provider launched")), \
                patch("subprocess.run", side_effect=AssertionError("provider launched")), \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(main(args), 0)
        self.assertTrue(access_modes)
        self.assertEqual(set(access_modes), {os.O_RDONLY})
        self.assertEqual({path.name: path.read_bytes() for path in self.vault.iterdir()}, before)

    def test_rejects_oversize_serialized_preview(self) -> None:
        directory = self.vault / ("d" * 180)
        directory.mkdir()
        heading = "H" * 128
        entries = []
        for index in range(32):
            name = f"note{index}.md"
            (directory / name).write_text(f"# {heading}\n" + "😀" * 64, encoding="utf-8")
            entries.append({"path": f"{directory.name}/{name}", "heading": heading})
        self.write_index({"projects": {"demo": entries}, "topics": {}})
        with self.assertRaisesRegex(VaultContextError, "output bytes"):
            self.run_preview()

    def test_cli_json_is_stable_and_does_not_execute_provider(self) -> None:
        args = ["context", "preview", "--vault", str(self.vault), "--index", str(self.index),
                "--project", "demo", "--agent", "codex", "--model", "example-model", "--effort", "medium"]
        outputs = []
        with patch("subprocess.Popen", side_effect=AssertionError("provider launched")):
            for _ in range(2):
                stdout = io.StringIO()
                with contextlib.redirect_stdout(stdout):
                    self.assertEqual(main(args), 0)
                outputs.append(stdout.getvalue())
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(json.loads(outputs[0])["context"]["excerpts"][0]["path"], "project.md")


if __name__ == "__main__":
    unittest.main()
