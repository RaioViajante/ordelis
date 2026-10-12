from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from runtime.codex_capabilities import inspect_codex_cli
from runtime.runtime_validation import (
    CAPABILITIES, MAX_EVIDENCE_RECORDS, check_probe_authorization, evaluate_validation, parse_evidence,
)
from runtime.synapse_plan import PlanError
from tests.test_codex_capabilities import EXEC_HELP, ROOT_HELP

SHA = "a" * 64
OTHER_SHA = "b" * 64


def record(capability: str, source: str, outcome: str = "pass", *, sha: str = SHA,
           requested: str | None = None, observed: str | None = None, probe_id: str = "fixture-1") -> dict:
    return {"capability": capability, "source": source, "outcome": outcome, "executable_sha256": sha,
            "requested": requested, "observed": observed, "probe_id": probe_id}


def full_evidence(*, sha: str = SHA, effort: bool = False, network: bool = False) -> list[dict]:
    items = [
        record("executable_identity", "runtime_probe", sha=sha),
        record("model_acceptance", "runtime_probe", sha=sha, requested="model-1", observed="model-1"),
        record("read_only_filesystem", "external_check", sha=sha),
        record("output_format", "runtime_probe", sha=sha),
        record("process_termination", "runtime_probe", sha=sha),
        record("session_persistence", "external_check", sha=sha),
    ]
    if effort:
        items.append(record("reasoning_effort", "runtime_probe", sha=sha, requested="medium", observed="medium"))
    if network:
        items.append(record("network_restriction", "external_check", sha=sha))
    return items


def evaluate(evidence: list[dict], **kwargs) -> dict:
    kwargs.setdefault("model", "model-1")
    return evaluate_validation(SHA, evidence, **kwargs)


