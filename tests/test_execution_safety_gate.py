from __future__ import annotations

import ast
import copy
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from runtime import execution_safety_gate
from runtime.adapters.codex import CodexAdapter
from runtime.contracts import TaskInput
from runtime.execution_safety_gate import (
    CONTROLS, MAX_ATTESTATIONS, command_specification_sha256, evaluate_execution_gate, parse_attestations,
)
from runtime.synapse_plan import PlanError
from tests.test_runtime_validation import record as probe_record
from tests.test_runtime_validation import full_evidence

SHA = "a" * 64
OTHER_SHA = "b" * 64
RUN = "run-1"
NOW = 1_000_000.0
HELP_COMMAND = ("exec", "--help")
TASK_COMMAND = ("exec", "--json", "--sandbox", "read-only")
OTHER_COMMAND = ("exec", "--dangerously-bypass-approvals-and-sandbox")
MECHANISM = {"probe_evidence": "none", "authorization_ledger": "private_ledger", "process_containment": "cgroup",
             "filesystem": "os_sandbox", "network": "os_sandbox", "cpu_limit": "rlimit",
             "memory_limit": "cgroup", "disk_limit": "quota_volume"}
HELP_CONTROLS = ("probe_evidence", "authorization_ledger", "process_containment")
PROBE_REQUIRED = ["executable_identity", "model_acceptance", "read_only_filesystem", "network_restriction",
                  "output_format", "process_termination", "session_persistence"]


def attestation(control: str, *, level: str = "enforced", origin: str = "synapse_enforced",
                outcome: str = "pass", mechanism: str | None = None, observed: str | None = None,
                sha: str = SHA, command: tuple[str, ...] = TASK_COMMAND, probe_id: str = RUN,
                platform: str = "linux", issued: float = NOW - 10, expires: float = NOW + 300) -> dict:
    return {"control": control, "level": level, "origin": origin, "outcome": outcome,
            "mechanism": mechanism or MECHANISM[control], "observed": observed, "executable_sha256": sha,
            "command_sha256": command_specification_sha256(command), "probe_id": probe_id,
            "platform": platform, "issued_at": issued, "expires_at": expires}


def help_attestations(**changes: object) -> list[dict]:
    kwargs = {"command": HELP_COMMAND, **changes}
    return [attestation("probe_evidence", level="observed", origin="synapse_observed", mechanism="none", **kwargs),
            attestation("authorization_ledger", **kwargs),
            attestation("process_containment", level="observed", origin="synapse_observed",
                        mechanism="process_group", **kwargs)]


def task_attestations(**changes: object) -> list[dict]:
    kwargs = {"command": TASK_COMMAND, **changes}
    return [attestation("probe_evidence", level="observed", origin="synapse_observed", mechanism="none", **kwargs),
            *[attestation(control, **kwargs) for control in CONTROLS if control != "probe_evidence"]]


def model_evaluate(evidence: list | None = None, attestations: list | None = None, **kwargs: object) -> dict:
    values = {"task_kind": "model_task", "executable_sha256": SHA, "command": TASK_COMMAND, "probe_id": RUN,
              "platform_name": "linux", "now": NOW, "model": "model-1",
              "evidence": full_evidence(network=True) if evidence is None else evidence,
              "attestations": task_attestations() if attestations is None else attestations}
    values.update(kwargs)
    return evaluate_execution_gate(**values)


def help_evaluate(evidence: list | None = None, attestations: list | None = None, **kwargs: object) -> dict:
    values = {"task_kind": "help_command", "executable_sha256": SHA, "command": HELP_COMMAND, "probe_id": RUN,
              "platform_name": "linux", "now": NOW,
              "evidence": [probe_record("executable_identity", "runtime_probe")] if evidence is None else evidence,
              "attestations": help_attestations() if attestations is None else attestations}
    values.update(kwargs)
    return evaluate_execution_gate(**values)


def codes(result: dict) -> list[tuple[str, str, str]]:
    return [(item["scope"], item["subject"], item["code"]) for item in result["decision"]["reasons"]]


def blockers(result: dict) -> list[tuple[str, str, str]]:
    return codes(result)[:-2]


def without(attestations: list[dict], control: str) -> list[dict]:
    return [item for item in attestations if item["control"] != control]


def replaced(attestations: list[dict], control: str, *extra: dict) -> list[dict]:
    return [*without(attestations, control), *extra]


