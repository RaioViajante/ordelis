"""Ownership and security reporting for this repository.

CODEOWNERS must route reviews to the Ordelis maintainer and cover every tracked file. The
issue-template security link must name this repository's canonical identity and no upstream
project. SECURITY.md must keep private vulnerability reporting as the primary route, offer the
official security address only as a private fallback in a correctly formed mailto link, never
tell reporters to disclose in public, and stay honest about an early-stage project. The checks look at behavior, not exact wording. Each rule has
a mutation test that applies one regression to the real file and expects it to be flagged.

The CODEOWNERS matcher supports only the two pattern forms the file uses (`*` and
`/directory/`) and raises on anything else, so a more complex pattern cannot silently
escape validation.
"""

from __future__ import annotations

import re
import subprocess
import unittest
import urllib.parse
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CODEOWNERS = ROOT / ".github" / "CODEOWNERS"
ISSUE_CONFIG = ROOT / ".github" / "ISSUE_TEMPLATE" / "config.yml"
SECURITY = ROOT / "SECURITY.md"
MAINTAINER = "@RaioViajante"
CANONICAL_REPOSITORY = "RaioViajante/ordelis"
SECURITY_POLICY_URL = f"https://github.com/{CANONICAL_REPOSITORY}/security/policy"
SECURITY_EMAIL = "security@ordelis.app"
EMAIL_ADDRESS = r"[\w.+-]+@[\w-]+(?:\.[\w-]+)*\.[A-Za-z]{2,}"
WELL_FORMED_ADDRESS = r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}"
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

# Public places where a vulnerability must not be described. "issue" only counts when it names the
# tracker or a GitHub issue ("a security issue" does not).
PUBLIC_CHANNEL = (
    r"\b(?:issue\s+trackers?|(?:an?|public|new|github)\s+issues?|issue\s+templates?|pull\s+requests?"
    r"|discussions?|bug[- ]reports?|public\s+(?:threads?|forums?|posts?))\b"
)
REPORTING_VERB = r"(?:open|file|create|submit|raise|post|use|describe|disclose|report|put|include|share|publish|mention|send|write|add|discuss|comment|tell)"
# A mention of a public channel is a violation unless a prohibition governs it. A prohibition is a
# negator followed directly by the instruction ("do not open ..."), allowing only harmless fillers.
# So "if you cannot use X, open an issue" (the negation is about X), "don't worry, just open ..." and
# "do not hesitate to open ..." (an encouragement) are not prohibitions. It covers the rest of the
# clause and stops at a clause break, so "do not open an issue; instead file a bug report" is caught.
PROHIBITION = (
    rf"\b(?:do not|don't|never|must not|should not|refrain from)\s+(?:(?:publicly|openly|ever|just|simply|also|even|please)\s+){{0,2}}{REPORTING_VERB}\b"
    r"|\bavoid\b"
)
CLAUSE_BREAK = rf";|\b(?:but|instead|otherwise|alternatively|however|then)\b|,\s*(?:and\s+)?(?:please\s+)?{REPORTING_VERB}\b"


def classify_public_channels(sentence: str) -> tuple[list[str], list[str]]:
    """The public channels a sentence mentions, split into those no prohibition governs and those one does."""
    covered = []
    for match in re.finditer(PROHIBITION, sentence):
        clause_break = re.search(CLAUSE_BREAK, sentence[match.end():])
        covered.append((match.start(), match.end() + (clause_break.start() if clause_break else len(sentence))))
    ungoverned, governed = [], []
    for channel in re.finditer(PUBLIC_CHANNEL, sentence):
        is_governed = any(low <= channel.start() < high for low, high in covered)
        (governed if is_governed else ungoverned).append(channel.group(0))
    return ungoverned, governed


