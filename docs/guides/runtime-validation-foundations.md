# Provider runtime-validation foundations

`runtime/runtime_validation.py` defines how Synapse will judge evidence about a real provider CLI **before** any dispatch is allowed. It is a library module only: it has no CLI command, runs no provider, starts no process, reads no credentials, and writes no files. `mco context preview`, `mco context plan`, and `mco context inspect-codex` are unchanged and remain non-dispatching, and the MCO invocation workflow is not wired into Synapse planning.

Evidence for a real provider does not exist yet. The tests use only synthetic records and a fake executable.

## Evidence model

Each record states one fact about one capability of one executable:

| Field | Meaning |
| --- | --- |
| `capability` | One of `executable_identity`, `model_acceptance`, `reasoning_effort`, `read_only_filesystem`, `network_restriction`, `output_format`, `process_termination`, `session_persistence`. |
| `source` | Where the fact came from; it fixes the evidence level (below). |
| `outcome` | `pass`, `fail`, or `unsupported`. |
| `executable_sha256` | Hash of the resolved executable file the fact was gathered against (the `binary.sha256` of `inspect-codex`). |
| `requested` / `observed` | Optional short values, such as the requested and the reported model. `null` means the record is not tied to a value. |
| `probe_id` | Short label identifying the fixture or future probe that produced the record. |

All string values must match `[A-Za-z0-9._:/@+=-]{1,128}`. The schema is closed: unknown fields are rejected, and there is no free-form text field, so evidence cannot carry a prompt, note excerpt, or credential. At most 64 records are accepted.

### Four evidence levels

| Source | Level | What it can establish |
| --- | --- | --- |
| `adapter_declaration` | `adapter_declared` | MCO's adapter says it supports a key or value. Says nothing about the installed CLI or the account. |
| `cli_help` | `cli_advertised` | The installed CLI's help lists a control. A parser option, not behavior. |
| `runtime_probe` | `runtime_observed` | Behavior seen in a controlled run of the provider, or a direct local observation such as hashing the executable file. The provider's own output is the witness. |
| `external_check` | `independently_verified` | A check made outside the provider process, for example comparing the scratch directory and an OS-level sandbox profile before and after a probe. |

A fifth state, **unknown**, means no evidence exists for the current executable. `unsupported` is an explicit outcome: the provider or adapter says the capability is unavailable.

### What counts as verified

Every capability needs `runtime_observed` or better. The isolation properties `read_only_filesystem`, `network_restriction`, and `session_persistence` need `independently_verified`. A model that answers successfully, or reports that it did not write a file, is not evidence that the filesystem or network was restricted. A provider can claim anything about its own sandbox, and a run that happened not to write proves nothing about enforcement.

Always required: `executable_identity`, `model_acceptance` (for the requested model), `read_only_filesystem`, `output_format`, `process_termination`, `session_persistence`. Required when requested: `reasoning_effort` (an effort was asked for) and `network_restriction` (the caller requires it).

### Evaluation

`evaluate_validation(executable_sha256, evidence, model=..., effort=..., require_network_restriction=...)` returns one status per capability, most severe first:

1. `failed`: any relevant record has outcome `fail`.
2. `mismatch`: a passing record for the requested value observed a different value (a different model or effort than requested).
3. `unsupported`: a relevant record says the capability is unavailable.
4. `satisfied`: a passing record reaches the minimum level and, where a value was requested, requested and observed both equal it.
5. `insufficient_evidence`: only passing evidence below the minimum level, or a probe that did not confirm the value.
6. `unknown`: no relevant evidence.

Conflicting evidence resolves to the more severe status. Records for a different `executable_sha256` are ignored and counted in `stale_evidence`; a rebuilt or replaced executable therefore starts again from `unknown`. Records tied to another requested value are not relevant to the current request. The result is deterministic and independent of record order.

Any required capability that is not `satisfied` appears in `validation.blocking_reasons` as `<capability>:<status>`, and `validation.status` is `blocked`. Even `validated` always carries `dispatch_authorized: false`.

### Synthetic example

Evidence against a fake executable whose hash is 64 `a` characters: model support only advertised in help, a filesystem pass that only the provider observed, a failed cancellation check, and nothing about session persistence.

```json
{"capability": "read_only_filesystem", "source": "runtime_probe", "outcome": "pass",
 "executable_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
 "requested": null, "observed": null, "probe_id": "fixture-1"}
```

Result for `model="model-1"`:

