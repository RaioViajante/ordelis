from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BRIDGE_DIR = ROOT / "desktop" / "runtime-bridge"
SCRIPT = BRIDGE_DIR / "status.py"
FLAGS = ["-I", "-S", "-B"]

# The exact runtime import closure of the identity query. Reaching config and vault_context is a consequence of
# the Execution Safety Gate's own imports (the gate is not modified); importing them reads nothing.
# Adding to this list must be a deliberate, reviewed change.
EXPECTED_RUNTIME_MODULES = [
    "runtime", "runtime.answer_transport", "runtime.artifacts", "runtime.config", "runtime.contracts",
    "runtime.execution_modes", "runtime.execution_safety_gate", "runtime.policy", "runtime.runtime_validation",
    "runtime.synapse_plan", "runtime.types", "runtime.vault_context",
]

# Runs status.main() under an audit hook and reports what it did. A regression tripwire, not confinement.
HARNESS = r"""
import json, os, sys
sys.path.insert(0, sys.argv[1])
import status
events = []
FORBIDDEN = ("socket.", "subprocess.", "os.system", "os.exec", "os.fork", "os.posix_spawn", "os.spawn",
             "ctypes.", "os.remove", "os.rename", "os.mkdir", "os.rmdir", "os.chmod", "os.chown", "os.symlink",
             "os.link", "shutil.", "urllib.", "http.", "ftplib.", "smtplib.", "os.putenv", "os.unsetenv",
             "os.startfile", "winreg.", "pty.")
opens = []
def hook(event, args):
    if event.startswith(FORBIDDEN):
        events.append(event)
    elif event == "open":
        path, mode, flags = args
        write = bool(flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC | os.O_APPEND)) if isinstance(flags, int) else False
        if isinstance(mode, str) and any(c in mode for c in "wax+"):
            write = True
        opens.append([os.fspath(path) if isinstance(path, (str, bytes, os.PathLike)) else repr(path), write])
sys.addaudithook(hook)
code = status.main()
sys.addaudithook(lambda *a: None)
runtime = sorted(m for m in sys.modules if m == "runtime" or m.startswith("runtime."))
print("\n" + json.dumps({"code": code, "events": events, "opens": opens, "runtime": runtime}))
"""


def run_python(args, cwd=None, env=None):
    return subprocess.run([sys.executable, *FLAGS, *args], capture_output=True, text=True, cwd=cwd,
                          env={} if env is None else env, timeout=30)


class RuntimeStatusEntryPointTests(unittest.TestCase):
    def test_reports_exactly_the_identity_keys(self):
        result = run_python([str(SCRIPT)])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        report = json.loads(result.stdout)
        self.assertEqual(list(report), ["schema_version", "runtime", "gate"])
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(list(report["runtime"]), ["version", "python"])
        self.assertEqual(list(report["gate"]), ["policy_version"])
        from runtime import __version__
        from runtime.execution_safety_gate import POLICY_VERSION
        self.assertEqual(report["runtime"]["version"], __version__)
        self.assertEqual(report["runtime"]["python"], "%d.%d.%d" % sys.version_info[:3])
        self.assertEqual(report["gate"]["policy_version"], POLICY_VERSION)

    def test_output_carries_no_paths_or_environment(self):
        with tempfile.TemporaryDirectory() as tmp:
            result = run_python([str(SCRIPT)], cwd=tmp, env={"SYNAPSE_CANARY": "canary-secret-value"})
        for needle in ("canary-secret-value", str(ROOT), tmp, "provenance"):
            self.assertNotIn(needle, result.stdout)

    def test_unsupported_python_is_reported_before_any_runtime_import(self):
        code = ("import sys; sys.path.insert(0, sys.argv[1]); import status; "
                "rc = status.main((3, 9, 18)); "
                "print(rc, any(m == 'runtime' or m.startswith('runtime.') for m in sys.modules), 'json' in sys.modules)")
        result = run_python(["-c", code, str(BRIDGE_DIR)])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.split(), ["78", "False", "False"])

    def test_the_bootstrap_has_no_syntax_newer_than_python_3_7(self):
        import ast
        source = SCRIPT.read_text()
        ast.parse(source, feature_version=(3, 7))
        self.assertNotIn('f"', source)
        self.assertNotIn("f'", source)

    def harness(self, cwd=None, env=None):
        result = run_python(["-c", HARNESS, str(BRIDGE_DIR)], cwd=cwd, env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout.strip().splitlines()[-1])

    def test_import_closure_is_pinned(self):
        observed = self.harness()
        self.assertEqual(observed["code"], 0)
        self.assertEqual(observed["runtime"], EXPECTED_RUNTIME_MODULES)

    def test_no_provider_adapter_session_or_cli_module_is_imported(self):
        runtime = self.harness()["runtime"]
        for forbidden in ("runtime.cli", "runtime.adapters", "runtime.session", "runtime.mcp_server",
                          "runtime.invocation_runtime", "runtime.probe_authorization_ledger"):
            self.assertFalse([m for m in runtime if m == forbidden or m.startswith(forbidden + ".")], forbidden)

    def test_no_network_process_credential_or_write_activity(self):
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, ".env").write_text("canary")
            Path(tmp, "vault").mkdir()
            Path(tmp, "vault", "note.md").write_text("canary")
            observed = self.harness(cwd=tmp, env={"HOME": tmp, "OPENAI_API_KEY": "canary"})
            self.assertEqual(observed["events"], [])
            self.assertEqual([o for o in observed["opens"] if o[1]], [], "no file may be opened for writing")
            self.assertEqual([o for o in observed["opens"] if tmp in o[0]], [], "nothing under cwd/HOME is opened")

    def test_only_checkout_runtime_sources_and_the_standard_library_are_read(self):
        allowed = tuple(str(p) for p in {
            Path(sys.base_prefix).resolve(), Path(sys.prefix).resolve(), Path(os.__file__).resolve().parent,
            ROOT / "runtime", BRIDGE_DIR,
        })
        opens = self.harness()["opens"]
        self.assertTrue(opens, "the audit hook must observe the runtime source reads")
        for path, _write in opens:
            real = str(Path(path).resolve())
            self.assertTrue(real.startswith(allowed), real)

    def test_no_bytecode_is_written(self):
        before = {p: p.stat().st_mtime_ns for p in ROOT.joinpath("runtime").rglob("*.pyc")}
        self.harness()
        run_python([str(SCRIPT)])
        after = {p: p.stat().st_mtime_ns for p in ROOT.joinpath("runtime").rglob("*.pyc")}
        self.assertEqual(before, after)
        self.assertFalse(list(BRIDGE_DIR.rglob("*.pyc")))
        self.assertFalse(list(BRIDGE_DIR.rglob("__pycache__")))


if __name__ == "__main__":
    unittest.main()
