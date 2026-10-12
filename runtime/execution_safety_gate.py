"""Pure, non-executing evaluation of whether evidence could justify a future provider execution.

Nothing here runs a process, touches the filesystem, network, environment or ledger, or imports a runner.
Evaluating a policy creates no sandbox: every decision is unauthorized, because all input is unauthenticated
caller data and no trusted execution coordinator exists. See docs/implementation/execution-safety-gate.md.
"""

from __future__ import annotations

import hashlib
import json
import math

from .runtime_validation import CAPABILITIES, _capability_status, _sha256, _token, parse_evidence
from .synapse_plan import PlanError, _keys

MAX_ATTESTATIONS = 64
MAX_ATTESTATION_LIFETIME_SECONDS = 900
MAX_COMMAND_ARGUMENTS = 256
MAX_COMMAND_ARGUMENT_LENGTH = 4096
TASK_KINDS = ("help_command", "model_task")
CONTROLS = ("probe_evidence", "authorization_ledger", "process_containment", "filesystem", "network",
            "cpu_limit", "memory_limit", "disk_limit")
# Unknown is the absence of an applicable record, so it is not a claimable level.
CLAIM_LEVELS = ("advertised", "observed", "enforced")
ORIGINS = ("provider_reported", "synapse_observed", "synapse_enforced")
OUTCOMES = ("pass", "fail", "unsupported")
MECHANISMS = ("none", "process_group", "rlimit", "os_sandbox", "cgroup", "quota_volume", "private_ledger")
OBSERVATIONS = ("descendant_escaped", "write_succeeded", "network_reachable", "limit_exceeded", "replayed")
_ENFORCED_ALL = {control: "enforced" for control in CONTROLS if control != "probe_evidence"}
REQUIREMENTS = {
    "help_command": {"probe_evidence": "observed", "authorization_ledger": "enforced",
                     "process_containment": "observed"},
    "model_task": {"probe_evidence": "observed", **_ENFORCED_ALL},
}
# Probe-record capabilities each tier needs satisfied; any failed, mismatched or unsupported record blocks regardless.
_PROBE_REQUIRED = {
    "help_command": {"executable_identity"},
    "model_task": {"executable_identity", "model_acceptance", "read_only_filesystem", "network_restriction",
                   "output_format", "process_termination", "session_persistence"},
}
_CONTRADICTING = {"failed", "mismatch", "unsupported"}
# Mechanisms accepted as evidence of enforcement. Synapse implements none of them yet; a missing entry accepts none.
PLATFORM_MECHANISMS = {
    "linux": {"authorization_ledger": ("private_ledger",), "process_containment": ("cgroup",),
              "filesystem": ("os_sandbox",), "network": ("os_sandbox",), "cpu_limit": ("rlimit", "cgroup"),
              "memory_limit": ("cgroup",), "disk_limit": ("quota_volume",)},
    "darwin": {"authorization_ledger": ("private_ledger",), "filesystem": ("os_sandbox",),
               "network": ("os_sandbox",), "cpu_limit": ("rlimit",), "disk_limit": ("quota_volume",)},
}
_ATTESTATION_FIELDS = {"control", "level", "origin", "outcome", "mechanism", "observed", "executable_sha256",
                       "command_sha256", "probe_id", "issued_at", "expires_at"}
_RANK = {name: index for index, name in enumerate(CLAIM_LEVELS)}
_TRUSTED_ORIGINS = {"observed": ("synapse_observed", "synapse_enforced"), "enforced": ("synapse_enforced",)}


def command_specification_sha256(command: object) -> str:
    """Digest an argument vector (excluding the executable, which is bound by its own hash)."""
    if (not isinstance(command, (list, tuple)) or not 1 <= len(command) <= MAX_COMMAND_ARGUMENTS
            or any(not isinstance(item, str) or len(item) > MAX_COMMAND_ARGUMENT_LENGTH or "\0" in item
                   for item in command)):
        raise PlanError("invalid command specification")
    return hashlib.sha256(json.dumps(list(command), separators=(",", ":")).encode()).hexdigest()


def _timestamp(value: object, label: str) -> float:
    if type(value) not in (int, float) or not math.isfinite(value):
        raise PlanError(f"invalid {label}")
    return float(value)


def parse_attestations(raw: object) -> list[dict[str, object]]:
    """Validate control attestations against a closed schema; free-form text cannot carry prompts or secrets."""
    if not isinstance(raw, list) or len(raw) > MAX_ATTESTATIONS:
        raise PlanError(f"attestations must be a list of at most {MAX_ATTESTATIONS} records")
    records = []
    for item in raw:
        record = _keys(item, _ATTESTATION_FIELDS, "attestation")
        for field, allowed in (("control", CONTROLS), ("level", CLAIM_LEVELS), ("origin", ORIGINS),
                               ("outcome", OUTCOMES), ("mechanism", MECHANISMS)):
            if record[field] not in allowed:
                raise PlanError(f"invalid attestation {field}")
        if record["observed"] is not None and (
                record["observed"] not in OBSERVATIONS or record["outcome"] == "pass"):
            raise PlanError("invalid attestation observation")
        issued, expires = _timestamp(record["issued_at"], "attestation issue time"), _timestamp(
            record["expires_at"], "attestation expiry")
        if not 0 < expires - issued <= MAX_ATTESTATION_LIFETIME_SECONDS:
            raise PlanError("attestation lifetime must be between 0 and 900 seconds")
        records.append({
            **{key: record[key] for key in ("control", "level", "origin", "outcome", "mechanism", "observed")},
            "executable_sha256": _sha256(record["executable_sha256"], "attestation executable hash"),
            "command_sha256": _sha256(record["command_sha256"], "attestation command hash"),
            "probe_id": _token(record["probe_id"], "attestation probe id"),
            "issued_at": issued, "expires_at": expires,
        })
    return sorted(records, key=lambda record: tuple(str(record[key]) for key in sorted(_ATTESTATION_FIELDS)))


