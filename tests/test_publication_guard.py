"""Publication guard: this repository must never publish the inherited npm package.

The root package is MCO's, under the upstream maintainer's npm scope. The inherited
publish workflow is kept for provenance but cannot reach its publish job, the package
is marked private, and no other workflow may publish. Ordelis release distribution is a
separate concern with its own, separately reviewed workflow.

These checks read the workflow files as text because CI has no YAML parser. Every rule
has a mutation test that applies one regression to the real text and expects it flagged.
"""

from __future__ import annotations

import http.server
import json
import os
import re
import shutil
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
WORKFLOW_DIR = ROOT / ".github" / "workflows"
GUARDED = "publish-npm.yml"
PREVIEW = "preview-package.yml"
GUARD_JOB = "publication-guard"

# Anything that can publish to, or authenticate against, an npm registry.
PUBLISH_PATTERNS = {
    "npm publish": r"\b(?:npm|pnpm)\s+publish\b|\byarn\s+(?:npm\s+)?publish\b",
    "npm credentials": r"NODE_AUTH_TOKEN|NPM_TOKEN|\bnpm\s+whoami\b",
    "registry setup": r"registry-url",
    "provenance": r"id-token\s*:\s*write|--provenance",
}


def _code(text: str) -> str:
    """The workflow without full-line comments, so prose cannot trigger a rule."""
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))


def _triggers(code: str) -> set[str]:
    lines = code.splitlines()
    start = next(i for i, line in enumerate(lines) if re.match(r"on\s*:", line))
    if lines[start].split(":", 1)[1].strip():
        raise AssertionError("unsupported inline `on:` form")
    found: set[str] = set()
    for line in lines[start + 1:]:
        if line and not line.startswith(" "):
            break
        match = re.match(r"  ([A-Za-z_]+)\s*:", line)
        if match:
            found.add(match.group(1))
    return found


def _jobs(code: str) -> dict[str, str]:
    lines = code.splitlines()
    start = next(i for i, line in enumerate(lines) if re.match(r"jobs\s*:\s*$", line))
    jobs: dict[str, list[str]] = {}
    current = None
    for line in lines[start + 1:]:
        if line and not line.startswith(" "):
            break
        match = re.match(r"  ([A-Za-z0-9_-]+)\s*:\s*$", line)
        if match:
            current = match.group(1)
            jobs[current] = []
        elif current is not None:
            jobs[current].append(line)
    return {name: "\n".join(body) for name, body in jobs.items()}


def _needs(block: str) -> list[str]:
    match = re.search(r"^    needs\s*:\s*(.*)$", block, re.MULTILINE)
    if not match:
        return []
    value = match.group(1).strip()
    if value.startswith("["):
        return [item.strip() for item in value.strip("[]").split(",") if item.strip()]
    if value:
        return [value]
    after = block[match.end():].splitlines()
    items = []
    for line in after:
        item = re.match(r"      -\s*(\S+)", line)
        if not item:
            break
        items.append(item.group(1))
    return items


def _reaches_guard(name: str, jobs: dict[str, str], seen: frozenset[str] = frozenset()) -> bool:
    if name in seen:
        return False
    for needed in _needs(jobs[name]):
        if needed == GUARD_JOB or (needed in jobs and _reaches_guard(needed, jobs, seen | {name})):
            return True
    return False


def guarded_workflow_violations(text: str) -> list[str]:
    code = _code(text)
    found = []
    triggers = _triggers(code)
    if triggers != {"release"}:
        found.append(f"the inherited workflow may only trigger on release, found {sorted(triggers)}")
    for token in ("always()", "failure()", "cancelled()", "continue-on-error"):
        if token in code:
            found.append(f"`{token}` could run a job after the guard fails")
    jobs = _jobs(code)
    guard = jobs.get(GUARD_JOB)
    if guard is None:
        return found + [f"missing the `{GUARD_JOB}` job"]
    if "exit 1" not in guard:
        found.append("the guard job must always exit 1")
    if re.search(r"^    if\s*:", guard, re.MULTILINE):
        found.append("the guard job must be unconditional")
    if re.search(r"secrets\.|\bvars\.|github\.repository|github\.actor|github\.event", guard):
        found.append("the guard must not depend on secrets, variables, repository identity or event data")
    for name, block in jobs.items():
        if name == GUARD_JOB:
            continue
        for needed in _needs(block):
            if needed not in jobs:
                found.append(f"job `{name}` needs unknown job `{needed}`")
        if not _reaches_guard(name, jobs):
            found.append(f"job `{name}` must depend, directly or transitively, on `{GUARD_JOB}`")
    return found


