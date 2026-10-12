# Vault context preview

`mco context preview` is the first read-only Synapse increment. It selects only notes listed for an explicit project or topic ID, extracts an optional Markdown heading section, and prints deterministic JSON. Note text is reference data; it never controls tool execution. The command neither invokes providers nor writes to the vault.

Keep real vaults, private indexes, configuration, saved previews, and logs outside this public checkout. The paths below create a synthetic vault in a temporary directory. Run from the repository root:

```bash
demo_dir="$(mktemp -d)"
mkdir -p "$demo_dir/vault"
printf '# Project\n## Plan\nShip a small preview.\n## Archive\nUnrelated.\n' > "$demo_dir/vault/project.md"
printf 'Other topic.\n' > "$demo_dir/vault/other.md"
printf '%s\n' '{"projects":{"demo":[{"path":"project.md","heading":"Plan"}]},"topics":{"other":[{"path":"other.md"}]}}' > "$demo_dir/index.json"
python3 -m runtime.cli context preview \
  --vault "$demo_dir/vault" --index "$demo_dir/index.json" \
  --project demo --agent codex --model example-model --effort medium --json
```

The selected excerpt is `Ship a small preview.` with `path: project.md` and `heading: Plan`; `other.md` is excluded. Agent, model, and effort are proposed values only. `validated_provider_settings: false` makes clear that the preview does not check installed CLI capabilities or authenticate a model. `policy.dispatch: false` and `policy.vault_access: read_only` describe this command's behavior.

The index has exactly two top-level maps, `projects` and `topics`. Each ID maps to a nonempty array of objects with a vault-relative Markdown `path` and optional `heading`. Path and heading pairs selected by both IDs are deduplicated and sorted. A missing heading yields an empty excerpt with `missing_heading: true`. Excerpts cut by a budget have `truncated: true`.

The index is limited to 64 KiB and 512 entries; at most 32 distinct path and heading pairs can be selected. Each selected note is read up to 64 KiB; a larger note is rejected. Defaults are 2,048 bytes per excerpt and 8,192 bytes across excerpts, adjustable downward with `--per-note-bytes` and `--total-bytes`. Serialized JSON, including metadata, is capped at 32 KiB; oversized previews are rejected. Invalid UTF-8, malformed indexes, unsafe paths, and symlinked note components fail before a preview is returned. No index is generated or saved by the command.

The current fork retains MCO's existing `run`, `review`, `doctor`, and agent commands. Its `run`/`review --dry-run --json` path already builds provider policy previews in `runtime/cli.py` and `runtime/policy.py`; that path can probe adapters and is separate from this vault-only preview. `runtime/config.py` defines provider policy, and `runtime/invocation_runtime.py` handles dispatch. This increment does not connect vault excerpts to dispatch, validate provider model or effort support, route automatically, write notes, or provide an interface beyond the CLI. Those steps require a separate design and mock-provider validation before authenticated calls.
