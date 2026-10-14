from __future__ import annotations

import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import tempfile
import textwrap
import time
import unittest
from contextlib import contextmanager
from datetime import date
from pathlib import Path

from runtime.adapters import adapter_registry
from runtime.contracts import PROVIDER_IDS
from runtime.contracts import TaskInput
from runtime.execution_modes import execution_permissions


ROOT = Path(__file__).resolve().parents[1]


class ProviderValidationScriptTests(unittest.TestCase):
    @contextmanager
    def _capability_probe_fixture(self):
        with tempfile.TemporaryDirectory() as tmp:
            test_root = Path(tmp)
            repo_root = test_root / "repo"
            scripts_dir = repo_root / "scripts"
            fake_bin_dir = test_root / "bin"
            home_dir = test_root / "home"
            artifact_parent = test_root / "probe-work"
            export_dir = test_root / "probe-export"
            scripts_dir.mkdir(parents=True)
            fake_bin_dir.mkdir()
            home_dir.mkdir()
            artifact_parent.mkdir()

            script_path = scripts_dir / "run_capability_probes.sh"
            shutil.copy2(ROOT / "scripts" / "run_capability_probes.sh", script_path)
            sanitizer = ROOT / "scripts" / "sanitize_capability_probe_artifacts.py"
            if sanitizer.exists():
                shutil.copy2(sanitizer, scripts_dir / sanitizer.name)

            fake_driver = fake_bin_dir / "fake-provider"
            fake_driver.write_text(
                textwrap.dedent(
                    r"""
                    #!/usr/bin/env bash
                    set -u

                    provider="$(basename "$0")"
                    if [ "$provider" = "agent" ]; then
                      provider="cursor"
                    fi

                    if [ -n "${FAKE_INVOCATION_MARKER:-}" ]; then
                      : > "$FAKE_INVOCATION_MARKER"
                    fi

                    for arg in "$@"; do
                      if [ "$arg" = "--version" ] || [ "$arg" = "-v" ]; then
                        echo "$provider 1.2.3 ${FAKE_VERSION_SUFFIX:-}"
                        exit 0
                      fi
                    done

                    if [ "${FAKE_CREATE_SENSITIVE:-}" = "1" ] && [ "$provider" = "claude" ]; then
                      mkdir -p "$FAKE_SENSITIVE_ROOT/shell_snapshots" "$FAKE_SENSITIVE_ROOT/sessions"
                      printf '%s\n' 'DUMMY_NONFUNCTIONAL_CREDENTIAL' > "$FAKE_SENSITIVE_ROOT/shell_snapshots/snapshot.sh"
                      printf '%s\n' 'DUMMY_PRIVATE_SESSION' > "$FAKE_SENSITIVE_ROOT/sessions/session.jsonl"
                      if [ "${FAKE_LOCK_SENSITIVE_PARENT:-}" = "1" ]; then
                        chmod 0500 "$FAKE_SENSITIVE_ROOT"
                      fi
                    fi

                    if [ "${FAKE_SLEEP_PROVIDER:-}" = "$provider" ]; then
                      printf '%s\n' "$$" > "$FAKE_PROVIDER_PID_FILE"
                      exec sleep 30
                    fi

                    args="$*"
                    prose_c2=false
                    case ",${FAKE_PROSE_C2_PROVIDERS:-}," in
                      *",$provider,"*) prose_c2=true ;;
                    esac
                    if [[ "$args" == *"--output-format stream-json"* ]]; then
                      printf '%s\n%s\n' '{"type":"message"}' '{"type":"result"}'
                      exit 0
                    fi

                    if [[ "$args" == *"--output-last-message"* ]]; then
                      while [ "$#" -gt 0 ]; do
                        if [ "$1" = "--output-last-message" ]; then
                          printf '%s\n' '{"probe":"c2","ok":true}' > "$2"
                          break
                        fi
                        shift
                      done
                      printf '%s\n' '{"type":"result"}'
                      exit 0
                    fi

                    if [[ "$args" == *"Return JSON object"* ]]; then
                      if [ "${FAKE_PLAIN_TEXT_C2_PROVIDER:-}" = "$provider" ]; then
                        printf '%s\n' 'model said {"probe":"c2","ok":true}'
                      elif [ "${FAKE_CODE_FENCED_C2_PROVIDER:-}" = "$provider" ]; then
                        printf '%s\n' '{"type":"result","text":"```json\n{\"probe\":\"c2\",\"ok\":true}\n```"}'
                      elif [ "$prose_c2" = true ]; then
                        if [ "$provider" = "gemini" ] || [ "$provider" = "hermes" ]; then
                          printf '%s\n' 'model said {"probe":"c2","ok":true}'
                        else
                          printf '%s\n' '{"type":"result","text":"model said {\"probe\":\"c2\",\"ok\":true}"}'
                        fi
                      elif [ "$provider" = "grok" ]; then
                        if [[ "$args" == *"--json-schema"* ]]; then
                          printf '%s\n' '{"type":"result","structuredOutput":{"probe":"c2","ok":true}}'
                        else
                          printf '%s\n' '{"type":"result","text":"```json\n{\"probe\":\"c2\",\"ok\":true}\n```"}'
                        fi
                      elif [ "$provider" = "gemini" ] && [[ "$args" != *"--output-format json"* ]]; then
                        printf '%s\n' 'model said {"probe":"c2","ok":true}'
                      elif [ "$provider" = "copilot" ] || [ "$provider" = "cursor" ] || [ "$provider" = "gemini" ] || [ "$provider" = "opencode" ] || [ "$provider" = "qwen" ] || [ "$provider" = "pi" ]; then
                        printf '%s\n' '{"type":"result","response":"{\"probe\":\"c2\",\"ok\":true}"}'
                      else
                        printf '%s\n' '{"probe":"c2","ok":true}'
                      fi
                      exit 0
                    fi

                    echo "OK"
                    """,
                ).lstrip(),
                encoding="utf-8",
            )
            fake_driver.chmod(0o755)
            for binary in (
                "claude", "codex", "gemini", "opencode", "qwen", "hermes", "pi",
                "copilot", "grok", "agent", "agy",
            ):
                (fake_bin_dir / binary).symlink_to(fake_driver)

            env = os.environ.copy()
            env["PATH"] = f"{fake_bin_dir}{os.pathsep}{env['PATH']}"
            env["HOME"] = str(home_dir)
            env.pop("CODEX_HOME", None)
            env["PROBE_CWD"] = str(test_root)
            env["PROBE_ARTIFACT_ROOT"] = str(artifact_parent)
            env["PROBE_CI_EXPORT_DIR"] = str(export_dir)
            yield {
                "test_root": test_root,
                "repo_root": repo_root,
                "script_path": script_path,
                "artifact_parent": artifact_parent,
                "export_dir": export_dir,
                "env": env,
            }

    def _run_capability_probes(
        self,
        *,
        code_fenced_c2_provider: str | None = None,
        plain_text_c2_provider: str | None = None,
        prose_c2_providers: tuple[str, ...] = (),
        create_sensitive: bool = False,
        cleanup_failure: bool = False,
        version_suffix: str = "",
    ) -> tuple[subprocess.CompletedProcess[str], dict[str, object]]:
        with self._capability_probe_fixture() as fixture:
            env = fixture["env"].copy()
            if code_fenced_c2_provider is not None:
                env["FAKE_CODE_FENCED_C2_PROVIDER"] = code_fenced_c2_provider
            if plain_text_c2_provider is not None:
                env["FAKE_PLAIN_TEXT_C2_PROVIDER"] = plain_text_c2_provider
            if prose_c2_providers:
                env["FAKE_PROSE_C2_PROVIDERS"] = ",".join(prose_c2_providers)
            sensitive_root = (
                fixture["artifact_parent"] / date.today().isoformat() / "synthetic-provider-home"
            )
            if create_sensitive or cleanup_failure:
                env["FAKE_CREATE_SENSITIVE"] = "1"
                env["FAKE_SENSITIVE_ROOT"] = str(sensitive_root)
            if cleanup_failure:
                env["FAKE_LOCK_SENSITIVE_PARENT"] = "1"
            if version_suffix:
                env["FAKE_VERSION_SUFFIX"] = version_suffix

            completed = subprocess.run(
                ["bash", str(fixture["script_path"])],
                cwd=fixture["repo_root"],
                env=env,
                capture_output=True,
                text=True,
                timeout=30,
                check=False,
            )
            if sensitive_root.exists():
                sensitive_root.chmod(0o700)
            probe_roots = list(fixture["artifact_parent"].iterdir())
            self.assertEqual(len(probe_roots), 1)
            probe_root = probe_roots[0]
            artifacts: dict[str, object] = {}
            if (probe_root / "lock-summary.yaml").exists():
                artifacts["lock_summary"] = (probe_root / "lock-summary.yaml").read_text(
                    encoding="utf-8",
                )
            if (probe_root / "schema_c2.json").exists():
                artifacts["schema_c2"] = json.loads(
                    (probe_root / "schema_c2.json").read_text(encoding="utf-8"),
                )
            if (probe_root / "summary.md").exists():
                artifacts["summary"] = (probe_root / "summary.md").read_text(encoding="utf-8")
            for provider in PROVIDER_IDS:
                result_path = probe_root / provider / "C2" / "result.json"
                if result_path.exists():
                    artifacts[provider] = json.loads(result_path.read_text(encoding="utf-8"))
            ci_summary = fixture["export_dir"] / "capability-probe-summary.json"
            artifacts["ci_export_files"] = sorted(
                path.relative_to(fixture["export_dir"]).as_posix()
                for path in fixture["export_dir"].rglob("*")
                if path.is_file()
            ) if fixture["export_dir"].exists() else []
            if ci_summary.exists():
                artifacts["ci_summary"] = json.loads(ci_summary.read_text(encoding="utf-8"))
            artifacts["sensitive_survivors"] = sorted(
                path.relative_to(probe_root).as_posix()
                for path in probe_root.rglob("*")
                if path.is_file() and (
                    "shell_snapshots" in path.parts or "sessions" in path.parts
                )
            )
            return completed, artifacts

    def test_capability_probe_provider_list_matches_builtin_contract(self) -> None:
        script = (ROOT / "scripts" / "run_capability_probes.sh").read_text(encoding="utf-8")
        match = re.search(r"^providers=\(([^)]*)\)$", script, re.MULTILINE)

        self.assertIsNotNone(match)
        self.assertEqual(tuple(shlex.split(match.group(1))), tuple(PROVIDER_IDS))

    def test_capability_probe_covers_required_tiers_for_every_builtin_provider(self) -> None:
        script = (ROOT / "scripts" / "run_capability_probes.sh").read_text(encoding="utf-8")
        probes = set(re.findall(r'run_probe\s+"([^"]+)"\s+"(C[0-6])"', script))

        for provider in PROVIDER_IDS:
            with self.subTest(provider=provider):
                self.assertTrue(
                    {(provider, "C0"), (provider, "C1"), (provider, "C2")} <= probes,
                )

    def test_c1_probe_commands_match_read_only_adapter_policy(self) -> None:
        script = (ROOT / "scripts" / "run_capability_probes.sh").read_text(encoding="utf-8")
        commands = dict(
            re.findall(r'^run_probe "([^"]+)" "C1" "([^"]+)" "', script, re.MULTILINE),
        )

        for provider, adapter in adapter_registry().items():
            with self.subTest(provider=provider):
                actual = shlex.split(commands[provider])
                actual[0] = adapter.binary_name
                permissions = execution_permissions(provider, "read_only") or {}
                task = TaskInput(
                    task_id="capability-probe",
                    prompt="Reply with exactly OK",
                    repo_root="$PROBE_CWD",
                    target_paths=[],
                    metadata={"provider_permissions": permissions},
                )
                self.assertEqual(actual, adapter._build_command(task))

    def test_c2_probes_preserve_read_only_permission_flags(self) -> None:
        script = (ROOT / "scripts" / "run_capability_probes.sh").read_text(encoding="utf-8")
        expected_fragments = {
            "claude": ("--permission-mode plan",),
            "codex": ("--ask-for-approval never", "--sandbox read-only"),
            "gemini": ("--approval-mode plan",),
            "opencode": ("--agent plan", "--dir '$PROBE_CWD'"),
            "qwen": ("--approval-mode plan",),
            "pi": ("--tools read,grep,find,ls",),
            "copilot": ("--deny-tool=write", "--deny-tool=shell"),
            "grok": ("--permission-mode plan",),
            "cursor": ("--mode ask", "--sandbox enabled"),
            "agy": ("--mode plan", "--sandbox"),
        }

        for provider, fragments in expected_fragments.items():
            with self.subTest(provider=provider):
                match = re.search(
                    rf'run_probe "{provider}" "C2"\s*(?:\\\s*)?"((?:\\.|[^"])*)"',
                    script,
                )
                self.assertIsNotNone(match)
                for fragment in fragments:
                    self.assertIn(fragment, match.group(1))

    def test_c2_probe_commands_request_native_json_output(self) -> None:
        script = (ROOT / "scripts" / "run_capability_probes.sh").read_text(encoding="utf-8")

        for provider in ("gemini", "copilot", "grok", "cursor"):
            with self.subTest(provider=provider):
                match = re.search(
                    rf'run_probe "{provider}" "C2"\s*\\\s*\n\s*"((?:\\.|[^"])*)"',
                    script,
                )
                self.assertIsNotNone(match)
                self.assertIn("--output-format json", match.group(1))

    def test_c2_rejects_plain_text_pseudo_json(self) -> None:
        _, artifacts = self._run_capability_probes(plain_text_c2_provider="copilot")

        self.assertEqual(artifacts["copilot"]["status"], "FAIL")

    def test_c2_schema_constrains_expected_probe_values(self) -> None:
        _, artifacts = self._run_capability_probes()
        schema = artifacts["schema_c2"]

        self.assertEqual(
            {
                name: schema["properties"][name].get("const")
                for name in ("probe", "ok")
            },
            {"probe": "c2", "ok": True},
        )

    def test_grok_c2_uses_schema_backed_structured_output(self) -> None:
        script = (ROOT / "scripts" / "run_capability_probes.sh").read_text(encoding="utf-8")
        match = re.search(
            r'run_probe "grok" "C2"\s*\\\s*\n\s*"((?:\\.|[^"])*)"',
            script,
        )

        self.assertIsNotNone(match)
        self.assertIn(
            r'''--json-schema \"\$(cat '$schema_file')\"''',
            match.group(1),
        )
        completed, artifacts = self._run_capability_probes()
        self.assertEqual((completed.returncode, artifacts["grok"]["status"]), (0, "PASS"))

    def test_grok_c2_rejects_code_fenced_text_in_json_envelope(self) -> None:
        _, artifacts = self._run_capability_probes(code_fenced_c2_provider="grok")

        self.assertEqual(artifacts["grok"]["status"], "FAIL")

    def test_c2_rejects_prose_pseudo_json_for_weak_providers(self) -> None:
        weak_providers = ("gemini", "opencode", "qwen", "hermes", "pi")
        _, artifacts = self._run_capability_probes(prose_c2_providers=weak_providers)

        self.assertEqual(
            {provider: artifacts[provider]["status"] for provider in weak_providers},
            {provider: "FAIL" for provider in weak_providers},
        )

    def test_aggregate_exit_code_requires_all_required_tiers_to_pass(self) -> None:
        passing_run, passing_artifacts = self._run_capability_probes()
        blocked_run, artifacts = self._run_capability_probes(
            plain_text_c2_provider="copilot",
        )

        self.assertEqual(passing_run.returncode, 0)
        self.assertEqual(
            {
                provider: passing_artifacts[provider]["status"]
                for provider in ("gemini", "opencode", "qwen", "hermes", "pi")
            },
            {provider: "PASS" for provider in ("gemini", "opencode", "qwen", "hermes", "pi")},
        )
        self.assertNotEqual(blocked_run.returncode, 0)
        lock_summary = str(artifacts["lock_summary"])
        for provider in PROVIDER_IDS:
            with self.subTest(provider=provider):
                self.assertIn(f"  {provider}:\n", lock_summary)
        self.assertRegex(
            lock_summary,
            r"(?s)  copilot:.*?C2: FAIL.*?gate_status: BLOCKED",
        )

    def test_ci_export_contains_only_validated_statuses(self) -> None:
        dummy_private_value = "DUMMY_NONFUNCTIONAL_PRIVATE_VALUE"
        completed, artifacts = self._run_capability_probes(
            version_suffix=dummy_private_value,
        )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(artifacts["ci_export_files"], ["capability-probe-summary.json"])
        ci_summary = artifacts["ci_summary"]
        self.assertEqual(
            set(ci_summary),
            {"schema_version", "run_date", "target_os", "required_capabilities", "providers", "gate_status"},
        )
        self.assertEqual(ci_summary["required_capabilities"], ["C0", "C1", "C2"])
        self.assertEqual(set(ci_summary["providers"]), set(PROVIDER_IDS))
        self.assertNotIn(dummy_private_value, json.dumps(ci_summary, sort_keys=True))
        for provider in PROVIDER_IDS:
            self.assertEqual(
                set(ci_summary["providers"][provider]),
                {"C0", "C1", "C2"},
            )
            self.assertTrue(
                set(ci_summary["providers"][provider].values()) <= {"PASS", "FAIL"},
            )

    def test_provider_failure_still_cleans_before_safe_export(self) -> None:
        completed, artifacts = self._run_capability_probes(
            plain_text_c2_provider="copilot",
            create_sensitive=True,
        )

        self.assertNotEqual(completed.returncode, 0)
        self.assertEqual(artifacts["sensitive_survivors"], [])
        self.assertEqual(artifacts["ci_export_files"], ["capability-probe-summary.json"])
        self.assertEqual(artifacts["ci_summary"]["gate_status"], "FAIL")

    def test_cleanup_failure_is_observable_and_suppresses_ci_export(self) -> None:
        completed, artifacts = self._run_capability_probes(cleanup_failure=True)

        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("cleanup failed", completed.stderr.lower())
        self.assertEqual(artifacts["ci_export_files"], [])

    def test_sigterm_cleans_artifacts_and_terminates_provider_process_group(self) -> None:
        with self._capability_probe_fixture() as fixture:
            provider_pid_file = fixture["test_root"] / "provider.pid"
            sensitive_root = (
                fixture["artifact_parent"] / date.today().isoformat() / "synthetic-provider-home"
            )
            env = fixture["env"].copy()
            env.update(
                {
                    "FAKE_CREATE_SENSITIVE": "1",
                    "FAKE_SENSITIVE_ROOT": str(sensitive_root),
                    "FAKE_SLEEP_PROVIDER": "claude",
                    "FAKE_PROVIDER_PID_FILE": str(provider_pid_file),
                },
            )
            process = subprocess.Popen(
                ["bash", str(fixture["script_path"])],
                cwd=fixture["repo_root"],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            deadline = time.monotonic() + 5
            while not provider_pid_file.exists() and process.poll() is None:
                if time.monotonic() >= deadline:
                    process.kill()
                    self.fail("fake provider did not start")
                time.sleep(0.02)
            provider_pid = int(provider_pid_file.read_text(encoding="utf-8"))
            process.send_signal(signal.SIGTERM)
            _, stderr = process.communicate(timeout=5)

            self.assertEqual(process.returncode, 143, stderr)
            self.assertFalse(any(
                path.is_dir() and path.name in {"shell_snapshots", "sessions"}
                for path in fixture["artifact_parent"].rglob("*")
            ))
            self.assertFalse((fixture["export_dir"] / "capability-probe-summary.json").exists())
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                try:
                    os.kill(provider_pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.02)
            else:
                state = subprocess.run(
                    ["ps", "-o", "pid=,ppid=,pgid=,state=,command=", "-p", str(provider_pid)],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.fail(f"fake provider process survived SIGTERM: {state.stdout.strip()}")

    def test_repo_local_provider_homes_are_rejected_before_provider_launch(self) -> None:
        with self._capability_probe_fixture() as fixture:
            marker = fixture["test_root"] / "provider-invoked"
            repo_local = fixture["repo_root"] / "arbitrary-provider-home"
            repo_local.mkdir()
            alias = fixture["test_root"] / "repo-alias"
            alias.symlink_to(repo_local, target_is_directory=True)
            unsafe_cases = (
                ("HOME", str(repo_local)),
                ("HOME", "arbitrary-provider-home"),
                ("HOME", str(alias)),
                ("CODEX_HOME", str(repo_local)),
                ("CODEX_HOME", str(fixture["artifact_parent"] / "another-name")),
            )

            for variable, value in unsafe_cases:
                with self.subTest(variable=variable, value=value):
                    marker.unlink(missing_ok=True)
                    env = fixture["env"].copy()
                    env["FAKE_INVOCATION_MARKER"] = str(marker)
                    env[variable] = value
                    completed = subprocess.run(
                        ["bash", str(fixture["script_path"])],
                        cwd=fixture["repo_root"],
                        env=env,
                        capture_output=True,
                        text=True,
                        timeout=5,
                        check=False,
                    )
                    self.assertNotEqual(completed.returncode, 0)
                    self.assertFalse(marker.exists(), completed.stderr)

    def test_copilot_c0_probe_denies_write_and_shell_tools(self) -> None:
        script = (ROOT / "scripts" / "run_capability_probes.sh").read_text(encoding="utf-8")
        match = re.search(
            r'^run_probe "copilot" "C0" "([^"]+)" ""$',
            script,
            re.MULTILINE,
        )

        self.assertIsNotNone(match)
        command = match.group(1)
        self.assertNotIn("--allow-all-tools", command)
        self.assertIn("--deny-tool=write", command)
        self.assertIn("--deny-tool=shell", command)

    def test_parallel_benchmark_defaults_to_read_only_builtin_providers(self) -> None:
        script = (ROOT / "scripts" / "run_step5_parallel_benchmark.sh").read_text(encoding="utf-8")
        match = re.search(r'^PROVIDERS="\$\{1:-([^}]*)\}"$', script, re.MULTILINE)

        self.assertIsNotNone(match)
        providers = tuple(match.group(1).split(","))
        expected = tuple(
            provider for provider in PROVIDER_IDS
            if execution_permissions(provider, "read_only") is not None
        )
        self.assertEqual(providers, expected)
        self.assertNotIn("hermes", providers)
        self.assertIn("Hermes is covered by capability probes", script)

    def test_benchmark_workflow_is_manual_only_until_self_hosted_macos_returns(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "benchmark.yml").read_text(encoding="utf-8")

        self.assertNotRegex(workflow, r"(?m)^\s+schedule:")
        self.assertRegex(workflow, r"(?m)^\s+workflow_dispatch:")
        self.assertRegex(workflow, r"(?m)^\s+- self-hosted$")
        self.assertRegex(workflow, r"(?m)^\s+- macOS$")
        self.assertIn("Manual-only until a self-hosted macOS runner is available.", workflow)
        self.assertIn("Hermes is excluded because review/read_only fails closed", workflow)


if __name__ == "__main__":
    unittest.main()