def preview_violations(text: str) -> list[str]:
    code = _code(text)
    found = []
    if re.search(r"\$\{\{\s*secrets\.", code):
        found.append("the preview workflow must not use secrets")
    if "release" in _triggers(code):
        found.append("the preview workflow must not run on release")
    if re.search(r"permissions\s*:\s*write-all", code):
        found.append("the preview workflow must not use write-all")
    for key in re.findall(r"^\s+([A-Za-z-]+)\s*:\s*write\s*$", code, re.MULTILINE):
        if key != "pull-requests":
            found.append(f"the preview workflow must not request `{key}: write`")
    return found


def repository_violations(workflows: dict[str, str]) -> list[str]:
    found = []
    for name, text in workflows.items():
        if name == GUARDED:
            continue
        code = _code(text)
        hits = [label for label, rx in PUBLISH_PATTERNS.items() if re.search(rx, code, re.IGNORECASE)]
        if hits:
            found.append(f"{name} must not publish to or authenticate against npm ({', '.join(hits)})")
    if GUARDED in workflows:
        found += [f"{GUARDED}: {item}" for item in guarded_workflow_violations(workflows[GUARDED])]
    if PREVIEW in workflows:
        found += [f"{PREVIEW}: {item}" for item in preview_violations(workflows[PREVIEW])]
    return found


def load_workflows() -> dict[str, str]:
    return {
        path.name: path.read_text(encoding="utf-8")
        for path in sorted([*WORKFLOW_DIR.glob("*.yml"), *WORKFLOW_DIR.glob("*.yaml")])
    }


def mutate(text: str, old: str, new: str) -> str:
    assert old in text, f"mutation anchor not found, the test would prove nothing: {old!r}"
    return text.replace(old, new, 1)


