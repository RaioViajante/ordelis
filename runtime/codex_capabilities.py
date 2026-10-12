"""Explicit, non-authenticating inspection of Codex CLI help for planning."""

from __future__ import annotations

import hashlib
import os
import re
import shutil
import signal
import subprocess
import tempfile
import threading
from pathlib import Path

from .adapters.codex import CodexAdapter
from .contracts import TaskInput
from .execution_modes import execution_permissions
from .platform import prepare_spawn


MAX_BINARY_BYTES = 512 * 1024 * 1024
MAX_HELP_BYTES = 65536
MAX_VERSION_BYTES = 256
PROBE_TIMEOUT_SECONDS = 5
_VERSION = re.compile(r"^codex-cli (\d+)\.(\d+)\.(\d+)(?:-[A-Za-z0-9.-]+)?$")
_OPTION_LINE = re.compile(r"^ {2,6}(?:(-[A-Za-z0-9]),\s+)?(--[a-z][a-z0-9-]*)(?=[\s,]|$)")
_SECTION_HEADING = re.compile(r"^[A-Za-z][A-Za-z ]+:$")
_CONTROLS = (
    "--model", "--config", "--sandbox", "--ask-for-approval", "--json",
    "--output-schema", "--output-last-message", "--skip-git-repo-check",
    "--ignore-user-config", "--ignore-rules", "--ephemeral", "--add-dir",
    "--dangerously-bypass-approvals-and-sandbox", "-C",
)


class CapabilityInspectionError(ValueError):
    pass


def _binary_identity(binary: str | None) -> tuple[Path, dict[str, object]]:
    selected = binary or shutil.which("codex")
    if not selected:
        raise CapabilityInspectionError("Codex CLI was not found on PATH")
    path = Path(selected)
    if not path.is_absolute():
        raise CapabilityInspectionError("Codex binary must be an absolute path")
    try:
        resolved = path.resolve(strict=True)
        if not resolved.is_file() or not os.access(resolved, os.X_OK):
            raise CapabilityInspectionError("Codex binary must be an executable file")
        before = resolved.stat()
        if not 0 < before.st_size <= MAX_BINARY_BYTES:
            raise CapabilityInspectionError("Codex binary exceeds the inspection size limit")
        digest = hashlib.sha256()
        read_bytes = 0
        with resolved.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                read_bytes += len(chunk)
                if read_bytes > MAX_BINARY_BYTES:
                    raise CapabilityInspectionError("Codex binary exceeds the inspection size limit")
                digest.update(chunk)
        after = resolved.stat()
    except OSError as exc:
        raise CapabilityInspectionError("Codex binary could not be inspected") from exc
    if read_bytes != before.st_size or (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
            after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
        raise CapabilityInspectionError("Codex binary changed during inspection")
    return resolved, {"path": str(resolved), "size_bytes": before.st_size,
                      "sha256": digest.hexdigest()}


def _stop(process: subprocess.Popen[bytes]) -> None:
    try:
        if os.name == "nt":
            from .platform import kill_process
            kill_process(process)
        else:
            os.killpg(process.pid, signal.SIGKILL)
    except (OSError, ProcessLookupError):
        if process.poll() is None:
            process.kill()
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=1)


def _help_command(binary: Path, args: tuple[str, ...], limit: int, cwd: str,
                  env: dict[str, str]) -> str:
    command, options = prepare_spawn([str(binary), *args], env)
    try:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   **options)
    except OSError as exc:
        raise CapabilityInspectionError("Codex help process could not start") from exc
    chunks: list[bytes] = []
    size = 0
    over_limit = False

    def collect() -> None:
        nonlocal size, over_limit
        assert process.stdout is not None
        while True:
            chunk = process.stdout.read(4096)
            if not chunk:
                return
            size += len(chunk)
            if size > limit:
                over_limit = True
                return
            chunks.append(chunk)

    reader = threading.Thread(target=collect, daemon=True)
    reader.start()
    reader.join(PROBE_TIMEOUT_SECONDS)
    timed_out = reader.is_alive()
    if timed_out or over_limit:
        _stop(process)
        assert process.stdout is not None
        process.stdout.close()
        reader.join(1)
        raise CapabilityInspectionError("Codex help process timed out" if timed_out and not over_limit
                                        else "Codex help output exceeds the byte limit")
    try:
        code = process.wait(timeout=1)
    except subprocess.TimeoutExpired as exc:
        _stop(process)
        raise CapabilityInspectionError("Codex help process did not exit") from exc
    finally:
        assert process.stdout is not None
        process.stdout.close()
    if code != 0:
        raise CapabilityInspectionError("Codex help command failed")
    try:
        return b"".join(chunks).decode("utf-8")
    except UnicodeError as exc:
        raise CapabilityInspectionError("Codex help output is not UTF-8") from exc