class NeverAuthorizesTests(unittest.TestCase):
    def assert_unauthorized(self, result: dict) -> None:
        decision = result["decision"]
        self.assertEqual(decision["status"], "unauthorized")
        for flag in ("evidence_authenticated", "dispatch_authorized", "execution_authorized",
                     "sandbox_enforcement_created"):
            self.assertIs(decision[flag], False, flag)
        self.assertEqual(codes(result)[-2:], [("gate", "evidence", "evidence_unauthenticated"),
                                              ("gate", "coordinator", "no_trusted_execution_coordinator")])

    def test_fully_satisfying_synthetic_model_task_evidence_remains_unauthorized(self) -> None:
        result = model_evaluate()
        self.assertIs(result["decision"]["requirements_satisfied"], True)
        self.assertEqual(blockers(result), [])
        self.assertTrue(all(item["status"] == "satisfied" for item in result["controls"].values()))
        self.assert_unauthorized(result)

    def test_fully_satisfying_synthetic_effort_task_remains_unauthorized(self) -> None:
        result = model_evaluate(full_evidence(effort=True, network=True), effort="medium")
        self.assertIs(result["decision"]["requirements_satisfied"], True)
        self.assert_unauthorized(result)

    def test_fully_satisfying_synthetic_help_command_remains_unauthorized(self) -> None:
        result = help_evaluate()
        self.assertIs(result["decision"]["requirements_satisfied"], True)
        self.assertEqual(blockers(result), [])
        self.assertEqual([name for name, item in result["controls"].items() if not item["required"]],
                         ["filesystem", "network", "cpu_limit", "memory_limit", "disk_limit"])
        self.assert_unauthorized(result)

    def test_no_scenario_ever_authorizes(self) -> None:
        scenarios = [model_evaluate([], []), help_evaluate([], []), model_evaluate(platform_name="windows"),
                     model_evaluate(attestations=[{**item, "origin": "provider_reported"}
                                                  for item in task_attestations()]),
                     model_evaluate(now=NOW + 10_000),
                     help_evaluate(attestations=help_attestations(platform="darwin"), platform_name="darwin")]
        for result in scenarios:
            self.assert_unauthorized(result)

    def test_policy_evaluation_creates_no_enforcement(self) -> None:
        self.assertIs(model_evaluate()["decision"]["sandbox_enforcement_created"], False)


class MissingAndBindingTests(unittest.TestCase):
    def test_missing_evidence_fails_closed_with_ordered_reasons(self) -> None:
        result = model_evaluate([], [])
        expected = [("evidence", name, "unknown") for name in PROBE_REQUIRED]
        expected += [("control", name, "unknown") for name in CONTROLS]
        self.assertEqual(blockers(result), expected)
        self.assertIs(result["decision"]["requirements_satisfied"], False)
        self.assertEqual(blockers(help_evaluate([], [])),
                         [("evidence", "executable_identity", "unknown")]
                         + [("control", name, "unknown") for name in HELP_CONTROLS])

    def test_expired_not_yet_valid_and_future_dated_evidence_does_not_count(self) -> None:
        for control, kwargs, status in (
                ("filesystem", {"expires": NOW - 1}, "expired"), ("filesystem", {"expires": NOW}, "expired"),
                ("network", {"issued": NOW + 1, "expires": NOW + 60}, "not_yet_valid")):
            with self.subTest(control=control, status=status):
                claims = replaced(task_attestations(), control, attestation(control, **kwargs))
                self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", control, status)])
        stale_run = model_evaluate(now=NOW + 301)
        self.assertEqual([item[2] for item in blockers(stale_run)], ["expired"] * len(CONTROLS))

    def test_expiry_does_not_clear_a_recorded_failure(self) -> None:
        claims = replaced(task_attestations(), "filesystem",
                          attestation("filesystem", outcome="fail", observed="write_succeeded",
                                      issued=NOW - 500, expires=NOW - 400))
        self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", "filesystem", "failed")])

    def test_wrong_executable_identity_cannot_authorize(self) -> None:
        result = model_evaluate(full_evidence(sha=OTHER_SHA, network=True), task_attestations(sha=OTHER_SHA))
        self.assertEqual(result["probe_records"]["stale_executable"], 7)
        self.assertEqual(result["attestations"]["stale_executable"], len(CONTROLS))
        self.assertEqual(result["attestations"]["total"], len(CONTROLS))
        self.assertTrue(all(item[2] == "unknown" for item in blockers(result)))
        self.assertEqual(len(blockers(result)), len(PROBE_REQUIRED) + len(CONTROLS))

    def test_wrong_command_specification_cannot_authorize(self) -> None:
        result = model_evaluate(attestations=task_attestations(command=OTHER_COMMAND))
        self.assertEqual(result["attestations"]["command_mismatch"], len(CONTROLS))
        self.assertEqual(blockers(result), [("control", name, "unknown") for name in CONTROLS])
        partial = replaced(task_attestations(), "network", attestation("network", command=OTHER_COMMAND))
        self.assertEqual(blockers(model_evaluate(attestations=partial)), [("control", "network", "unknown")])

    def test_evidence_from_another_probe_run_cannot_authorize(self) -> None:
        result = model_evaluate(attestations=task_attestations(probe_id="run-0"))
        self.assertEqual(result["attestations"]["unbound_probe"], len(CONTROLS))
        self.assertEqual(len(blockers(result)), len(CONTROLS))
        probes = model_evaluate(evidence=[{**item, "probe_id": "run-0"} for item in full_evidence(network=True)])
        self.assertEqual(probes["probe_records"]["unbound_probe"], 7)
        self.assertEqual(len(blockers(probes)), len(PROBE_REQUIRED))

    def test_failures_recorded_for_another_executable_do_not_block(self) -> None:
        failing = attestation("filesystem", outcome="fail", observed="write_succeeded", sha=OTHER_SHA)
        result = model_evaluate(attestations=[*task_attestations(), failing])
        self.assertEqual(blockers(result), [])
        self.assertEqual(result["attestations"]["stale_executable"], 1)