class PublicationGuardTests(unittest.TestCase):
    def test_no_workflow_can_publish_to_npm(self) -> None:
        self.assertEqual(repository_violations(load_workflows()), [])

    def test_guard_job_comes_first_and_every_other_job_depends_on_it(self) -> None:
        jobs = _jobs(_code(load_workflows()[GUARDED]))
        self.assertEqual(next(iter(jobs)), GUARD_JOB)
        self.assertTrue(_reaches_guard("publish", jobs))
        self.assertTrue(_reaches_guard("gate", jobs))

    def test_the_guard_does_not_rely_on_a_missing_token(self) -> None:
        guard = _jobs(_code(load_workflows()[GUARDED]))[GUARD_JOB]
        self.assertNotIn("NPM_TOKEN", guard)
        self.assertNotIn("secrets.", guard)

    def test_preview_workflow_cannot_publish(self) -> None:
        text = load_workflows()[PREVIEW]
        self.assertEqual(preview_violations(text), [])
        self.assertNotRegex(_code(text), r"npm\s+publish")

    def test_root_package_is_private(self) -> None:
        package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
        self.assertIs(package.get("private"), True)
        self.assertEqual(package["name"], "@tt-a1i/mco")

    @unittest.skipUnless(shutil.which("npm"), "npm is not installed")
    def test_npm_refuses_to_upload_the_root_package_even_when_logged_in(self) -> None:
        # The dangerous case is a configured token, so one is supplied, but only to a registry
        # on loopback that this test controls. Without a login npm stops earlier with ENEEDAUTH,
        # which would not prove the package is guarded.
        package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
        without_flag = {key: value for key, value in package.items() if key != "private"}

        refused, refused_requests = self._publish_to_local_registry(package)
        control, control_requests = self._publish_to_local_registry(without_flag)

        self.assertNotEqual(refused.returncode, 0)
        self.assertIn("EPRIVATE", refused.stdout + refused.stderr)
        self.assertNotIn("PUT", refused_requests)
        # The control shows the sandbox would have uploaded, so the refusal comes from `private`.
        self.assertIn("PUT", control_requests)

    def _publish_to_local_registry(self, package: dict) -> tuple[subprocess.CompletedProcess[str], list[str]]:
        requests: list[str] = []

        class Registry(http.server.BaseHTTPRequestHandler):
            def _answer(self) -> None:
                self.rfile.read(int(self.headers.get("content-length") or 0))
                requests.append(self.command)
                body = json.dumps({"error": "local test registry"}).encode()
                self.send_response(404 if self.command == "GET" else 403)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            do_GET = do_PUT = do_POST = do_DELETE = do_HEAD = _answer

            def log_message(self, *args: object) -> None:
                pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Registry)
        port = server.server_address[1]
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                directory = Path(tmp)
                (directory / "package.json").write_text(json.dumps(package), encoding="utf-8")
                (directory / ".npmrc").write_text(
                    f"registry=http://127.0.0.1:{port}/\n//127.0.0.1:{port}/:_authToken=local-test-token\n",
                    encoding="utf-8",
                )
                env = {k: v for k, v in os.environ.items() if k not in ("NODE_AUTH_TOKEN", "NPM_TOKEN")}
                env.update(
                    NPM_CONFIG_USERCONFIG=str(directory / "user-npmrc"),
                    NPM_CONFIG_GLOBALCONFIG=str(directory / "global-npmrc"),
                    NPM_CONFIG_CACHE=str(directory / "cache"),
                    NPM_CONFIG_FETCH_RETRIES="0",
                )
                result = subprocess.run(
                    ["npm", "publish", "--ignore-scripts"],
                    cwd=directory,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=120,
                    check=False,
                )
        finally:
            server.shutdown()
            server.server_close()
        return result, requests


