// The provider catalog: the agent CLIs Ordelis targets, and what each vendor's documentation says about them.
//
// Every statement here is vendor documentation read on the stated date, and documentation establishes at most an
// advertised capability (evidence.rs gives it the weakest level). Nothing here says that a CLI is installed, which
// version is installed, that it behaves as documented, or that an account can use it. The catalog holds no path, no
// search location, no argument vector and no authentication check: a documented executable name is metadata about
// the product, never something to look up or run.

/// Stable provider identifiers. `codex`, `claude` and `opencode` match the inherited runtime's adapter ids.
/// `kimi` is the dedicated Kimi Code CLI; Kimi through OpenCode is a separate route that is not decided (#108).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ProviderId {
    Codex,
    Claude,
    OpenCode,
    Kimi,
}

impl ProviderId {
    pub(crate) const ALL: [ProviderId; 4] =
        [ProviderId::Codex, ProviderId::Claude, ProviderId::OpenCode, ProviderId::Kimi];

    /// The stable identifier. Never renamed: later records and decisions refer to providers by it.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            ProviderId::Codex => "codex",
            ProviderId::Claude => "claude",
            ProviderId::OpenCode => "opencode",
            ProviderId::Kimi => "kimi",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<ProviderId> {
        ProviderId::ALL.into_iter().find(|provider| provider.id() == id)
    }
}

/// A calendar date as written on a documentation page or in a retrieval record. Never read from a clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Date {
    pub(crate) year: u16,
    pub(crate) month: u8,
    pub(crate) day: u8,
}

/// The date every reference below was retrieved.
const RETRIEVED: Date = Date { year: 2026, month: 11, day: 5 };

/// One vendor documentation page.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    pub(crate) title: &'static str,
    pub(crate) url: &'static str,
    pub(crate) retrieved_on: Date,
    /// The page's own last-updated date, when the page shows one.
    pub(crate) page_updated: Option<Date>,
}

/// What a provider's documentation is checked for. A documented feature is a claim, not a capability Ordelis has
/// observed; it maps onto the evidence vocabulary only through evidence::documentation_records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Feature {
    /// A mode that takes one prompt and exits, without an interactive interface.
    NonInteractiveMode,
    /// Machine-readable output, such as JSON events.
    StructuredOutput,
    /// Choosing a model for a run.
    ModelSelection,
    /// Choosing a reasoning effort or an equivalent. Providers use different, incomparable scales.
    ReasoningEffort,
    /// The provider's own permission or approval modes.
    PermissionModes,
    /// A documented way for another program to answer the provider's permission requests.
    ApprovalRouting,
    /// The caller choosing a new session's identifier.
    HostAssignedSessionId,
    /// Continuing an earlier session.
    SessionResume,
    /// Running without saving session data.
    SessionPersistenceOff,
    /// A sandbox the provider applies to itself. Never evidence of confinement by Ordelis.
    ProviderSandbox,
    /// An Agent Client Protocol mode.
    AgentClientProtocol,
    /// A server mode that other processes connect to.
    LocalServer,
}

impl Feature {
    pub(crate) const ALL: [Feature; 12] = [
        Feature::NonInteractiveMode,
        Feature::StructuredOutput,
        Feature::ModelSelection,
        Feature::ReasoningEffort,
        Feature::PermissionModes,
        Feature::ApprovalRouting,
        Feature::HostAssignedSessionId,
        Feature::SessionResume,
        Feature::SessionPersistenceOff,
        Feature::ProviderSandbox,
        Feature::AgentClientProtocol,
        Feature::LocalServer,
    ];
}

/// What the reviewed documentation says about a feature. Only `Documented` is a positive claim, and it is still only
/// a claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocStatus {
    /// The documentation describes the feature.
    Documented,
    /// The reviewed pages were read for it and do not describe it. Absence from a page is not proof of absence.
    NotFound,
    /// The reviewed pages contradict each other.
    Contradictory,
    /// Nobody has read the documentation for this feature yet.
    NotReviewed,
}

/// How much of the page the statement rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reading {
    /// Read from the page's own text.
    Verbatim,
    /// Read from a summary or a search excerpt of the page, not its full text.
    Summarized,
    /// Not read.
    NotRead,
}