class ContradictionTests(unittest.TestCase):
    def test_a_failure_is_never_overridden_by_a_pass_of_any_origin_or_level(self) -> None:
        for passing in (attestation("filesystem"),
                        attestation("filesystem", origin="provider_reported"),
                        attestation("filesystem", level="observed", origin="synapse_observed")):
            with self.subTest(passing=passing["origin"], level=passing["level"]):
                claims = replaced(task_attestations(), "filesystem", passing,
                                  attestation("filesystem", outcome="fail", observed="write_succeeded",
                                              origin="provider_reported", level="advertised"))
                self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", "filesystem", "failed")])

    def test_unsupported_is_not_overridden_by_a_pass(self) -> None:
        claims = [*task_attestations(), attestation("network", outcome="unsupported")]
        self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", "network", "unsupported")])

    def test_enforced_claims_naming_different_mechanisms_conflict(self) -> None:
        claims = [*task_attestations(), attestation("filesystem", mechanism="process_group")]
        result = model_evaluate(attestations=claims)
        self.assertEqual(blockers(result), [("control", "filesystem", "conflicting")])

    def test_probe_record_failure_blocks_even_when_enforcement_is_attested(self) -> None:
        for capability, observed in (("read_only_filesystem", "write_succeeded"),
                                     ("network_restriction", "network_reachable"),
                                     ("session_persistence", "session_data_written")):
            with self.subTest(capability=capability):
                evidence = full_evidence(network=True) + [probe_record(capability, "external_check", "fail",
                                                                       observed=observed)]
                self.assertEqual(blockers(model_evaluate(evidence)), [("evidence", capability, "failed")])

    def test_contradicting_probe_records_block_a_tier_that_does_not_require_them(self) -> None:
        evidence = [probe_record("executable_identity", "runtime_probe"),
                    probe_record("read_only_filesystem", "external_check", "fail", observed="write_succeeded")]
        self.assertEqual(blockers(help_evaluate(evidence)), [("evidence", "read_only_filesystem", "failed")])

    def test_model_mismatch_blocks(self) -> None:
        evidence = [item for item in full_evidence(network=True) if item["capability"] != "model_acceptance"]
        evidence.append(probe_record("model_acceptance", "runtime_probe", requested="model-1", observed="model-2"))
        self.assertEqual(blockers(model_evaluate(evidence)), [("evidence", "model_acceptance", "mismatch")])


