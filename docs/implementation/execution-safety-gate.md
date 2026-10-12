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
   `mechanism`, executable, command and platform binding, and a validity window.

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

- **Binding order.** An attestation counts toward a control only if its executable hash, command digest, platform
  and `probe_id` all equal the request. Mismatches are counted separately (`stale_executable`, `command_mismatch`,
  `platform_mismatch`, `unbound_probe`) and then ignored. A different executable, command or platform can never
  authorize silently. Platform binding matters because a mechanism name such as `os_sandbox` denotes different
  kernel facilities on different operating systems.
- **Model and effort.** Enforcement attestations are not bound to a model or effort value: confinement is a property
  of the executable, its argument vector and the platform. The model and effort a command requests appear in its
  argument vector and are therefore bound through the command digest; the gate does not parse the vector to check
  that it agrees with the `model` and `effort` inputs. Probe records bind a model and effort through `requested`.
- **Policy version.** Every result carries `policy_version`, identifying the requirement and platform tables that
  produced it. Attestations are not bound to it; a changed table changes the version, and results from different
  versions must not be compared.
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
- **Scope of a failure.** A failure poisons only the exact executable, command, platform and `probe_id` it was
  recorded for. A new probe run, a different command or a changed executable is a distinct configuration and is not
  blocked by it. Probe records carry no command, so a probe-record failure applies to every command evaluated under
  the same executable and `probe_id`; a single-use `probe_id` binds one specification, so this does not cross runs.
- **Any party can only worsen a control.** Records from every origin, including `provider_reported`, take part in
  failure and conflict detection, so a provider can turn a satisfied control into `conflicting` or `failed` but can
  never improve one.
- **Omission cannot be detected.** The gate sees only what the caller supplies. A caller who leaves out a failing
  record gets a better result; this is part of why no result is authenticated.

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

Four statements are kept apart, and the table below makes only the third and fourth:

1. A mechanism **exists on the operating system** (for example Linux cgroups, Landlock, seccomp and namespaces;
   macOS Seatbelt profiles; Windows job objects and AppContainer). This ADR does not survey or rely on that.
2. Synapse **implements** it. It implements none yet.
3. It was **independently verified in one execution**. That is what an attestation would claim, and only a trusted
   producer could make it credible.
4. It is **sufficient for this gate's threat model**. The table lists the mechanisms the gate currently *accepts* as
   a basis for an `enforced` claim.

The table is an accepted set, not a statement about what an operating system can or cannot do. Omitting a mechanism
means "not yet evaluated, treated as unknown", and the resulting status `mechanism_insufficient` reads as "no
accepted mechanism was named", never as "impossible". Accepting another mechanism is a change to this table and
`POLICY_VERSION`, with its own review. The platform is an explicit input, never read from `os`; attestations are
bound to it.

| Platform | filesystem | network | process containment | cpu | memory | disk |
| --- | --- | --- | --- | --- | --- | --- |
| `linux` | `os_sandbox` | `os_sandbox` | `cgroup` | `rlimit`, `cgroup` | `cgroup` | `quota_volume` |
| `darwin` | `os_sandbox` | `os_sandbox` | not evaluated | `rlimit` | not evaluated | `quota_volume` |
| any other name, including `windows` | not evaluated; the gate reports the platform `unsupported` |

- **Linux.** Other mechanisms may be adequate (for example PID or user namespaces for containment) but are not in
  the accepted set; they are unknown to the gate, not rejected on their merits.
- **macOS.** Descendant containment and memory limits are *not evaluated*. Process groups can be left by a
  descendant, which is why `process_group` is not accepted for enforcement; whether another macOS facility could
  meet the threat model is unknown here. Until a reviewed amendment names one, a macOS model task cannot satisfy
  the gate.
- **Windows.** Unsupported by policy for this increment because Synapse's probe runner is POSIX-only, not because
  of a conclusion about Windows. Job objects and AppContainer are unevaluated.
- **Disk.** `quota_volume` is named because `RLIMIT_FSIZE` bounds the size of a single file, not total usage; other
  total-usage mechanisms are unevaluated.
- Because macOS is the platform the project is developed on, a result of `mechanism_insufficient` there is the
  designed outcome of an incomplete accepted set, not a defect to be silenced by adding unverified mechanisms.

## Contract

`evaluate_execution_gate(task_kind=, executable_sha256=, command=, probe_id=, platform_name=, now=, evidence=,
attestations=, model=None, effort=None)` (attestation fields: `control`, `level`, `origin`, `outcome`, `mechanism`,
`observed`, `executable_sha256`, `command_sha256`, `probe_id`, `platform`, `issued_at`, `expires_at`) has no defaults for security-significant values. `command` is the argument
vector after the executable, digested by `command_specification_sha256` (canonical JSON, so any inherited MCO
vector can be bound). `model` is required for `model_task` and invalid for `help_command`; `effort` needs a model.
Malformed input, including non-finite or oversized numbers, raises `PlanError`, matching the existing library; semantic problems (stale, forged, missing) are
decision reasons. The result lists every control in a fixed order with `required`, `minimum_level`, `status` and
`highest_claimed_level`, evidence counts, and a `decision` holding the reasons in a fixed order: platform, probe
records (capability order), controls (control order), then the permanent reasons.

## Limits

- Everything is unauthenticated caller input. Hostile same-user code can fabricate a record that satisfies the
  requirements; the permanent reasons exist so that this is never read as permission.
- No mechanism in the platform table is implemented, so no real provider can satisfy a model task.
- Legacy probe records carry no window. Their freshness comes only from a live `probe_evidence` attestation for the
  same `probe_id`, and that attestation names no digest of the records it covers: records reused under the same
  `probe_id` within the window are indistinguishable from fresh ones. Single-use `probe_id`s limit this to
  one run, but the gate cannot verify it.
- Importing the gate transitively imports `runtime.config`, `runtime.policy` and `runtime.vault_context`. This was
  audited to read only the `HOME` variable (to locate user configuration paths), open no file and import no runner,
  ledger, confirmation, capability-inspection, CLI, adapter or session module; a test pins the import closure.
- The ledger guarantee is the existing one (cooperating processes, local filesystems).
- The gate is not wired into the CLI, planner, runner or ledger, and a test pins that.
