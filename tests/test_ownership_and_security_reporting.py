"""Ownership and security reporting for this repository.

CODEOWNERS must route reviews to the Ordelis maintainer and cover every tracked file,
the issue-template security link must not point at an upstream project, and SECURITY.md
must stay honest about an early-stage project. Each rule has a mutation test that applies
one regression to the real file and expects it to be flagged.

The CODEOWNERS matcher supports only the two pattern forms the file uses (`*` and
`/directory/`) and raises on anything else, so a more complex pattern cannot silently
escape validation.
"""

from __future__ import annotations

import re
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CODEOWNERS = ROOT / ".github" / "CODEOWNERS"
ISSUE_CONFIG = ROOT / ".github" / "ISSUE_TEMPLATE" / "config.yml"
SECURITY = ROOT / "SECURITY.md"
MAINTAINER = "@RaioViajante"
UPSTREAM_MARKERS = ("tt-a1i", "mco-org")


def parse_codeowners(text: str) -> list[tuple[str, list[str]]]:
    rules = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        pattern, *owners = stripped.split()
        rules.append((pattern, owners))
    return rules


def matches(pattern: str, path: str) -> bool:
    if pattern == "*":
        return True
    if re.fullmatch(r"/[A-Za-z0-9._-]+/", pattern):
        return path.startswith(pattern[1:])
    raise AssertionError(f"unsupported CODEOWNERS pattern {pattern!r}: extend the matcher deliberately")


def owners_for(rules: list[tuple[str, list[str]]], path: str) -> list[str]:
    """The last matching rule wins, as GitHub documents."""
    found: list[str] = []
    for pattern, owners in rules:
        if matches(pattern, path):
            found = owners
    return found


def codeowners_violations(text: str, paths: list[str]) -> list[str]:
    rules = parse_codeowners(text)
    found = []
    if not rules or rules[0][0] != "*":
        found.append("the first rule must be the catch-all `*`")
    for pattern, owners in rules:
        if not owners:
            found.append(f"rule {pattern!r} names no owner")
        for owner in owners:
            if owner != MAINTAINER:
                found.append(f"rule {pattern!r} names {owner}, expected only {MAINTAINER}")
    for path in paths:
        if owners_for(rules, path) != [MAINTAINER]:
            found.append(f"{path} is not owned by {MAINTAINER}")
            break
    return found


def security_link_violations(text: str) -> list[str]:
    urls = re.findall(r"^\s*url:\s*(\S+)\s*$", text, re.MULTILINE)
    if not urls:
        return ["no contact link found"]
    found = []
    for url in urls:
        if not url.startswith("https://github.com/"):
            found.append(f"{url} must be an https://github.com/ URL")
        if any(marker in url for marker in UPSTREAM_MARKERS):
            found.append(f"{url} points at an upstream project")
        if not url.endswith("/security/policy"):
            found.append(f"{url} must point at the repository's security policy page")
    return found


def security_policy_violations(text: str) -> list[str]:
    found = []
    lowered = text.lower()
    if "report a vulnerability" not in lowered or "privately" not in lowered:
        found.append("must describe private vulnerability reporting")
    if "not production-ready" not in lowered:
        found.append("must say the project is not production-ready")
    if re.search(r"(?<!not )production[- ]ready", lowered):
        found.append("must not claim the project is production-ready")
    if "## supported versions" not in lowered:
        found.append("must state the supported-version policy")
    if re.search(r"\bwithin\s+\d+\s+(?:business\s+)?(?:hours?|days?|weeks?)\b|\bsla\b", lowered):
        found.append("must not promise a response time")
    if re.search(r"[\w.+-]+@[\w-]+\.[\w.-]+", text):
        found.append("must not publish a contact address that has not been established")
    if "sec1" in lowered:
        found.append("must not refer to internal incident tracking")
    return found


def tracked_files() -> list[str]:
    result = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=False)
    return result.stdout.splitlines() if result.returncode == 0 else []


def mutate(text: str, old: str, new: str) -> str:
    assert old in text, f"mutation anchor not found, the test would prove nothing: {old!r}"
    return text.replace(old, new, 1)


class RepositoryConfigurationTests(unittest.TestCase):
    def test_codeowners_routes_every_tracked_file_to_the_maintainer(self) -> None:
        paths = tracked_files()
        if not paths:
            self.skipTest("not a git checkout")
        self.assertGreater(len(paths), 100)
        self.assertEqual(codeowners_violations(CODEOWNERS.read_text(encoding="utf-8"), paths), [])

    def test_codeowners_names_no_upstream_owner(self) -> None:
        text = CODEOWNERS.read_text(encoding="utf-8")
        for marker in UPSTREAM_MARKERS:
            self.assertNotIn(marker, text)

    def test_codeowners_keeps_credit_to_the_original_authors(self) -> None:
        self.assertIn("MCO", CODEOWNERS.read_text(encoding="utf-8"))
        self.assertIn("MIT License", (ROOT / "LICENSE").read_text(encoding="utf-8"))
        self.assertIn("mco maintainers", (ROOT / "LICENSE").read_text(encoding="utf-8"))

    def test_security_link_points_at_this_project_policy(self) -> None:
        self.assertEqual(security_link_violations(ISSUE_CONFIG.read_text(encoding="utf-8")), [])

    def test_blank_issues_stay_disabled(self) -> None:
        self.assertIn("blank_issues_enabled: false", ISSUE_CONFIG.read_text(encoding="utf-8"))

    def test_security_policy_is_honest_about_an_early_stage_project(self) -> None:
        self.assertEqual(security_policy_violations(SECURITY.read_text(encoding="utf-8")), [])