class ForgedAndClaimedEvidenceTests(unittest.TestCase):
    def test_provider_reported_evidence_cannot_satisfy_any_enforcement_requirement(self) -> None:
        claims = [{**item, "origin": "provider_reported"} for item in task_attestations()]
        result = model_evaluate(attestations=claims)
        self.assertEqual(blockers(result), [("control", name, "untrusted_origin") for name in CONTROLS])
        self.assertTrue(all(item["highest_claimed_level"] for item in result["controls"].values()))

    def test_provider_reported_evidence_cannot_satisfy_a_help_command(self) -> None:
        claims = [{**item, "origin": "provider_reported"} for item in help_attestations()]
        self.assertEqual(blockers(help_evaluate(attestations=claims)),
                         [("control", name, "untrusted_origin") for name in HELP_CONTROLS])

    def test_observed_origin_cannot_stand_in_for_enforcement(self) -> None:
        claims = [{**item, "origin": "synapse_observed"} for item in task_attestations()]
        self.assertEqual(blockers(model_evaluate(attestations=claims)),
                         [("control", name, "untrusted_origin") for name in CONTROLS if name != "probe_evidence"])

    def test_claimed_or_observed_filesystem_and_network_restrictions_are_not_enforced(self) -> None:
        for control in ("filesystem", "network"):
            for level, origin in (("advertised", "provider_reported"), ("advertised", "synapse_observed"),
                                  ("observed", "synapse_observed"), ("observed", "provider_reported")):
                with self.subTest(control=control, level=level, origin=origin):
                    claim = attestation(control, level=level, origin=origin, mechanism="none")
                    claims = replaced(task_attestations(), control, claim)
                    self.assertEqual(blockers(model_evaluate(attestations=claims)),
                                     [("control", control, "insufficient_evidence")])

    def test_enforced_level_without_a_real_mechanism_is_rejected(self) -> None:
        for control in ("filesystem", "network"):
            for mechanism in ("none", "process_group", "rlimit"):
                with self.subTest(control=control, mechanism=mechanism):
                    claims = replaced(task_attestations(), control, attestation(control, mechanism=mechanism))
                    self.assertEqual(blockers(model_evaluate(attestations=claims)),
                                     [("control", control, "mechanism_insufficient")])

    def test_a_probe_that_saw_nothing_wrong_is_not_isolation(self) -> None:
        evidence = [item for item in full_evidence(network=True)
                    if item["capability"] not in {"read_only_filesystem", "network_restriction"}]
        evidence += [probe_record("read_only_filesystem", "runtime_probe"),
                     probe_record("network_restriction", "runtime_probe")]
        self.assertEqual(blockers(model_evaluate(evidence)),
                         [("evidence", "read_only_filesystem", "insufficient_evidence"),
                          ("evidence", "network_restriction", "insufficient_evidence")])

    def test_empty_scratch_without_enforcement_attestation_does_not_satisfy_filesystem(self) -> None:
        self.assertEqual(blockers(model_evaluate(attestations=without(task_attestations(), "filesystem"))),
                         [("control", "filesystem", "unknown")])
        self.assertEqual(blockers(model_evaluate(attestations=without(task_attestations(), "network"))),
                         [("control", "network", "unknown")])