/// One statement about one feature of one provider. `note` is a reviewer's short description, not page text.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Claim {
    pub(crate) feature: Feature,
    pub(crate) status: DocStatus,
    pub(crate) reading: Reading,
    pub(crate) reference: Option<&'static Reference>,
    pub(crate) note: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Provider {
    pub(crate) id: ProviderId,
    pub(crate) display_name: &'static str,
    /// The command name the vendor documents. Metadata only: never searched for, resolved or run.
    pub(crate) executable_name: &'static str,
    pub(crate) executable_name_reference: &'static Reference,
    /// Caveats about the entry as a whole.
    pub(crate) notes: &'static [&'static str],
    /// Exactly one claim per feature, in Feature::ALL order.
    pub(crate) claims: &'static [Claim],
}

pub(crate) fn provider(id: ProviderId) -> &'static Provider {
    match id {
        ProviderId::Codex => &CODEX,
        ProviderId::Claude => &CLAUDE,
        ProviderId::OpenCode => &OPENCODE,
        ProviderId::Kimi => &KIMI,
    }
}

impl Provider {
    pub(crate) fn claim(&self, feature: Feature) -> Option<&'static Claim> {
        self.claims.iter().find(|claim| claim.feature == feature)
    }
}

const fn documented(feature: Feature, reading: Reading, reference: &'static Reference, note: &'static str) -> Claim {
    Claim { feature, status: DocStatus::Documented, reading, reference: Some(reference), note }
}

const fn not_found(feature: Feature, reading: Reading, reference: &'static Reference, note: &'static str) -> Claim {
    Claim { feature, status: DocStatus::NotFound, reading, reference: Some(reference), note }
}

const fn not_reviewed(feature: Feature) -> Claim {
    Claim { feature, status: DocStatus::NotReviewed, reading: Reading::NotRead, reference: None, note: "Not reviewed." }
}

// References. Retrieved 2026-11-05; only the OpenCode pages showed a last-updated date.

static CODEX_COMMANDS: Reference = Reference {
    title: "Codex: developer commands (redirected from developers.openai.com/codex/cli/reference)",
    url: "https://learn.chatgpt.com/docs/developer-commands?surface=cli",
    retrieved_on: RETRIEVED,
    page_updated: None,
};
static CODEX_SANDBOX: Reference = Reference {
    title: "Codex: sandbox",
    url: "https://learn.chatgpt.com/docs/sandboxing",
    retrieved_on: RETRIEVED,
    page_updated: None,
};
static CODEX_NON_INTERACTIVE: Reference = Reference {
    title: "Codex: non-interactive mode",
    url: "https://developers.openai.com/pt-BR/docs/non-interactive-mode",
    retrieved_on: RETRIEVED,
    page_updated: None,
};
static CLAUDE_CLI: Reference = Reference {
    title: "Claude Code: CLI reference",
    url: "https://code.claude.com/docs/en/cli-reference",
    retrieved_on: RETRIEVED,
    page_updated: None,
};
static CLAUDE_SANDBOX: Reference = Reference {
    title: "Claude Code: configure the sandboxed Bash tool",
    url: "https://code.claude.com/docs/en/sandboxing",
    retrieved_on: RETRIEVED,
    page_updated: None,
};
static OPENCODE_CLI: Reference = Reference {
    title: "OpenCode: CLI",
    url: "https://opencode.ai/docs/cli/",
    retrieved_on: RETRIEVED,
    page_updated: Some(Date { year: 2026, month: 11, day: 5 }),
};
static OPENCODE_PERMISSIONS: Reference = Reference {
    title: "OpenCode: permissions",
    url: "https://opencode.ai/docs/permissions/",
    retrieved_on: RETRIEVED,
    page_updated: Some(Date { year: 2026, month: 11, day: 5 }),
};
static OPENCODE_SERVER: Reference = Reference {
    title: "OpenCode: server",
    url: "https://opencode.ai/docs/server/",
    retrieved_on: RETRIEVED,
    page_updated: None,
};
static KIMI_COMMAND: Reference = Reference {
    title: "Kimi Code CLI: the kimi command",
    url: "https://www.kimi.com/code/docs/en/kimi-code-cli/reference/kimi-command.html",
    retrieved_on: RETRIEVED,
    page_updated: None,
};

use Feature::*;
use Reading::{Summarized, Verbatim};

