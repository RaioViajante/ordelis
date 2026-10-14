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
   (2 shell snapshots, 2 session rollouts, 18 bundled skill files, 4 temporary files),
   across two stacked PRs. The base PR removes 24 and keeps the two credential-bearing
   shell snapshots byte-identical to `main`, so its own rendered diff contains no
   section for them at all; the stacked removal PR deletes those two. Before deletion it
   was verified that no runtime module, test, script, workflow or document references
   them; curated probe evidence (result, parsed and summary files) was left untouched.
2. **Narrow `.gitignore` rules** so the same categories cannot be staged again:
   - `.probe_*_home/` — probe-local `HOME`/`CODEX_HOME` directories at any depth
     (shell snapshots, session rollouts, provider auth state).
   - `shell_snapshots/` — environment captures, by directory name.
   - `docs/probes/**/raw/` — raw provider stdout/stderr transcripts from probe runs.
3. **Diff redaction:** `.gitattributes` marks probe-home and shell-snapshot paths `-diff`,
   so local git diffs (`git show`, `git log -p`, `git diff`) report them as binary and
   never print their contents. GitHub's hosted diff views ignore this attribute — see
   "What this does not protect".
4. **Probe script sanitization:** `scripts/run_capability_probes.sh` rejects any `HOME`
   or `CODEX_HOME` that resolves inside the repository or probe artifact root, including
   relative paths and symlink aliases. Legitimate external provider homes remain
   unchanged, so configured authentication is not copied or stripped. Probe subprocesses
   run in their own process groups; timeout and signal handling terminates the group.
   Exit and signal traps remove `shell_snapshots/` and `sessions/` only below the
   verified, dedicated run root, preserve the probe exit status, and turn cleanup errors
   into failures.
5. **Fail-closed CI export:** raw logs, `parsed.json`, `result.json`, human summaries and
   lock files are not uploaded. They contain provider output, provider-controlled version
   text or local paths and therefore have no safe upload contract. Provider subprocesses
   never receive a trusted export path. Only after the probe step succeeds and cleanup
   succeeds does a separate workflow step create a randomly named mode-`0700` directory,
   reconstruct one mode-`0600` JSON document from a strict allowlist, and validate its
   path, ownership, type, directory contents and canonical bytes. The upload step requires
   both preceding steps to have succeeded plus the staging step's explicit authorization
   output; it does not use `always()`.
6. **Regression tests:** `tests/test_probe_artifact_hygiene.py` asserts that no
   probe-home or shell-snapshot path is tracked beyond a documented pending-removal set
   that the stacked removal PR empties, that sensitive paths are ignored, that curated
   evidence remains committable, and that the paths are marked `-diff` for local Git.
   Probe fixture tests cover ordinary provider failure, cleanup failure, signals, process
   termination, unsafe provider homes, provider-forged summaries, unexpected files,
   symlink attempts and mutation attempts against trusted staging.

## What this protects

- New probe runs reject repository-local provider homes, known sensitive paths are
  ignored, and cleanup removes environment/session captures from the verified run root.
- CI never uploads the working probe tree. Failure, cancellation, interruption or cleanup
  failure prevents trusted staging and makes the upload step ineligible. A provider-created
  file at the former export filename is untrusted even if it is structurally valid, because
  the workflow neither validates nor uploads that location.
- Curated probe evidence (`result.json`, `parsed.json`, `summary.md`,
  `lock-summary.yaml`) remains locally committable for the documented workflow, but is
  deliberately excluded from CI uploads because its content is not bounded tightly
  enough to be considered non-sensitive.

## What this does not protect

- **Git history is unchanged.** The credential remains in historical commits.
  Removing files from the current tree neither revokes a credential nor erases it from
  history, and this change must not be described as doing so.
- **GitHub still renders deleted file contents.** Hosted diff views (pull requests,
  compare pages, commit pages) ignore the `-diff` attribute and display the removed
  lines of the shell snapshots. Those lines are identical to the content `main` has
  always served at the same paths, so no new material is disclosed — but any hosted
  diff of this deletion will show them. Local git diffs remain silent thanks to the
  `-diff` rule.
- **Revocation is still required** and is still pending. Until revocation is confirmed,
  the credential must be treated as compromised regardless of what the tree contains.
- **Historical raw transcripts stay tracked for now.** Audit evidence indicates they
  contain absolute developer paths and third-party session output but no credential-shaped
  values; removing or retaining them is a separate, narrowly justified decision.
- **Traps are not absolute cleanup guarantees.** `SIGKILL`, machine crashes and abrupt
  runner termination cannot execute shell traps. Temporary raw artifacts and a provider-
  forged file can remain on that machine until runner/workspace cleanup. GitHub Actions
  cancellation/failure semantics skip the success-gated trusted staging and upload steps,
  so those files do not become upload candidates even when probe cleanup never runs.

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