class EvidenceEvaluationTests(unittest.TestCase):
    def test_complete_evidence_validates_but_never_authorizes_dispatch(self) -> None:
        result = evaluate(full_evidence(effort=True, network=True), effort="medium",
                          require_network_restriction=True)
        self.assertEqual(result["validation"], {"status": "validated", "blocking_reasons": [],
                                                "dispatch_authorized": False})
        self.assertEqual(result["stale_evidence"], 0)
        self.assertEqual(list(result["capabilities"]), list(CAPABILITIES))
        self.assertTrue(all(item["status"] == "satisfied" for item in result["capabilities"].values()))

    def test_missing_evidence_is_unknown_and_blocking(self) -> None:
        result = evaluate([], effort="medium", require_network_restriction=True)
        self.assertEqual(result["validation"]["status"], "blocked")
        self.assertEqual(result["validation"]["blocking_reasons"], [f"{name}:unknown" for name in CAPABILITIES])

    def test_optional_capabilities_do_not_block_when_not_requested(self) -> None:
        result = evaluate(full_evidence())
        self.assertFalse(result["capabilities"]["network_restriction"]["required"])
        self.assertFalse(result["capabilities"]["reasoning_effort"]["required"])
        self.assertEqual(result["validation"]["status"], "validated")
        self.assertEqual(evaluate(full_evidence(), require_network_restriction=True)["validation"]["blocking_reasons"],
                         ["network_restriction:unknown"])

    def test_advertised_or_declared_support_is_insufficient(self) -> None:
        evidence = [item for item in full_evidence() if item["capability"] != "model_acceptance"]
        evidence += [record("model_acceptance", "adapter_declaration", requested="model-1", observed="model-1"),
                     record("model_acceptance", "cli_help", requested="model-1")]
        result = evaluate(evidence)
        self.assertEqual(result["capabilities"]["model_acceptance"]["status"], "insufficient_evidence")
        self.assertEqual(result["capabilities"]["model_acceptance"]["highest_level"], "cli_advertised")
        self.assertEqual(result["validation"]["blocking_reasons"], ["model_acceptance:insufficient_evidence"])

    def test_successful_execution_never_proves_isolation(self) -> None:
        evidence = [item for item in full_evidence(network=True)
                    if item["capability"] not in {"read_only_filesystem", "network_restriction", "session_persistence"}]
        evidence += [record(name, "runtime_probe") for name in
                     ("read_only_filesystem", "network_restriction", "session_persistence")]
        result = evaluate(evidence, require_network_restriction=True)
        self.assertEqual(result["validation"]["blocking_reasons"],
                         [f"{name}:insufficient_evidence" for name in
                          ("read_only_filesystem", "network_restriction", "session_persistence")])
        for name in ("read_only_filesystem", "network_restriction", "session_persistence"):
            self.assertEqual(result["capabilities"][name]["highest_level"], "runtime_observed")
            self.assertEqual(result["capabilities"][name]["minimum_level"], "independently_verified")

    def test_failed_unsupported_and_conflicting_evidence_block(self) -> None:
        failed = full_evidence() + [record("read_only_filesystem", "external_check", "fail",
                                           observed="write_succeeded", probe_id="fixture-2")]
        result = evaluate(failed)
        self.assertEqual(result["validation"]["blocking_reasons"], ["read_only_filesystem:failed"])
        unsupported = full_evidence() + [record("output_format", "adapter_declaration", "unsupported")]
        self.assertEqual(evaluate(unsupported)["validation"]["blocking_reasons"], ["output_format:unsupported"])

    def test_timeout_and_cancellation_failures_block(self) -> None:
        for observed in ("timeout", "descendant_survived"):
            with self.subTest(observed=observed):
                evidence = full_evidence() + [record("process_termination", "runtime_probe", "fail",
                                                     observed=observed, probe_id="fixture-2")]
                self.assertEqual(evaluate(evidence)["validation"]["blocking_reasons"],
                                 ["process_termination:failed"])

    def test_model_and_effort_mismatch_block(self) -> None:
        evidence = [item for item in full_evidence(effort=True) if item["capability"] not in
                    {"model_acceptance", "reasoning_effort"}]
        evidence += [record("model_acceptance", "runtime_probe", requested="model-1", observed="model-2"),
                      record("reasoning_effort", "runtime_probe", requested="medium", observed="low")]
        result = evaluate(evidence, effort="medium")
        self.assertEqual(result["validation"]["blocking_reasons"],
                         ["model_acceptance:mismatch", "reasoning_effort:mismatch"])

    def test_evidence_for_another_value_or_without_a_value_does_not_satisfy(self) -> None:
        base = [item for item in full_evidence() if item["capability"] != "model_acceptance"]
        other = base + [record("model_acceptance", "runtime_probe", requested="model-2", observed="model-2")]
        self.assertEqual(evaluate(other)["capabilities"]["model_acceptance"]["status"], "unknown")
        agnostic = base + [record("model_acceptance", "runtime_probe")]
        self.assertEqual(evaluate(agnostic)["capabilities"]["model_acceptance"]["status"], "insufficient_evidence")

    def test_stale_executable_evidence_is_ignored_and_counted(self) -> None:
        result = evaluate(full_evidence(sha=OTHER_SHA))
        self.assertEqual(result["stale_evidence"], 6)
        self.assertEqual(result["validation"]["status"], "blocked")
        self.assertTrue(all(item["status"] == "unknown" for item in result["capabilities"].values()
                            if item["required"]))

    def test_changed_executable_makes_earlier_evidence_stale(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            binary = Path(temp) / "fake-codex"

            def write(version: str) -> str:
                binary.write_text("#!/usr/bin/env python3\nimport sys\n"
                                  f"VERSION = {version!r}\nROOT = {ROOT_HELP!r}\nEXEC = {EXEC_HELP!r}\n"
                                  "if sys.argv[1:] == ['--version']:\n    print(VERSION)\n"
                                  "elif sys.argv[1:] == ['--help']:\n    print(ROOT)\n"
                                  "elif sys.argv[1:] == ['exec', '--help']:\n    print(EXEC)\n", encoding="utf-8")
                binary.chmod(0o755)
                return str(inspect_codex_cli(str(binary))["binary"]["sha256"])

            first = write("codex-cli 0.162.1")
            evidence = full_evidence(sha=first)
            self.assertEqual(evaluate_validation(first, evidence, model="model-1")["validation"]["status"],
                             "validated")
            second = write("codex-cli 0.162.2")
            self.assertNotEqual(first, second)
            result = evaluate_validation(second, evidence, model="model-1")
            self.assertEqual(result["stale_evidence"], 6)
            self.assertEqual(result["validation"]["status"], "blocked")

    def test_result_is_deterministic_and_independent_of_evidence_order(self) -> None:
        evidence = full_evidence(effort=True) + [record("output_format", "cli_help", probe_id="fixture-2")]
        first = evaluate(evidence, effort="medium")
        self.assertEqual(first, evaluate(list(reversed(evidence)), effort="medium"))
        self.assertEqual(first, evaluate(evidence, effort="medium"))

    def test_evaluation_starts_no_process_and_writes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as temp, patch("subprocess.Popen", side_effect=AssertionError("spawned")):
            before = os.getcwd()
            os.chdir(temp)
            try:
                evaluate(full_evidence())
                self.assertEqual(list(Path(temp).iterdir()), [])
            finally:
                os.chdir(before)


class EvidenceSchemaTests(unittest.TestCase):
    def test_invalid_records_are_rejected(self) -> None:
        good = record("output_format", "runtime_probe")
        bad_records = [
            {**good, "capability": "network"}, {**good, "source": "provider_said_so"},
            {**good, "outcome": "ok"}, {**good, "executable_sha256": "abc"},
            {**good, "probe_id": "contains space"}, {**good, "observed": "line\nbreak"},
            {**good, "observed": "x" * 129}, {**good, "requested": 3}, {**good, "extra": "field"},
            {key: value for key, value in good.items() if key != "probe_id"}, "not-a-record",
        ]
        for bad in bad_records:
            with self.subTest(bad=bad), self.assertRaises(PlanError):
                parse_evidence([bad])
        for bad in ("not-a-list", [good] * (MAX_EVIDENCE_RECORDS + 1)):
            with self.assertRaises(PlanError):
                parse_evidence(bad)

    def test_invalid_evaluation_inputs_are_rejected(self) -> None:
        with self.assertRaises(PlanError):
            evaluate_validation("short", [], model="model-1")
        with self.assertRaises(PlanError):
            evaluate_validation(SHA, [], model="has space")
        with self.assertRaises(PlanError):
            evaluate_validation(SHA, [], model="model-1", effort="two words")


class ProbeAuthorizationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.repo = self.root / "repo"
        self.vault = self.root / "vault"
        self.scratch = self.root / "scratch"
        for path in (self.repo, self.vault, self.scratch):
            path.mkdir()

    def authorization(self, **changes: object) -> dict:
        value = {"schema_version": 1, "executable_sha256": SHA, "capabilities": ["read_only_filesystem"],
                 "scratch_root": str(self.scratch), "execution_mode": "read_only", "max_model_calls": 0,
                 "network": "none", "hard_timeout_seconds": 30}
        value.update(changes)
        return value

    def check(self, raw: object, sha: str = SHA) -> dict:
        return check_probe_authorization(raw, repo_root=self.repo, protected_roots=(self.vault,),
                                         executable_sha256=sha)

    def test_absent_authorization_is_denied(self) -> None:
        self.assertEqual(self.check(None), {"probe_authorized": False, "dispatch_authorized": False,
                                            "blocking_reasons": ["no_authorization_supplied"]})

    def test_valid_authorization_permits_a_probe_but_not_dispatch(self) -> None:
        self.assertEqual(self.check(self.authorization()),
                         {"probe_authorized": True, "blocking_reasons": [], "dispatch_authorized": False})
        calls = self.authorization(max_model_calls=1, network="provider_endpoint_only")
        self.assertTrue(self.check(calls)["probe_authorized"])

    def test_stale_authorization_is_denied(self) -> None:
        result = self.check(self.authorization(), sha=OTHER_SHA)
        self.assertEqual(result["blocking_reasons"], ["authorization_executable_mismatch"])
        self.assertFalse(result["probe_authorized"])

    def test_malformed_or_weakened_authorization_is_rejected(self) -> None:
        other = self.root / "other"
        other.mkdir()
        (self.root / "link").symlink_to(other)
        (other / "x").write_text("data", encoding="utf-8")
        bad = [
            self.authorization(schema_version=2), self.authorization(execution_mode="write"),
            self.authorization(capabilities=[]), self.authorization(capabilities=["unknown"]),
            self.authorization(capabilities=["output_format", "output_format"]),
            self.authorization(max_model_calls=4, network="provider_endpoint_only"),
            self.authorization(max_model_calls=1, network="none"),
            self.authorization(network="provider_endpoint_only"),
            self.authorization(hard_timeout_seconds=301), self.authorization(executable_sha256="abc"),
            self.authorization(scratch_root="relative"), self.authorization(scratch_root=str(self.root / "link")),
            self.authorization(scratch_root=str(other)),
            self.authorization(extra="field"), "not-an-object",
        ]
        for raw in bad:
            with self.subTest(raw=raw), self.assertRaises(PlanError):
                self.check(raw)

    def test_scratch_must_not_overlap_checkout_or_protected_roots(self) -> None:
        inside_repo = self.repo / "scratch"
        inside_vault = self.vault / "scratch"
        inside_repo.mkdir()
        inside_vault.mkdir()
        for path in (self.repo, inside_repo, inside_vault, self.vault, self.root):
            with self.subTest(path=path), self.assertRaises(PlanError):
                self.check(self.authorization(scratch_root=str(path)))

    def test_authorization_check_starts_no_process_and_leaves_scratch_empty(self) -> None:
        with patch("subprocess.Popen", side_effect=AssertionError("spawned")):
            self.check(self.authorization())
        self.assertEqual(list(self.scratch.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