def _option_section(help_text: str, flag: str) -> str:
    lines = help_text.splitlines()
    start: int | None = None
    for index, line in enumerate(lines):
        option = _OPTION_LINE.match(line)
        if start is not None and (option or _SECTION_HEADING.match(line)):
            return "\n".join(lines[start:index])
        if option and flag in option.groups():
            start = index
    return "\n".join(lines[start:]) if start is not None else ""


def _advertised(help_text: str, flag: str) -> bool:
    return bool(_option_section(help_text, flag))


def inspect_codex_cli(binary: str | None = None) -> dict[str, object]:
    """Run only version/help commands in an isolated temporary home; never run a task."""
    path, identity = _binary_identity(binary)
    with tempfile.TemporaryDirectory(prefix="mco-codex-help-") as home:
        env = {key: value for key, value in os.environ.items()
               if key in {"PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "LANG", "LC_ALL"}}
        env.update({"HOME": home, "USERPROFILE": home, "CODEX_HOME": home, "TMPDIR": home,
                    "TMP": home, "TEMP": home})
        version_text = _help_command(path, ("--version",), MAX_VERSION_BYTES, home, env).strip()
        root_help = _help_command(path, ("--help",), MAX_HELP_BYTES, home, env)
        exec_help = _help_command(path, ("exec", "--help"), MAX_HELP_BYTES, home, env)
    match = _VERSION.fullmatch(version_text)
    if match is None or "Usage: codex" not in root_help or "Usage: codex exec" not in exec_help:
        raise CapabilityInspectionError("Codex version or help output is malformed")
    adapter = CodexAdapter()
    permissions = execution_permissions("codex", "read_only")
    assert permissions is not None
    command = adapter._build_command(TaskInput(
        task_id="inspection-only", prompt="<not-executed>", repo_root="<repo-root>",
        target_paths=["."], timeout_seconds=1,
        metadata={"model": "<model>", "provider_permissions": permissions,
                  "output_schema_path": "<schema>"},
    ))
    required = sorted({item for item in command if item.startswith("-")})
    observed = {flag: _advertised(root_help, flag) or _advertised(exec_help, flag)
                for flag in _CONTROLS}
    # A help flag is evidence of a parser option, not evidence of its runtime effect.
    before_exec = command.index("exec")
    missing = [flag for flag in required if not _advertised(
        root_help if command.index(flag) < before_exec else exec_help, flag)]
    sandbox_section = _option_section(exec_help, "--sandbox")
    approval_section = _option_section(root_help, "--ask-for-approval")
    read_only_advertised = re.search(r"\[possible values:[^\]]*\bread-only\b", sandbox_section,
                                     re.IGNORECASE) is not None
    never_advertised = re.search(r"(?m)^\s+-?\s*never(?:\s|:|$)", approval_section) is not None
    minimum = adapter.capabilities().min_supported_version
    assert minimum is not None
    version_supported = tuple(map(int, match.groups())) >= tuple(map(int, minimum.split(".")))
    reasons = (["version_below_adapter_minimum"] if not version_supported else [])
    reasons.extend(f"adapter_flag_not_advertised:{flag}" for flag in missing)
    if not read_only_advertised:
        reasons.append("required_value_not_advertised:sandbox=read-only")
    if not never_advertised:
        reasons.append("required_value_not_advertised:approval_policy=never")
    return {
        "schema_version": 1,
        "provider": "codex",
        "binary": identity,
        "cli_version": version_text,
        "adapter": {"minimum_version": minimum,
                    "model_keys": adapter.supported_model_keys(),
                    "permission_keys": adapter.supported_permission_keys(),
                    "effort_values": [], "execution_flags": required,
                    "cancellation": "process_termination"},
        "observed_cli_controls": observed,
        "observed_required_values": {"sandbox_read_only": read_only_advertised,
                                     "approval_never": never_advertised},
        "reasoning_effort": {"dedicated_flag_advertised": False,
                             "generic_config_override_advertised": observed["--config"],
                             "specific_key_verified_by_help": False},
        "runtime_verification": {"authentication": "not_checked", "model_availability": "not_verified",
                                 "effort_behavior": "not_verified", "filesystem_enforcement": "not_verified",
                                 "output_events": "not_verified", "cancellation": "not_verified"},
        "inspection": {"status": "supported_controls_observed" if not reasons else "unsupported",
                       "blocking_reasons": reasons, "dispatch_authorized": False},
    }
