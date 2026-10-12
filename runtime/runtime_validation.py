"""Evidence model and probe-authorization check for future provider runtime validation.

Nothing here runs a provider, reads credentials, or writes files. It only judges supplied data.
Evidence is unauthenticated caller input: a sufficient result is never a dispatch gate.
"""

from __future__ import annotations

import re
from pathlib import Path

from .synapse_plan import PlanError, _external_path, _integer, _keys, _overlaps

MAX_EVIDENCE_RECORDS = 64
CAPABILITIES = (
    "executable_identity", "model_acceptance", "reasoning_effort", "read_only_filesystem",
    "network_restriction", "output_format", "process_termination", "session_persistence",
)
# Evidence sources, weakest to strongest; each source implies exactly one evidence level.
SOURCES = ("adapter_declaration", "cli_help", "runtime_probe", "external_check")
LEVELS = ("adapter_declared", "cli_advertised", "runtime_observed", "independently_verified")
OUTCOMES = ("pass", "fail", "unsupported")
# Only these capabilities carry values; every other capability's records carry no values.
_VALUED = {"model_acceptance", "reasoning_effort"}
# Closed diagnostic vocabulary for failure details on capabilities without values.
OBSERVATIONS = ("timeout", "cancelled", "descendant_survived", "write_succeeded", "network_reachable",
                "session_data_written", "malformed_output")
# Isolation properties cannot be inferred from a provider's own behavior or output.
_ISOLATION = {"read_only_filesystem", "network_restriction", "session_persistence"}
_MINIMUM_LEVEL = {name: 3 if name in _ISOLATION else 2 for name in CAPABILITIES}
_TOKEN = re.compile(r"[A-Za-z0-9._:/@+=-]{1,128}")
_SHA256 = re.compile(r"[0-9a-f]{64}")
_RECORD_FIELDS = {"capability", "source", "outcome", "executable_sha256", "requested", "observed", "probe_id"}
_AUTHORIZATION_FIELDS = {"schema_version", "probe_id", "executable_sha256", "capabilities", "scratch_root",
                         "execution_mode", "max_model_calls", "network", "hard_timeout_seconds"}


def _sha256(value: object, label: str) -> str:
    if not isinstance(value, str) or _SHA256.fullmatch(value) is None:
        raise PlanError(f"invalid {label}")
    return value


def _token(value: object, label: str, *, optional: bool = False) -> str | None:
    if value is None and optional:
        return None
    if not isinstance(value, str) or _TOKEN.fullmatch(value) is None:
        raise PlanError(f"invalid {label}")
    return value


def parse_evidence(raw: object) -> list[dict[str, object]]:
    """Validate evidence against a closed schema; free-form text cannot carry prompts or secrets."""
    if not isinstance(raw, list) or len(raw) > MAX_EVIDENCE_RECORDS:
        raise PlanError(f"evidence must be a list of at most {MAX_EVIDENCE_RECORDS} records")
    records = []
    for item in raw:
        record = _keys(item, _RECORD_FIELDS, "evidence record")
        if record["capability"] not in CAPABILITIES:
            raise PlanError("invalid evidence capability")
        if record["source"] not in SOURCES:
            raise PlanError("invalid evidence source")
        if record["outcome"] not in OUTCOMES:
            raise PlanError("invalid evidence outcome")
        requested = _token(record["requested"], "evidence requested value", optional=True)
        observed = _token(record["observed"], "evidence observed value", optional=True)
        if record["capability"] not in _VALUED:
            if requested is not None or (observed is not None and (
                    observed not in OBSERVATIONS or record["outcome"] == "pass")):
                raise PlanError("invalid values for a capability without values")
        records.append({
            "capability": record["capability"], "source": record["source"], "outcome": record["outcome"],
            "executable_sha256": _sha256(record["executable_sha256"], "evidence executable hash"),
            "requested": requested, "observed": observed,
            "probe_id": _token(record["probe_id"], "evidence probe id"),
        })
    return sorted(records, key=lambda record: tuple(str(record[key]) for key in sorted(_RECORD_FIELDS)))


def _capability_status(capability: str, wanted: str | None,
                       records: list[dict[str, object]]) -> tuple[str, str | None]:
    relevant = [record for record in records
                if wanted is None or record["requested"] in (None, wanted)]
    passed = [record for record in relevant if record["outcome"] == "pass"]
    highest = max((SOURCES.index(str(record["source"])) for record in passed), default=None)
    highest_level = LEVELS[highest] if highest is not None else None
    if any(record["outcome"] == "fail" for record in relevant):
        return "failed", highest_level
    if wanted is not None and any(record["observed"] not in (None, wanted) for record in passed):
        return "mismatch", highest_level
    if any(record["outcome"] == "unsupported" for record in relevant):
        return "unsupported", highest_level
    if any(SOURCES.index(str(record["source"])) >= _MINIMUM_LEVEL[capability]
           and (wanted is None or (record["requested"] == wanted and record["observed"] == wanted))
           for record in passed):
        return "satisfied", highest_level
    return ("insufficient_evidence" if passed else "unknown"), highest_level


