# Provider catalog and evidence vocabulary

**Status:** Foundation. Implemented for [D3.2a (#109)](https://github.com/RaioViajante/ordelis/issues/109), part of the [D3.2 design record (#108)](https://github.com/RaioViajante/ordelis/issues/108). Pure Rust in `desktop/src-tauri/src/agents/`, with no command, no caller and no interface.
**Scope:** which agent CLIs Ordelis targets, what their vendors' documentation claims, and the vocabulary that keeps claims apart from observations. Nothing here finds, runs or checks an agent.
**Related:** [Agents](../architecture/agents.md) · [Runtime-validation foundations](../guides/runtime-validation-foundations.md) · [Execution Safety Gate](execution-safety-gate.md) · [Desktop architecture](desktop-architecture.md)

## Where agent integration stands

| Part | State |
| --- | --- |
| Planned architecture (#108) | Passive discovery before any active probing, one audited native process boundary, pure execution-state contracts, an append-only journal, isolated native QA. A direction, not an implementation. |
| **Provider catalog and evidence vocabulary** | **Implemented here (D3.2a).** Compiled into every build, used by nothing outside its tests. |
| Passive executable discovery (finding a CLI on this Mac) | Not implemented. The search locations are subject to a later security review. |
| Active probes (`--version`, `--help` or anything else) | Not authorized. Running a CLI, even for its version, is external code execution. |
| Authentication checks | Out of scope for D3.2. Authentication stays unchecked. |
| Agent execution from the desktop | Not implemented. Release builds still contain no process-spawn code. |
| The Agents page | Still the static prototype. It does not read the catalog and does not show installed agents. |

Cataloguing a provider authorizes nothing: not locating it, not running it, not checking its account.

## Provider catalog (`agents/catalog.rs`)

| `ProviderId` | Display name | Documented executable name |
| --- | --- | --- |
| `codex` | Codex | `codex` |
| `claude` | Claude Code | `claude` |
| `opencode` | OpenCode | `opencode` |
| `kimi` | Kimi Code CLI | `kimi` |

- **Identifiers are stable.** `codex`, `claude` and `opencode` match the inherited runtime's adapter ids (a test reads `runtime/contracts.py`). `kimi` has no inherited adapter, and the entry describes the dedicated Kimi Code CLI only; Kimi through OpenCode is a separate, undecided route.
- **The executable name is metadata.** The catalog holds no path, search location, argument vector or authentication check, and a test rejects path-like text in it.
- **Twelve documented features per provider**, each with exactly one claim: non-interactive mode, structured output, model selection, reasoning effort, permission modes, approval routing, host-assigned session id, session resume, running without session persistence, provider sandbox, ACP mode, local server mode.
- **Each claim** has a status (`Documented`, `NotFound`, `Contradictory`, `NotReviewed`), how it was read (`Verbatim`, `Summarized` from a summary or search excerpt, or `NotRead`), a reference (title, URL, retrieval date, the page's own update date when shown) and a short note. `NotReviewed` is exactly the claims with no reference and no reading.
- **Documented claims are claims.** They say nothing about an installed version, an account or actual behavior. A provider's own sandbox is described as the provider's control, never as confinement.

All references were retrieved on 2026-11-05. The statements the D3.2 design relies on are pinned by tests:

- Kimi's documentation contradicts itself on non-interactive permissions.
- The reviewed pages describe neither of the ACP launch commands the inherited runtime uses for Codex and Claude Code.
- Of the reviewed pages, only Claude Code's describe a caller-chosen session id; Kimi's were not reviewed for it.
- The reviewed Codex pages describe no approval routing; its `app-server` is marked Experimental.
- Kimi's model option appears only in an example, so model selection is recorded as not found.
- The Kimi entry rests on search excerpts only.

## Evidence vocabulary (`agents/evidence.rs`)

Mirrored from `runtime/runtime_validation.py` ([guide](../guides/runtime-validation-foundations.md)). Its eight capabilities, three outcomes, closed observation list, token and SHA-256 rules, 64-record limit, valued-capability rules, minimum levels and status rules are reproduced with the same names and meaning, and tests compare them with the Python source text.

| Source | Level | Class | Can satisfy |
| --- | --- | --- | --- |
| `vendor_documentation` (added) | `vendor_documented` | Advertised | Nothing |
| `adapter_declaration` | `adapter_declared` | Advertised | Nothing |
| `cli_help` | `cli_advertised` | Advertised | Nothing |
| `runtime_probe` | `runtime_observed` | Observed | Non-isolation capabilities |
| `external_check` | `independently_verified` | Verified | Every capability |

- **Ceilings.** Each source fixes one level. Isolation capabilities (`read_only_filesystem`, `network_restriction`, `session_persistence`) need `independently_verified`; the others need `runtime_observed`. Documentation, adapter declarations and help text can never satisfy anything, however many records there are.
- **Status, most severe first:** `failed`, `mismatch`, `unsupported`, `satisfied`, `insufficient_evidence`, `unknown`. The first that applies wins, so a conflict always resolves to the more severe status.
- **Failures persist.** No rule reads a record's timestamp, so a later pass never clears an earlier failure, and the weakest source's failure blocks the strongest source's pass.
- **Identity binding.** A probe record is bound to one executable SHA-256 and one probe id. Records for another executable are stale; records for another probe run are unbound. Both are counted and ignored, in both directions: a stale pass does not carry over, and a stale failure does not poison the new file.
- **Unknown** means no applicable record.
- **No authorization.** An evaluation has no field that could mean authorized, dispatched or enforced, and a static test pins that. Records are unauthenticated caller data.
- **No "enforced" class.** `independently_verified` is an observation made from outside the provider. It is not the gate's *independently enforced* control, which needs a mechanism Ordelis applied; nothing here can express that.

`documentation_records(provider, recorded_at_ms)` turns a provider's `Documented` claims about structured output, model selection, reasoning effort and running without persistence into `vendor_documentation` passes for `output_format`, `model_acceptance`, `reasoning_effort` and `session_persistence`.

- **Meaning:** as with a `cli_help` pass, the pass means a control for the capability is described. A model option is not a model being accepted, and an option to run without persistence is not proof that nothing is written.
- **Everything else produces nothing:** every other claim leaves the capability unknown.
- **Tested:** every provider's documentation, evaluated with every capability required, satisfies nothing.

### Differences from `runtime_validation.py`

| # | Rust | Python | Why |
| --- | --- | --- | --- |
| 1 | Adds the source `vendor_documentation` with level `vendor_documented`, weaker than every existing level | Four sources and levels | Documentation is about a product, not an installed file. The four existing names and their order are unchanged. |
| 2 | A documentation record is bound to a provider, not to an executable and probe run. It applies to that provider whatever the executable, never goes stale, is always a pass and carries no values. Records about another provider are counted (`other_provider_evidence`) | Every record has an executable hash and probe id | Documentation cannot witness a failure or observe a value, and is not about one file. It cannot block a capability either: absence from a page is not evidence of absence, and a block that never goes stale would outlive every version. It can only move a capability from unknown to insufficient evidence. |
| 3 | Every record carries `recorded_at_ms`, a caller-stated timestamp up to 2^53−1 | No timestamp | Provenance. No rule reads it, so it cannot be used to let a later pass override a failure. |
| 4 | The caller states the requirements. A valued capability (`model_acceptance`, `reasoning_effort`) must be required with its value and no other capability may have one; a duplicate requirement is an error | `evaluate_validation` fixes its own requirement set: `model_acceptance` always for the named model, effort when given, network optionally | The desktop has no planner yet. Requiring the value keeps Python's meaning: a model is never "accepted" without saying which. |
| 5 | No `validation.status`, `evidence_authenticated` or `dispatch_authorized` fields | Fields that are always false | A type that cannot say "authorized" is stricter than a field that must stay false. |
| 6 | A coarse class (Advertised, Observed, Verified) for display | None | For a later interface. No class means enforced. |
| 7 | Typed constructors: an invalid record or requirement cannot be built | Validates dictionaries against closed keys and sorts them | Same rules; the result does not depend on record order (tested). |
| 8 | Not ported: `check_probe_authorization` and the gate's control attestations, validity windows and platform table | Present | Probe authorization reads the filesystem and belongs with a future, separately approved process boundary. The gate is not changed, copied or bypassed. |
| 9 | Documentation records count toward the 64-record limit | No documentation records | A caller combining many probe records with documentation can hit the limit sooner. Fail closed. |

## Static boundary

`config.test.ts` checks the compiled `src/agents*` files, listed in `scripts/native-sources.json` and verified against the compiler's dep-info by `verify-native-boundary.mjs`. Before the rules apply:

- comments are dropped, nested block comments included;
- string, character and byte literals are emptied, so documentation text is never read as code and a quote character cannot hide code;
- spaces around `::` are removed.

The rules:

- **No operating system access:** no `std`, `core` or `alloc` path, and therefore no process, file, network, environment, clock or thread.
- **No hidden state or foreign code:** no global or lazily initialized state, no interior mutability, no `unsafe`, `extern` or linkage attribute, no `#[path]`.
- **Nothing reachable from the webview:** no Tauri, serde, serde_json, libc or rfd.
- **No escape from the module:** no `crate` path, no sibling module name, no `super` in `agents.rs` (where it is the crate root), and only `super::catalog` or `super::evidence` in the two submodules. No item is visible outside the crate.
- **No way to name something to run:** no identifier containing path, argv, args, dir, directory, search, location, candidate, command, cmd, launch, invoke, program, exe (other than executable), binary, template or flag (compound names such as `executable_path` included), and no raw string of any kind (`r`, `br`, `cr`).
- **Only one macro:** `matches!`.

Fixtures show each rule catches its bypass, including those found in independent review: the crate root through `super`, a renamed `crate`, spaced paths, code between quote characters or after a nested block comment, byte and C raw strings, and compound or alternative names for something to run. Another fixture shows that text in strings and comments does not trip any rule. Further pins:

- `lib.rs` declares `mod agents;` and names it nowhere else.
- No other compiled production file mentions it.
- The ten commands, their grants, the generated permissions and the direct dependency set are unchanged; their existing tests pass without modification.

The module is compiled into release builds as inert data and functions; `#[cfg_attr(not(test), allow(dead_code))]` silences the unused-code warning outside tests only. The documentation URLs may therefore be present in the binary as text, unless the linker removes the unused data; nothing reads them at run time.

## Tests

- `src/agents/catalog/tests.rs`: stable identifiers and near misses, inherited adapter ids, one claim per feature, references and dates, the pinned uncertainties, provider-sandbox wording, and no local path in the catalog.
- `src/agents/evidence/tests.rs`:
  - **Validation:** tokens and hashes, the closed record schema (documentation is a pass only), requirement and record limits, valued requirements needing a value.
  - **Ceilings:** ceilings for every capability and source, no promotion of documentation, and the real catalog satisfying nothing.
  - **Statuses:** unknown and unsupported, and conflict order, including a mismatch with no requested value.
  - **Persistence and order:** failure persistence in any order and at any time, and identical results for reordered records.
  - **Identity:** stale and unbound evidence (a record differing in both counts as stale), and documentation bound to its provider.
  - **Parity:** parity with `runtime_validation.py` (vocabulary, minimum levels, token and hash patterns, status order and rules), read as text.

No test starts a process or reads anything but the two inherited Python files named above.

## Limits

- The catalog is what documentation said on 2026-11-05, partly from summaries and search excerpts, as each claim records. Vendors change their documentation and their CLIs; a stale catalog is still only advertised evidence.
- Parity with Python is checked textually. A semantic change written differently in Python could pass the textual check; the Python suite remains the authority for the Python module.
- Not reaching the webview is enforced by the absence of a command and by static rules, the same tripwires used for the stores. A compromised Rust process is out of scope, as elsewhere.

## Next steps (not authorized)

Each needs its own approval under #108:

- pure execution-state contracts;
- the append-only journal protocol;
- passive discovery, after the review of search locations;
- the native process boundary, after a separately approved release safety policy;
- the Agents page on real data;
- macOS containment research.
