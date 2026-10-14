# Probe artifact hygiene

Capability probes invoke third-party CLIs on the machine that runs them. Those tools can
capture environment data as a side effect: Codex, for example, writes shell snapshots
(verbatim copies of the invoking shell environment) and session transcripts into its
`CODEX_HOME`. If a provider's home directory lands inside the repository, that data —
including anything exported in the developer's shell — becomes a commit candidate.

## What happened (sanitized summary)

Early capability-probe runs inherited from the MCO history created a probe-local home
directory inside `docs/probes/`. Its shell snapshots and session transcripts were
committed, and one snapshot captured an apparent credential from the running machine's
environment. The value has never been printed, tested or used by this project. Revocation
has been requested from the possible owner through a private security channel and is
**not confirmed**. No credential value, fingerprint or secret-bearing line is recorded in
this document, the issue tracker, or the pull request.

## What we changed

1. **Removed all 26 tracked probe-local home artifacts from the current tree**
   (2 shell snapshots, 2 session rollouts, 18 bundled skill files, 4 temporary files).
   Before deletion it was verified that no runtime module, test, script, workflow or
   document references them; curated probe evidence (result, parsed and summary files)
   was left untouched. The two credential-bearing shell snapshots are deleted here, in
   a stacked PR whose merge base already contains the `.gitattributes` `-diff` rule, so
   their deletion renders as "binary files differ" — no rendered diff in either PR of
   this work displays their contents.
2. **Narrow `.gitignore` rules** so the same categories cannot be staged again:
   - `.probe_*_home/` — probe-local `HOME`/`CODEX_HOME` directories at any depth
     (shell snapshots, session rollouts, provider auth state).
   - `shell_snapshots/` — environment captures, by directory name.
   - `docs/probes/**/raw/` — raw provider stdout/stderr transcripts from probe runs.
3. **Diff redaction:** `.gitattributes` marks probe-home and shell-snapshot paths `-diff`,
   so their contents are never rendered as line text in any diff whose merge base
   carries that rule — which is what makes the stacked follow-up deletion render as
   "binary files differ" instead of line content.
4. **Probe script sanitization:** `scripts/run_capability_probes.sh` now prunes
   `shell_snapshots/` and `sessions/` subtrees from the artifacts root after probes
   finish and before summaries are written, so environment captures are absent from the
   artifacts that CI uploads. The script still never assigns `HOME` or `CODEX_HOME`:
   it does not strip credentials providers need for their C0 auth checks, and it never
   touches a home outside the artifacts root. No environment is ever logged in full.
5. **Regression tests:** `tests/test_probe_artifact_hygiene.py` asserts that no
   probe-home or shell-snapshot path is tracked, that nothing survives under the
   historical probe home, that sensitive paths are ignored, that curated evidence
   remains committable and that sensitive paths are marked `-diff`, and that the probe
   script keeps its pruning and never points a provider at a repository-local home.

## What this protects

- New probe runs cannot reintroduce environment captures or session transcripts into
  Git history: the paths are ignored, and the script removes them from the artifact set
  before CI collects it.
- Curated probe evidence (`result.json`, `parsed.json`, `summary.md`,
  `lock-summary.yaml`) remains committable, so the documented capability-probe workflow
  and the gate summary contract are unchanged.

## What this does not protect

- **Git history is unchanged.** The credential remains in historical commits.
  Removing files from the current tree neither revokes a credential nor erases it from
  history, and this change must not be described as doing so.
- **Revocation is still required** and is still pending. Until revocation is confirmed,
  the credential must be treated as compromised regardless of what the tree contains.
- **Historical raw transcripts stay tracked for now.** Audit evidence indicates they
  contain absolute developer paths and third-party session output but no credential-shaped
  values; removing or retaining them is a separate, narrowly justified decision.
- **A caller can still export `CODEX_HOME` into the repository.** Ignore rules and
  script-level pruning contain the damage, but rejecting that configuration outright
  trades it for an auth-state isolation decision (probes need the provider's real home to
  check login status) and is deferred to a separate issue.

## Why Git history needs a separate decision

Purging history means rewriting every commit and force-pushing a public repository: all
clones must be re-created, open pull requests and issue references are disturbed, and
other agents' feature branches would have to be rebased. That is an explicit, coordinated
decision — never a side effect of a cleanup PR. Until such a decision is made, assume the
historical copy remains readable by anyone who cloned or cached the repository.

## Why credential revocation remains necessary

The artifacts were pushed to a public repository, so exposure cannot be undone by file
removal; only the credential owner can invalidate the value. Rotation (or provider-side
revocation) is the single action that ends the risk, and it must be confirmed by the
owner — not inferred from this repository change.