class PublicationGuardRegressionTests(unittest.TestCase):
    """Each test applies one regression to the real files and expects it to be flagged."""

    def setUp(self) -> None:
        self.workflows = load_workflows()
        self.guarded = self.workflows[GUARDED]
        self.assertEqual(repository_violations(self.workflows), [], "the real files must pass first")

    def _flagged(self, name: str, text: str, needle: str) -> None:
        mutated = dict(self.workflows)
        mutated[name] = text
        found = repository_violations(mutated)
        self.assertTrue(any(needle in item for item in found), f"expected {needle!r} in {found}")

    def test_manual_dispatch_trigger_is_flagged(self) -> None:
        text = mutate(self.guarded, "on:\n  release:", "on:\n  workflow_dispatch:\n  release:")
        self._flagged(GUARDED, text, "may only trigger on release")

    def test_additional_triggers_are_flagged(self) -> None:
        for trigger in ("push:", "pull_request:", "schedule:", "workflow_run:"):
            with self.subTest(trigger=trigger):
                text = mutate(self.guarded, "on:\n  release:", f"on:\n  {trigger}\n  release:")
                self._flagged(GUARDED, text, "may only trigger on release")

    def test_removing_the_guard_job_is_flagged(self) -> None:
        text = mutate(self.guarded, f"  {GUARD_JOB}:", "  not-the-guard:")
        self._flagged(GUARDED, text, f"missing the `{GUARD_JOB}` job")

    def test_a_guard_that_can_pass_is_flagged(self) -> None:
        text = mutate(self.guarded, "          exit 1", "          exit 0")
        self._flagged(GUARDED, text, "must always exit 1")

    def test_a_conditional_guard_is_flagged(self) -> None:
        for condition in ("${{ secrets.NPM_TOKEN != '' }}", "${{ vars.ALLOW_PUBLISH }}",
                          "${{ github.repository == 'example/other' }}", "false"):
            with self.subTest(condition=condition):
                text = mutate(self.guarded, "    runs-on: ubuntu-latest\n    permissions: {}",
                              f"    runs-on: ubuntu-latest\n    if: {condition}\n    permissions: {{}}")
                self._flagged(GUARDED, text, "must be unconditional")

    def test_a_guard_step_that_reads_a_secret_is_flagged(self) -> None:
        text = mutate(self.guarded, "      - name: Refuse to publish",
                      "      - name: Refuse to publish\n        env:\n          T: ${{ secrets.NPM_TOKEN }}")
        self._flagged(GUARDED, text, "must not depend on secrets")

    def test_gate_without_the_dependency_is_flagged(self) -> None:
        text = mutate(self.guarded, "    needs: publication-guard\n", "")
        self._flagged(GUARDED, text, "job `gate` must depend")

    def test_publish_without_any_path_to_the_guard_is_flagged(self) -> None:
        text = mutate(mutate(self.guarded, "    needs: publication-guard\n", ""), "    needs: gate\n", "")
        self._flagged(GUARDED, text, "job `publish` must depend")

    def test_status_functions_that_override_the_dependency_are_flagged(self) -> None:
        for expression in ("always()", "failure()", "cancelled()"):
            with self.subTest(expression=expression):
                text = mutate(self.guarded, "    needs: gate\n", f"    needs: gate\n    if: ${{{{ {expression} }}}}\n")
                self._flagged(GUARDED, text, expression)

    def test_continue_on_error_is_flagged(self) -> None:
        text = mutate(self.guarded, "    permissions: {}", "    permissions: {}\n    continue-on-error: true")
        self._flagged(GUARDED, text, "continue-on-error")

    def test_a_dependency_on_an_unknown_job_is_flagged(self) -> None:
        text = mutate(self.guarded, "    needs: gate\n", "    needs: [gate, missing-job]\n")
        self._flagged(GUARDED, text, "unknown job `missing-job`")

    def test_a_new_workflow_that_publishes_is_flagged(self) -> None:
        for body in ("run: npm publish", "run: pnpm publish", "run: yarn npm publish",
                     "env:\n  NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}", "uses: actions/setup-node@v4\n  with:\n    registry-url: x",
                     "permissions:\n  id-token: write", "run: npm install --provenance"):
            with self.subTest(body=body):
                mutated = dict(self.workflows)
                mutated["ordelis-release.yml"] = f"name: x\non:\n  release:\n    types: [published]\njobs:\n  j:\n    {body}\n"
                self.assertTrue(any("ordelis-release.yml must not publish" in item for item in repository_violations(mutated)))

    def test_every_existing_workflow_is_checked(self) -> None:
        for name in self.workflows:
            if name == GUARDED:
                continue
            with self.subTest(workflow=name):
                self._flagged(name, self.workflows[name] + "\n      - run: npm publish\n", f"{name} must not publish")

    def test_the_preview_workflow_cannot_gain_publication_powers(self) -> None:
        cases = (
            ("      - name: Checkout\n", "      - run: echo ${{ secrets.NPM_TOKEN }}\n      - name: Checkout\n", "must not use secrets"),
            ("permissions:\n  contents: read", "permissions:\n  contents: write", "contents: write"),
            ("permissions:\n  contents: read", "permissions:\n  contents: read\n  packages: write", "packages: write"),
            ("  workflow_dispatch:", "  workflow_dispatch:\n  release:", "must not run on release"),
        )
        preview = self.workflows[PREVIEW]
        for old, new, needle in cases:
            with self.subTest(needle=needle):
                self._flagged(PREVIEW, mutate(preview, old, new), needle)

    def test_a_comment_that_mentions_publishing_is_not_a_violation(self) -> None:
        mutated = dict(self.workflows)
        mutated["gate.yml"] = "# never run npm publish here\n" + mutated["gate.yml"]
        mutated[GUARDED] = "# if: always() would defeat the guard\n" + mutated[GUARDED]
        self.assertEqual(repository_violations(mutated), [])


if __name__ == "__main__":
    unittest.main()
