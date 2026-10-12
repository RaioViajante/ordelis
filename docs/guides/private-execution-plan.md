# Private execution-plan preview

`mco context plan` combines an explicit user task and agent selection with indexed vault excerpts and a versioned private configuration. It prints deterministic JSON; it never launches an agent, probes authentication, writes to the vault, or persists the task or excerpts. The output is a planning record, **not** permission to dispatch.

The JSON's `workspace.repo_root` is the canonical resolved path of the required `--repo` target, so plans for different repositories identify different workspaces. CLI exit code `0` means the preview was produced successfully; it does **not** mean a plan with `validation.status: "blocked"` is executable. Invalid input returns exit code `2`.

The command requires `--config` and `--repo`. The configuration, vault, index, and state directory must already exist at absolute paths outside the repository. The state directory must also be disjoint from the read-only vault. The command does not create state directories or fall back to MCO's repo-local `reports/review` or `.mco/sessions` defaults. It rejects symlinked config/state targets and any configured path overlapping the repository. Keep real paths and saved output private.

### Path overlap checks

Overlap with the repository, and between the vault and the state directory, is decided by file identity (`st_dev` and `st_ino`) of each path and its ancestors, not by comparing path text. A path spelled with different letter case on a case-insensitive filesystem (the macOS and Windows default), or reached through a symlinked ancestor, is therefore rejected as overlapping. The same check guards the probe `scratch_root`. If a path cannot be inspected, the command fails with an input error instead of treating it as non-overlapping.

Limits: the checks are point-in-time and do not close the gap between validation and later use. A directory exposed under a different device number (for example the same directory mounted twice) is not detected as the same.

## Version 1 configuration

The JSON file must contain exactly these fields; unknown or missing fields, duplicate keys, invalid UTF-8, and files over 64 KiB are rejected:

```json
{
  "schema_version": 1,
  "vault": {"root": "/external/vault", "index": "/external/index.json", "read_only": true},
  "context": {"per_note_bytes": 2048, "total_excerpt_bytes": 8192},
  "execution": {"mode": "read_only", "hard_timeout_seconds": 180, "max_parallelism": 1, "max_retries": 0},
  "state": {"root": "/external/private-state"}
}
```

`per_note_bytes` is 1–2,048 and `total_excerpt_bytes` is 1–8,192; the existing vault reader also limits index size, selected notes, individual reads, UTF-8 excerpts, and serialized context. `hard_timeout_seconds` is 1–3,600; `max_parallelism` is 1–4. Only zero retries and read-only mode are accepted in this increment. `state.root` is a reserved external location, not a database or memory store. The preview includes proposed `artifact_root` and `session_root` beneath it, but creates neither directory and sets `persist: false`.

## Synthetic example

Run from the repository root. All synthetic files are created outside the checkout:

```bash
demo_dir="$(mktemp -d)"
mkdir -p "$demo_dir/repo" "$demo_dir/vault" "$demo_dir/state"
printf '# Work\nRelevant note.\n# Other\nExcluded.\n' > "$demo_dir/vault/selected.md"
printf 'Unrelated note.\n' > "$demo_dir/vault/unrelated.md"
printf '%s\n' '{"projects":{"demo":[{"path":"selected.md","heading":"Work"}]},"topics":{"other":[{"path":"unrelated.md"}]}}' > "$demo_dir/index.json"
printf '{"schema_version":1,"vault":{"root":"%s/vault","index":"%s/index.json","read_only":true},"context":{"per_note_bytes":64,"total_excerpt_bytes":128},"execution":{"mode":"read_only","hard_timeout_seconds":60,"max_parallelism":1,"max_retries":0},"state":{"root":"%s/state"}}\n' "$demo_dir" "$demo_dir" "$demo_dir" > "$demo_dir/synapse.json"
python3 -m runtime.cli context plan --config "$demo_dir/synapse.json" --repo "$demo_dir/repo" \
  --project demo --task 'Summarize work.' --agent pi --model synthetic-model --json
```

The plan contains only `selected.md` under heading `Work`; `unrelated.md` is excluded. The adapter declaration says Pi accepts a `model` key and `tool_profile` permission key, but the command does not verify that `synthetic-model` exists, that an account can use it, or that the installed Pi CLI enforces read-only filesystem access. Consequently the JSON has `validation.status: "blocked"`, explicit `blocking_reasons`, null `effective_settings`, and `execution.dispatch: false`. A required effort value is rejected unless the selected adapter declares that exact value; even then, local CLI verification remains unknown and the plan stays blocked. The built-in adapters currently expose no common effort control.

Required unsupported model-selection or read-only permission keys fail with an input error. Unknown model availability or read-only enforcement blocks the otherwise well-formed plan; it is never silently treated as supported. The command uses MCO's `execution_permissions` mapping and strict `provider_policy_preview` for adapter-key checks. Adapter declarations are not local CLI evidence, and a user-authored config cannot assert verified capabilities. There is no capability-probe flag in this command.

The explicit [Codex CLI capability inspection](codex-cli-capability-inspection.md) can now record version, the resolved executable path and that file's hash, and help-advertised controls without running a task. It is not called by `context plan` and does not unblock the plan. Future real-provider readiness still requires separate Mac-local checks of exact model and effort behavior, entitlement/authentication, applied read-only permission flags and filesystem isolation, cancellation, output/usage events, and allowed context. Authenticated dispatch, Kimi selection, continuity storage, and parallel writers remain outside this increment.
