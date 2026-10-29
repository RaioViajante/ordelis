"""Ownership and security reporting for this repository.

CODEOWNERS must route reviews to the Ordelis maintainer and cover every tracked file. The
issue-template security link must name this repository's canonical identity and no upstream
project. SECURITY.md must keep private vulnerability reporting as the primary route, never
tell reporters to disclose in public or to put details in a first message, and stay honest
about an early-stage project. The checks look at behavior, not exact wording. Each rule has
a mutation test that applies one regression to the real file and expects it to be flagged.

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
CANONICAL_REPOSITORY = "RaioViajante/ordelis"
SECURITY_POLICY_URL = f"https://github.com/{CANONICAL_REPOSITORY}/security/policy"
FORMER_REPOSITORY_NAMES = ("synapse",)
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
    found = []
    if SECURITY_POLICY_URL not in urls:
        found.append(f"the contact links must include {SECURITY_POLICY_URL}")
    for url in urls:
        if any(marker in url for marker in UPSTREAM_MARKERS):
            found.append(f"{url} points at an upstream project")
        if any(f"/{name}/" in url for name in FORMER_REPOSITORY_NAMES):
            found.append(f"{url} uses a former repository name")
    return found


PRIVATE_REPORTING = r"private\s+vulnerability\s+reporting"
REPORT_VERBS = r"\b(?:open|file|create|submit|raise|post|use)\b[^.\n]*\b(?:issues?|pull requests?|discussions?|bug[- ]reports?|issue templates?)\b"
# A negation only counts when it governs the instruction verb ("do not open ..."). A stray "not"
# elsewhere in the sentence ("if that is not available, open an issue") must not excuse it.
NEGATED_INSTRUCTION = r"\b(?:do not|don't|never|must not|should not|cannot|can't)\s+(?:\w+\s+){0,3}?(?:open|file|create|submit|raise|post|use)\b"


def security_policy_violations(text: str) -> list[str]:
    found = []
    lowered = text.lower()
    private = re.search(PRIVATE_REPORTING, lowered)
    if not private:
        found.append("must make private vulnerability reporting the primary route")
    for sentence in re.split(r"(?<=[.!?])\s+|\n+", lowered):
        if re.search(REPORT_VERBS, sentence) and not re.search(NEGATED_INSTRUCTION, sentence):
            found.append(f"must not tell reporters to use an issue, pull request or discussion: {sentence.strip()[:60]!r}")
    fallback = re.search(r"\bprofile\b|contact the maintainer", lowered)
    if fallback:
        if private and fallback.start() < private.start():
            found.append("the fallback must not come before private vulnerability reporting")
        if not re.search(r"(?:do not|don't|never)\s+include[^.]{0,60}details|without\s+(?:any\s+)?details|no\s+details", lowered):
            found.append("the fallback must say not to put vulnerability details in a first message")
    if "sample data" not in lowered:
        found.append("must say the interface is a preview over sample data")
    if re.search(r"(?<!not )(?<!not yet )production[- ]ready", lowered):
        found.append("must not claim the project is production-ready")
    if not re.search(r"^#+[^\n]*\bsupport", lowered, re.MULTILINE):
        found.append("must state the supported-version policy")
    if re.search(r"\bwithin\s+\d+\s+(?:business\s+)?(?:hours?|days?|weeks?)\b|\bsla\b", lowered):
        found.append("must not promise a response time")
    if re.search(r"[\w.+-]+@[\w-]+\.[\w.-]+", text):
        found.append("must not publish a contact address that has not been established")
    if re.search(r"\bsec\d+\b", lowered):
        found.append("must not refer to internal incident identifiers")
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

    def test_security_link_uses_the_canonical_repository(self) -> None:
        self.assertEqual(security_link_violations(ISSUE_CONFIG.read_text(encoding="utf-8")), [])

    def test_blank_issues_stay_disabled(self) -> None:
        self.assertIn("blank_issues_enabled: false", ISSUE_CONFIG.read_text(encoding="utf-8"))

    def test_security_policy_keeps_private_reporting_primary_and_honest(self) -> None:
        self.assertEqual(security_policy_violations(SECURITY.read_text(encoding="utf-8")), [])

    def test_the_security_policy_has_no_public_reporting_path_through_issue_templates(self) -> None:
        # With blank issues disabled, any issue a reporter opens goes through a template, and the
        # bug template requires reproduction details. The policy must not send reporters there.
        self.assertNotIn("bug_report", SECURITY.read_text(encoding="utf-8"))
        self.assertIn("blank_issues_enabled: false", ISSUE_CONFIG.read_text(encoding="utf-8"))


class RepositoryConfigurationRegressionTests(unittest.TestCase):
    """Each test applies one regression to the real files and expects it to be flagged."""

    def setUp(self) -> None:
        self.codeowners = CODEOWNERS.read_text(encoding="utf-8")
        self.config = ISSUE_CONFIG.read_text(encoding="utf-8")
        self.policy = SECURITY.read_text(encoding="utf-8")
        self.paths = ["README.md", "runtime/cli.py", "desktop/src/main.tsx", ".github/CODEOWNERS", "docs/README.md"]
        self.assertEqual(codeowners_violations(self.codeowners, self.paths), [], "the real file must pass first")
        self.assertEqual(security_link_violations(self.config), [], "the real file must pass first")
        self.assertEqual(security_policy_violations(self.policy), [], "the real file must pass first")

    def _flagged(self, found: list[str], needle: str) -> None:
        self.assertTrue(any(needle in item for item in found), f"expected {needle!r} in {found}")

    # -- CODEOWNERS
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

    # -- issue-template security link
    def test_a_link_to_a_former_repository_name_is_flagged(self) -> None:
        for name in FORMER_REPOSITORY_NAMES:
            with self.subTest(name=name):
                text = mutate(self.config, SECURITY_POLICY_URL, SECURITY_POLICY_URL.replace("/ordelis/", f"/{name}/"))
                found = security_link_violations(text)
                self._flagged(found, "former repository name")
                self._flagged(found, "must include")

    def test_an_upstream_security_url_is_flagged(self) -> None:
        text = mutate(self.config, SECURITY_POLICY_URL, "https://github.com/tt-a1i/mco/security/advisories/new")
        found = security_link_violations(text)
        self._flagged(found, "points at an upstream project")
        self._flagged(found, "must include")

    def test_a_different_repository_or_host_is_flagged(self) -> None:
        for url in ("https://github.com/RaioViajante/another-repo/security/policy", "https://example.com/security/policy"):
            with self.subTest(url=url):
                self._flagged(security_link_violations(mutate(self.config, SECURITY_POLICY_URL, url)), "must include")

    def test_a_missing_contact_link_is_flagged(self) -> None:
        self._flagged(security_link_violations("blank_issues_enabled: false\n"), "must include")

    # -- SECURITY.md disclosure behavior
    def test_removing_private_vulnerability_reporting_is_flagged(self) -> None:
        # The policy names the route more than once, so every mention has to go.
        self.assertGreater(self.policy.count("private vulnerability reporting"), 1)
        text = self.policy.replace("private vulnerability reporting", "a web form")
        self._flagged(security_policy_violations(text), "primary route")

    def test_any_public_reporting_instruction_is_flagged(self) -> None:
        for sentence in ("If that option is not available, open an issue titled \"Security contact request\" that contains no details.",
                         "Open an issue titled Security contact request.",
                         "Please file a bug report describing the problem.",
                         "You can submit a pull request with the details.",
                         "Create a discussion and describe what you found.",
                         "Use the issue template to describe it."):
            with self.subTest(sentence=sentence):
                self._flagged(security_policy_violations(self.policy + "\n" + sentence + "\n"), "must not tell reporters")

    def test_a_negated_instruction_is_not_a_violation(self) -> None:
        text = self.policy + "\nNever open an issue to describe a vulnerability.\n"
        self.assertEqual(security_policy_violations(text), [])

    def test_a_fallback_that_allows_details_in_a_first_message_is_flagged(self) -> None:
        text = mutate(self.policy, "Do not include any details of the vulnerability in that first message.",
                      "Send the full report there.")
        self._flagged(security_policy_violations(text), "first message")

    def test_a_fallback_ahead_of_private_reporting_is_flagged(self) -> None:
        text = "Contact the maintainer through their profile for anything. Do not include details.\n\n" + self.policy
        self._flagged(security_policy_violations(text), "must not come before")

    def test_policy_honesty_regressions_are_flagged(self) -> None:
        cases = (
            ("Ordelis is in early development and is not production-ready.", "Ordelis is production-ready.", "production-ready"),
            ("a preview over sample data", "a finished product", "sample data"),
            ("## Supported versions", "## Other notes", "supported-version policy"),
            ("No response or fix time is guaranteed.", "We respond within 48 hours.", "response time"),
            ("No response or fix time is guaranteed.", "Email security@example.org for help.", "contact address"),
            ("No response or fix time is guaranteed.", "See SEC9 for details.", "incident identifiers"),
        )
        for old, new, needle in cases:
            with self.subTest(needle=needle):
                self._flagged(security_policy_violations(mutate(self.policy, old, new)), needle)

    def test_a_differently_worded_policy_with_the_same_behavior_passes(self) -> None:
        # The checks are about behavior. A rewrite with other headings and sentences must still pass.
        reworded = (
            "# Security\n\n"
            "Ordelis is a preview, it is not yet production-ready, and its screens show sample data.\n\n"
            "## How to report\n\n"
            "Report problems privately through GitHub's Private Vulnerability Reporting on the Security tab. "
            "Never describe a vulnerability in a public thread.\n\n"
            "Without a GitHub account, ask the maintainer for a private channel through their profile page. "
            "Send no details until one exists.\n\n"
            "## Which versions are supported\n\n"
            "Only the main branch.\n"
        )
        self.assertEqual(security_policy_violations(reworded), [])

    def test_a_not_yet_production_ready_statement_is_not_a_violation(self) -> None:
        text = self.policy + "\nOrdelis is not yet production-ready.\n"
        self.assertEqual(security_policy_violations(text), [])


if __name__ == "__main__":
    unittest.main()
