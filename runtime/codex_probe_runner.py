"""Bounded, authorization-gated runner for zero-model-call Codex CLI probes.

Library only: no CLI command, no persistence, no dispatch. It runs exactly `--version`, `--help` and
`exec --help` against one explicitly authorized executable, inside a scratch directory it creates and owns.
The scratch directory is a hygiene boundary, not an OS sandbox.

The runner is the only code allowed to build `runtime_probe` / `external_check` records from these probes.
It cannot establish who approved a probe: `ProbeAuthorization` is caller-asserted, and the evidence it
returns stays unauthenticated. Provider output is hashed and counted, never copied into results.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import stat
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path

from .codex_capabilities import CapabilityInspectionError, MAX_HELP_BYTES, _binary_identity, _stop
from .platform import prepare_spawn
from .runtime_validation import _sha256, _token, check_probe_authorization, parse_evidence
from .synapse_plan import PlanError, _external_path, _integer, _overlaps

ALLOWED_COMMANDS = (("--version",), ("--help",), ("exec", "--help"))
MAX_AUTHORIZATION_SECONDS = 900
MAX_COMMAND_TIMEOUT_SECONDS = 10
MAX_TOTAL_TIMEOUT_SECONDS = 30
MAX_PROTECTED_ROOTS = 8
MAX_SCAN_ENTRIES = 1000
PIPE_DRAIN_SECONDS = 0.5
GROUP_CLEANUP_WAIT_SECONDS = 1.0
_ISOLATED_DIRS = ("home", "codex-home", "tmp", "work")
_ENV_KEYS = frozenset({"PATH", "HOME", "CODEX_HOME", "TMPDIR", "TMP", "TEMP"})
_VERIFICATION = {
    "authentication": "not_checked", "model_acceptance": "not_verified", "reasoning_effort": "not_verified",
    "filesystem_enforcement": "not_verified", "network_restriction": "unknown",
    "session_persistence": "not_verified", "cancellation": "not_verified",
}
# Process-local single-use registry. Ids are never pruned, so a probe_id cannot be reused after its first
# authorization expires; the registry is bounded and fails closed when full.
MAX_CONSUMED_IDS = 4096
_CONSUMED: set[str] = set()
_CONSUMED_LOCK = threading.Lock()


class ProbeError(PlanError):
    pass


def _number(value: object, label: str) -> float:
    if type(value) not in (int, float):
        raise PlanError(f"invalid {label}")
    return float(value)


def _absolute(value: object, label: str) -> str:
    if not isinstance(value, str) or not value or not os.path.isabs(value):
        raise PlanError(f"{label} must be an absolute path")
    return value


@dataclass(frozen=True)
class ProbeAuthorization:
    """One explicit, time-limited, single-use permission for one zero-model-call probe run.

    The runner cannot tell who created this object. Construct it only after genuine user approval of these
    exact values, never from a file, environment variable, model response or provider output.
    """

    probe_id: str
    executable_path: str
    executable_sha256: str
    scratch_parent: str
    protected_roots: tuple[str, ...]
    execution_mode: str
    max_model_calls: int
    command_timeout_seconds: int
    total_timeout_seconds: int
    max_output_bytes: int
    issued_at: float
    expires_at: float

    def __post_init__(self) -> None:
        _token(self.probe_id, "probe id")
        _sha256(self.executable_sha256, "authorized executable hash")
        _absolute(self.executable_path, "executable_path")
        _absolute(self.scratch_parent, "scratch_parent")
        if (not isinstance(self.protected_roots, tuple) or not 1 <= len(self.protected_roots) <= MAX_PROTECTED_ROOTS):
            raise PlanError("protected_roots must be a tuple of 1 to 8 absolute paths")
        for root in self.protected_roots:
            _absolute(root, "protected root")
        if self.execution_mode != "read_only":
            raise PlanError("probe authorization must use read_only execution")
        _integer(self.max_model_calls, 0, 0, "max model calls")
        command = _integer(self.command_timeout_seconds, 1, MAX_COMMAND_TIMEOUT_SECONDS, "command timeout")
        total = _integer(self.total_timeout_seconds, 1, MAX_TOTAL_TIMEOUT_SECONDS, "total timeout")
        if total < command:
            raise PlanError("total timeout must not be below the command timeout")
        _integer(self.max_output_bytes, 1, MAX_HELP_BYTES, "output byte limit")
        lifetime = _number(self.expires_at, "expiry") - _number(self.issued_at, "issue time")
        if not 0 < lifetime <= MAX_AUTHORIZATION_SECONDS:
            raise PlanError("authorization lifetime must be between 0 and 900 seconds")


def _consume(authorization: ProbeAuthorization) -> None:
    with _CONSUMED_LOCK:
        if authorization.probe_id in _CONSUMED:
            raise ProbeError("authorization_reused")
        if len(_CONSUMED) >= MAX_CONSUMED_IDS:
            raise ProbeError("authorization_registry_full")
        _CONSUMED.add(authorization.probe_id)


def _file_state(path: Path) -> tuple[int, ...]:
    found = os.stat(path)
    return found.st_dev, found.st_ino, found.st_size, found.st_mtime_ns, found.st_ctime_ns


def _plain_path(raw: str, label: str) -> None:
    """Reject paths with symlinked components or non-canonical spelling."""
    if os.path.realpath(raw) != raw:
        raise ProbeError(f"{label} must not contain symlinks or non-canonical components")


def _bind_executable(authorization: ProbeAuthorization, forbidden: tuple[Path, ...]) -> tuple[Path, dict, tuple]:
    raw = authorization.executable_path
    if os.path.realpath(raw) != raw:
        raise ProbeError("executable_path_not_resolved")
    try:
        for root in forbidden:
            if _overlaps(Path(raw), root):
                raise ProbeError("executable_location_not_allowed")
        before = _file_state(Path(raw))
        binary, identity = _binary_identity(raw)
        after = _file_state(Path(raw))
    except (OSError, CapabilityInspectionError) as exc:
        raise ProbeError("executable_unusable") from exc
    if before != after or str(binary) != raw:
        raise ProbeError("executable_changed_during_binding")
    if identity["sha256"] != authorization.executable_sha256:
        raise ProbeError("executable_hash_mismatch")
    return binary, identity, after


def _safe_path(forbidden: tuple[Path, ...]) -> str:
    """Keep only absolute, existing PATH entries outside the checkout, protected roots and scratch parent.

    An empty or relative entry would make launcher lookups use the working directory.
    """
    kept = []
    for entry in os.environ.get("PATH", "").split(os.pathsep):
        if not entry or not os.path.isabs(entry):
            continue
        try:
            if any(_overlaps(Path(entry), root) for root in forbidden):
                continue
        except OSError:
            continue
        kept.append(entry)
    return os.pathsep.join(kept) or "/usr/bin:/bin"


def _build_env(scratch: Path, forbidden: tuple[Path, ...]) -> dict[str, str]:
    isolated = {"HOME": scratch / "home", "CODEX_HOME": scratch / "codex-home", "TMPDIR": scratch / "tmp",
                "TMP": scratch / "tmp", "TEMP": scratch / "tmp"}
    return {"PATH": _safe_path(forbidden), **{key: str(value) for key, value in isolated.items()}}


def _check_env(env: dict[str, str], scratch: Path) -> None:
    """Fail closed unless the environment is exactly the allowlist with isolated paths under scratch."""
    prefix = str(scratch) + os.sep
    if set(env) != _ENV_KEYS or any(
            not os.path.realpath(env[key]).startswith(prefix) for key in _ENV_KEYS - {"PATH"}) or not all(
            os.path.isabs(entry) for entry in env["PATH"].split(os.pathsep)):
        raise ProbeError("environment_not_isolated")


def _group_alive(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _wait_group_gone(pgid: int) -> bool:
    end = time.monotonic() + GROUP_CLEANUP_WAIT_SECONDS
    while _group_alive(pgid):
        if time.monotonic() >= end:
            return False
        time.sleep(0.01)
    return True


def _run_command(binary: Path, args: tuple[str, ...], env: dict[str, str], cwd: str, timeout: float,
                 limit: int, cancel: threading.Event | None) -> dict[str, object]:
    """Run one allowlisted command and observe its outcome. Provider output is hashed, never returned."""
    if args not in ALLOWED_COMMANDS:
        raise ProbeError("command_not_allowed")
    command, options = prepare_spawn([str(binary), *args], env)
    try:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0, **options)
    except OSError:
        return {"status": "spawn_failed", "exit_code": None, "output_bytes": 0, "output_sha256": None,
                "survivors": False, "cleanup": "not_needed"}
    assert process.stdout is not None
    pgid = process.pid
    collected = bytearray()
    over_limit = threading.Event()

    def collect() -> None:
        while True:
            try:
                chunk = process.stdout.read(4096)
            except (OSError, ValueError):
                return
            if not chunk:
                return
            collected.extend(chunk[:limit + 1 - len(collected)])
            if len(collected) > limit:
                over_limit.set()
                return

    reader = threading.Thread(target=collect, daemon=True)
    reader.start()
    status: str | None = None
    survivors = False
    cleanup = "not_needed"
    try:
        deadline = time.monotonic() + timeout
        while process.poll() is None:
            if cancel is not None and cancel.is_set():
                status = "cancelled"
            elif over_limit.is_set():
                status = "output_limit"
            elif time.monotonic() >= deadline:
                status = "timeout"
            if status:
                break
            time.sleep(0.01)
        if status is None:
            reader.join(PIPE_DRAIN_SECONDS)
            if over_limit.is_set():
                status = "output_limit"
            # A live reader means a descendant still holds the pipe; a live group means any survivor.
            survivors = reader.is_alive() or _group_alive(pgid)
        if status or survivors:
            _stop(process)
            cleanup = "verified" if _wait_group_gone(pgid) else "failed"
            reader.join(PIPE_DRAIN_SECONDS)
            if reader.is_alive():
                # Something outside the killed group still holds the pipe: it was not cleaned up.
                cleanup = "failed"
    finally:
        if process.poll() is None:
            _stop(process)
        process.stdout.close()
        reader.join(PIPE_DRAIN_SECONDS)
    data = bytes(collected[:limit])
    if status is None:
        if process.returncode != 0:
            status = "nonzero_exit"
        else:
            try:
                data.decode("utf-8")
                status = "ok"
            except UnicodeError:
                status = "not_utf8"
    return {"status": status, "exit_code": process.returncode, "output_bytes": len(data),
            "output_sha256": hashlib.sha256(data).hexdigest(), "survivors": survivors, "cleanup": cleanup}


def _count_unexpected(scratch: Path, identity: tuple[int, int]) -> int:
    """Count entries other than the empty isolation directories. Names are never reported."""
    try:
        top = os.lstat(scratch)
    except OSError:
        return 1
    if not stat.S_ISDIR(top.st_mode) or (top.st_dev, top.st_ino) != identity:
        return 1
    unexpected = scanned = 0
    pending = [scratch]
    while pending:
        directory = pending.pop()
        try:
            with os.scandir(directory) as entries:
                for entry in entries:
                    scanned += 1
                    if scanned > MAX_SCAN_ENTRIES:
                        return max(unexpected, 1)
                    descend = entry.is_dir(follow_symlinks=False)
                    if directory == scratch and descend and entry.name in _ISOLATED_DIRS:
                        pending.append(Path(entry.path))
                        continue
                    unexpected += 1
                    if descend:
                        pending.append(Path(entry.path))
        except OSError:
            unexpected += 1
    return unexpected


def _cleanup(scratch: Path, identity: tuple[int, int]) -> str:
    """Remove only the directory this run created, and only if it is still that directory."""
    try:
        current = os.lstat(scratch)
    except FileNotFoundError:
        return "complete"
    except OSError:
        return "failed"
    if not stat.S_ISDIR(current.st_mode) or (current.st_dev, current.st_ino) != identity:
        return "skipped_identity_mismatch"
    try:
        shutil.rmtree(scratch)
    except OSError:
        return "failed"
    return "complete"


def _create_scratch(authorization: ProbeAuthorization, repo: Path, protected: tuple[Path, ...],
                    parent: Path) -> tuple[Path, tuple[int, int]]:
    scratch = Path(tempfile.mkdtemp(prefix="synapse-codex-probe-", dir=parent))
    try:
        created = os.lstat(scratch)
        if not stat.S_ISDIR(created.st_mode) or stat.S_IMODE(created.st_mode) != 0o700:
            raise ProbeError("scratch_not_private")
        check = check_probe_authorization(
            {"schema_version": 1, "probe_id": authorization.probe_id,
             "executable_sha256": authorization.executable_sha256, "capabilities": ["executable_identity"],
             "scratch_root": str(scratch), "execution_mode": "read_only", "max_model_calls": 0,
             "network": "none", "hard_timeout_seconds": 300},
            repo_root=repo, protected_roots=protected, executable_sha256=authorization.executable_sha256)
        if not check["probe_authorized"]:
            raise ProbeError("scratch_authorization_denied")
        for name in _ISOLATED_DIRS:
            os.mkdir(scratch / name, 0o700)
    except BaseException:
        try:
            os.rmdir(scratch)
        except OSError:
            pass
        raise
    return scratch, (created.st_dev, created.st_ino)


def _evidence(authorization: ProbeAuthorization, identity_ok: bool, observations: set[str],
              unexpected_scratch: int, parent_changes: int) -> list[dict[str, object]]:
    def record(capability: str, source: str, outcome: str, observed: str | None = None) -> dict[str, object]:
        return {"capability": capability, "source": source, "outcome": outcome,
                "executable_sha256": authorization.executable_sha256, "requested": None,
                "observed": observed, "probe_id": authorization.probe_id}

    records = [record("executable_identity", "runtime_probe", "pass" if identity_ok else "fail")]
    records.extend(record("process_termination", "runtime_probe", "fail", name)
                   for name in ("timeout", "cancelled", "descendant_survived") if name in observations)
    if "malformed_output" in observations:
        records.append(record("output_format", "runtime_probe", "fail", "malformed_output"))
    if unexpected_scratch:
        records.append(record("session_persistence", "external_check", "fail", "session_data_written"))
    if parent_changes:
        records.append(record("read_only_filesystem", "external_check", "fail", "write_succeeded"))
    return parse_evidence(records)


def run_codex_probe(authorization: ProbeAuthorization, *, repo_root: Path,
                    cancel: threading.Event | None = None) -> dict[str, object]:
    """Run the three allowlisted non-model commands once and return bounded, unauthenticated evidence.

    Raises ProbeError (or PlanError) on any refusal. A valid authorization is consumed before any
    filesystem or process work, so a refused or failed run cannot be retried with the same probe id.
    """
    if os.name != "posix":
        raise ProbeError("unsupported_platform")
    if not isinstance(authorization, ProbeAuthorization):
        raise ProbeError("authorization_missing")
    now = time.time()
    if authorization.issued_at > now:
        raise ProbeError("authorization_not_yet_valid")
    if authorization.expires_at <= now:
        raise ProbeError("authorization_expired")
    _consume(authorization)

    if not repo_root.is_dir():
        raise ProbeError("repository must be an existing directory")
    repo = repo_root.resolve(strict=True)
    try:
        protected = tuple(Path(root).resolve(strict=True) for root in authorization.protected_roots)
    except OSError as exc:
        raise ProbeError("protected roots must be existing directories") from exc
    if not all(root.is_dir() for root in protected):
        raise ProbeError("protected roots must be existing directories")
    _plain_path(authorization.scratch_parent, "scratch_parent")
    parent = _external_path(authorization.scratch_parent, repo, "scratch_parent", directory=True)
    try:
        if any(_overlaps(parent, root) for root in protected):
            raise ProbeError("scratch_parent overlaps a protected root")
        binary, identity, state = _bind_executable(authorization, (repo, *protected, parent))
        parent_before = set(os.listdir(parent))
    except OSError as exc:
        raise ProbeError("scratch_parent could not be inspected") from exc

    scratch, scratch_id = _create_scratch(authorization, repo, protected, parent)
    commands: list[dict[str, object]] = []
    stop_reason: str | None = None
    identity_ok = True
    observations: set[str] = set()
    unexpected = 0
    group_survivors = False
    group_cleanup = "not_needed"
    try:
        env = _build_env(scratch, (repo, *protected, parent))
        _check_env(env, scratch)
        total_deadline = time.monotonic() + authorization.total_timeout_seconds
        for args in ALLOWED_COMMANDS:
            entry: dict[str, object] = {"argv": list(args), "status": "not_run", "exit_code": None,
                                        "output_bytes": 0, "output_sha256": None}
            commands.append(entry)
            if stop_reason is not None:
                continue
            remaining = total_deadline - time.monotonic()
            try:
                changed = _file_state(binary) != state
            except OSError:
                changed = True
            if cancel is not None and cancel.is_set():
                stop_reason = "cancelled"
            elif remaining <= 0:
                stop_reason = "total_timeout"
            elif changed:
                identity_ok, stop_reason = False, "executable_changed"
            if stop_reason is not None:
                continue
            outcome = _run_command(binary, args, env, str(scratch / "work"),
                                   min(remaining, authorization.command_timeout_seconds),
                                   authorization.max_output_bytes, cancel)
            entry.update({key: outcome[key] for key in ("status", "exit_code", "output_bytes", "output_sha256")})
            if outcome["survivors"]:
                group_survivors = True
                observations.add("descendant_survived")
            if outcome["cleanup"] != "not_needed":
                group_cleanup = str(outcome["cleanup"])
                if group_cleanup == "failed":
                    observations.add("descendant_survived")
            unexpected = _count_unexpected(scratch, scratch_id)
            observations.update({"timeout": {"timeout"}, "cancelled": {"cancelled"},
                                 "output_limit": {"malformed_output"}, "not_utf8": {"malformed_output"}
                                 }.get(str(outcome["status"]), set()))
            if outcome["status"] != "ok":
                stop_reason = "command_failed"
            elif outcome["survivors"]:
                stop_reason = "descendant_survived"
            elif unexpected:
                stop_reason = "unexpected_scratch_entries"
    finally:
        try:
            now_state = _file_state(binary)
            _, after = _binary_identity(str(binary))
            identity_ok = (identity_ok and now_state == state and after["sha256"] == identity["sha256"])
        except (OSError, CapabilityInspectionError):
            identity_ok = False
        cleanup = _cleanup(scratch, scratch_id)
    try:
        parent_changes = len((set(os.listdir(parent)) - {scratch.name}) ^ parent_before)
    except OSError:
        parent_changes = 1
    isolation: dict[str, object] = {
        "kind": "temporary_directory_not_os_sandbox", "mode": "0700", "cleanup": cleanup,
        "scratch_unexpected_entries": unexpected, "parent_unexpected_changes": parent_changes}
    if cleanup not in ("complete", "skipped_identity_mismatch"):
        isolation["scratch_leftover"] = str(scratch)
    return {
        "schema_version": 1, "provider": "codex", "probe_id": authorization.probe_id,
        "authorization": {"origin": "caller_asserted_unverified", "single_use_scope": "process",
                          "expires_at": authorization.expires_at},
        "executable": {"path": identity["path"], "sha256": identity["sha256"],
                       "size_bytes": identity["size_bytes"], "identity_scope": identity["identity_scope"],
                       "identity_check": "unchanged" if identity_ok else "changed_during_probe",
                       "native_binary_identity": "not_established", "race_free_enforcement": False},
        "limits": {"max_model_calls": 0, "declared_execution_mode": "read_only",
                   "execution_mode_enforced": False,
                   "command_timeout_seconds": authorization.command_timeout_seconds,
                   "total_timeout_seconds": authorization.total_timeout_seconds,
                   "max_output_bytes": authorization.max_output_bytes},
        "isolation": isolation,
        "process": {"descendant_containment": "process_group_only",
                    "group_survivors_detected": group_survivors, "group_cleanup": group_cleanup},
        "commands": commands, "stop_reason": stop_reason,
        "evidence": _evidence(authorization, identity_ok, observations, unexpected, parent_changes),
        "verification": dict(_VERIFICATION),
        "evidence_authenticated": False, "dispatch_authorized": False,
    }