class RepositoryConfigurationRegressionTests(unittest.TestCase):
    """Each test applies one regression to the real files and expects it to be flagged."""

    def setUp(self) -> None:
        self.codeowners = CODEOWNERS.read_text(encoding="utf-8")
        self.config = ISSUE_CONFIG.read_text(encoding="utf-8")
        self.policy = SECURITY.read_text(encoding="utf-8")
        self.paths = ["README.md", "runtime/cli.py", "desktop/src/main.tsx", ".github/CODEOWNERS", "docs/README.md"]
        self.assertEqual(codeowners_violations(self.codeowners, self.paths), [], "the real file must pass first")

    def _flagged(self, found: list[str], needle: str) -> None:
        self.assertTrue(any(needle in item for item in found), f"expected {needle!r} in {found}")

    def test_an_upstream_owner_on_any_rule_is_flagged(self) -> None:
        text = mutate(self.codeowners, "/docs/ @RaioViajante", "/docs/ @tt-a1i")
        self._flagged(codeowners_violations(text, self.paths), "names @tt-a1i")

    def test_an_extra_owner_that_would_require_their_approval_is_flagged(self) -> None:
        text = mutate(self.codeowners, "* @RaioViajante", "* @RaioViajante @tt-a1i")
        self._flagged(codeowners_violations(text, self.paths), "names @tt-a1i")

    def test_a_later_rule_that_overrides_ownership_is_flagged(self) -> None:
        text = self.codeowners + "\n/desktop/ @someone-else\n"
        self._flagged(codeowners_violations(text, self.paths), "desktop/src/main.tsx is not owned")

    def test_a_missing_catch_all_is_flagged(self) -> None:
        text = mutate(self.codeowners, "* @RaioViajante\n", "")
        found = codeowners_violations(text, self.paths)
        self._flagged(found, "first rule must be the catch-all")
        self._flagged(found, "is not owned")

    def test_a_rule_without_an_owner_is_flagged(self) -> None:
        text = mutate(self.codeowners, "/docs/ @RaioViajante", "/docs/")
        self._flagged(codeowners_violations(text, self.paths), "names no owner")

    def test_patterns_the_matcher_does_not_understand_fail_loudly(self) -> None:
        for pattern in ("docs/*", "**/logs", "*.md", "/docs/**"):
            with self.subTest(pattern=pattern):
                with self.assertRaises(AssertionError):
                    codeowners_violations(self.codeowners + f"\n{pattern} @RaioViajante\n", self.paths)

    def test_the_last_matching_rule_wins(self) -> None:
        rules = parse_codeowners("* @a\n/docs/ @b\n")
        self.assertEqual(owners_for(rules, "docs/x.md"), ["@b"])
        self.assertEqual(owners_for(rules, "README.md"), ["@a"])
        self.assertEqual(owners_for(rules, "desktop/docs/x.md"), ["@a"])

    def test_the_upstream_security_url_is_flagged(self) -> None:
        text = mutate(self.config, "https://github.com/RaioViajante/synapse/security/policy",
                      "https://github.com/tt-a1i/mco/security/advisories/new")
        found = security_link_violations(text)
        self._flagged(found, "points at an upstream project")
        self._flagged(found, "must point at the repository's security policy page")

    def test_a_non_https_or_off_site_link_is_flagged(self) -> None:
        for url in ("http://github.com/x/y/security/policy", "https://example.com/security/policy"):
            with self.subTest(url=url):
                text = mutate(self.config, "https://github.com/RaioViajante/synapse/security/policy", url)
                self._flagged(security_link_violations(text), "must be an https://github.com/ URL")

    def test_a_missing_contact_link_is_flagged(self) -> None:
        text = "blank_issues_enabled: false\n"
        self._flagged(security_link_violations(text), "no contact link found")

    def test_policy_regressions_are_flagged(self) -> None:
        cases = (
            ("**Report a vulnerability**", "**Submit a report**", "private vulnerability reporting"),
            ("it is not production-ready", "it is ready for production use", "not production-ready"),
            ("## Supported versions", "## Versions", "supported-version policy"),
            ("No response or fix time is guaranteed.", "We respond within 48 hours.", "response time"),
            ("No response or fix time is guaranteed.", "Email security@example.org for help.", "contact address"),
            ("No response or fix time is guaranteed.", "See SEC1 for details.", "incident tracking"),
        )
        for old, new, needle in cases:
            with self.subTest(needle=needle):
                self._flagged(security_policy_violations(mutate(self.policy, old, new)), needle)

    def test_a_production_readiness_claim_is_flagged(self) -> None:
        text = mutate(self.policy, "## Reporting a vulnerability", "Ordelis is production-ready.\n\n## Reporting a vulnerability")
        self._flagged(security_policy_violations(text), "must not claim the project is production-ready")


if __name__ == "__main__":
    unittest.main()