def claims_production_readiness(lowered: str) -> bool:
    """True if "production-ready" is asserted, i.e. no negation shortly before it in the same clause."""
    for match in re.finditer(r"production[- ]ready", lowered):
        # The negation has to be in the same clause: "do not worry: it is production-ready" is a claim.
        clause_before = re.split(r"[.!?\n:;,]", lowered[max(0, match.start() - 80):match.start()])[-1]
        if not re.search(r"\b(?:not|isn't|aren't|never|no|nothing)\b(?:\W+\w+){0,4}\W*$", clause_before):
            return True
    return False


def security_email_violations(text: str, sentences: list[str]) -> list[str]:
    """The official address is the only one, is a usable mailto link, and is a private fallback."""
    found = []
    addresses = {address.rstrip(".").lower() for address in re.findall(EMAIL_ADDRESS, text)}
    if addresses - {SECURITY_EMAIL}:
        found.append("must not publish a contact address other than the official security address")
    target = f"mailto:{SECURITY_EMAIL}"
    links = re.findall(r"\[([^\]]+)\]\((mailto:[^)]*)\)", text)
    usable = [
        link for link in links
        if link[0] == SECURITY_EMAIL and link[1] == target and re.fullmatch(WELL_FORMED_ADDRESS, link[1][len("mailto:"):])
    ]
    if not usable:
        return found + [f"must offer {SECURITY_EMAIL} as a mailto link whose text and target both name it"]
    # The primary route has to be stated in a different sentence, ahead of the address. The fallback sentence
    # mentions private reporting too ("if you cannot use it"), so that mention alone proves nothing.
    address_at = next(index for index, item in enumerate(sentences) if target in item)
    primary_at = next((index for index, item in enumerate(sentences) if re.search(PRIVATE_REPORTING, item) and target not in item), None)
    if primary_at is None or primary_at > address_at:
        found.append("the email fallback must not come before private vulnerability reporting")
    sentence = sentences[address_at]
    if not (re.search(r"\b(?:if|when|unless|where)\b", sentence) and re.search(PRIVATE_REPORTING + r"|\bgithub\b", sentence)):
        found.append("the email must be offered only as a fallback when private vulnerability reporting cannot be used")
    if not (re.search(r"\b(?:send|email|submit|report)\b", sentence) and re.search(r"\bprivately\b|\bprivate\s+reports?\b|\bin\s+confidence\b", sentence)):
        found.append("must explicitly allow private reports to be sent to the address")
    return found


