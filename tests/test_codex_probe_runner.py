from __future__ import annotations

import hashlib
import json
import os
import stat
import sys
import tempfile
import threading
import time
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from runtime import codex_probe_runner as runner
from runtime.codex_probe_runner import ALLOWED_COMMANDS, ProbeAuthorization, ProbeError, run_codex_probe
from runtime.runtime_validation import evaluate_validation, parse_evidence
from runtime.synapse_plan import PlanError

SECRET_MARKER = "PROVIDER-OUTPUT-MARKER-7731"
LEAKY_ENV = {
    "OPENAI_API_KEY": "sk-test", "CODEX_API_KEY": "ck-test", "ANTHROPIC_API_KEY": "ak-test",
    "GITHUB_TOKEN": "gh-test", "AWS_SECRET_ACCESS_KEY": "aws-test", "HTTPS_PROXY": "http://proxy.invalid",
    "https_proxy": "http://proxy.invalid", "ALL_PROXY": "http://proxy.invalid", "NO_PROXY": "x",
    "SYNAPSE_UNLISTED": "1", "CODEX_HOME": "/should/not/leak", "HOME": "/should/not/leak",
}

FAKE_TEMPLATE = r'''#!{python}
import json, os, subprocess, sys, time
SIDE = {side!r}
PARENT = {parent!r}
BEHAVIOR = {behavior!r}
SELF = os.path.realpath(sys.argv[0])
args = sys.argv[1:]
with open(os.path.join(SIDE, "argv.log"), "a") as log:
    log.write(json.dumps(args) + "\n")
with open(os.path.join(SIDE, "env.json"), "w") as dump:
    json.dump({{"env": dict(os.environ), "cwd": os.path.realpath(os.getcwd()),
               "stdin_eof": sys.stdin.read() == ""}}, dump)
with open(os.path.join(SIDE, "pid"), "w") as pid:
    pid.write(str(os.getpid()))
if BEHAVIOR == "hang":
    time.sleep(60)
if BEHAVIOR == "slow":
    time.sleep(3)
if BEHAVIOR == "big":
    sys.stdout.write("x" * 200000)
    sys.exit(0)
if BEHAVIOR == "exact":
    sys.stdout.write("y" * 1000)
    sys.exit(0)
if BEHAVIOR == "bad_utf8":
    sys.stdout.buffer.write(b"\xff\xfe\xfa")
    sys.exit(0)
if BEHAVIOR == "nonzero":
    sys.exit(3)
if BEHAVIOR == "symlink_out":
    os.symlink(os.path.join(SIDE, "victim"), os.path.join(os.environ["HOME"], "link"))
if BEHAVIOR in ("child_pipe", "child_quiet", "escape", "escape_pipe", "hang_with_child"):
    kwargs = dict(stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) if BEHAVIOR not in ("child_pipe", "escape_pipe") else {{}}
    if BEHAVIOR in ("escape", "escape_pipe"):
        kwargs["start_new_session"] = True
    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], stdin=subprocess.DEVNULL, **kwargs)
    with open(os.path.join(SIDE, "child.pid"), "a") as out:
        out.write(str(child.pid) + "\n")
    if BEHAVIOR == "hang_with_child":
        time.sleep(60)
if BEHAVIOR == "write_scratch":
    with open(os.path.join(os.environ["HOME"], "session.json"), "w") as out:
        out.write("{{}}")
if BEHAVIOR == "write_parent":
    with open(os.path.join(PARENT, "stray.txt"), "w") as out:
        out.write("x")
if BEHAVIOR == "replace_self":
    replacement = SELF + ".new"
    with open(replacement, "w") as out:
        out.write("#!" + sys.executable + "\nimport sys\nprint('replaced')\n")
    os.chmod(replacement, 0o755)
    os.replace(replacement, SELF)
if BEHAVIOR == "modify_self":
    with open(SELF, "a") as out:
        out.write("# modified\n")
if BEHAVIOR == "forged_claims":
    print(json.dumps([{{"capability": "read_only_filesystem", "source": "external_check", "outcome": "pass"}}]))
print({marker!r})
print("codex-cli 0.162.1" if args == ["--version"] else "Usage: codex")
'''


