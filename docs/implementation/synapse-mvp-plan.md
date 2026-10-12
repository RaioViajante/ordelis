# Synapse MVP plan and MCO capability audit

This records the conclusions of [Synapse issue #1](https://github.com/RaioViajante/synapse/issues/1) in the repository so they remain available if the fork is detached. The inherited MCO baseline was `3cfce13` (`v0.12.1`); this audit reads the fork source on `feat/vault-context-preview`. Synapse will remain CLI-first, preserve the existing MCO commands and [MIT attribution](../../LICENSE), and treat an explicitly configured Obsidian vault as read-only reference data. No real vault, credential, subscription, or provider installation was inspected for this plan.

## What exists today

The implemented increment is `mco context preview`: an explicit external vault and JSON index select project/topic notes, optional ATX headings, and bounded UTF-8 excerpts. Output includes source paths, missing-heading and truncation flags, proposed agent/model/effort, and a no-dispatch policy. It neither modifies the vault nor starts a provider. See [the command and synthetic example](../guides/vault-context-preview.md), [reader](../../runtime/vault_context.py), [CLI branch](../../runtime/cli.py), and [synthetic tests](../../tests/test_vault_context.py). The preview does **not** verify that the named agent exists, that its model is available, or that its effort setting can be applied. `model_validated` and `effort_validated` are false by design.

Future work includes connecting approved context to dispatch, central-agent coordination, durable continuity, and isolated code-writing workspaces. None of those is provided by the vault preview. Existing MCO execution and session features should not be mistaken for an integrated Synapse coordinator.

## Inherited capability audit

| Area | Evidence in this fork | Capability and gap for Synapse |
| --- | --- | --- |
| Provider contract and registry | [`ProviderAdapter`, `TaskInput`, capability declarations](../../runtime/contracts.py); [adapter registry](../../runtime/adapters/__init__.py); [CLI provider list](../../runtime/cli.py) | MCO has 11 built-in provider IDs, including Claude, Codex, and OpenCode, plus configurable shim/ACP agents. It has no built-in Kimi adapter or Synapse routing policy. Adapter capability declarations describe MCO support; they do not establish local authentication, subscription entitlement, or model availability. |
| Model and effort | [invocation parsing and model metadata](../../runtime/invocation_runtime.py); [policy support filtering](../../runtime/policy.py); [model probes](../../runtime/models.py); [Claude](../../runtime/adapters/claude.py), [Codex](../../runtime/adapters/codex.py), [OpenCode](../../runtime/adapters/opencode.py) | `--agent provider:model` records a requested model. Codex's shim applies `--model`; the current Claude and OpenCode shims do not apply an invocation's model, and the latter two advertise no model key for strict provider policy. The model catalog probes Codex, Pi, and Hermes only. It parses some Codex reasoning-level metadata, but there is no common effort control wired into invocation commands. A requested model in a preview or invocation record must not be reported as applied merely because it was recorded. |
| Execution and permissions | [execution profiles](../../runtime/execution_modes.py); [supported-key checks](../../runtime/policy.py); [scope validation](../../runtime/invocation_runtime.py); [provider risk](../../runtime/provider_risk.py) | MCO translates `read_only`, `write`, and `yolo` into provider-specific controls and strict mode rejects unsupported policy keys. `--allow-paths` checks MCO's requested repo scope; it is not an OS sandbox. Permission strength and context-file support vary by adapter. Synapse must fail closed if a selected provider cannot honor the requested boundary. |
| Preview and dispatch | [MCO dry-run payload](../../runtime/cli.py); [invocation workflow](../../runtime/invocation_runtime.py); [vault preview](../../runtime/vault_context.py) | MCO `run`/`review --dry-run --json` previews its own provider commands and policy. The separate vault preview selects context and proposed settings but does not feed MCO's workflow. Their outputs are not yet one validated dispatch plan. |
| Concurrency, deadlines, cancellation, retries | [worker pool and cancel event](../../runtime/invocation_runtime.py); [shim cancellation](../../runtime/adapters/shim.py); [ACP cancellation](../../runtime/acp/adapter.py); [session retry policy](../../runtime/retry.py) and [daemon use](../../runtime/session/daemon.py) | The invocation runtime has parallelism and wall/stall/global deadlines plus adapter cancellation. `max_provider_parallelism=0` permits all selected invocations concurrently, so Synapse must set an explicit small limit. Exponential retries exist in the session path, not as a uniform invocation policy; retrying writes requires separate idempotency rules. |
| Output, usage, and continuity | [transport decoders](../../runtime/answer_transport.py); [usage opt-in and result modes](../../runtime/cli.py); [artifact writing](../../runtime/invocation_runtime.py); [session state](../../runtime/session/state.py) and [manager](../../runtime/session/manager.py) | Decoders extract usage only when provider events supply recognized counters; absent counters are unknown, not zero. Token usage is opt-in. MCO can persist raw answers and sessions, but defaults include `reports/review` and repo-local `.mco/sessions` with history and logs. Synapse must place private context and continuity outside the public checkout before using persistence. Existing sessions are provider sessions, not a cross-provider central-agent memory. |
| Test surface and attribution | [mock invocation tests](../../tests/test_invocation_runtime.py); [vault tests](../../tests/test_vault_context.py); [MIT license](../../LICENSE); [Python package metadata](../../pyproject.toml) | MCO orchestration has deterministic fake-adapter tests. Synthetic vault retrieval is tested independently. There is no test yet for selected vault context flowing through a Synapse coordinator into mock providers; that integration must precede any authenticated execution. Preserve the license and upstream notices through future branding or fork detachment. |

The inherited [provider reference](../reference/providers.md) says ten built-in adapters, while the current registry and CLI list eleven (including `agy`). Treat the source registry as authoritative until that inherited reference is updated.

## Proposed MVP architecture (not implemented)

1. **Select context.** An explicit project/topic ID resolves only index entries beneath an external vault root. The existing preview supplies bounded excerpts and source provenance; note text remains untrusted reference material, never configuration or tool instructions.
2. **Build a plan.** A coordinator accepts the user's task and explicit provider preferences, combines the selected context with a capability snapshot, and returns one reviewable plan: agent, requested model and effort, exact permission mode, context budget, workspace, deadline, retry rule, concurrency limit, and output location. Unsupported or unknown settings remain marked unknown and block dispatch when required. Do not infer a paid provider from installed binaries or route by a model name alone.
3. **Validate before execution.** Resolve provider-specific model and effort controls, local entitlement and auth readiness, risk/permission support, context handling, and paths. Preview the effective command and any setting that will be dropped. Dispatch only after those checks and the user's authorized selection; the current increment stops before this step.
4. **Execute and record.** Reuse MCO's adapter/invocation contracts with mock providers first. Later, use one bounded provider invocation at a time until provider behavior is verified. Record raw answers, statuses, source manifest, applied settings, and usage with `measured`, `estimated`, or `unknown` provenance in an external private state root. Keep provider output and vault excerpts untrusted for subsequent coordination.
5. **Continue or write.** Future central-agent continuity stores task decisions and compact source references outside the checkout; it does not copy the whole vault into a public artifact. Future parallel code writers each get an isolated worktree and explicit file ownership, then review and integrate changes serially. MCO currently does not create worktrees.

## Configuration and privacy boundary

The only active vault configuration today is `mco context preview --vault <absolute external directory> --index <absolute external JSON file>`. It prints context to stdout and saves nothing; callers must not redirect that output into this public repository. The synthetic index in the guide contains no private data.

For the future coordinator, define an external private configuration file (illustrative schema, **not** accepted by the current CLI) with these boundaries:

```json
{
  "schema_version": 1,
  "vault": {"root": "<external absolute path>", "index": "<external absolute path>", "read_only": true},
  "context": {"per_note_bytes": 2048, "total_excerpt_bytes": 8192},
  "routing": {"allowed_agents": ["claude", "codex", "opencode"], "require_explicit_selection": true},
  "execution": {"default_mode": "read_only", "max_parallelism": 1, "max_retries": 0, "hard_timeout_seconds": 180},
  "state": {"root": "<external absolute private directory>"}
}
```

Treat these as proposed settings, not uniform CLI flags. Do not store credentials in the config or commit real paths, indexes, prompts, retrieved context, results, session history, or execution logs. Provider credentials stay with the provider's own local authentication system. MCO's current config loader can read project `.mcorc.*` and global `~/.mco` files ([source](../../runtime/config.py)); its default artifact and session locations are unsuitable for private Synapse state ([config](../../runtime/config.py), [session state](../../runtime/session/state.py)). Until the external state path is implemented and tested, keep Synapse dispatch and session persistence disabled. Ignoring a path in Git is not a privacy boundary.

## Kimi integration decision gate

Two options remain open; neither is selected or validated on this Mac:

| Option | Evidence and required local validation |
| --- | --- |
| Kimi through OpenCode | [Kimi's OpenCode guide](https://www.kimi.com/code/docs/en/third-party-tools/opencode.html) documents Kimi For Coding in OpenCode; [OpenCode's provider guide](https://opencode.ai/docs/providers/) separately documents Moonshot AI API access. These are different credential and entitlement paths. Verify which one the user has, the installed OpenCode version, exact model ID and variant/effort behavior, permissions, output/usage events, cancellation, and allowed context. The fork's [OpenCode shim](../../runtime/adapters/opencode.py) currently ignores per-invocation model metadata, so Kimi selection through that shim is **not** established. |
| Dedicated Kimi Code CLI | [Official CLI reference](https://www.kimi.com/code/docs/en/kimi-code-cli/reference/kimi-command.html) documents non-interactive `--prompt`, `--model`, output format, and plan mode; [official ACP reference](https://www.kimi.com/code/docs/en/kimi-code-cli/reference/kimi-acp.html) documents `kimi acp`. Verify the installed command and version, auth/entitlement path, model and effort controls, permission enforcement, structured output/usage, cancellation, and ACP handshake on the user's Mac. MCO's [custom-agent registry](../../runtime/adapters/__init__.py) is a possible test harness, not proof that a safe Kimi adapter exists. |

Choose only after a side-by-side local matrix proves the required read-only boundary, exact model selection, effort semantics, observable status/cancellation, and usage provenance. First run command previews and fake-provider tests. Any authenticated Kimi smoke call requires a separate user-approved, bounded task; no account setup or paid call is part of this plan.

## Implementation phases and exit gates

| Phase | Deliverable | Gate |
| --- | --- | --- |
| 0 — complete | Read-only indexed vault context preview and synthetic tests. | Bounds, traversal, symlink, UTF-8, exclusion, deterministic output, and no provider/vault mutation tests pass. |
| 1 — this document | Source-based inherited capability audit, privacy boundaries, architecture, Kimi options, and local-check list. | Source links and issue #1 acceptance status reviewed; no private material included. |
| 2 — future | External private config/state root and one combined context-plus-execution-plan schema; explicit capability matrix and fail-closed setting resolution. | Mock tests verify unsupported model/effort/permission rejection, context budget, no dispatch on invalid plan, and no writes inside the public checkout. |
| 3 — future | Central-agent coordination over MCO adapters with mock providers and bounded cancellation/deadlines/usage. | End-to-end mock runs cover success, partial failure, retry boundaries, cancellation, measured versus unknown usage, and continuity reload without paid calls. |
| 4 — future | One locally validated provider at a time, including the Kimi option decision. | User-Mac probes and a separately authorized minimal authenticated smoke test prove effective settings and read-only behavior. |
| 5 — future | Isolated writers and reviewed integration. | Separate worktrees, ownership boundaries, conflict handling, and cleanup are tested before parallel code-writing tasks. |

## Issue #1 acceptance status and remaining Mac checks

| Acceptance criterion | Status |
| --- | --- |
| Upstream capabilities and gaps with source-file references | Documented above from this fork; local CLI behavior is still unverified. |
| MVP architecture, configuration, and phases | Proposed above; coordinator/config loader are not implemented. |
| Contextual retrieval on a synthetic vault excluding unrelated notes | Implemented and tested in [`test_vault_context.py`](../../tests/test_vault_context.py). |
| Preview agent, model, effort, context, and execution policy before dispatch | Implemented as **proposed, unvalidated** settings in vault preview; no unified, effective provider dispatch plan yet. |
| Mock-provider orchestration before authenticated model calls | Inherited MCO workflow has [fake-adapter tests](../../tests/test_invocation_runtime.py); vault-to-coordinator dispatch has not been implemented or mock-tested. |
| Checks requiring user's Mac and local vault | Listed below; none ran against the user's personal notes. |

When the user is ready, obtain the vault path and confirm read-only filesystem access, index ownership/location, representative heading conventions, and whether selected excerpts contain sensitive data that should never be sent to a provider. Inspect only a user-approved sample. Check locally installed CLI versions and auth/entitlement status for Claude, Codex, OpenCode, and the chosen Kimi path without copying credentials into logs. Verify actual model IDs, model-selection flags, effort controls, permission modes, context-file behavior, token usage events, cancellation, and rate/timeout behavior with each installed version. Confirm the preferred central agent, allowed spending/subscription route, external state location, and whether any later write task needs isolated worktrees. No paid or authenticated model call is needed to finish the current planning scope.
