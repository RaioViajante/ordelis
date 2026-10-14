#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import stat
import sys
import tempfile
from datetime import date
from pathlib import Path


PROVIDERS = (
    "claude",
    "codex",
    "gemini",
    "opencode",
    "qwen",
    "hermes",
    "pi",
    "copilot",
    "grok",
    "cursor",
    "agy",
)
REQUIRED_CAPABILITIES = ("C0", "C1", "C2")
SENSITIVE_DIRECTORY_NAMES = {"sessions", "shell_snapshots"}
VALID_STATUSES = {"PASS", "FAIL"}
VALID_TARGETS = {"linux", "macos", "windows"}


def _resolve(path: str, cwd: Path) -> Path:
    candidate = Path(path).expanduser()
    if not candidate.is_absolute():
        candidate = cwd / candidate
    return candidate.resolve(strict=False)


def _within(candidate: Path, root: Path) -> bool:
    return candidate == root or root in candidate.parents


def validate_layout(
    *,
    repo_root: str,
    artifact_parent: str,
    run_root: str,
    cwd: str,
    home: str | None,
    codex_home: str | None,
) -> None:
    resolved_cwd = Path(cwd).resolve(strict=True)
    resolved_repo = _resolve(repo_root, resolved_cwd)
    resolved_parent = _resolve(artifact_parent, resolved_cwd)
    resolved_run = _resolve(run_root, resolved_cwd)

    if resolved_run.parent != resolved_parent:
        raise ValueError("probe run root must be a direct child of the artifact root")
    if resolved_run == Path(resolved_run.anchor):
        raise ValueError("probe run root cannot be a filesystem root")
    if resolved_parent == resolved_repo or resolved_parent in resolved_repo.parents:
        raise ValueError("probe artifact root cannot contain the repository")

    provider_homes: list[tuple[str, Path]] = []
    for name, value in (("HOME", home), ("CODEX_HOME", codex_home)):
        if not value:
            continue
        resolved_home = _resolve(value, resolved_cwd)
        provider_homes.append((name, resolved_home))
        if _within(resolved_home, resolved_repo):
            raise ValueError(f"{name} resolves inside the repository")
        if _within(resolved_home, resolved_parent):
            raise ValueError(f"{name} resolves inside the probe artifact root")

def cleanup_sensitive_directories(*, run_root: str, artifact_parent: str) -> None:
    root = Path(run_root).resolve(strict=True)
    parent = Path(artifact_parent).resolve(strict=True)
    if root.parent != parent:
        raise ValueError("refusing cleanup outside the verified artifact root")

    def raise_walk_error(error: OSError) -> None:
        raise error

    for current, directories, _ in os.walk(root, topdown=True, followlinks=False, onerror=raise_walk_error):
        current_path = Path(current)
        for name in tuple(directories):
            if name not in SENSITIVE_DIRECTORY_NAMES:
                continue
            candidate = current_path / name
            directories.remove(name)
            if candidate.is_symlink():
                candidate.unlink()
                continue
            resolved_candidate = candidate.resolve(strict=True)
            if not _within(resolved_candidate, root):
                raise ValueError("refusing cleanup outside the verified run root")
            shutil.rmtree(candidate)


def _validated_summary(*, run_root: str, run_date: str, target_os: str) -> dict[str, object]:
    root = Path(run_root).resolve(strict=True)
    if date.fromisoformat(run_date).isoformat() != run_date:
        raise ValueError("invalid probe run date")
    if target_os not in VALID_TARGETS:
        raise ValueError("unsupported target OS")

    statuses: dict[str, dict[str, str]] = {}
    for provider in PROVIDERS:
        provider_statuses: dict[str, str] = {}
        for capability in REQUIRED_CAPABILITIES:
            result_path = root / provider / capability / "result.json"
            payload = json.loads(result_path.read_text(encoding="utf-8"))
            if not isinstance(payload, dict):
                raise ValueError(f"invalid result document for {provider}/{capability}")
            if payload.get("provider") != provider:
                raise ValueError(f"provider mismatch for {provider}/{capability}")
            if payload.get("capability") != capability:
                raise ValueError(f"capability mismatch for {provider}/{capability}")
            if payload.get("os") != target_os:
                raise ValueError(f"target OS mismatch for {provider}/{capability}")
            status = payload.get("status")
            if status not in VALID_STATUSES:
                raise ValueError(f"invalid status for {provider}/{capability}")
            provider_statuses[capability] = status
        statuses[provider] = provider_statuses

    gate_status = "PASS" if all(
        status == "PASS"
        for provider_statuses in statuses.values()
        for status in provider_statuses.values()
    ) else "FAIL"
    return {
        "schema_version": 1,
        "run_date": run_date,
        "target_os": target_os,
        "required_capabilities": list(REQUIRED_CAPABILITIES),
        "providers": statuses,
        "gate_status": gate_status,
    }


