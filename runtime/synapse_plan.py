"""Read private Synapse configuration and build a non-dispatching plan."""

from __future__ import annotations

import json
from pathlib import Path

from .config import ReviewPolicy
from .contracts import ProviderAdapter
from .execution_modes import execution_permissions
from .policy import provider_policy_preview
from .vault_context import MAX_EXCERPT_BYTES, MAX_TOTAL_BYTES, _read_bounded, _unique_object, preview

MAX_CONFIG_BYTES = 65536
MAX_PLAN_BYTES = 65536


class PlanError(ValueError):
    pass


def _keys(value: object, expected: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != expected:
        raise PlanError(f"invalid {label} fields")
    return value


def _integer(value: object, minimum: int, maximum: int, label: str) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise PlanError(f"invalid {label}")
    return value


def _external_path(raw: object, repo: Path, label: str, *, directory: bool) -> Path:
    if not isinstance(raw, str) or not raw or not Path(raw).is_absolute():
        raise PlanError(f"{label} must be an absolute external path")
    path = Path(raw)
    if path.is_symlink() or not (path.is_dir() if directory else path.is_file()):
        raise PlanError(f"{label} must be an existing, non-symlink {'directory' if directory else 'file'}")
    resolved = path.resolve(strict=True)
    if resolved == repo or resolved in repo.parents or repo in resolved.parents:
        raise PlanError(f"{label} overlaps the repository")
    return resolved


def load_private_config(path: Path, repo_root: Path) -> dict[str, object]:
    if not repo_root.is_dir():
        raise PlanError("repository must be an existing directory")
    repo = repo_root.resolve(strict=True)
    config_path = _external_path(str(path), repo, "configuration", directory=False)
    try:
        raw = json.loads(_read_bounded(config_path, MAX_CONFIG_BYTES).decode("utf-8"),
                         object_pairs_hook=_unique_object)
    except (UnicodeError, ValueError, OSError, RecursionError) as exc:
        raise PlanError("configuration must be bounded, valid UTF-8 JSON") from exc
    config = _keys(raw, {"schema_version", "vault", "context", "execution", "state"}, "configuration")
    if type(config["schema_version"]) is not int or config["schema_version"] != 1:
        raise PlanError("unsupported configuration schema version")
    vault = _keys(config["vault"], {"root", "index", "read_only"}, "vault")
    if vault["read_only"] is not True:
        raise PlanError("vault.read_only must be true")
    vault_root = _external_path(vault["root"], repo, "vault.root", directory=True)
    index = _external_path(vault["index"], repo, "vault.index", directory=False)
    context = _keys(config["context"], {"per_note_bytes", "total_excerpt_bytes"}, "context")
    per_note = _integer(context["per_note_bytes"], 1, MAX_EXCERPT_BYTES, "per-note byte limit")
    total = _integer(context["total_excerpt_bytes"], 1, MAX_TOTAL_BYTES, "total excerpt byte limit")
    execution = _keys(config["execution"], {"mode", "hard_timeout_seconds", "max_parallelism", "max_retries"}, "execution")
    if execution["mode"] != "read_only":
        raise PlanError("only read_only execution mode is supported")
    deadline = _integer(execution["hard_timeout_seconds"], 1, 3600, "hard timeout")
    parallelism = _integer(execution["max_parallelism"], 1, 4, "max parallelism")
    retries = _integer(execution["max_retries"], 0, 0, "max retries")
    state = _keys(config["state"], {"root"}, "state")
    state_root = _external_path(state["root"], repo, "state.root", directory=True)
    if state_root == vault_root or state_root in vault_root.parents or vault_root in state_root.parents:
        raise PlanError("state.root must not overlap the read-only vault")
    return {"vault_root": vault_root, "index": index, "per_note_bytes": per_note,
            "total_excerpt_bytes": total, "hard_timeout_seconds": deadline,
            "max_parallelism": parallelism, "max_retries": retries,
            "state_root": state_root}


def build_plan(*, config_path: Path, repo_root: Path, task: str, project: str | None,
               topic: str | None, agent: str, model: str, effort: str | None,
               adapter: ProviderAdapter) -> dict[str, object]:
    if not isinstance(task, str) or not task.strip():
        raise PlanError("task is required")
    try:
        if len(task.encode("utf-8")) > 4096:
            raise PlanError("task exceeds 4096 UTF-8 bytes")
    except UnicodeError as exc:
        raise PlanError("task must be valid UTF-8") from exc
    if adapter.id != agent:
        raise PlanError("selected agent does not match adapter")
    config = load_private_config(config_path, repo_root)
    permissions = execution_permissions(agent, "read_only")
    if permissions is None:
        raise PlanError("selected agent has no MCO read-only policy")
    policy = ReviewPolicy(provider_permissions={agent: permissions},
                          provider_models={agent: {"model": model}}, enforcement_mode="strict")
    support = provider_policy_preview(agent, adapter, policy)
    if support["unknown_permission_keys"]:
        raise PlanError("adapter does not support required read-only permission keys")
    if support["unknown_model_keys"]:
        raise PlanError("adapter does not support required model selection")
    if effort is not None:
        values = getattr(adapter, "supported_effort_values", lambda: [])()
        if not isinstance(values, list) or effort not in values:
            raise PlanError("adapter does not support required effort setting")

    selected = preview(config["vault_root"], config["index"], project, topic, agent,
                       model, effort or "not-requested", config["per_note_bytes"],
                       config["total_excerpt_bytes"])
    reasons = ["model_not_verified_against_local_cli", "read_only_not_verified_against_local_cli"]
    if effort is not None:
        reasons.append("effort_not_verified_against_local_cli")
    result = {
        "schema_version": 1,
        "task": task,
        "selection": selected["selection"],
        "context": {**selected["context"], "vault_root": str(config["vault_root"])},
        "requested_settings": {"agent": agent, "model": model, "effort": effort,
                               "execution_mode": "read_only"},
        "capability_evidence": {
            "adapter_declaration": {"model_keys": support["supported_model_keys"],
                                    "permission_keys": support["supported_permission_keys"],
                                    "effort_values": values if effort is not None else []},
            "local_cli": {"model": "not_verified", "effort": "not_verified" if effort else "not_requested",
                          "read_only": "not_verified", "auth": "not_checked",
                          "filesystem_isolation": "not_verified"},
        },
        "effective_settings": {"model": None, "effort": None, "read_only_permissions": None},
        "execution": {"requested_read_only_permissions": permissions,
                      "hard_timeout_seconds": config["hard_timeout_seconds"],
                      "max_parallelism": config["max_parallelism"],
                      "max_retries": config["max_retries"],
                      "state_root": str(config["state_root"]),
                      "artifact_root": str(config["state_root"] / "artifacts"),
                      "session_root": str(config["state_root"] / "sessions"),
                      "persist": False, "dispatch": False, "vault_access": "read_only"},
        "validation": {"schema": "valid", "status": "blocked", "blocking_reasons": reasons},
    }
    if len(json.dumps(result, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")) > MAX_PLAN_BYTES:
        raise PlanError(f"plan exceeds {MAX_PLAN_BYTES} output bytes")
    return result
