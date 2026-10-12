# Explicit Codex CLI capability inspection

`mco context inspect-codex` is a non-authenticating, non-dispatching inspection for the Synapse execution planner. It locates `codex` on `PATH`, or accepts an explicit absolute `--binary` path, and runs only `--version`, `--help`, and `exec --help`. Ordinary `mco context preview` and `mco context plan` never run this inspection. No model prompt, login status, credential file, account probe, or vault read is involved.

```bash
python3 -m runtime.cli context inspect-codex --json
python3 -m runtime.cli context inspect-codex --binary /absolute/path/to/codex --json
```

The command returns deterministic JSON with the canonical resolved binary path, SHA-256 and size, parsed CLI version, MCO Codex adapter declarations and command flags, observed help controls and required values, and explicit unknown runtime properties. It produces no evidence file by default. Its exit code `0` means the inspection report was produced, **not** that dispatch is authorized. `inspection.status: "unsupported"` identifies an adapter-minimum version failure or a required adapter flag/value missing from the help surface where the adapter places it. Malformed output, non-UTF-8, missing binaries, oversize output, and timeouts return an input error. Version/help output is limited to 256/65,536 bytes per command, with a five-second read deadline and process-group cleanup; the executable is hashed in bounded chunks and limited to 512 MiB. Probes use an isolated temporary home outside the checkout and do not inherit API key variables. Keep captured JSON, especially machine-specific paths and hashes, outside the public repository.

This is parser-surface evidence only. The inspector reports authentication as `not_checked`, and model availability, effort behavior, filesystem enforcement, output events, and cancellation as `not_verified`. `--model` does not prove that a named model is available to an account. `--sandbox read-only` being advertised does not prove OS-level isolation on this machine. Generic `--config` being advertised does not prove that `model_reasoning_effort` is accepted by the installed binary or by any selected model. A valid inspection report must not unblock an execution plan by itself.

## Adapter comparison and unresolved discrepancies

The inherited [Codex adapter](../../runtime/adapters/codex.py) builds `codex [--ask-for-approval never] exec --skip-git-repo-check -C <repo> --sandbox read-only --json [--output-schema <file>] [--model <model>] <prompt>` for a read-only task. The [read-only policy](../../runtime/execution_modes.py) requests `sandbox=read-only` and `approval_policy=never`. Inspection checks pre-`exec` flags against top-level help and post-`exec` flags against `exec --help`; it does not execute this command. The adapter's declared minimum is `0.46.0`, not a guarantee that later versions retain behavior.

| Area | Observed help surface and adapter relationship | What remains unknown |
| --- | --- | --- |
| Model | `--model` is used by the adapter and advertised by `exec --help`. | Exact model IDs, account entitlement, and whether an invocation applies the model. |
| Reasoning effort | No dedicated effort flag is advertised. Generic `--config` is advertised, and [official OpenAI documentation](https://learn.chatgpt.com/docs/developer-settings) shows `model_reasoning_effort` as a one-run configuration override. The adapter exposes no effort values and does not pass this key. | Installed-version acceptance, model-specific values, and effective effort. No adapter flag is changed in this increment. |
| Sandbox and approval | The adapter's `--ask-for-approval` precedes `exec`; `--sandbox` follows it. Their corresponding help surfaces advertise both controls. `--add-dir` is advertised but grants writable access, so it is not a read-only vault solution. | Read-only filesystem enforcement, managed-policy overrides, and actual approval behavior. |
| Output | The adapter uses `--json` for JSONL and optionally `--output-schema`; help also advertises `--output-last-message`, which the adapter does not use. | Actual event schema, usage counters, schema enforcement, and output reliability. |
| Cancellation and persistence | MCO cancels the shim process externally; no cancellation flag is advertised. Help advertises `--ephemeral`, but the adapter does not use it for execution. | Descendant cleanup and whether a real invocation persists session data; dispatch remains disabled in Synapse planning. |

The [official OpenAI CLI command reference](https://learn.chatgpt.com/docs/developer-commands) describes the current command flags, including model, sandbox, approval, JSONL, and output controls. Documentation and help are not runtime verification. Before changing execution flags or allowing dispatch, a separate, explicitly authorized local validation must verify effective settings, auth and entitlement, read-only enforcement, cancellation, output and usage, and private-state placement. Do not run an authenticated model task as part of inspection.
