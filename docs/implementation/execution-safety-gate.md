# Execution Safety Gate

Status: accepted for issue [#19](https://github.com/RaioViajante/synapse/issues/19).

## Decision

`runtime/execution_safety_gate.py` is a pure, deterministic library function that answers one question: *would the
supplied evidence be sufficient, under a stated policy, to let a future trusted coordinator consider executing this
exact command against this exact executable?* In this increment the answer to "may it execute" is **always no**.

**Evaluating a policy creates no sandbox, no confinement and no authorization.** The gate reads dictionaries and
returns a dictionary. It cannot start a process, apply a profile, set a limit, consume a ledger entry or contact a
provider. A `requirements_satisfied: true` result describes consistency of caller-supplied claims, nothing more.
Every decision reports `execution_authorized`, `dispatch_authorized` and `evidence_authenticated` as `false`, plus two
permanent reasons (`evidence_unauthenticated`, `no_trusted_execution_coordinator`), even for fully satisfying
synthetic evidence. A future trusted execution coordinator that applies confinement, produces authenticated evidence
and owns the allow decision must be designed, implemented and reviewed separately; it must not be built by adding a
branch to this function.

## Threat model

Assets: the user's checkout, vault, other projects, credentials and provider accounts, network reachability, CPU,
memory and disk, and the integrity of the authorization record.

Adversaries and failure sources:

| Source | Capability | Consequence the gate must not allow |
| --- | --- | --- |
| The provider CLI or model | Runs arbitrary code as the user once started; can print any text, including claims about its own sandbox; can spawn and detach descendants | Its statements about itself becoming evidence of confinement |
| Prompt or note injection | Steers a model into running tools | Content from a prompt, note or output becoming a policy input |
| A replaced or upgraded binary | Different bytes behind the same path or name | Evidence about one executable authorizing another |
| A different or edited command | Extra flags (`--dangerously-bypass-approvals-and-sandbox`, `--add-dir`) | Evidence about one argument vector authorizing another |
| Stale or replayed evidence | Old probe results, reused authorizations | A past success standing in for a present guarantee |
| Buggy or optimistic callers | Forget a check, merge conflicting results | Conflicts resolved in favor of execution |
| Hostile same-user code | Can edit files, the ledger and Python objects | **Out of scope.** Nothing in this gate resists it; see Limits. |

## Capability knowledge: four states

| State | Meaning | Example | Can satisfy |
| --- | --- | --- | --- |
| Advertised | Someone says the control exists | `--sandbox read-only` in help text; an adapter declaration | Nothing |
| Observed | Synapse watched a run and saw behavior | A probe that did not write outside scratch | Only requirements that ask for `observed` |
| Independently enforced | Synapse applied a mechanism the provider cannot disable, from outside the provider process, and has evidence it is in force | An OS sandbox profile Synapse applied before `exec` | Enforcement requirements |
| Unknown | No applicable, current evidence | Everything today for a real provider | Nothing; blocks |

Absence is never promoted. No observed network connection is not network isolation, and an empty scratch directory
is not filesystem isolation, because a run that did not misbehave proves nothing about what was prevented. The gate
has no input that means "nothing bad was seen", and a `pass` record with mechanism `none` can never satisfy an
enforcement requirement.

## Evidence provenance and authentication

Two layers of input are accepted, both unauthenticated caller data:

1. **Probe records** are the existing `runtime_validation` records, parsed by `parse_evidence` and judged by the same
   per-capability rules, unchanged. They bind to an executable hash and a `probe_id` but carry neither a command nor
   a validity window.
2. **Control attestations** are a new closed schema that adds what probe records lack: `origin`, `level`,
   `mechanism`, command binding and a validity window.

`origin` is one of `provider_reported`, `synapse_observed`, `synapse_enforced`. Only Synapse code may construct the
last two, in the same process that evaluates them and from its own observations; the trusted-construction rule of
the [runtime-validation guide](../guides/runtime-validation-foundations.md#evidence-trust-boundary) applies. Because
the gate cannot check who built a record, it never reports evidence as authenticated. Authentication (a trusted
producer the coordinator invokes itself, rather than records handed to it) is a coordinator requirement and is the
reason the decision cannot become an allow.

A record with `origin: provider_reported` can never satisfy a requirement above `advertised`, whatever `level` or
`mechanism` it states. The gate reports this as `untrusted_origin` instead of rejecting the record, so forgery is
visible in the decision.

### Why a provider cannot attest to its own confinement

Confinement is a property that must hold *against* the provider. A program that is confined, escaped, or simply
dishonest produces the same words as a program that is confined and honest, so its report carries no information
about which case holds. Even a truthful provider can only report its own parser's view (a flag it accepted), not
what the kernel enforces. A guarantee is therefore credible only if (a) the mechanism is applied by a party outside
the provider's control before it runs, and (b) the evidence about it comes from that party. This is also why the
existing runtime-validation rules reserve isolation capabilities for `independently_verified` evidence, and why this
gate additionally requires a named enforcing mechanism rather than a label.

## User approval versus OS-enforced confinement

User approval (the terminal confirmation of increment #13) answers *"does the person accept this exact operation?"*.
It binds consent to a specification and, through the ledger, to a single use. It does not restrict what the
approved program then does. OS-enforced confinement answers *"what can the program do even if it misbehaves?"*. The
two are independent and both are required for a model task; neither substitutes for the other. A high-trust
approval of a program that can write anywhere is still a program that can write anywhere.

## Required controls

Each control is judged from attestations bound to the exact executable hash, command digest and `probe_id`.

| Control | Meaning of "enforced" | Rejected as insufficient |
| --- | --- | --- |
| `probe_evidence` | The probe records for this `probe_id` were produced by Synapse within a stated window | Probe records with no window |
| `authorization_ledger` | A single-use authorization for this exact specification is recorded in the private ledger, with no replay | Missing record; any `replayed` failure |
| `process_containment` | Every descendant is bound to a mechanism that survives `setsid`/double fork and can be killed as a unit (cgroup on Linux) | Process group only; any `descendant_escaped` |
| `filesystem` | An OS sandbox applied by Synapse denies writes outside an allowlist (and reads outside the project scope) | A provider flag; an empty scratch directory |
| `network` | An OS sandbox or namespace denies sockets except an explicitly allowed endpoint | Absence of observed connections |
| `cpu_limit`, `memory_limit`, `disk_limit` | A kernel-enforced cap (rlimit, cgroup, size-bounded volume) | A timeout alone; an output byte cap; a flag |
| Executable identity | Hash of the resolved file matches, via probe records | A name, a path, a version string |

Executable identity is bound twice: records for a different `executable_sha256` are ignored and counted as stale.
Its limit stays as documented: the hash identifies the launcher file at one moment, not the native binary behind it
and not a later swap. Re-hashing immediately before spawn belongs to the coordinator.

**Authorization replay.** The pure gate cannot consult the ledger, and must not (it has no access to state
roots). A caller supplies a ledger attestation. Missing evidence is `unknown`, a `replayed` failure is `failed`, and
an attestation bound to another command digest is not applicable. The ledger remains single-use by `probe_id` among
cooperating processes only.

## Evidence validity, staleness and conflict

- **Binding order.** An attestation counts toward a control only if its executable hash, command digest and
  `probe_id` all equal the request. Mismatches are counted separately (`stale_executable`, `command_mismatch`,
  `unbound_probe`) and then ignored. A different executable or command can never authorize silently.
- **Validity window.** A record is live when `issued_at <= now < expires_at`; `now` is an explicit input, never read
  from the clock, so results are reproducible. Windows longer than 900 seconds (the authorization cap) are
  rejected. Expired and not-yet-valid records do not count.
- **Failures never expire.** A `fail` or `unsupported` record from an applicable run blocks regardless of its
  window, so waiting out a failed check cannot clear it.
- **Conflicts resolve to the more severe state.** Order: `failed`, `unsupported`, `conflicting` (live enforced
  records naming different mechanisms), `expired` or `not_yet_valid` (passing records exist, none live), `unknown`
  (nothing live), `insufficient_evidence` (live, but below the required level), `untrusted_origin`,
  `mechanism_insufficient`, then `satisfied`. A pass never overrides a failure from any origin or level.
  The same rule applies to probe records, including for capabilities a tier does not require: a failed or
  contradicting record blocks.
- **Non-required controls.** A control a tier does not require is reported `not_required`, but failures against it
  still block.

## Help commands versus model tasks

| Requirement | `help_command` (`--version`, `--help`) | `model_task` |
| --- | --- | --- |
| Executable identity | required | required |
| `probe_evidence`, `authorization_ledger` | required (`observed`, `enforced`) | required |
| `process_containment` | `observed` (process group acceptable) | `enforced` |
| `filesystem`, `network`, `cpu_limit`, `memory_limit`, `disk_limit` | not required | `enforced` |
| Model acceptance, output format, session persistence, read-only filesystem, network restriction (probe records) | not required | required (effort if asked) |

Help commands do no model call, take no task input, and run argument vectors the runner allowlists, so a hash pin,
a single-use user approval and bounded termination are the accepted risk; a malicious binary could still do harm
even for `--help`, and the existing runner is explicitly not an OS sandbox. This asymmetry is a policy choice. A
model task puts untrusted prompt content, repository data and network reachability in play, so every control must
be independently enforced. The gate is the only place where the two tiers are defined; neither is executable today.

## Platform requirements

The platform is an explicit input, never read from `os`. A platform not in the table is unsupported: it blocks both
tiers, and no enforced control can be satisfied on it. These are the mechanisms the gate will *accept as evidence*;
Synapse implements none of them yet.

| Platform | filesystem | network | process containment | cpu | memory | disk |
| --- | --- | --- | --- | --- | --- | --- |
| `linux` | `os_sandbox` | `os_sandbox` | `cgroup` | `rlimit`, `cgroup` | `cgroup` | `quota_volume` |
| `darwin` | `os_sandbox` | `os_sandbox` | none accepted | `rlimit` | none accepted | `quota_volume` |
| other (including `windows`) | unsupported | | | | | |

macOS has no accepted descendant-containment or memory-cap mechanism: process groups are escapable and memory
rlimits are not reliably enforced, so a macOS model task cannot satisfy the gate until a later reviewed ADR adds a
mechanism. This is deliberate fail-closed behavior, not an oversight. Windows is unsupported because the probe
runner is POSIX-only; job objects and AppContainer need their own design. Disk quotas need a size-bounded volume
because `RLIMIT_FSIZE` caps single-file size, not total usage.

## Contract

`evaluate_execution_gate(task_kind=, executable_sha256=, command=, probe_id=, platform_name=, now=, evidence=,
attestations=, model=None, effort=None)` has no defaults for security-significant values. `command` is the argument
vector after the executable, digested by `command_specification_sha256` (canonical JSON, so any inherited MCO
vector can be bound). `model` is required for `model_task` and invalid for `help_command`; `effort` needs a model.
Malformed input raises `PlanError`, matching the existing library; semantic problems (stale, forged, missing) are
decision reasons. The result lists every control in a fixed order with `required`, `minimum_level`, `status` and
`highest_claimed_level`, evidence counts, and a `decision` holding the reasons in a fixed order: platform, probe
records (capability order), controls (control order), then the permanent reasons.

## Limits

- Everything is unauthenticated caller input. Hostile same-user code can fabricate a record that satisfies the
  requirements; the permanent reasons exist so that this is never read as permission.
- No mechanism in the platform table is implemented, so no real provider can satisfy a model task.
- Legacy probe records carry no window; their freshness is only as good as the `probe_evidence` attestation that
  brackets the same `probe_id`.
- The ledger guarantee is the existing one (cooperating processes, local filesystems).
- The gate is not wired into the CLI, planner, runner or ledger, and a test pins that.