class ContainmentLimitsAndLedgerTests(unittest.TestCase):
    def test_process_group_only_containment_fails_a_model_task_but_not_the_observed_help_tier(self) -> None:
        claims = replaced(task_attestations(), "process_containment",
                          attestation("process_containment", mechanism="process_group"))
        self.assertEqual(blockers(model_evaluate(attestations=claims)),
                         [("control", "process_containment", "mechanism_insufficient")])
        self.assertEqual(blockers(help_evaluate()), [])

    def test_escaped_descendants_fail_both_tiers(self) -> None:
        escaped = attestation("process_containment", level="observed", origin="synapse_observed",
                              outcome="fail", mechanism="process_group", observed="descendant_escaped")
        claims = replaced(task_attestations(), "process_containment", escaped)
        self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", "process_containment", "failed")])
        help_claims = replaced(help_attestations(), "process_containment", {**escaped, "command_sha256":
                               command_specification_sha256(HELP_COMMAND)})
        self.assertEqual(blockers(help_evaluate(attestations=help_claims)),
                         [("control", "process_containment", "failed")])

    def test_surviving_descendants_in_probe_records_block(self) -> None:
        evidence = [item for item in full_evidence(network=True) if item["capability"] != "process_termination"]
        evidence.append(probe_record("process_termination", "runtime_probe", "fail", observed="descendant_survived"))
        self.assertEqual(blockers(model_evaluate(evidence)), [("evidence", "process_termination", "failed")])

    def test_each_missing_resource_limit_blocks(self) -> None:
        for control in ("cpu_limit", "memory_limit", "disk_limit"):
            with self.subTest(control=control):
                self.assertEqual(blockers(model_evaluate(attestations=without(task_attestations(), control))),
                                 [("control", control, "unknown")])
        self.assertEqual([item[1] for item in blockers(model_evaluate(attestations=[
            item for item in task_attestations() if not item["control"].endswith("_limit")]))],
            ["cpu_limit", "memory_limit", "disk_limit"])

    def test_timeouts_flags_and_labels_are_not_resource_limits(self) -> None:
        for control in ("cpu_limit", "memory_limit", "disk_limit"):
            for mechanism in ("none", "process_group", "private_ledger"):
                with self.subTest(control=control, mechanism=mechanism):
                    claims = replaced(task_attestations(), control, attestation(control, mechanism=mechanism))
                    self.assertEqual(blockers(model_evaluate(attestations=claims)),
                                     [("control", control, "mechanism_insufficient")])

    def test_exceeded_limit_fails(self) -> None:
        claims = [*task_attestations(), attestation("memory_limit", outcome="fail", observed="limit_exceeded")]
        self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", "memory_limit", "failed")])

    def test_missing_or_replayed_ledger_evidence_blocks_both_tiers(self) -> None:
        self.assertEqual(blockers(model_evaluate(attestations=without(task_attestations(), "authorization_ledger"))),
                         [("control", "authorization_ledger", "unknown")])
        self.assertEqual(blockers(help_evaluate(attestations=without(help_attestations(), "authorization_ledger"))),
                         [("control", "authorization_ledger", "unknown")])
        replay = attestation("authorization_ledger", outcome="fail", observed="replayed")
        claims = replaced(task_attestations(), "authorization_ledger", replay)
        self.assertEqual(blockers(model_evaluate(attestations=claims)),
                         [("control", "authorization_ledger", "failed")])
        reused = replaced(task_attestations(), "authorization_ledger",
                          attestation("authorization_ledger", command=OTHER_COMMAND))
        self.assertEqual(blockers(model_evaluate(attestations=reused)),
                         [("control", "authorization_ledger", "unknown")])

    def test_ledger_evidence_must_be_enforced_by_synapse(self) -> None:
        for kwargs, status in (({"origin": "provider_reported"}, "untrusted_origin"),
                               ({"level": "observed", "origin": "synapse_observed"}, "insufficient_evidence"),
                               ({"mechanism": "none"}, "mechanism_insufficient")):
            with self.subTest(kwargs=kwargs):
                claims = replaced(task_attestations(), "authorization_ledger",
                                  attestation("authorization_ledger", **kwargs))
                self.assertEqual(blockers(model_evaluate(attestations=claims)),
                                 [("control", "authorization_ledger", status)])


class PlatformTests(unittest.TestCase):
    def test_unsupported_operating_systems_are_blocked_in_both_tiers(self) -> None:
        for name in ("windows", "freebsd", "plan9", "Linux"):
            with self.subTest(name=name):
                result = model_evaluate(platform_name=name)
                self.assertIs(result["platform"]["supported"], False)
                self.assertEqual(blockers(result)[0], ("platform", "platform", "unsupported"))
                self.assertTrue(all(item["status"] != "satisfied" for item in result["controls"].values()
                                    if item["minimum_level"] == "enforced"))
                self.assertEqual(blockers(help_evaluate(platform_name=name))[0],
                                 ("platform", "platform", "unsupported"))

    def test_darwin_has_no_accepted_containment_or_memory_mechanism_yet(self) -> None:
        result = model_evaluate(attestations=task_attestations(platform="darwin"), platform_name="darwin")
        self.assertIs(result["platform"]["supported"], True)
        self.assertEqual(blockers(result), [("control", "process_containment", "mechanism_insufficient"),
                                            ("control", "memory_limit", "mechanism_insufficient")])
        self.assertEqual(blockers(help_evaluate(attestations=help_attestations(platform="darwin"),
                                                platform_name="darwin")), [])

    def test_mechanisms_are_judged_per_platform(self) -> None:
        claims = replaced(task_attestations(), "process_containment",
                          attestation("process_containment", mechanism="os_sandbox"))
        self.assertEqual(blockers(model_evaluate(attestations=claims)),
                         [("control", "process_containment", "mechanism_insufficient")])


