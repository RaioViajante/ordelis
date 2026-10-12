# Provider runtime-validation foundations

`runtime/runtime_validation.py` defines how Synapse will judge evidence about a real provider CLI **before** any dispatch is allowed. It is a library module only: it has no CLI command, runs no provider, starts no process, reads no credentials, and writes no files. `mco context preview`, `mco context plan`, and `mco context inspect-codex` are unchanged and remain non-dispatching, and the MCO invocation workflow is not wired into Synapse planning. A test pins that no other runtime module references this one.

Evidence for a real provider does not exist yet. The tests use only synthetic records and a fake executable.

**The evaluator is a consistency check over unauthenticated input, not a trusted gate.** It cannot tell a record produced by trusted Synapse code from a forged one. Its output therefore says `evidence_authenticated: false` and `dispatch_authorized: false` in every case, including when all requirements are met.

## Evidence model

Each record states one fact about one capability of one executable, gathered in one probe run:

| Field | Meaning |
| --- | --- |
| `capability` | One of `executable_identity`, `model_acceptance`, `reasoning_effort`, `read_only_filesystem`, `network_restriction`, `output_format`, `process_termination`, `session_persistence`. |
| `source` | Where the fact claims to come from; it fixes the evidence level (below). |
| `outcome` | `pass`, `fail`, or `unsupported`. |
| `executable_sha256` | Hash of the resolved executable file the fact was gathered against (the `binary.sha256` of `inspect-codex`). |
| `requested` / `observed` | Only `model_acceptance` and `reasoning_effort` carry values (the requested and the reported model or effort). Every other capability must have `requested: null`, and `observed` is either `null` or one of a closed failure vocabulary (`timeout`, `cancelled`, `descendant_survived`, `write_succeeded`, `network_reachable`, `session_data_written`, `malformed_output`) that is never allowed on a `pass`. |
| `probe_id` | Identifier of the authorized probe run that produced the record. |

All string values must match `[A-Za-z0-9._:/@+=-]{1,128}`. The schema is closed: unknown fields are rejected and at most 64 records are accepted. For capabilities without values, the closed vocabulary leaves no room for free text. The two valued capabilities still accept any token of that shape, so a short secret-shaped string would fit: code that builds records must never copy provider output into them.

### Four evidence levels

| Source | Level | What it can establish |
| --- | --- | --- |
| `adapter_declaration` | `adapter_declared` | MCO's adapter says it supports a key or value. Says nothing about the installed CLI or the account. |
| `cli_help` | `cli_advertised` | The installed CLI's help lists a control. A parser option, not behavior. |
| `runtime_probe` | `runtime_observed` | Behavior seen in a controlled run of the provider, or a direct local observation such as hashing the executable file. The provider's own output is the witness. |
| `external_check` | `independently_verified` | A check made by Synapse outside the provider process, for example comparing the scratch directory before and after a probe, or inspecting a sandbox profile that Synapse applied. |

A fifth state, **unknown**, means no evidence exists for the current executable and probe run. `unsupported` is an explicit outcome: the provider or adapter says the capability is unavailable.

### What counts as verified

Every capability needs `runtime_observed` or better. The isolation properties `read_only_filesystem`, `network_restriction`, and `session_persistence` need `independently_verified`. A model that answers successfully, or reports that it did not write a file, is not evidence that the filesystem or network was restricted. A provider can claim anything about its own sandbox, and a run that happened not to write proves nothing about enforcement.

Always required: `executable_identity`, `model_acceptance` (for the requested model), `read_only_filesystem`, `output_format`, `process_termination`, `session_persistence`. Required when requested: `reasoning_effort` (an effort was asked for) and `network_restriction` (the caller requires it).

### Evaluation

`evaluate_validation(executable_sha256, evidence, probe_id=..., model=..., effort=..., require_network_restriction=...)` returns one status per capability, most severe first:

1. `failed`: any relevant record has outcome `fail`, from any source.
2. `mismatch`: any relevant passing record observed a value other than the requested one, whether or not the record named a request.
3. `unsupported`: a relevant record says the capability is unavailable.
4. `satisfied`: a passing record reaches the minimum level and, where a value was requested, requested and observed both equal it.
5. `insufficient_evidence`: only passing evidence below the minimum level, or a probe that did not confirm the value.
6. `unknown`: no relevant evidence.

Conflicting evidence resolves to the more severe status, so a failed, unsupported, or contradicting record is never overridden by a passing record of any trust level. Duplicate records do not change the outcome.