def security_policy_violations(text: str) -> list[str]:
    found = []
    lowered = text.lower()
    private = re.search(PRIVATE_REPORTING, lowered)
    if not private:
        found.append("must make private vulnerability reporting the primary route")
    sentences = re.split(r"(?<=[.!?])\s+|\n+", lowered)
    prohibits_public_disclosure = False
    for sentence in sentences:
        ungoverned, governed = classify_public_channels(sentence)
        prohibits_public_disclosure = prohibits_public_disclosure or bool(governed)
        if ungoverned:
            found.append(f"must not tell reporters to use an issue, pull request or discussion: {sentence.strip()[:60]!r}")
    if not prohibits_public_disclosure:
        found.append("must tell reporters not to describe vulnerabilities in public issues, pull requests or discussions")
    found += security_email_violations(text, sentences)
    if "sample data" not in lowered:
        found.append("must say the interface is a preview over sample data")
    if claims_production_readiness(lowered):
        found.append("must not claim the project is production-ready")
    if not re.search(r"^#+[^\n]*\bsupport", lowered, re.MULTILINE):
        found.append("must state the supported-version policy")
    if re.search(r"\bwithin\s+\d+\s+(?:business\s+)?(?:hours?|days?|weeks?)\b|\bsla\b", lowered):
        found.append("must not promise a response time")
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

    def test_the_security_address_is_a_well_formed_mailto_link(self) -> None:
        text = SECURITY.read_text(encoding="utf-8")
        links = re.findall(r"\[([^\]]+)\]\((mailto:[^)]*)\)", text)
        self.assertEqual(links, [(SECURITY_EMAIL, f"mailto:{SECURITY_EMAIL}")])
        parsed = urllib.parse.urlparse(links[0][1])
        self.assertEqual((parsed.scheme, parsed.path, parsed.query), ("mailto", SECURITY_EMAIL, ""))
        self.assertRegex(parsed.path, WELL_FORMED_ADDRESS)
        self.assertNotRegex(links[0][1], r"\s")

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

    def test_negation_bypasses_found_in_review_are_flagged(self) -> None:
        # Each of these was shown to evade an earlier version of the check.
        for sentence in (
            "If you cannot use private vulnerability reporting, open an issue with the details.",
            "If you can't use the Security tab, file a bug report.",
            "Do not hesitate to open a public issue describing it.",
            "Alternatively, tell us in the issue tracker.",
        ):
            with self.subTest(sentence=sentence):
                self._flagged(security_policy_violations(self.policy + "\n" + sentence + "\n"), "must not tell reporters")

    def test_variants_of_the_same_bypasses_are_flagged(self) -> None:
        for sentence in (
            "Never hesitate to open an issue about it.",
            "Don't worry, just open an issue.",
            "Do not open a public issue; instead file a bug report.",
            "Do not open an issue, file a bug report instead.",
            "If private reporting is unavailable, post it in the discussions.",
            "You may also mention it in a pull request.",
            "When in doubt, use the issue tracker.",
            "Feel free to describe it in a public issue.",
        ):
            with self.subTest(sentence=sentence):
                self._flagged(security_policy_violations(self.policy + "\n" + sentence + "\n"), "must not tell reporters")

    def test_legitimate_prohibitions_and_unrelated_uses_of_the_word_issue_pass(self) -> None:
        for sentence in (
            "Do not open a public issue.",
            "Please do not use the issue tracker or a bug report for vulnerabilities.",
            "Never post details in a pull request, a discussion or an issue.",
            "Do not publicly open an issue or describe the problem in a discussion.",
            "Avoid public issues and discussions.",
            "Do not open an issue, or file a bug report, or start a discussion.",
            "If you find a security issue, report it privately.",
            "If you cannot use private vulnerability reporting, email security@ordelis.app privately.",
        ):
            with self.subTest(sentence=sentence):
                self.assertEqual(security_policy_violations(self.policy + "\n" + sentence + "\n"), [])

    def test_a_negated_instruction_is_not_a_violation(self) -> None:
        text = self.policy + "\nNever open an issue to describe a vulnerability.\n"
        self.assertEqual(security_policy_violations(text), [])

    # -- the official email fallback
    def test_removing_the_security_address_is_flagged(self) -> None:
        text = mutate(self.policy, "[security@ordelis.app](mailto:security@ordelis.app)", "the maintainer")
        self._flagged(security_policy_violations(text), f"must offer {SECURITY_EMAIL} as a mailto link")

    def test_an_address_that_is_not_the_official_one_is_flagged(self) -> None:
        for wrong in ("security@ordelis.com", "secuirty@ordelis.app", "security@example.org", "maintainer@ordelis.app"):
            with self.subTest(address=wrong):
                found = security_policy_violations(self.policy.replace(SECURITY_EMAIL, wrong))
                self._flagged(found, "other than the official security address")
                self._flagged(found, f"must offer {SECURITY_EMAIL} as a mailto link")

    def test_a_second_address_next_to_the_official_one_is_flagged(self) -> None:
        text = self.policy + "\nYou can also write to admin@ordelis.app.\n"
        self._flagged(security_policy_violations(text), "other than the official security address")

    def test_a_plain_text_address_is_not_a_usable_mailto_link(self) -> None:
        text = mutate(self.policy, "[security@ordelis.app](mailto:security@ordelis.app)", "security@ordelis.app")
        self._flagged(security_policy_violations(text), f"must offer {SECURITY_EMAIL} as a mailto link")

    def test_a_malformed_or_mismatched_mailto_link_is_flagged(self) -> None:
        good = "[security@ordelis.app](mailto:security@ordelis.app)"
        for bad in ("[email us](mailto:security@ordelis.app)",
                    "[security@ordelis.app](mailto:security@ordelis.app?subject=Report)",
                    "[security@ordelis.app](mailto: security@ordelis.app)",
                    "[security@ordelis.app](https://ordelis.app/security)",
                    "[security@ordelis.app](mailto:Security@Ordelis.app)"):
            with self.subTest(link=bad):
                self._flagged(security_policy_violations(mutate(self.policy, good, bad)), f"must offer {SECURITY_EMAIL} as a mailto link")

    def test_an_email_that_is_not_a_conditional_private_fallback_is_flagged(self) -> None:
        sentence = ("If you cannot use private vulnerability reporting, you may send your report privately by email to "
                    "[security@ordelis.app](mailto:security@ordelis.app).")
        unconditional = "Send your report privately by email to [security@ordelis.app](mailto:security@ordelis.app)."
        self._flagged(security_policy_violations(mutate(self.policy, sentence, unconditional)), "only as a fallback")
        not_permitting = ("If you cannot use private vulnerability reporting, see [security@ordelis.app](mailto:security@ordelis.app).")
        self._flagged(security_policy_violations(mutate(self.policy, sentence, not_permitting)), "explicitly allow private reports")

    def test_removing_the_public_disclosure_prohibition_is_flagged(self) -> None:
        text = mutate(self.policy, "Do not open a public issue, pull request or discussion that describes a vulnerability.", "")
        self._flagged(security_policy_violations(text), "not to describe vulnerabilities in public")

    def test_an_email_ahead_of_private_reporting_is_flagged(self) -> None:
        # Move the real fallback sentence in front of the primary route. It mentions private reporting
        # itself ("if you cannot use it"), which must not count as stating the primary route.
        fallback = ("If you cannot use private vulnerability reporting, you may send your report privately by email to "
                    "[security@ordelis.app](mailto:security@ordelis.app).")
        moved = fallback + "\n\n" + mutate(self.policy, fallback, "")
        self._flagged(security_policy_violations(moved), "must not come before")

    def test_policy_honesty_regressions_are_flagged(self) -> None:
        cases = (
            ("Ordelis is in early development and is not production-ready.", "Ordelis is production-ready.", "production-ready"),
            ("a preview over sample data", "a finished product", "sample data"),
            ("## Supported versions", "## Other notes", "supported-version policy"),
            ("No response or fix time is guaranteed.", "We respond within 48 hours.", "response time"),
            ("No response or fix time is guaranteed.", "Email security@example.org for help.", "other than the official security address"),
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
            "If GitHub's private vulnerability reporting is unavailable to you, you can send a report privately by email to "
            "[security@ordelis.app](mailto:security@ordelis.app).\n\n"
            "## Which versions are supported\n\n"
            "Only the main branch.\n"
        )
        self.assertEqual(security_policy_violations(reworded), [])

    def test_production_readiness_negations_pass_and_claims_are_flagged(self) -> None:
        for sentence in ("Ordelis is not currently production-ready.", "It isn't production-ready.",
                         "The app is not yet fully production-ready.", "Nothing here is production ready yet, which is not a promise."):
            with self.subTest(passes=sentence):
                self.assertEqual(security_policy_violations(self.policy + "\n" + sentence + "\n"), [])
        for sentence in ("Ordelis is production-ready.", "The desktop app is nearly production ready.",
                         "Do not worry: it is production-ready."):
            with self.subTest(flagged=sentence):
                self._flagged(security_policy_violations(self.policy + "\n" + sentence + "\n"), "production-ready")

    def test_the_migration_record_does_not_describe_the_security_item_as_unassessed(self) -> None:
        record = (ROOT / "docs" / "migration" / "ordelis.md").read_text(encoding="utf-8")
        for stale in (r"open and unassessed", r"not assessed", r"high until assessed"):
            self.assertNotRegex(record.lower(), stale)

    def test_a_not_yet_production_ready_statement_is_not_a_violation(self) -> None:
        text = self.policy + "\nOrdelis is not yet production-ready.\n"
        self.assertEqual(security_policy_violations(text), [])


if __name__ == "__main__":
    unittest.main()