class DeterminismTests(unittest.TestCase):
    def test_output_is_deterministic_and_independent_of_record_order(self) -> None:
        evidence = full_evidence(network=True) + [probe_record("output_format", "cli_help")]
        claims = task_attestations() + [attestation("filesystem", origin="provider_reported")]
        first = model_evaluate(evidence, claims)
        self.assertEqual(first, model_evaluate(list(reversed(evidence)), list(reversed(claims))))
        self.assertEqual(first, model_evaluate(evidence, claims))
        self.assertEqual(list(first["controls"]), list(CONTROLS))

    def test_reason_order_is_fixed_regardless_of_failure_order(self) -> None:
        result = model_evaluate([], [], platform_name="windows")
        self.assertEqual([item[0] for item in codes(result)],
                         ["platform"] + ["evidence"] * 7 + ["control"] * 8 + ["gate"] * 2)

    def test_inputs_are_not_mutated(self) -> None:
        evidence, claims = full_evidence(network=True), task_attestations()
        before = copy.deepcopy((evidence, claims))
        model_evaluate(evidence, claims)
        self.assertEqual((evidence, claims), before)


class IsolationFromSideEffectsTests(unittest.TestCase):
    class Tripwire(dict):
        def __getitem__(self, key): raise AssertionError("environment read")
        def get(self, *args): raise AssertionError("environment read")
        def __iter__(self): raise AssertionError("environment read")
        def __contains__(self, key): raise AssertionError("environment read")

    def test_evaluation_has_no_process_filesystem_network_credential_or_ledger_side_effects(self) -> None:
        import sqlite3
        with tempfile.TemporaryDirectory() as temp:
            before = os.getcwd()
            os.chdir(temp)
            try:
                with patch("subprocess.Popen", side_effect=AssertionError("spawned")), \
                        patch("os.system", side_effect=AssertionError("shell")), \
                        patch("socket.socket", side_effect=AssertionError("network")), \
                        patch("socket.create_connection", side_effect=AssertionError("network")), \
                        patch("builtins.open", side_effect=AssertionError("file opened")), \
                        patch("os.mkdir", side_effect=AssertionError("mkdir")), \
                        patch("os.replace", side_effect=AssertionError("write")), \
                        patch("sqlite3.connect", side_effect=AssertionError("ledger")), \
                        patch.object(os, "environ", self.Tripwire()), \
                        patch("runtime.probe_authorization_ledger.consume_authorization",
                              side_effect=AssertionError("ledger consumed")), \
                        patch("runtime.probe_authorization_ledger.check_authorization_available",
                              side_effect=AssertionError("ledger read")), \
                        patch("runtime.codex_probe_runner._run_probe", side_effect=AssertionError("runner")), \
                        patch("runtime.codex_probe_runner._run_command", side_effect=AssertionError("runner")):
                    for result in (model_evaluate(), help_evaluate(), model_evaluate([], [])):
                        self.assertEqual(result["decision"]["status"], "unauthorized")
                self.assertEqual(list(Path(temp).iterdir()), [])
            finally:
                os.chdir(before)
        self.assertTrue(sqlite3.connect)

    def test_module_imports_nothing_that_can_execute_or_touch_state(self) -> None:
        tree = ast.parse(Path(execution_safety_gate.__file__).read_text(encoding="utf-8"))
        imported = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                imported.update(alias.name for alias in node.names)
            elif isinstance(node, ast.ImportFrom):
                imported.add("." * node.level + (node.module or ""))
        self.assertEqual(imported, {"__future__", "hashlib", "json", "math", ".runtime_validation",
                                    ".synapse_plan"})

    def test_gate_is_not_wired_into_any_other_runtime_module(self) -> None:
        runtime = Path(__file__).resolve().parent.parent / "runtime"
        for path in runtime.rglob("*.py"):
            if path.name != "execution_safety_gate.py":
                with self.subTest(path=path.name):
                    self.assertNotIn("execution_safety_gate", path.read_text(encoding="utf-8"))

    def test_gate_has_no_public_cli_surface(self) -> None:
        root = Path(__file__).resolve().parent.parent
        for relative in ("runtime/cli.py", "bin/mco.js", "mco"):
            path = root / relative
            if path.is_file():
                self.assertNotIn("safety_gate", path.read_text(encoding="utf-8"))