Records are filtered before judgment. Records for a different `executable_sha256` are ignored and counted in `stale_evidence`. Records from a different `probe_id` are ignored and counted in `unbound_evidence`, so results from an earlier run cannot satisfy a later request. Records tied to another requested value are not relevant to the current request. The result is deterministic and independent of record order.

Any required capability that is not `satisfied` appears in `validation.blocking_reasons` as `<capability>:<status>`, and `validation.status` is `blocked`. Otherwise the status is `evidence_sufficient`. The name is deliberate: it says the supplied records are internally sufficient, not that they are true.

### Synthetic example

Evidence against a fake executable whose hash is 64 `a` characters: model support only advertised in help, a filesystem pass that only the provider observed, a failed cancellation check, and nothing about session persistence.

```json
{"capability": "read_only_filesystem", "source": "runtime_probe", "outcome": "pass",
 "executable_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
 "requested": null, "observed": null, "probe_id": "run-1"}
```

Result for `model="model-1"`, `probe_id="run-1"`:

```json
{"status": "blocked", "evidence_authenticated": false,
 "blocking_reasons": ["model_acceptance:insufficient_evidence", "read_only_filesystem:insufficient_evidence",
                      "process_termination:failed", "session_persistence:unknown"],
 "dispatch_authorized": false}
```

## Evidence trust boundary

**Who can create records today.** Any code that can build a dictionary. No production module creates evidence yet; only tests do. Nothing in the library binds a record to its real producer, and `probe_id` is an unauthenticated label.

**What this means.**

- Provider output must never be parsed into a record. A provider is the party being constrained, so its statements about itself are at most `runtime_probe` evidence, and only when trusted Synapse code observed them.
- `source: "external_check"` is a label. Anyone can write it; a test shows that fully forged records produce `evidence_sufficient`. The label only expresses what a trusted producer is obliged to mean by it.
- Replay: the `probe_id` filter stops accidental reuse of earlier runs, not deliberate forgery, because the forger also chooses the `probe_id`.
- A successful model run cannot satisfy an isolation requirement, because only `external_check` can. A forged `external_check` can.

**Trusted construction rule.** Records with source `runtime_probe` or `external_check` may only be built by reviewed Synapse probe-runner code, in the same process and invocation that evaluates them, from the runner's own observations. They must not be read from a file, a command-line argument, an environment variable, a prompt, a vault note, or provider output. Following that rule is what makes `evidence_sufficient` meaningful. A simpler trusted construction boundary like this is enough for the planned runner; cryptographic signing would only be needed if evidence ever had to cross a trust boundary, which is not planned.

**No component is currently trusted as a producer**, because none exists. The only trusted inputs today are the repository's own code and the user's own authorization file.

**Why `dispatch_authorized` stays false.** Three separate decisions exist and none implies another:

1. **Validation** (`evaluate_validation`): are these records internally sufficient for this executable and request? Informational and unauthenticated.
2. **Probe authorization** (`check_probe_authorization`): may one bounded, controlled probe run? Defaults to denied.
3. **Dispatch authorization**: may a task run through a provider? Not implemented. It would need authenticated evidence from a trusted runner, a fresh identity check, a separately authorized plan, and a later increment.

Real execution therefore remains unauthorized. A sufficient report does not unblock `context plan`; that command's `validation.status: "blocked"` stays in place until a later increment integrates verified evidence. Any future dispatch gate must not accept an `evidence_sufficient` result from storage or from a caller; it must obtain the evidence itself.

### Why external witnesses are required

A confined program cannot vouch for its own confinement. If the provider reports "I could not write outside the scratch directory," that statement is only as strong as the provider's honesty and its own sandbox. `external_check` evidence must come from Synapse observing effects from outside the provider process, for example by snapshotting the scratch directory and the protected roots before and after a probe, by applying an operating-system confinement itself and inspecting the applied profile, or by observing network attempts at a layer the provider cannot bypass. Absence of an observed effect from one run shows that run did not misbehave, not that enforcement exists; that is why such checks must actively try the forbidden action (for example, attempt a write to a canary path) rather than only watch.

### Limits of SHA-256 executable identity

The hash identifies the bytes of the resolved executable file at one moment. It does not identify a native binary behind a launcher script, shared libraries, configuration the binary reads, or the account it runs as. It says nothing about a file replaced after hashing: there is a time-of-check/time-of-use window between hashing and any later execution. A future runner must re-hash (and compare file identity) immediately before spawning and treat a change as a hard failure. Evidence is also not time-limited.

## Probe authorization

A future probe needs an explicit authorization written by the user. No authorization means denial. The library cannot tell who wrote the record: an authorization file that a model agent or a provider can write is not user authorization, so it must live outside any location they can modify. The record has exactly these fields, and any other shape is rejected:

```json
{
  "schema_version": 1,
  "probe_id": "run-1",
  "executable_sha256": "<64 hex characters of the inspected executable>",
  "capabilities": ["read_only_filesystem"],
  "scratch_root": "/external/empty/scratch/directory",
  "execution_mode": "read_only",
  "max_model_calls": 0,
  "network": "none",
  "hard_timeout_seconds": 30
}
```

- `probe_id` names the single run this authorization covers; the evaluator accepts only records carrying it.
- `executable_sha256` must equal the hash of the executable being probed; otherwise the authorization is stale and denied.
- `capabilities` lists the capabilities the probe may examine; duplicates, non-string items, and unknown names are rejected.
- `execution_mode` must be `read_only`. There is no fallback to a less restrictive mode.
- `max_model_calls` is 0 to 3. `network` must be `none` with zero model calls and `provider_endpoint_only` otherwise. Nothing else grants network access, and nothing installs dependencies.
- `scratch_root` must be an absolute, existing, **empty**, non-symlink directory that does not overlap the repository checkout or any caller-supplied protected root (the vault, the private state root, and real project directories). Overlap is decided by file identity (`samefile`), not by comparing path text, so a path spelled in a different case on a case-insensitive filesystem cannot hide an overlap. Probes never run in the Synapse checkout, in a vault, or in a real project.
- `hard_timeout_seconds` is 1 to 300.

On success the result carries `probe_id`, the canonical `scratch_root`, and `path_checks: "point_in_time"`. The checks hold only at the moment of the call. Directories, symlinks, and contents can change afterwards, so a runner must not treat the result as a lasting guarantee. Authorization has no expiry yet (see follow-ups).

Synapse never authenticates for the user, reads credential files, or logs prompts. A probe may only use the provider's own existing local login, and only when `max_model_calls` is above zero. Authorization permits a probe; it never permits dispatch.

## Future controlled-probe workflow

Not implemented here. Each step needs its own explicitly authorized increment:

1. Run `inspect-codex` and record the executable hash.
2. The user writes a probe authorization bound to that hash with an empty scratch directory outside the checkout.
3. `check_probe_authorization` must return `probe_authorized: true`.
4. A probe runner (new code) executes bounded probes in the scratch directory with an isolated home, no inherited API keys, hard deadlines, and process-group cleanup, as `inspect-codex` already does for help commands.
5. The runner builds `runtime_probe` and `external_check` records from its own observations, bound to the hash and `probe_id`, and passes them in memory to `evaluate_validation`.
6. Persistence of any evidence goes to the external private state root and is a separate decision.

## Inherited MCO integration risks (reviewed, not changed)

- `artifact_base` defaults to `reports/review` relative to the working directory ([config](../../runtime/config.py)), and session state lives under `<repo>/.mco/sessions` ([session state](../../runtime/session/state.py)). Both are inside a checkout and incompatible with Synapse privacy.
- MCO loads `<repo>/.mcorc.json`, `.mcorc.yaml`, `.mco/agents.yaml`, and global `~/.mco` settings. A provider run against an untrusted checkout can be influenced by that configuration.
- The Codex adapter passes `-C <repo_root>` and does not pass `--ephemeral`, so where a real Codex run stores its own session data is unverified and is exactly what `session_persistence` evidence must establish.
- Retries exist in the session path and the planning config accepts only zero retries; a probe must not reuse that retry path.
- `runtime/synapse_plan.py` `_external_path` decides repository overlap by comparing path text. On a case-insensitive filesystem (the macOS default), `context plan` configuration paths spelled in a different case can pass that check while resolving inside the checkout. This predates the validation foundations, is not changed here, and is recorded as a follow-up.

## Known limitations and follow-ups

- No real provider evidence exists and no probe runner exists. Every real-provider capability is `unknown` today.
- Evidence is unauthenticated (see the trust boundary). Producing it only from trusted code is a rule, not something the library can enforce.
- The two valued capabilities accept arbitrary short tokens in `requested` and `observed`.
- Authorization has no expiry or single-use marker. `probe_id` binding limits replay but a stale authorization file can be reused for the same run id. The runner increment must define expiry and consumption.
- The runner must create and own its scratch directory (private parent, restrictive mode, descriptor-based operations that refuse symlinks) instead of accepting a pre-existing one, to remove the check-then-use gap. The point-in-time scratch emptiness check is a pre-flight filter only.
- Staleness covers only the executable file. Account entitlement, configuration, and operating-system changes are not tracked.
- Environment exposure, cancellation, and descendant cleanup are properties of the future runner. The library neither spawns nor signals processes.