def evaluate_validation(executable_sha256: str, evidence: object, *, probe_id: str, model: str,
                        effort: str | None = None, require_network_restriction: bool = False) -> dict[str, object]:
    """Judge supplied evidence for one executable and one probe run. It never authorizes dispatch.

    The evidence is unauthenticated: this function cannot tell a trusted probe runner from a forged record.
    """
    current_hash = _sha256(executable_sha256, "executable hash")
    run_id = _token(probe_id, "probe id")
    required: dict[str, str | None] = {
        "executable_identity": None, "model_acceptance": _token(model, "model"),
        "read_only_filesystem": None, "output_format": None, "process_termination": None,
        "session_persistence": None,
    }
    if effort is not None:
        required["reasoning_effort"] = _token(effort, "effort")
    if require_network_restriction:
        required["network_restriction"] = None
    records = parse_evidence(evidence)
    current = [record for record in records if record["executable_sha256"] == current_hash]
    bound = [record for record in current if record["probe_id"] == run_id]
    capabilities = {}
    reasons = []
    for capability in CAPABILITIES:
        status, highest_level = _capability_status(
            capability, required.get(capability),
            [record for record in bound if record["capability"] == capability])
        capabilities[capability] = {"required": capability in required, "requested": required.get(capability),
                                    "minimum_level": LEVELS[_MINIMUM_LEVEL[capability]],
                                    "highest_level": highest_level, "status": status}
        if capability in required and status != "satisfied":
            reasons.append(f"{capability}:{status}")
    return {
        "schema_version": 1,
        "executable_sha256": current_hash,
        "probe_id": run_id,
        "capabilities": capabilities,
        "stale_evidence": len(records) - len(current),
        "unbound_evidence": len(current) - len(bound),
        "validation": {"status": "blocked" if reasons else "evidence_sufficient",
                       "evidence_authenticated": False, "blocking_reasons": reasons,
                       "dispatch_authorized": False},
    }


def _denied(reason: str) -> dict[str, object]:
    return {"probe_authorized": False, "blocking_reasons": [reason], "dispatch_authorized": False}


def check_probe_authorization(raw: object, *, repo_root: Path, protected_roots: tuple[Path, ...],
                              executable_sha256: str) -> dict[str, object]:
    """Validate an explicit user-supplied authorization for one future controlled probe.

    Absence means denial. Authorization permits a probe only; it is never dispatch authorization.
    """
    current_hash = _sha256(executable_sha256, "executable hash")
    if raw is None:
        return _denied("no_authorization_supplied")
    auth = _keys(raw, _AUTHORIZATION_FIELDS, "probe authorization")
    if type(auth["schema_version"]) is not int or auth["schema_version"] != 1:
        raise PlanError("unsupported probe authorization schema version")
    run_id = _token(auth["probe_id"], "authorized probe id")
    authorized_hash = _sha256(auth["executable_sha256"], "authorized executable hash")
    capabilities = auth["capabilities"]
    if (not isinstance(capabilities, list) or not capabilities
            or any(item not in CAPABILITIES for item in capabilities)
            or len(set(capabilities)) != len(capabilities)):
        raise PlanError("invalid probe authorization capabilities")
    if auth["execution_mode"] != "read_only":
        raise PlanError("probe authorization must use read_only execution")
    calls = _integer(auth["max_model_calls"], 0, 3, "max model calls")
    if auth["network"] != ("provider_endpoint_only" if calls else "none"):
        raise PlanError("probe network access must be none without model calls and provider_endpoint_only with them")
    _integer(auth["hard_timeout_seconds"], 1, 300, "probe hard timeout")
    if not repo_root.is_dir():
        raise PlanError("repository must be an existing directory")
    repo = repo_root.resolve(strict=True)
    scratch = _external_path(auth["scratch_root"], repo, "scratch_root", directory=True)
    try:
        for protected in (repo, *protected_roots):
            if _overlaps(scratch, protected.resolve(strict=True)):
                raise PlanError("scratch_root overlaps the repository or a protected root")
        if any(scratch.iterdir()):
            raise PlanError("scratch_root must be an empty directory")
    except OSError as exc:
        raise PlanError("scratch_root and protected roots must be readable existing directories") from exc
    if authorized_hash != current_hash:
        return _denied("authorization_executable_mismatch")
    return {"probe_authorized": True, "probe_id": run_id, "scratch_root": str(scratch),
            "path_checks": "point_in_time", "blocking_reasons": [], "dispatch_authorized": False}