class InheritedCommandCompatibilityTests(unittest.TestCase):
    def inherited(self, **metadata: object) -> list[str]:
        task = TaskInput(task_id="gate-compat", prompt="<not-executed>", repo_root="<repo-root>",
                         target_paths=["."], timeout_seconds=1,
                         metadata={"model": "model-1", "output_schema_path": "<schema>", **metadata})
        return CodexAdapter()._build_command(task)

    def test_inherited_mco_command_vectors_are_bindable_without_running_the_adapter(self) -> None:
        read_only = self.inherited(provider_permissions={"sandbox": "read-only", "approval_policy": "never"})
        bypass = self.inherited(provider_permissions={"bypass": "true"})
        self.assertNotEqual(command_specification_sha256(read_only), command_specification_sha256(bypass))
        evidence = full_evidence(network=True)
        result = model_evaluate(evidence, task_attestations(command=tuple(read_only)), command=read_only)
        self.assertIs(result["decision"]["requirements_satisfied"], True)
        self.assertEqual(result["command_sha256"], command_specification_sha256(read_only))
        self.assertIs(result["decision"]["execution_authorized"], False)

    def test_evidence_for_the_read_only_command_cannot_authorize_the_bypass_command(self) -> None:
        read_only = self.inherited(provider_permissions={"sandbox": "read-only", "approval_policy": "never"})
        bypass = self.inherited(provider_permissions={"bypass": "true"})
        self.assertIn("--dangerously-bypass-approvals-and-sandbox", bypass)
        result = model_evaluate(attestations=task_attestations(command=tuple(read_only)), command=bypass)
        self.assertEqual(result["attestations"]["command_mismatch"], len(CONTROLS))
        self.assertEqual(blockers(result), [("control", name, "unknown") for name in CONTROLS])

    def test_digest_is_a_function_of_the_exact_vector(self) -> None:
        self.assertEqual(command_specification_sha256(["a", "b"]), command_specification_sha256(("a", "b")))
        self.assertNotEqual(command_specification_sha256(["a", "b"]), command_specification_sha256(["b", "a"]))
        self.assertNotEqual(command_specification_sha256(["a b"]), command_specification_sha256(["a", "b"]))


class SchemaTests(unittest.TestCase):
    def test_invalid_attestations_are_rejected(self) -> None:
        good = attestation("filesystem")
        bad = [{**good, "control": "magic"}, {**good, "level": "unknown"}, {**good, "origin": "user"},
               {**good, "outcome": "ok"}, {**good, "mechanism": "trust_me"}, {**good, "observed": "free text"},
               {**good, "observed": "replayed"}, {**good, "executable_sha256": "abc"},
               {**good, "command_sha256": "abc"}, {**good, "probe_id": "two words"}, {**good, "platform": "two words"},
               {**good, "issued_at": "now"}, {**good, "issued_at": True}, {**good, "expires_at": float("nan")},
               {**good, "expires_at": float("inf")}, {**good, "expires_at": good["issued_at"]},
               {**good, "expires_at": good["issued_at"] - 1}, {**good, "expires_at": good["issued_at"] + 901},
               {**good, "extra": "field"}, {key: value for key, value in good.items() if key != "probe_id"},
               "not-a-record"]
        for item in bad:
            with self.subTest(item=item), self.assertRaises(PlanError):
                parse_attestations([item])
        for item in ("not-a-list", [good] * (MAX_ATTESTATIONS + 1)):
            with self.assertRaises(PlanError):
                parse_attestations(item)

    def test_invalid_requests_are_rejected(self) -> None:
        for override in ({"task_kind": "dispatch"}, {"executable_sha256": "short"}, {"command": []},
                         {"command": "exec --help"}, {"command": ["ok", 3]}, {"command": ["a\0b"]},
                         {"command": ["x" * 4097]}, {"command": ["a"] * 257}, {"probe_id": "two words"},
                         {"platform_name": "two words"}, {"now": "now"}, {"now": True},
                         {"now": float("nan")}, {"model": None}, {"model": "two words"},
                         {"effort": "two words"}, {"evidence": "nope"}, {"attestations": "nope"}):
            with self.subTest(override=override), self.assertRaises(PlanError):
                model_evaluate(**override)
        for override in ({"model": "model-1"}, {"effort": "medium"}):
            with self.subTest(override=override), self.assertRaises(PlanError):
                help_evaluate(**override)

    def test_effort_is_required_only_when_requested(self) -> None:
        self.assertEqual(blockers(model_evaluate(effort="medium")), [("evidence", "reasoning_effort", "unknown")])
        self.assertEqual(blockers(model_evaluate()), [])


