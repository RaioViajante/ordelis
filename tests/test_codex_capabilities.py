from __future__ import annotations

import contextlib
import io
import json
import os
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from runtime.cli import main
from runtime.codex_capabilities import CapabilityInspectionError, inspect_codex_cli


ROOT_HELP = """Codex CLI
Usage: codex [OPTIONS] [PROMPT]
Options:
  -a, --ask-for-approval <POLICY>
          - never: Never ask for approval
  -c, --config <key=value>
"""
EXEC_HELP = """Run Codex non-interactively
Usage: codex exec [OPTIONS] [PROMPT]
Options:
  -C, --cd <DIR>
  -m, --model <MODEL>
  -s, --sandbox <MODE>
          [possible values: read-only, workspace-write, danger-full-access]
      --skip-git-repo-check
      --json
      --output-schema <FILE>
      --output-last-message <FILE>
      --ephemeral
"""


class CodexCapabilityTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "fake-codex"

    def fake_cli(self, *, version: str = "codex-cli 0.162.1", root: str = ROOT_HELP,
                 exec_help: str = EXEC_HELP, behavior: str = "normal") -> None:
        script = ("#!/usr/bin/env python3\n"
                  "import os, subprocess, sys, time\n"
                  f"VERSION = {version!r}\nROOT = {root!r}\nEXEC = {exec_help!r}\n"
                  f"MARKER = {str(self.root / 'child-alive')!r}\n"
                  f"STARTED = {str(self.root / 'child-started')!r}\n"
                  f"BEHAVIOR = {behavior!r}\n"
                  "if BEHAVIOR == 'check_env':\n"
                  "    assert os.environ['HOME'] == os.environ['CODEX_HOME']\n"
                  "    assert os.path.realpath(os.environ['HOME']) == os.getcwd()\n"
                  "    assert 'OPENAI_API_KEY' not in os.environ\n"
                  "if BEHAVIOR == 'timeout' and sys.argv[1:] == ['exec', '--help']:\n"
                  "    child = subprocess.Popen([sys.executable, '-c', "
                  "'import pathlib,sys,time; time.sleep(1); pathlib.Path(sys.argv[1]).write_text(\"alive\"); time.sleep(60)', MARKER])\n"
                  "    open(STARTED, 'w').write(str(child.pid))\n"
                  "    time.sleep(60)\n"
                  "if BEHAVIOR == 'oversize' and sys.argv[1:] == ['exec', '--help']:\n"
                  "    print('x' * 70000)\n    sys.exit(0)\n"
                  "if BEHAVIOR == 'invalid_utf8' and sys.argv[1:] == ['exec', '--help']:\n"
                  "    os.write(1, b'\\xff')\n    sys.exit(0)\n"
                  "if sys.argv[1:] == ['--version']:\n    print(VERSION)\n"
                  "elif sys.argv[1:] == ['--help']:\n    print(ROOT)\n"
                  "elif sys.argv[1:] == ['exec', '--help']:\n    print(EXEC)\n"
                  "else:\n    raise SystemExit('unexpected invocation')\n")
        self.binary.write_text(script, encoding="utf-8")
        self.binary.chmod(0o755)

    def test_successful_bounded_inspection_and_deterministic_evidence(self) -> None:
        self.fake_cli()
        first = inspect_codex_cli(str(self.binary))
        second = inspect_codex_cli(str(self.binary))
        self.assertEqual(first, second)
        self.assertEqual(first["inspection"], {"status": "supported_controls_observed",
                                                "blocking_reasons": [], "dispatch_authorized": False})
        self.assertEqual(first["cli_version"], "codex-cli 0.162.1")
        self.assertEqual(first["binary"]["path"], str(self.binary.resolve()))
        self.assertEqual(len(first["binary"]["sha256"]), 64)
        self.assertTrue(first["observed_cli_controls"]["--model"])
        self.assertTrue(first["observed_cli_controls"]["--ask-for-approval"])
        self.assertEqual(first["observed_required_values"],
                         {"sandbox_read_only": True, "approval_never": True})
        self.assertTrue(first["reasoning_effort"]["generic_config_override_advertised"])
        self.assertFalse(first["reasoning_effort"]["specific_key_verified_by_help"])
        self.assertEqual(set(first["runtime_verification"].values()),
                         {"not_checked", "not_verified"})
        self.assertEqual(list(self.root.iterdir()), [self.binary])

    def test_unsupported_version_and_missing_adapter_flag(self) -> None:
        self.fake_cli(version="codex-cli 0.45.9")
        result = inspect_codex_cli(str(self.binary))
        self.assertEqual(result["inspection"]["status"], "unsupported")
        self.assertIn("version_below_adapter_minimum", result["inspection"]["blocking_reasons"])
        self.fake_cli(exec_help=EXEC_HELP.replace("      --json\n", ""))
        result = inspect_codex_cli(str(self.binary))
        self.assertIn("adapter_flag_not_advertised:--json", result["inspection"]["blocking_reasons"])
        self.assertFalse(result["observed_cli_controls"]["--json"])
        self.fake_cli(root=ROOT_HELP.replace("          - never: Never ask for approval\n", ""),
                      exec_help=EXEC_HELP.replace("read-only, ", ""))
        result = inspect_codex_cli(str(self.binary))
        self.assertIn("required_value_not_advertised:sandbox=read-only",
                      result["inspection"]["blocking_reasons"])
        self.assertIn("required_value_not_advertised:approval_policy=never",
                      result["inspection"]["blocking_reasons"])

    def test_wrong_flag_position_is_not_accepted(self) -> None:
        self.fake_cli(root=ROOT_HELP.replace("  -a, --ask-for-approval <POLICY>\n", ""),
                      exec_help=EXEC_HELP + "  -a, --ask-for-approval <POLICY>\n")
        result = inspect_codex_cli(str(self.binary))
        self.assertIn("adapter_flag_not_advertised:--ask-for-approval",
                      result["inspection"]["blocking_reasons"])

    def test_malformed_version_help_utf8_and_path_rejected(self) -> None:
        for changed in ({"version": "not codex"}, {"root": "garbage"},
                        {"exec_help": "garbage"}, {"behavior": "invalid_utf8"}):
            with self.subTest(changed=changed):
                self.fake_cli(**changed)
                with self.assertRaises(CapabilityInspectionError):
                    inspect_codex_cli(str(self.binary))
        with self.assertRaisesRegex(CapabilityInspectionError, "absolute"):
            inspect_codex_cli("fake-codex")
        with patch("runtime.codex_capabilities.MAX_BINARY_BYTES", 10):
            with self.assertRaisesRegex(CapabilityInspectionError, "size limit"):
                inspect_codex_cli(str(self.binary))

    def test_path_lookup_resolves_to_binary_identity(self) -> None:
        self.fake_cli()
        with patch("runtime.codex_capabilities.shutil.which", return_value=str(self.binary)):
            result = inspect_codex_cli()
        self.assertEqual(result["binary"]["path"], str(self.binary.resolve()))

    def test_output_limit_and_timeout_clean_up(self) -> None:
        self.fake_cli(behavior="oversize")
        with self.assertRaisesRegex(CapabilityInspectionError, "byte limit"):
            inspect_codex_cli(str(self.binary))
        self.fake_cli(behavior="timeout")
        start = time.monotonic()
        with patch("runtime.codex_capabilities.PROBE_TIMEOUT_SECONDS", 0.25):
            with self.assertRaisesRegex(CapabilityInspectionError, "timed out"):
                inspect_codex_cli(str(self.binary))
        self.assertLess(time.monotonic() - start, 3)
        # A child inherits the probe's process group; it must not survive timeout.
        if os.name != "nt":
            self.assertTrue((self.root / "child-started").exists())
            time.sleep(1.1)
            self.assertFalse((self.root / "child-alive").exists())

    def test_probe_has_isolated_home_and_no_inherited_api_key(self) -> None:
        self.fake_cli(behavior="check_env")
        with patch.dict(os.environ, {"OPENAI_API_KEY": "synthetic-secret"}):
            result = inspect_codex_cli(str(self.binary))
        self.assertEqual(result["inspection"]["status"], "supported_controls_observed")

    def test_explicit_cli_only_and_ordinary_plan_remains_non_probing(self) -> None:
        self.fake_cli()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(main(["context", "inspect-codex", "--binary", str(self.binary), "--json"]), 0)
        self.assertEqual(json.loads(output.getvalue())["inspection"]["status"],
                         "supported_controls_observed")
        with patch("runtime.cli.inspect_codex_cli", side_effect=AssertionError("automatic probe")):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(main(["context", "preview", "--vault", str(self.root),
                                       "--index", str(self.root / "missing.json"), "--project", "demo",
                                       "--agent", "codex", "--model", "synthetic", "--effort", "medium"]), 2)


if __name__ == "__main__":
    unittest.main()