```json
{"status": "blocked",
 "blocking_reasons": ["model_acceptance:insufficient_evidence", "read_only_filesystem:insufficient_evidence",
                      "process_termination:failed", "session_persistence:unknown"],
 "dispatch_authorized": false}
```

## Validation is not dispatch authorization

Three separate decisions exist and none implies another:

1. **Validation** (`evaluate_validation`): is the evidence sufficient for this executable and request? Informational; always `dispatch_authorized: false`.
2. **Probe authorization** (`check_probe_authorization`): may one bounded, controlled probe run? Defaults to denied.
3. **Dispatch authorization**: may a task run through a provider? Not implemented. It needs a validated report, a separately authorized plan, and a later increment.

A validated report does not unblock `context plan`; that command's `validation.status: "blocked"` stays in place until a later increment integrates verified evidence.

## Probe authorization

A future probe needs an explicit, user-supplied authorization. No authorization means denial. The record has exactly these fields, and any other shape is rejected:

```json
{
  "schema_version": 1,
  "executable_sha256": "<64 hex characters of the inspected executable>",
  "capabilities": ["read_only_filesystem"],
  "scratch_root": "/external/empty/scratch/directory",
  "execution_mode": "read_only",
  "max_model_calls": 0,
  "network": "none",
  "hard_timeout_seconds": 30
}
```

- `executable_sha256` must equal the hash of the executable being probed; otherwise the authorization is stale and denied.
- `capabilities` lists the capabilities the probe may examine; duplicates and unknown names are rejected.
- `execution_mode` must be `read_only`. There is no fallback to a less restrictive mode.
- `max_model_calls` is 0 to 3. `network` must be `none` with zero model calls and `provider_endpoint_only` otherwise. Nothing else grants network access, and nothing installs dependencies.
- `scratch_root` must be an absolute, existing, **empty**, non-symlink directory that does not overlap the repository checkout or any caller-supplied protected root (the vault, the private state root, and real project directories). Probes never run in the Synapse checkout, in a vault, or in a real project.
- `hard_timeout_seconds` is 1 to 300.

Synapse never authenticates for the user, reads credential files, or logs prompts. A probe may only use the provider's own existing local login, and only when `max_model_calls` is above zero. Authorization permits a probe; it never permits dispatch.

## Future controlled-probe workflow

Not implemented here. Each step needs its own explicitly authorized increment:

1. Run `inspect-codex` and record the executable hash.
2. The user writes a probe authorization bound to that hash with an empty scratch directory outside the checkout.
3. `check_probe_authorization` must return `probe_authorized: true`.
4. A probe runner (new code) executes bounded probes in the scratch directory with an isolated home, no inherited API keys, hard deadlines, and process-group cleanup, as `inspect-codex` already does for help commands.
5. Probes emit `runtime_probe` records. Independent checks outside the provider emit `external_check` records. Both are bound to the executable hash and use only the bounded token vocabulary.
6. `evaluate_validation` judges the records in memory. Persistence of any evidence goes to the external private state root and is a separate decision.

## Inherited MCO integration risks (reviewed, not changed)

- `artifact_base` defaults to `reports/review` relative to the working directory ([config](../../runtime/config.py)), and session state lives under `<repo>/.mco/sessions` ([session state](../../runtime/session/state.py)). Both are inside a checkout and incompatible with Synapse privacy.
- MCO loads `<repo>/.mcorc.json`, `.mcorc.yaml`, `.mco/agents.yaml`, and global `~/.mco` settings. A provider run against an untrusted checkout can be influenced by that configuration.
- The Codex adapter passes `-C <repo_root>` and does not pass `--ephemeral`, so where a real Codex run stores its own session data is unverified and is exactly what `session_persistence` evidence must establish.
- Retries exist in the session path and the planning config accepts only zero retries; a probe must not reuse that retry path.

## Known limitations

- No real provider evidence exists and no probe runner exists. Every real-provider capability is `unknown` today.
- Evidence is trusted as supplied. The module checks shape, hash binding, and level; it cannot confirm that a record was truthfully produced, and a caller who fabricates an `external_check` record defeats the model. Record production must therefore come from reviewed Synapse code, not from provider output.
- Hash binding identifies the resolved executable file only; a launcher script can hide a different native binary (see [the inspection guide](codex-cli-capability-inspection.md)). It does not prove that an unchanged file behaves the same across environments or account changes.
- Staleness covers only the executable. Evidence is not time-limited, and account entitlement, configuration, or OS changes are not tracked.
- Authorization checks paths at the moment of the call; it does not hold them against later changes. It also does not cover credentials that a provider reads for itself.