static CODEX: Provider = Provider {
    id: ProviderId::Codex,
    display_name: "Codex",
    executable_name: "codex",
    executable_name_reference: &CODEX_COMMANDS,
    notes: &[
        "The command reference redirected to learn.chatgpt.com. Parts of it, and of the sandbox page, were read \
        only as a summary.",
    ],
    claims: &[
        documented(NonInteractiveMode, Verbatim, &CODEX_COMMANDS, "codex exec runs Codex non-interactively."),
        documented(StructuredOutput, Summarized, &CODEX_NON_INTERACTIVE, "codex exec --json emits JSON Lines events."),
        documented(
            ModelSelection, Verbatim, &CODEX_COMMANDS,
            "A global --model option takes a model name. Whether an account can use a model is not documented here.",
        ),
        not_found(
            ReasoningEffort, Summarized, &CODEX_COMMANDS,
            "No dedicated effort option; only the generic --config key=value override. The repository's Codex \
            inspection guide cites a model_reasoning_effort key that was not re-read.",
        ),
        documented(
            PermissionModes, Verbatim, &CODEX_COMMANDS,
            "--sandbox read-only, workspace-write or danger-full-access; --ask-for-approval on-request or never.",
        ),
        not_found(
            ApprovalRouting, Verbatim, &CODEX_COMMANDS,
            "codex app-server is marked Experimental; the reviewed official pages do not describe its approval \
            requests.",
        ),
        not_found(
            HostAssignedSessionId, Summarized, &CODEX_COMMANDS,
            "No option to choose a new session's identifier was found.",
        ),
        documented(
            SessionResume, Summarized, &CODEX_COMMANDS,
            "codex exec resume takes a session identifier or --last.",
        ),
        documented(SessionPersistenceOff, Summarized, &CODEX_COMMANDS, "codex exec --ephemeral."),
        documented(
            ProviderSandbox, Summarized, &CODEX_SANDBOX,
            "Codex sandboxes itself with Seatbelt on macOS and bubblewrap on Linux. Whether workspace-write blocks \
            the network by default was not found.",
        ),
        not_found(
            AgentClientProtocol, Summarized, &CODEX_COMMANDS,
            "No ACP mode in the reviewed reference, so the inherited runtime's ACP launch command for Codex is \
            undocumented.",
        ),
        documented(
            LocalServer, Verbatim, &CODEX_COMMANDS,
            "codex app-server exists and is marked Experimental. Its transport was not reviewed.",
        ),
    ],
};

static CLAUDE: Provider = Provider {
    id: ProviderId::Claude,
    display_name: "Claude Code",
    executable_name: "claude",
    executable_name_reference: &CLAUDE_CLI,
    notes: &[
        "The CLI reference was read partly verbatim and partly as a summary; the claims marked verbatim were checked \
        against its text.",
    ],
    claims: &[
        documented(
            NonInteractiveMode, Verbatim, &CLAUDE_CLI,
            "-p or --print answers without the interactive interface.",
        ),
        documented(
            StructuredOutput, Verbatim, &CLAUDE_CLI,
            "--output-format text, json or stream-json; --input-format text or stream-json.",
        ),
        documented(ModelSelection, Verbatim, &CLAUDE_CLI, "--model takes an alias or a full model name."),
        documented(
            ReasoningEffort, Verbatim, &CLAUDE_CLI,
            "--effort low, medium, high, xhigh, max or ultracode. Which values a given model accepts is not \
            documented here.",
        ),
        documented(
            PermissionModes, Verbatim, &CLAUDE_CLI,
            "--permission-mode default, acceptEdits, plan, auto, dontAsk or bypassPermissions.",
        ),
        documented(
            ApprovalRouting, Verbatim, &CLAUDE_CLI,
            "--permission-prompt-tool names an MCP tool that answers permission prompts in non-interactive mode.",
        ),
        documented(HostAssignedSessionId, Verbatim, &CLAUDE_CLI, "--session-id takes a UUID chosen by the caller."),
        documented(SessionResume, Verbatim, &CLAUDE_CLI, "--resume, --continue and --fork-session."),
        documented(SessionPersistenceOff, Verbatim, &CLAUDE_CLI, "--no-session-persistence, in print mode only."),
        documented(
            ProviderSandbox, Verbatim, &CLAUDE_SANDBOX,
            "Seatbelt on macOS and bubblewrap on Linux, off by default, covering shell commands only: file tools, \
            MCP servers and hooks run outside it, and a command may be retried unsandboxed unless settings forbid \
            it.",
        ),
        not_found(
            AgentClientProtocol, Summarized, &CLAUDE_CLI,
            "No ACP mode in the reviewed reference, so the inherited runtime's ACP launch command for Claude Code \
            is undocumented.",
        ),
        not_found(LocalServer, Summarized, &CLAUDE_CLI, "No server mode was found in the reviewed reference."),
    ],
};