def stage_ci_summary(*, run_root: str, staging_parent: str, run_date: str, target_os: str) -> Path:
    root = Path(run_root).resolve(strict=True)
    parent_path = Path(staging_parent)
    if parent_path.is_symlink():
        raise ValueError("trusted staging parent cannot be a symlink")
    parent = parent_path.resolve(strict=True)
    if not parent.is_dir():
        raise ValueError("trusted staging parent must be a directory")
    if _within(parent, root):
        raise ValueError("trusted staging parent cannot be inside the probe run root")

    summary = _validated_summary(
        run_root=run_root,
        run_date=run_date,
        target_os=target_os,
    )
    expected = (json.dumps(summary, indent=2, sort_keys=True) + "\n").encode("utf-8")
    staging_dir = Path(tempfile.mkdtemp(prefix="capability-probe-export-", dir=parent))
    staging_dir.chmod(0o700)
    output_path = staging_dir / "capability-probe-summary.json"
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW

    try:
        descriptor = os.open(output_path, flags, 0o600)
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(expected)
            handle.flush()
            os.fsync(handle.fileno())

        directory_stat = staging_dir.lstat()
        candidate_stat = output_path.lstat()
        if stat.S_IMODE(directory_stat.st_mode) != 0o700:
            raise ValueError("trusted staging directory must have mode 0700")
        if directory_stat.st_uid != os.getuid():
            raise ValueError("trusted staging directory must be owned by the current user")
        if output_path.is_symlink() or not stat.S_ISREG(candidate_stat.st_mode):
            raise ValueError("staged CI summary must be a regular file")
        if stat.S_IMODE(candidate_stat.st_mode) != 0o600:
            raise ValueError("staged CI summary must have mode 0600")
        if candidate_stat.st_uid != os.getuid():
            raise ValueError("staged CI summary must be owned by the current user")
        if {path.name for path in staging_dir.iterdir()} != {output_path.name}:
            raise ValueError("trusted staging directory contains unexpected files")

        read_flags = os.O_RDONLY
        if hasattr(os, "O_NOFOLLOW"):
            read_flags |= os.O_NOFOLLOW
        descriptor = os.open(output_path, read_flags)
        with os.fdopen(descriptor, "rb") as handle:
            opened_stat = os.fstat(handle.fileno())
            actual = handle.read()
        if (opened_stat.st_dev, opened_stat.st_ino) != (
            candidate_stat.st_dev,
            candidate_stat.st_ino,
        ):
            raise ValueError("staged CI summary changed during validation")
        if actual != expected:
            raise ValueError("staged CI summary does not match trusted reconstruction")
        if json.loads(actual) != summary:
            raise ValueError("staged CI summary failed canonical validation")
    except Exception:
        output_path.unlink(missing_ok=True)
        try:
            staging_dir.rmdir()
        except OSError:
            pass
        raise
    return output_path


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate = subparsers.add_parser("validate-layout")
    validate.add_argument("--repo-root", required=True)
    validate.add_argument("--artifact-parent", required=True)
    validate.add_argument("--run-root", required=True)
    validate.add_argument("--cwd", required=True)
    validate.add_argument("--home")
    validate.add_argument("--codex-home")

    cleanup = subparsers.add_parser("cleanup")
    cleanup.add_argument("--run-root", required=True)
    cleanup.add_argument("--artifact-parent", required=True)

    stage = subparsers.add_parser("stage")
    stage.add_argument("--run-root", required=True)
    stage.add_argument("--staging-parent", required=True)
    stage.add_argument("--run-date", required=True)
    stage.add_argument("--target-os", required=True)

    args = parser.parse_args()
    try:
        if args.command == "validate-layout":
            validate_layout(
                repo_root=args.repo_root,
                artifact_parent=args.artifact_parent,
                run_root=args.run_root,
                cwd=args.cwd,
                home=args.home,
                codex_home=args.codex_home,
            )
        elif args.command == "cleanup":
            cleanup_sensitive_directories(
                run_root=args.run_root,
                artifact_parent=args.artifact_parent,
            )
        else:
            output = stage_ci_summary(
                run_root=args.run_root,
                staging_parent=args.staging_parent,
                run_date=args.run_date,
                target_os=args.target_os,
            )
            print(output)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"capability probe artifact sanitization failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
