#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
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
    export_dir: str | None,
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

    if export_dir:
        resolved_export = _resolve(export_dir, resolved_cwd)
        if _within(resolved_export, resolved_repo):
            raise ValueError("CI export directory resolves inside the repository")
        if _within(resolved_export, resolved_parent):
            raise ValueError("CI export directory resolves inside the probe artifact root")
        for name, resolved_home in provider_homes:
            if name == "CODEX_HOME" and _within(resolved_export, resolved_home):
                raise ValueError("CI export directory resolves inside CODEX_HOME")


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


def export_ci_summary(*, run_root: str, output: str, run_date: str, target_os: str) -> None:
    root = Path(run_root).resolve(strict=True)
    output_path = Path(output)
    if date.fromisoformat(run_date).isoformat() != run_date:
        raise ValueError("invalid probe run date")
    if target_os not in VALID_TARGETS:
        raise ValueError("unsupported target OS")
    if _within(output_path.resolve(strict=False), root):
        raise ValueError("CI summary must be staged outside the probe artifact root")

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
    summary = {
        "schema_version": 1,
        "run_date": run_date,
        "target_os": target_os,
        "required_capabilities": list(REQUIRED_CAPABILITIES),
        "providers": statuses,
        "gate_status": gate_status,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    if output_path.is_symlink():
        raise ValueError("CI summary path cannot be a symlink")
    descriptor, temporary_name = tempfile.mkstemp(
        dir=output_path.parent,
        prefix=f".{output_path.name}.",
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            json.dump(summary, handle, indent=2, sort_keys=True)
            handle.write("\n")
        temporary_path.chmod(0o600)
        os.replace(temporary_path, output_path)
    finally:
        temporary_path.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    validate = subparsers.add_parser("validate-layout")
    validate.add_argument("--repo-root", required=True)
    validate.add_argument("--artifact-parent", required=True)
    validate.add_argument("--run-root", required=True)
    validate.add_argument("--export-dir")
    validate.add_argument("--cwd", required=True)
    validate.add_argument("--home")
    validate.add_argument("--codex-home")

    cleanup = subparsers.add_parser("cleanup")
    cleanup.add_argument("--run-root", required=True)
    cleanup.add_argument("--artifact-parent", required=True)

    export = subparsers.add_parser("export")
    export.add_argument("--run-root", required=True)
    export.add_argument("--output", required=True)
    export.add_argument("--run-date", required=True)
    export.add_argument("--target-os", required=True)

    args = parser.parse_args()
    try:
        if args.command == "validate-layout":
            validate_layout(
                repo_root=args.repo_root,
                artifact_parent=args.artifact_parent,
                run_root=args.run_root,
                export_dir=args.export_dir,
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
            export_ci_summary(
                run_root=args.run_root,
                output=args.output,
                run_date=args.run_date,
                target_os=args.target_os,
            )
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"capability probe artifact sanitization failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