static OPENCODE: Provider = Provider {
    id: ProviderId::OpenCode,
    display_name: "OpenCode",
    executable_name: "opencode",
    executable_name_reference: &OPENCODE_CLI,
    notes: &["The server page was read only through search excerpts."],
    claims: &[
        documented(
            NonInteractiveMode, Verbatim, &OPENCODE_CLI,
            "opencode run takes a message and runs without the interactive interface.",
        ),
        documented(StructuredOutput, Verbatim, &OPENCODE_CLI, "opencode run --format json emits raw JSON events."),
        documented(ModelSelection, Verbatim, &OPENCODE_CLI, "--model takes provider/model."),
        documented(
            ReasoningEffort, Verbatim, &OPENCODE_CLI,
            "--variant selects a provider-specific reasoning effort; there is no common scale.",
        ),
        documented(
            PermissionModes, Verbatim, &OPENCODE_PERMISSIONS,
            "Rules allow, ask or deny per tool, and most tools default to allow. --auto approves every request that \
            is not denied. What ask does in a non-interactive run is not documented.",
        ),
        documented(
            ApprovalRouting, Summarized, &OPENCODE_SERVER,
            "The HTTP server has an endpoint that answers a pending permission request.",
        ),
        not_found(
            HostAssignedSessionId, Verbatim, &OPENCODE_CLI,
            "--session continues an existing session; no option to choose a new session's identifier was found.",
        ),
        documented(SessionResume, Verbatim, &OPENCODE_CLI, "--session, --continue and --fork."),
        not_found(
            SessionPersistenceOff, Verbatim, &OPENCODE_CLI,
            "No option to run without saving the session was found.",
        ),
        not_found(
            ProviderSandbox, Verbatim, &OPENCODE_PERMISSIONS,
            "No sandbox is documented. Permission rules are not confinement.",
        ),
        documented(
            AgentClientProtocol, Verbatim, &OPENCODE_CLI,
            "opencode acp starts an ACP server over standard input and output.",
        ),
        documented(
            LocalServer, Summarized, &OPENCODE_SERVER,
            "opencode serve listens on 127.0.0.1:4096 by default, with optional HTTP basic authentication.",
        ),
    ],
};

static KIMI: Provider = Provider {
    id: ProviderId::Kimi,
    display_name: "Kimi Code CLI",
    executable_name: "kimi",
    executable_name_reference: &KIMI_COMMAND,
    notes: &[
        "The dedicated Kimi Code CLI only. Kimi through OpenCode is a separate route, and the Kimi decision gate is \
        open.",
        "Read from search excerpts of the official reference, not its full text.",
    ],
    claims: &[
        documented(NonInteractiveMode, Summarized, &KIMI_COMMAND, "--print runs non-interactively."),
        documented(
            StructuredOutput, Summarized, &KIMI_COMMAND,
            "--output-format stream-json emits JSON Lines, only together with --prompt.",
        ),
        documented(
            ModelSelection, Summarized, &KIMI_COMMAND,
            "Only an example with -m was found; the option itself is not described.",
        ),
        not_found(ReasoningEffort, Summarized, &KIMI_COMMAND, "No reasoning or thinking control was found."),
        Claim {
            feature: PermissionModes,
            status: DocStatus::Contradictory,
            reading: Summarized,
            reference: Some(&KIMI_COMMAND),
            note: "One version says print mode implies --yolo; another says non-interactive mode approves \
                  automatically by default and rejects --yolo, --auto and --plan with --prompt.",
        },
        not_reviewed(ApprovalRouting),
        not_reviewed(HostAssignedSessionId),
        not_reviewed(SessionResume),
        not_reviewed(SessionPersistenceOff),
        not_reviewed(ProviderSandbox),
        documented(
            AgentClientProtocol, Summarized, &KIMI_COMMAND,
            "kimi acp starts an ACP server over standard input and output; the --acp option is deprecated.",
        ),
        not_reviewed(LocalServer),
    ],
};

#[cfg(test)]
mod tests;