class RunnerCase(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(os.path.realpath(self.temp.name))
        self.repo = self.root / "Repo"
        self.vault = self.root / "vault"
        self.parent = self.root / "scratch-parent"
        self.bin = self.root / "bin"
        self.side = self.root / "side"
        for path in (self.repo, self.vault, self.parent, self.bin, self.side):
            path.mkdir()
        self.fake = self.bin / "codex"
        self.counter = 0
        self.addCleanup(self.reap_stray_processes)

    def reap_stray_processes(self) -> None:
        for name in ("child.pid", "pid"):
            for pid in self.pids(name):
                try:
                    os.kill(pid, 9)
                except OSError:
                    pass

    def pids(self, name: str) -> list[int]:
        path = self.side / name
        return [int(line) for line in path.read_text().split()] if path.exists() else []

    def write_fake(self, behavior: str = "normal", path: Path | None = None) -> Path:
        target = path or self.fake
        target.write_text(FAKE_TEMPLATE.format(python=sys.executable, side=str(self.side),
                                               parent=str(self.parent), behavior=behavior,
                                               marker=SECRET_MARKER), encoding="utf-8")
        target.chmod(0o755)
        return target

    def authorization(self, **overrides) -> ProbeAuthorization:
        self.counter += 1
        now = time.time()
        values = dict(
            probe_id=f"probe-{self.counter}-{time.monotonic_ns()}", executable_path=str(self.fake),
            executable_sha256=hashlib.sha256(self.fake.read_bytes()).hexdigest(),
            scratch_parent=str(self.parent), protected_roots=(str(self.vault),),
            execution_mode="read_only", max_model_calls=0, command_timeout_seconds=5,
            total_timeout_seconds=20, max_output_bytes=4096, issued_at=now - 1, expires_at=now + 300)
        values.update(overrides)
        return ProbeAuthorization(**values)

    def run_probe(self, behavior: str = "normal", *, cancel=None, **overrides) -> dict:
        self.write_fake(behavior)
        return run_codex_probe(self.authorization(**overrides), repo_root=self.repo, cancel=cancel)

    def argv_log(self) -> list[list[str]]:
        path = self.side / "argv.log"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def record_kinds(self, result: dict) -> set[tuple[str, str, str, str | None]]:
        return {(item["capability"], item["source"], item["outcome"], item["observed"])
                for item in result["evidence"]}

    def assertDead(self, pid_file: str) -> None:
        pids = self.pids(pid_file)
        self.assertTrue(pids, f"{pid_file} was never written")
        for pid in pids:
            for _ in range(100):
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    break
                time.sleep(0.02)
            else:
                self.fail(f"process {pid} from {pid_file} is still alive")


class AllowlistTests(RunnerCase):
    def test_exactly_three_vectors_are_allowed(self) -> None:
        self.assertEqual(set(ALLOWED_COMMANDS), {("--version",), ("--help",), ("exec", "--help")})

    def test_runner_spawns_only_the_allowlisted_vectors_in_order(self) -> None:
        result = self.run_probe()
        self.assertEqual(self.argv_log(), [["--version"], ["--help"], ["exec", "--help"]])
        self.assertEqual([item["argv"] for item in result["commands"]],
                         [["--version"], ["--help"], ["exec", "--help"]])

    def test_any_other_vector_is_rejected_before_spawn(self) -> None:
        self.write_fake()
        for args in ((), ("exec",), ("exec", "do something"), ("exec", "--help", "--json"), ("login",),
                     ("--help", "--version"), ("--model", "x"), ("EXEC", "--help"), ["--help"]):
            with self.subTest(args=args), self.assertRaisesRegex(ProbeError, "command_not_allowed"):
                runner._run_command(self.fake, args, {}, str(self.root), 1, 100, None)
        self.assertEqual(self.argv_log(), [])

    def test_public_api_accepts_no_command_arguments(self) -> None:
        self.write_fake()
        with self.assertRaises(TypeError):
            run_codex_probe(self.authorization(), repo_root=self.repo, args=("exec", "hi"))  # type: ignore[call-arg]
        self.assertEqual(self.argv_log(), [])


class AuthorizationTests(RunnerCase):
    def test_missing_or_wrong_type_authorization_is_denied(self) -> None:
        self.write_fake()
        for value in (None, {"probe_id": "x"}, "yes", True):
            with self.subTest(value=value), self.assertRaisesRegex(ProbeError, "authorization_missing"):
                run_codex_probe(value, repo_root=self.repo)  # type: ignore[arg-type]
        self.assertEqual(self.argv_log(), [])
        self.assertEqual(list(self.parent.iterdir()), [])

    def test_environment_variable_does_not_authorize(self) -> None:
        self.write_fake()
        with patch.dict(os.environ, {"SYNAPSE_PROBE_AUTHORIZED": "1", "SYNAPSE_PROBE_ID": "x"}):
            with self.assertRaisesRegex(ProbeError, "authorization_missing"):
                run_codex_probe(None, repo_root=self.repo)  # type: ignore[arg-type]

    def test_expired_authorization_is_denied(self) -> None:
        self.write_fake()
        now = time.time()
        auth = self.authorization(issued_at=now - 600, expires_at=now - 1)
        with self.assertRaisesRegex(ProbeError, "authorization_expired"):
            run_codex_probe(auth, repo_root=self.repo)
        self.assertEqual(self.argv_log(), [])

    def test_not_yet_valid_authorization_is_denied(self) -> None:
        self.write_fake()
        now = time.time()
        auth = self.authorization(issued_at=now + 100, expires_at=now + 200)
        with self.assertRaisesRegex(ProbeError, "authorization_not_yet_valid"):
            run_codex_probe(auth, repo_root=self.repo)

    def test_lifetime_is_capped_at_fifteen_minutes(self) -> None:
        self.write_fake()
        now = time.time()
        self.authorization(issued_at=now - 1, expires_at=now - 1 + 900)
        for lifetime in (901, 3600, 0, -5):
            with self.subTest(lifetime=lifetime), self.assertRaises(PlanError):
                self.authorization(issued_at=now - 1, expires_at=now - 1 + lifetime)

    def test_authorization_is_single_use_even_after_a_failed_run(self) -> None:
        self.write_fake("nonzero")
        auth = self.authorization()
        first = run_codex_probe(auth, repo_root=self.repo)
        self.assertEqual(first["commands"][0]["status"], "nonzero_exit")
        for attempt in (auth, replace(auth, command_timeout_seconds=2)):
            with self.subTest(attempt=attempt.command_timeout_seconds), \
                    self.assertRaisesRegex(ProbeError, "authorization_reused"):
                run_codex_probe(attempt, repo_root=self.repo)
        self.assertEqual(len(self.argv_log()), 1)

    def test_probe_id_stays_consumed_after_the_first_authorization_expires(self) -> None:
        self.write_fake()
        first = self.authorization(probe_id="replay-1")
        run_codex_probe(first, repo_root=self.repo)
        later = time.time() + 2000
        second = self.authorization(probe_id="replay-1", issued_at=later - 1, expires_at=later + 300)
        with patch.object(runner.time, "time", return_value=later):
            with self.assertRaisesRegex(ProbeError, "authorization_reused"):
                run_codex_probe(second, repo_root=self.repo)

    def test_refused_run_after_validation_still_consumes_the_authorization(self) -> None:
        self.write_fake()
        auth = self.authorization(executable_sha256="0" * 64)
        with self.assertRaisesRegex(ProbeError, "executable_hash_mismatch"):
            run_codex_probe(auth, repo_root=self.repo)
        with self.assertRaisesRegex(ProbeError, "authorization_reused"):
            run_codex_probe(auth, repo_root=self.repo)

    def test_out_of_range_or_unsafe_fields_are_rejected(self) -> None:
        self.write_fake()
        bad = [
            {"max_model_calls": 1}, {"max_model_calls": True}, {"max_model_calls": -1},
            {"execution_mode": "write"}, {"execution_mode": "yolo"}, {"execution_mode": None},
            {"command_timeout_seconds": 0}, {"command_timeout_seconds": 11},
            {"total_timeout_seconds": 31}, {"total_timeout_seconds": 3, "command_timeout_seconds": 5},
            {"max_output_bytes": 0}, {"max_output_bytes": 65537}, {"protected_roots": ()},
            {"protected_roots": [str(self.vault)]}, {"protected_roots": ("relative",)},
            {"probe_id": "has space"}, {"executable_sha256": "abc"}, {"executable_path": "codex"},
            {"scratch_parent": "relative/dir"}, {"issued_at": "now"}, {"expires_at": None},
        ]
        for overrides in bad:
            with self.subTest(overrides=overrides), self.assertRaises(PlanError):
                self.authorization(**overrides)

    def test_authorization_is_immutable(self) -> None:
        self.write_fake()
        auth = self.authorization()
        with self.assertRaises(Exception):
            auth.max_model_calls = 5  # type: ignore[misc]


class ExecutableBindingTests(RunnerCase):
    def test_hash_mismatch_denies_without_spawning_or_creating_scratch(self) -> None:
        self.write_fake()
        auth = self.authorization(executable_sha256="1" * 64)
        with self.assertRaisesRegex(ProbeError, "executable_hash_mismatch"):
            run_codex_probe(auth, repo_root=self.repo)
        self.assertEqual(self.argv_log(), [])
        self.assertEqual(list(self.parent.iterdir()), [])

    def test_authorization_for_a_different_executable_is_denied(self) -> None:
        self.write_fake()
        other = self.write_fake("hang", self.bin / "other")
        auth = self.authorization(executable_path=str(other))
        with self.assertRaisesRegex(ProbeError, "executable_hash_mismatch"):
            run_codex_probe(auth, repo_root=self.repo)
        self.assertEqual(self.argv_log(), [])

    def test_symlinked_executable_path_is_not_the_resolved_file(self) -> None:
        self.write_fake()
        link = self.bin / "link"
        link.symlink_to(self.fake)
        with self.assertRaisesRegex(ProbeError, "executable_path_not_resolved"):
            run_codex_probe(self.authorization(executable_path=str(link)), repo_root=self.repo)
        self.assertEqual(self.argv_log(), [])

    def test_executable_inside_checkout_protected_root_or_scratch_parent_is_refused(self) -> None:
        self.write_fake()
        for directory in (self.repo, self.vault, self.parent):
            with self.subTest(directory=directory.name):
                inside = self.write_fake("normal", directory / "codex")
                auth = self.authorization(executable_path=str(inside))
                with self.assertRaisesRegex(ProbeError, "executable_location_not_allowed"):
                    run_codex_probe(auth, repo_root=self.repo)
        self.assertEqual(self.argv_log(), [])

    def test_missing_or_non_executable_file_is_refused(self) -> None:
        self.write_fake()
        auth = self.authorization()
        self.fake.chmod(0o644)
        with self.assertRaisesRegex(ProbeError, "executable_unusable"):
            run_codex_probe(auth, repo_root=self.repo)


class ScratchPathTests(RunnerCase):
    def refused(self, parent: Path | str, pattern: str = "scratch_parent") -> None:
        self.write_fake()
        with self.assertRaisesRegex(PlanError, pattern):
            run_codex_probe(self.authorization(scratch_parent=str(parent)), repo_root=self.repo)
        self.assertEqual(self.argv_log(), [])

    def test_parent_that_does_not_exist_or_is_a_file_is_refused(self) -> None:
        (self.root / "file").write_text("x")
        self.refused(self.root / "missing")
        self.refused(self.root / "file")

    def test_parent_overlapping_checkout_or_protected_root_is_refused(self) -> None:
        inner = self.repo / "inner"
        inner.mkdir()
        vault_inner = self.vault / "inner"
        vault_inner.mkdir()
        for parent in (self.repo, inner, self.root, self.vault, vault_inner):
            with self.subTest(parent=str(parent)):
                self.refused(parent, "overlap")
        self.assertEqual(list(inner.iterdir()), [])
        self.assertEqual(list(vault_inner.iterdir()), [])

    def test_symlinked_parent_and_symlinked_ancestor_are_refused(self) -> None:
        real = self.root / "real"
        (real / "child").mkdir(parents=True)
        link = self.root / "link"
        link.symlink_to(real)
        self.refused(link)
        self.refused(link / "child", "symlink")
        self.assertEqual(list(real.joinpath("child").iterdir()), [])

    def test_case_aliased_checkout_path_is_refused(self) -> None:
        alias = self.root / "REPO"
        if not alias.exists():
            self.skipTest("filesystem is case-sensitive; case aliases cannot exist")
        inner = self.repo / "inner"
        inner.mkdir()
        self.refused(self.root / "REPO" / "inner", "overlap")
        self.refused(alias, "overlap")
        self.assertEqual(list(inner.iterdir()), [])

    def test_case_aliased_vault_path_is_refused(self) -> None:
        if not (self.root / "VAULT").exists():
            self.skipTest("filesystem is case-sensitive; case aliases cannot exist")
        self.refused(self.root / "VAULT", "overlap")

    def test_preexisting_entries_in_the_parent_are_preserved(self) -> None:
        (self.parent / "mine.txt").write_text("keep")
        (self.parent / "dir").mkdir()
        (self.parent / "dir" / "inner.txt").write_text("keep")
        result = self.run_probe()
        self.assertEqual(result["isolation"]["cleanup"], "complete")
        self.assertEqual(sorted(path.name for path in self.parent.iterdir()), ["dir", "mine.txt"])
        self.assertEqual((self.parent / "dir" / "inner.txt").read_text(), "keep")
        self.assertEqual(result["isolation"]["parent_unexpected_changes"], 0)

    def test_nonempty_fresh_scratch_is_refused_and_not_deleted(self) -> None:
        self.write_fake()
        user_dir = self.parent / "user-created"
        user_dir.mkdir()
        (user_dir / "precious.txt").write_text("keep")
        user_dir.chmod(0o755)
        with patch.object(runner.tempfile, "mkdtemp", return_value=str(user_dir)):
            with self.assertRaisesRegex(ProbeError, "scratch_not_private"):
                run_codex_probe(self.authorization(), repo_root=self.repo)
        user_dir.chmod(0o700)
        with patch.object(runner.tempfile, "mkdtemp", return_value=str(user_dir)):
            with self.assertRaisesRegex(PlanError, "empty"):
                run_codex_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual((user_dir / "precious.txt").read_text(), "keep")
        self.assertEqual(self.argv_log(), [])

    def test_scratch_is_private_fresh_and_distinct_per_run(self) -> None:
        self.run_probe()
        first = json.loads((self.side / "env.json").read_text())["cwd"]
        self.run_probe()
        second = json.loads((self.side / "env.json").read_text())["cwd"]
        self.assertNotEqual(first, second)
        self.assertTrue(first.startswith(str(self.parent) + os.sep))
        self.assertFalse(first.startswith(str(self.repo)))
        with patch.object(runner, "_cleanup", return_value="failed"):
            result = self.run_probe()
        leftover = Path(result["isolation"]["scratch_leftover"])
        self.assertEqual(stat.S_IMODE(leftover.stat().st_mode), 0o700)
        runner.shutil.rmtree(leftover)


class EnvironmentTests(RunnerCase):
    def test_child_sees_only_the_minimal_isolated_environment(self) -> None:
        with patch.dict(os.environ, LEAKY_ENV):
            result = self.run_probe()
        seen = json.loads((self.side / "env.json").read_text())
        self.assertLessEqual(set(seen["env"]), {"PATH", "HOME", "CODEX_HOME", "TMPDIR", "TMP", "TEMP",
                                                "__CF_USER_TEXT_ENCODING", "LC_CTYPE"})
        for key in LEAKY_ENV:
            if key not in {"CODEX_HOME", "HOME"}:
                self.assertNotIn(key, seen["env"])
        scratch_prefix = str(self.parent) + os.sep
        locations = {seen["env"][key] for key in ("HOME", "CODEX_HOME", "TMPDIR", "TMP", "TEMP")}
        self.assertEqual(len(locations), 3)
        for value in locations | {seen["cwd"]}:
            self.assertTrue(os.path.realpath(value).startswith(scratch_prefix), value)
        self.assertNotIn("/should/not/leak", json.dumps(seen))
        self.assertIs(seen["stdin_eof"], True)
        self.assertEqual(result["isolation"]["cleanup"], "complete")

    def test_leaky_environment_fails_closed_before_spawn(self) -> None:
        self.write_fake()
        real_build = runner._build_env
        for extra in ({"OPENAI_API_KEY": "sk"}, {"HTTPS_PROXY": "http://p"}, {"PATH": "x", "HOME": "/elsewhere"}):
            def leaky(scratch: Path, forbidden=(), extra=extra) -> dict[str, str]:
                return {**real_build(scratch, forbidden), **extra}
            with self.subTest(extra=list(extra)), patch.object(runner, "_build_env", leaky):
                with self.assertRaisesRegex(ProbeError, "environment_not_isolated"):
                    run_codex_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(self.argv_log(), [])
        self.assertEqual(list(self.parent.iterdir()), [])


class PathTests(RunnerCase):
    def child_path(self, value: str | None) -> list[str]:
        env = {key: item for key, item in os.environ.items() if key != "PATH"}
        if value is not None:
            env["PATH"] = value
        with patch.dict(os.environ, env, clear=True):
            self.run_probe()
        return json.loads((self.side / "env.json").read_text())["env"]["PATH"].split(os.pathsep)

    def test_unsafe_path_entries_are_removed(self) -> None:
        dirs = {name: base / "tools" for name, base in (("repo", self.repo), ("vault", self.vault),
                                                         ("parent", self.parent))}
        for directory in dirs.values():
            directory.mkdir()
        (self.root / "relative").mkdir()
        elsewhere = self.root / "elsewhere"
        elsewhere.mkdir()
        raw = os.pathsep.join(["", ".", "relative", "missing/dir", str(dirs["repo"]), str(dirs["vault"]),
                               str(dirs["parent"]), str(self.repo), str(elsewhere), "/usr/bin"])
        self.assertEqual(self.child_path(raw), [str(elsewhere), "/usr/bin"])

    def test_empty_or_missing_path_never_falls_back_to_the_working_directory(self) -> None:
        for value in ("", None, ":", "."):
            with self.subTest(value=value):
                entries = self.child_path(value)
                self.assertEqual(entries, ["/usr/bin", "/bin"])
                self.assertTrue(all(os.path.isabs(entry) for entry in entries))

    def test_relative_path_entry_fails_the_environment_check(self) -> None:
        scratch = self.parent / "s"
        env = runner._build_env(scratch, ())
        env["PATH"] = "bin" + os.pathsep + env["PATH"]
        with self.assertRaisesRegex(ProbeError, "environment_not_isolated"):
            runner._check_env(env, scratch)


class OutputTests(RunnerCase):
    def test_oversized_output_is_cut_off_and_the_process_is_killed(self) -> None:
        result = self.run_probe("big", max_output_bytes=1024)
        command = result["commands"][0]
        self.assertEqual(command["status"], "output_limit")
        self.assertLessEqual(command["output_bytes"], 1024)
        self.assertIn(("output_format", "runtime_probe", "fail", "malformed_output"), self.record_kinds(result))
        self.assertEqual([item["status"] for item in result["commands"][1:]], ["not_run", "not_run"])
        self.assertEqual(len(self.argv_log()), 1)

    def test_output_exactly_at_the_limit_is_accepted(self) -> None:
        result = self.run_probe("exact", max_output_bytes=1000)
        self.assertEqual([item["status"] for item in result["commands"]], ["ok"] * 3)
        self.assertEqual(result["commands"][0]["output_bytes"], 1000)

    def test_invalid_utf8_is_reported_and_stops_the_run(self) -> None:
        result = self.run_probe("bad_utf8")
        self.assertEqual(result["commands"][0]["status"], "not_utf8")
        self.assertIn(("output_format", "runtime_probe", "fail", "malformed_output"), self.record_kinds(result))
        self.assertEqual(len(self.argv_log()), 1)

    def test_nonzero_exit_is_reported_without_success_evidence(self) -> None:
        result = self.run_probe("nonzero")
        self.assertEqual(result["commands"][0]["status"], "nonzero_exit")
        self.assertEqual(result["commands"][0]["exit_code"], 3)

    def test_provider_output_is_never_copied_into_the_result(self) -> None:
        result = self.run_probe()
        self.assertNotIn(SECRET_MARKER, json.dumps(result))
        digest = hashlib.sha256(f"{SECRET_MARKER}\ncodex-cli 0.162.1\n".encode()).hexdigest()
        self.assertEqual(result["commands"][0]["output_sha256"], digest)


class ProcessTests(RunnerCase):
    def test_hanging_process_times_out_and_is_killed(self) -> None:
        started = time.monotonic()
        result = self.run_probe("hang", command_timeout_seconds=3, total_timeout_seconds=10)
        self.assertLess(time.monotonic() - started, 8)
        self.assertEqual([item["status"] for item in result["commands"]], ["timeout", "not_run", "not_run"])
        self.assertIn(("process_termination", "runtime_probe", "fail", "timeout"), self.record_kinds(result))
        self.assertDead("pid")
        self.assertEqual(result["process"]["group_cleanup"], "verified")

    def test_timeout_kills_the_whole_process_group(self) -> None:
        result = self.run_probe("hang_with_child", command_timeout_seconds=3, total_timeout_seconds=10)
        self.assertEqual(result["commands"][0]["status"], "timeout")
        self.assertDead("pid")
        self.assertDead("child.pid")

    def test_total_timeout_budget_is_enforced_across_commands(self) -> None:
        result = self.run_probe("slow", command_timeout_seconds=5, total_timeout_seconds=5)
        self.assertEqual([item["status"] for item in result["commands"]], ["ok", "timeout", "not_run"])

    def test_surviving_child_holding_the_pipe_is_reported_and_killed(self) -> None:
        result = self.run_probe("child_pipe")
        self.assertIn(("process_termination", "runtime_probe", "fail", "descendant_survived"),
                      self.record_kinds(result))
        self.assertTrue(result["process"]["group_survivors_detected"])
        self.assertEqual(result["process"]["group_cleanup"], "verified")
        self.assertDead("child.pid")
        self.assertEqual(len(self.argv_log()), 1)

    def test_surviving_child_without_the_pipe_is_found_through_the_process_group(self) -> None:
        result = self.run_probe("child_quiet")
        self.assertTrue(result["process"]["group_survivors_detected"])
        self.assertIn(("process_termination", "runtime_probe", "fail", "descendant_survived"),
                      self.record_kinds(result))
        self.assertDead("child.pid")

    def test_descendant_that_leaves_the_process_group_is_not_detected(self) -> None:
        # Documents a limitation: a new session escapes the process-group check entirely.
        result = self.run_probe("escape")
        pid = self.pids("child.pid")[0]
        self.assertTrue(self._alive(pid))
        self.assertFalse(result["process"]["group_survivors_detected"])
        self.assertEqual(result["process"]["descendant_containment"], "process_group_only")
        self.assertNotIn("descendant_survived", {item["observed"] for item in result["evidence"]})

    @staticmethod
    def _alive(pid: int) -> bool:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        return True

    def test_escaped_descendant_holding_the_pipe_is_not_reported_as_cleaned_up(self) -> None:
        result = self.run_probe("escape_pipe")
        self.addCleanup(self.reap_stray_processes)
        self.assertTrue(result["process"]["group_survivors_detected"])
        self.assertEqual(result["process"]["group_cleanup"], "failed")
        self.assertIn(("process_termination", "runtime_probe", "fail", "descendant_survived"),
                      self.record_kinds(result))

    def test_group_cleanup_failure_is_reported(self) -> None:
        with patch.object(runner, "_group_alive", return_value=True), \
                patch.object(runner, "GROUP_CLEANUP_WAIT_SECONDS", 0.05):
            result = self.run_probe()
        self.assertEqual(result["process"]["group_cleanup"], "failed")
        self.assertIn(("process_termination", "runtime_probe", "fail", "descendant_survived"),
                      self.record_kinds(result))

    def test_cancellation_stops_and_cleans_up(self) -> None:
        self.write_fake("hang_with_child")
        cancel = threading.Event()
        pid_file = self.side / "child.pid"

        def trigger() -> None:
            for _ in range(500):
                if pid_file.exists() and pid_file.read_text():
                    break
                time.sleep(0.01)
            cancel.set()
        thread = threading.Thread(target=trigger)
        thread.start()
        started = time.monotonic()
        result = run_codex_probe(self.authorization(), repo_root=self.repo, cancel=cancel)
        thread.join()
        self.assertLess(time.monotonic() - started, 5)
        self.assertEqual([item["status"] for item in result["commands"]], ["cancelled", "not_run", "not_run"])
        self.assertIn(("process_termination", "runtime_probe", "fail", "cancelled"), self.record_kinds(result))
        self.assertDead("pid")
        self.assertDead("child.pid")
        self.assertEqual(result["isolation"]["cleanup"], "complete")

    def test_cancellation_before_start_runs_nothing(self) -> None:
        cancel = threading.Event()
        cancel.set()
        result = self.run_probe(cancel=cancel)
        self.assertEqual([item["status"] for item in result["commands"]], ["not_run"] * 3)
        self.assertEqual(self.argv_log(), [])
        self.assertEqual(result["isolation"]["cleanup"], "complete")


class ExecutableIdentityTests(RunnerCase):
    def test_replacement_during_a_probe_is_detected(self) -> None:
        result = self.run_probe("replace_self")
        self.assertEqual(result["executable"]["identity_check"], "changed_during_probe")
        self.assertEqual([item["outcome"] for item in result["evidence"]
                          if item["capability"] == "executable_identity"], ["fail"])
        self.assertEqual(len(self.argv_log()), 1, "no further command may run after replacement")
        self.assertEqual(result["commands"][1]["status"], "not_run")

    def test_in_place_modification_is_detected(self) -> None:
        result = self.run_probe("modify_self")
        self.assertEqual(result["executable"]["identity_check"], "changed_during_probe")
        self.assertEqual(len(self.argv_log()), 1)

    def test_identical_bytes_in_a_new_file_are_still_reported_as_changed(self) -> None:
        self.write_fake()
        original = self.fake.read_bytes()
        auth = self.authorization()
        real_spawn = runner._run_command

        def swap_then_run(*args, **kwargs):
            outcome = real_spawn(*args, **kwargs)
            replacement = self.bin / "codex.new"
            replacement.write_bytes(original)
            replacement.chmod(0o755)
            os.replace(replacement, self.fake)
            return outcome
        with patch.object(runner, "_run_command", swap_then_run):
            result = run_codex_probe(auth, repo_root=self.repo)
        self.assertEqual(result["executable"]["identity_check"], "changed_during_probe")

    def test_unchanged_executable_is_bound_by_hash_with_honest_scope(self) -> None:
        result = self.run_probe()
        executable = result["executable"]
        self.assertEqual(executable["identity_check"], "unchanged")
        self.assertEqual(executable["sha256"], hashlib.sha256(self.fake.read_bytes()).hexdigest())
        self.assertEqual(executable["path"], str(self.fake))
        self.assertEqual(executable["identity_scope"], "resolved_executable_file")
        self.assertEqual(executable["native_binary_identity"], "not_established")
        self.assertEqual(executable["race_free_enforcement"], False)


class FilesystemObservationTests(RunnerCase):
    def test_write_into_the_isolated_home_is_reported_and_removed(self) -> None:
        result = self.run_probe("write_scratch")
        self.assertGreaterEqual(result["isolation"]["scratch_unexpected_entries"], 1)
        self.assertIn(("session_persistence", "external_check", "fail", "session_data_written"),
                      self.record_kinds(result))
        self.assertEqual(len(self.argv_log()), 1)
        self.assertEqual(result["isolation"]["cleanup"], "complete")
        self.assertEqual(list(self.parent.iterdir()), [])
        self.assertNotIn("session.json", json.dumps(result))

    def test_file_written_next_to_the_scratch_directory_is_reported_and_not_deleted(self) -> None:
        result = self.run_probe("write_parent")
        self.assertEqual(result["isolation"]["parent_unexpected_changes"], 1)
        self.assertIn(("read_only_filesystem", "external_check", "fail", "write_succeeded"),
                      self.record_kinds(result))
        self.assertEqual((self.parent / "stray.txt").read_text(), "x")
        self.assertNotIn("stray.txt", json.dumps(result))

    def test_clean_run_leaves_nothing_and_writes_nothing_into_the_checkout(self) -> None:
        before = sorted(str(path) for path in self.root.rglob("*") if self.side not in path.parents
                        and self.bin not in path.parents and path not in (self.side, self.bin))
        result = self.run_probe()
        after = sorted(str(path) for path in self.root.rglob("*") if self.side not in path.parents
                       and self.bin not in path.parents and path not in (self.side, self.bin))
        self.assertEqual(before, after)
        self.assertEqual(result["isolation"]["scratch_unexpected_entries"], 0)
        self.assertEqual(result["isolation"]["cleanup"], "complete")
        self.assertNotIn("scratch_leftover", result["isolation"])

    def test_symlink_created_in_scratch_is_not_followed_or_deleted_through(self) -> None:
        victim = self.side / "victim"
        victim.mkdir()
        (victim / "precious.txt").write_text("keep")
        result = self.run_probe("symlink_out")
        self.assertEqual((victim / "precious.txt").read_text(), "keep")
        self.assertGreaterEqual(result["isolation"]["scratch_unexpected_entries"], 1)
        self.assertEqual(result["isolation"]["cleanup"], "complete")
        self.assertEqual(list(self.parent.iterdir()), [])

    def test_parent_swapped_for_a_symlink_into_a_protected_root_is_caught_after_creation(self) -> None:
        self.write_fake()
        real = tempfile.mkdtemp
        moved = self.root / "moved-parent"

        def swap_then_make(*args, **kwargs):
            self.parent.rename(moved)
            self.parent.symlink_to(self.vault)
            return real(*args, **kwargs)
        with patch.object(runner.tempfile, "mkdtemp", swap_then_make):
            with self.assertRaises(PlanError):
                run_codex_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(list(self.vault.iterdir()), [])
        self.assertEqual(self.argv_log(), [])

    def test_replaced_scratch_directory_is_not_deleted(self) -> None:
        victim = self.root / "victim"
        victim.mkdir()
        (victim / "precious.txt").write_text("keep")
        real_run = runner._run_command

        def swap_scratch(binary, args, env, cwd, *rest):
            outcome = real_run(binary, args, env, cwd, *rest)
            scratch = Path(cwd).parent
            runner.shutil.rmtree(scratch)
            os.symlink(victim, scratch)
            return outcome
        with patch.object(runner, "_run_command", swap_scratch):
            result = self.run_probe()
        self.assertEqual((victim / "precious.txt").read_text(), "keep")
        self.assertEqual(result["isolation"]["cleanup"], "skipped_identity_mismatch")
        self.assertGreaterEqual(result["isolation"]["scratch_unexpected_entries"], 1)
        for path in self.parent.iterdir():
            path.unlink()


class EvidenceTests(RunnerCase):
    def evaluate(self, result: dict, **kwargs) -> dict:
        kwargs.setdefault("model", "model-1")
        return evaluate_validation(result["executable"]["sha256"], result["evidence"],
                                   probe_id=result["probe_id"], **kwargs)

    def test_records_pass_the_closed_schema_and_come_only_from_observation_sources(self) -> None:
        result = self.run_probe()
        self.assertEqual(parse_evidence(result["evidence"]), result["evidence"])
        self.assertTrue(all(item["source"] in {"runtime_probe", "external_check"} for item in result["evidence"]))
        self.assertTrue(all(item["executable_sha256"] == result["executable"]["sha256"]
                            and item["probe_id"] == result["probe_id"] for item in result["evidence"]))

    def test_clean_run_establishes_only_executable_identity(self) -> None:
        result = self.run_probe()
        self.assertEqual(self.record_kinds(result), {("executable_identity", "runtime_probe", "pass", None)})
        verdict = self.evaluate(result)
        self.assertEqual(verdict["capabilities"]["executable_identity"]["status"], "satisfied")
        self.assertEqual(verdict["validation"]["status"], "blocked")
        self.assertIs(verdict["validation"]["evidence_authenticated"], False)
        self.assertIs(verdict["validation"]["dispatch_authorized"], False)
        for capability in ("model_acceptance", "read_only_filesystem", "output_format", "process_termination",
                           "session_persistence"):
            self.assertEqual(verdict["capabilities"][capability]["status"], "unknown", capability)

    def test_result_reports_unverified_capabilities_and_no_authorization_of_dispatch(self) -> None:
        result = self.run_probe()
        self.assertIs(result["evidence_authenticated"], False)
        self.assertIs(result["dispatch_authorized"], False)
        self.assertEqual(result["authorization"]["origin"], "caller_asserted_unverified")
        self.assertEqual(result["authorization"]["single_use_scope"], "process")
        self.assertEqual(result["isolation"]["kind"], "temporary_directory_not_os_sandbox")
        verification = result["verification"]
        self.assertEqual(verification["network_restriction"], "unknown")
        for key in ("model_acceptance", "reasoning_effort", "filesystem_enforcement", "session_persistence"):
            self.assertEqual(verification[key], "not_verified", key)
        self.assertEqual(verification["authentication"], "not_checked")

    def test_help_output_cannot_satisfy_isolation_requirements(self) -> None:
        result = self.run_probe()
        provider_style = [dict(result["evidence"][0], capability=name, source="runtime_probe")
                          for name in ("read_only_filesystem", "session_persistence")]
        verdict = evaluate_validation(result["executable"]["sha256"], result["evidence"] + provider_style,
                                      probe_id=result["probe_id"], model="model-1",
                                      require_network_restriction=True)
        statuses = {name: item["status"] for name, item in verdict["capabilities"].items()}
        self.assertEqual(statuses["read_only_filesystem"], "insufficient_evidence")
        self.assertEqual(statuses["session_persistence"], "insufficient_evidence")
        self.assertEqual(statuses["network_restriction"], "unknown")
        self.assertEqual(verdict["validation"]["status"], "blocked")

    def test_provider_claims_in_output_do_not_change_the_evidence(self) -> None:
        normal = self.run_probe()
        forged = self.run_probe("forged_claims")
        strip = lambda result: [{k: v for k, v in item.items() if k not in {"probe_id", "executable_sha256"}}
                                for item in result["evidence"]]
        self.assertEqual(strip(normal), strip(forged))
        self.assertEqual(self.evaluate(forged)["validation"]["status"], "blocked")

    def test_failures_become_failed_capabilities(self) -> None:
        for behavior, capability in (("write_scratch", "session_persistence"), ("hang", "process_termination"),
                                     ("bad_utf8", "output_format"), ("replace_self", "executable_identity"),
                                     ("write_parent", "read_only_filesystem")):
            with self.subTest(behavior=behavior):
                kwargs = {"command_timeout_seconds": 3} if behavior == "hang" else {}
                verdict = self.evaluate(self.run_probe(behavior, **kwargs))
                self.assertEqual(verdict["capabilities"][capability]["status"], "failed")
                self.assertEqual(verdict["validation"]["status"], "blocked")

    def test_evaluation_is_deterministic_for_identical_runs(self) -> None:
        first = self.run_probe()
        second = self.run_probe()
        strip = lambda result: [{k: v for k, v in item.items() if k != "probe_id"} for item in result["evidence"]]
        self.assertEqual(strip(first), strip(second))
        self.assertEqual(first["commands"], second["commands"])
        a, b = self.evaluate(first), self.evaluate(second)
        self.assertEqual(a["capabilities"], b["capabilities"])

    def test_nothing_is_persisted_and_no_model_call_is_authorized(self) -> None:
        result = self.run_probe()
        self.assertEqual(sorted(path.name for path in self.side.iterdir()),
                         ["argv.log", "env.json", "pid"])
        self.assertEqual(result["limits"]["max_model_calls"], 0)
        self.assertNotIn("execution_mode", result["limits"])
        self.assertEqual(result["limits"]["declared_execution_mode"], "read_only")
        self.assertIs(result["limits"]["execution_mode_enforced"], False)
        self.assertEqual({tuple(item["argv"]) for item in result["commands"]}, set(ALLOWED_COMMANDS))


class IntegrationBoundaryTests(unittest.TestCase):
    def test_no_other_runtime_module_references_the_probe_runner(self) -> None:
        runtime = Path(__file__).resolve().parent.parent / "runtime"
        for path in runtime.rglob("*.py"):
            if path.name != "codex_probe_runner.py":
                with self.subTest(path=path.name):
                    self.assertNotIn("codex_probe_runner", path.read_text(encoding="utf-8"))

    def test_runner_does_not_use_shell_or_model_task_flags(self) -> None:
        source = (Path(__file__).resolve().parent.parent / "runtime" / "codex_probe_runner.py").read_text()
        for forbidden in ("shell=True", "os.system", "os.popen", "--model", "--json", "login"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main()