class ReviewRegressionTests(unittest.TestCase):
    def test_oversized_numbers_are_rejected_as_plan_errors(self) -> None:
        with self.assertRaises(PlanError):
            model_evaluate(now=10**400)
        with self.assertRaises(PlanError):
            parse_attestations([attestation("filesystem", issued=10**400, expires=10**400 + 1)])

    def test_claims_for_another_platform_cannot_satisfy_a_control(self) -> None:
        # "os_sandbox" names different mechanisms on different operating systems.
        result = model_evaluate(attestations=task_attestations(platform="linux"), platform_name="darwin")
        self.assertEqual(result["attestations"]["platform_mismatch"], len(CONTROLS))
        self.assertEqual(blockers(result), [("control", name, "unknown") for name in CONTROLS])
        self.assertEqual(blockers(help_evaluate(attestations=help_attestations(platform="linux"),
                                                platform_name="darwin")),
                         [("control", name, "unknown") for name in HELP_CONTROLS])

    def test_decision_records_the_policy_version(self) -> None:
        self.assertEqual(model_evaluate()["policy_version"], 1)

    def test_legacy_probe_records_alone_never_satisfy_without_a_probe_evidence_window(self) -> None:
        claims = without(task_attestations(), "probe_evidence")
        self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", "probe_evidence", "unknown")])
        expired = replaced(task_attestations(), "probe_evidence",
                           attestation("probe_evidence", level="observed", origin="synapse_observed",
                                       mechanism="none", expires=NOW - 1))
        self.assertEqual(blockers(model_evaluate(attestations=expired)), [("control", "probe_evidence", "expired")])

    def test_a_historical_failure_does_not_poison_a_distinct_configuration(self) -> None:
        failures = [attestation("filesystem", outcome="fail", observed="write_succeeded", **change)
                    for change in ({"command": OTHER_COMMAND}, {"probe_id": "run-0"}, {"platform": "darwin"},
                                   {"sha": OTHER_SHA})]
        result = model_evaluate(attestations=[*task_attestations(), *failures])
        self.assertEqual(blockers(result), [])
        probe_failure = [{**probe_record("read_only_filesystem", "external_check", "fail",
                                         observed="write_succeeded"), "probe_id": "run-0"}]
        self.assertEqual(blockers(model_evaluate(full_evidence(network=True) + probe_failure)), [])

    def test_any_party_can_only_worsen_a_control(self) -> None:
        # A provider-origin record naming another mechanism turns a satisfied control into a conflict.
        claims = [*task_attestations(), attestation("filesystem", origin="provider_reported",
                                                    mechanism="process_group")]
        self.assertEqual(blockers(model_evaluate(attestations=claims)), [("control", "filesystem", "conflicting")])

    def test_failures_against_a_control_the_tier_does_not_require_block(self) -> None:
        claims = [*help_attestations(), attestation("network", outcome="fail", observed="network_reachable",
                                                    command=HELP_COMMAND)]
        self.assertEqual(blockers(help_evaluate(attestations=claims)), [("control", "network", "failed")])

    def test_validity_window_boundaries(self) -> None:
        live = replaced(task_attestations(), "filesystem", attestation("filesystem", issued=NOW, expires=NOW + 900))
        self.assertEqual(blockers(model_evaluate(attestations=live)), [])
        with self.assertRaises(PlanError):
            parse_attestations([attestation("filesystem", issued=NOW, expires=NOW + 900.001)])
        self.assertEqual(model_evaluate(attestations=live, now=NOW + 900)["controls"]["filesystem"]["status"],
                         "expired")

    def test_import_closure_reaches_no_runner_ledger_cli_or_session_module(self) -> None:
        import subprocess
        import sys
        root = Path(__file__).resolve().parent.parent
        code = ("import sys; import runtime.execution_safety_gate; "
                "print(sorted(name for name in sys.modules if name.startswith('runtime.')))")
        found = subprocess.run([sys.executable, "-I", "-c", f"import sys; sys.path.insert(0, {str(root)!r}); {code}"],
                               capture_output=True, text=True, check=True, cwd=tempfile.gettempdir()).stdout
        for forbidden in ("codex_probe_runner", "probe_authorization_ledger", "probe_confirmation",
                          "codex_capabilities", "runtime.cli", "invocation_runtime", "runtime.session",
                          "mcp_server", "runtime.adapters"):
            self.assertNotIn(forbidden, found)


if __name__ == "__main__":
    unittest.main()