def _control_status(minimum: str | None, allowed: tuple[str, ...], now: float,
                    records: list[dict[str, object]]) -> tuple[str, str | None]:
    outcomes = {record["outcome"] for record in records}
    passed = [record for record in records if record["outcome"] == "pass"]
    live = [record for record in passed if record["issued_at"] <= now < record["expires_at"]]
    highest = max((_RANK[str(record["level"])] for record in live), default=None)
    highest_level = CLAIM_LEVELS[highest] if highest is not None else None
    # Failures never expire, and conflicts resolve to the more severe state.
    if "fail" in outcomes:
        return "failed", highest_level
    if "unsupported" in outcomes:
        return "unsupported", highest_level
    if len({record["mechanism"] for record in live if record["level"] == "enforced"}) > 1:
        return "conflicting", highest_level
    if minimum is None:
        return "not_required", highest_level
    if passed and not live:
        return ("expired" if any(now >= record["expires_at"] for record in passed) else "not_yet_valid"), highest_level
    if not live:
        return "unknown", highest_level
    sufficient = [record for record in live if _RANK[str(record["level"])] >= _RANK[minimum]]
    if not sufficient:
        return "insufficient_evidence", highest_level
    trusted = [record for record in sufficient if record["origin"] in _TRUSTED_ORIGINS[minimum]]
    if not trusted:
        return "untrusted_origin", highest_level
    if minimum == "enforced" and not any(record["mechanism"] in allowed for record in trusted):
        return "mechanism_insufficient", highest_level
    return "satisfied", highest_level


def _reason(scope: str, subject: str, code: str) -> dict[str, str]:
    return {"scope": scope, "subject": subject, "code": code}


def evaluate_execution_gate(*, task_kind: str, executable_sha256: str, command: object, probe_id: str,
                            platform_name: str, now: float, evidence: object, attestations: object,
                            model: str | None = None, effort: str | None = None) -> dict[str, object]:
    """Judge supplied evidence for one executable, one command and one probe run. It never authorizes execution.

    `now` and `platform_name` are explicit so results are reproducible. Malformed input raises PlanError;
    stale, forged, missing or contradictory evidence is reported as ordered reasons.
    """
    if task_kind not in TASK_KINDS:
        raise PlanError("invalid task kind")
    current_hash = _sha256(executable_sha256, "executable hash")
    command_hash = command_specification_sha256(command)
    run_id = _token(probe_id, "probe id")
    platform = _token(platform_name, "platform")
    clock = _timestamp(now, "evaluation time")
    if task_kind == "model_task":
        wanted = {"model_acceptance": _token(model, "model"), "reasoning_effort": _token(effort, "effort", optional=True)}
    elif model is not None or effort is not None:
        raise PlanError("model and effort apply only to model tasks")
    else:
        wanted = {}
    probe_required = _PROBE_REQUIRED[task_kind] | ({"reasoning_effort"} if wanted.get("reasoning_effort") else set())
    supported = PLATFORM_MECHANISMS.get(platform)

    reasons = [] if supported is not None else [_reason("platform", "platform", "unsupported")]
    records = parse_evidence(evidence)
    current = [record for record in records if record["executable_sha256"] == current_hash]
    bound = [record for record in current if record["probe_id"] == run_id]
    for capability in CAPABILITIES:
        status, _ = _capability_status(capability, wanted.get(capability),
                                       [record for record in bound if record["capability"] == capability])
        if status != "satisfied" and (capability in probe_required or status in _CONTRADICTING):
            reasons.append(_reason("evidence", capability, status))

    claims = parse_attestations(attestations)
    same_executable = [claim for claim in claims if claim["executable_sha256"] == current_hash]
    same_command = [claim for claim in same_executable if claim["command_sha256"] == command_hash]
    applicable = [claim for claim in same_command if claim["probe_id"] == run_id]
    controls = {}
    for control in CONTROLS:
        minimum = REQUIREMENTS[task_kind].get(control)
        status, highest_level = _control_status(
            minimum, (supported or {}).get(control, ()), clock,
            [claim for claim in applicable if claim["control"] == control])
        controls[control] = {"required": minimum is not None, "minimum_level": minimum, "status": status,
                             "highest_claimed_level": highest_level}
        if status not in ("satisfied", "not_required"):
            reasons.append(_reason("control", control, status))

    requirements_satisfied = not reasons
    reasons.append(_reason("gate", "evidence", "evidence_unauthenticated"))
    reasons.append(_reason("gate", "coordinator", "no_trusted_execution_coordinator"))
    return {
        "schema_version": 1,
        "task_kind": task_kind,
        "executable_sha256": current_hash,
        "command_sha256": command_hash,
        "probe_id": run_id,
        "platform": {"name": platform, "supported": supported is not None},
        "evaluated_at": clock,
        "controls": controls,
        "probe_records": {"total": len(records), "stale_executable": len(records) - len(current),
                          "unbound_probe": len(current) - len(bound)},
        "attestations": {"total": len(claims), "stale_executable": len(claims) - len(same_executable),
                         "command_mismatch": len(same_executable) - len(same_command),
                         "unbound_probe": len(same_command) - len(applicable)},
        "decision": {"status": "unauthorized", "requirements_satisfied": requirements_satisfied,
                     "reasons": reasons, "evidence_authenticated": False, "dispatch_authorized": False,
                     "execution_authorized": False, "sandbox_enforcement_created": False},
    }
