import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const root = join(__dirname, "..", "..");
const COMMANDS = [
  "ipc_boundary_status",
  "get_runtime_status",
  "list_projects",
  "add_project",
  "remove_project",
  "reset_projects",
  "list_tasks",
  "get_task",
  "create_task",
  "update_task",
];
const read = (p: string) => readFileSync(join(root, p), "utf8");

// The files rustc compiles for the desktop library in each macOS configuration. verify-native-boundary.mjs checks
// this list against the compiler's dep-info, so any other compiled file fails CI however it was brought in.
const nativeSources: { configurations: Record<string, string[]> } = JSON.parse(read("scripts/native-sources.json"));

// The production storage files: the project registry, the task store and the protocol they share (src/store.rs), as
// listed (and compiler-verified) above, keyed by path under src-tauri/src.
function projectSources(): Record<string, string> {
  const production = new Set(
    ["default", "release", "dev-runtime"]
      .flatMap((c) => nativeSources.configurations[c])
      .filter((f) => f.startsWith("src/projects") || f.startsWith("src/tasks") || f === "src/store.rs"),
  );
  return Object.fromEntries([...production].map((f) => [f.slice("src/".length), read(`src-tauri/${f}`)]));
}

// Criterion 21's one reviewed exception (issue #82): registry.rs may list the app data directory it was given, once,
// in backup_present, to see whether a corrupt-registry backup exists. Exactly this call in that file is set aside
// before the rules below; every other listing, in any file, is still a violation. Its scope is pinned below.
const REVIEWED_LISTING = "std::fs::read_dir(dir)";

// Content rules for the project registry and the task store: no listing, contents, processes, network, Git, Python or
// gate, and only registry.rs and store.rs touch files (the task store has no file operation of its own). Which files exist is not decided here; the compiler-verified list above decides it.
function projectContentViolations(files: Record<string, string>): string[] {
  const violations: string[] = [];
  const fsAllowed: Record<string, string[]> = {
    "projects/registry.rs": ["hard_link", "remove_file", "symlink_metadata"],
    "store.rs": ["DirBuilder", "File", "Metadata", "OpenOptions", "remove_file", "rename", "symlink_metadata"],
    "projects/folder.rs": ["Metadata", "canonicalize", "symlink_metadata"],
  };
  const libcAllowed = new Set([
    "EINTR", "EINVAL", "EIO", "ENOENT", "ENOTDIR", "ENOTSUP", "ERANGE", "EWOULDBLOCK", "LOCK_EX", "LOCK_NB",
    "O_DIRECTORY", "O_NOFOLLOW", "O_NONBLOCK", "c_char", "flock", "geteuid", "getpwuid_r", "passwd",
  ]);
  for (const [file, text] of Object.entries(files)) {
    // Code only: full-line comments may describe what the code must not do.
    const code = text.replace(/^[ \t]*\/\/.*$/gm, "");
    // String replace: the first occurrence only.
    const reviewed = file === "projects/registry.rs" ? code.replace(REVIEWED_LISTING, "") : code;
    const rust = reviewed.replaceAll("spawn_blocking", "").replaceAll("tauri::async_runtime::spawn(", "");
    const forbidden = [
      /read_dir|walkdir|glob|std::process|\bCommand\b|\bspawn\b|std::net|TcpStream|UdpSocket|reqwest|hyper|tokio::net|\.plugin\(/,
      /runtime_bridge|python|\bgit\b|[Gg]ate|execute|provider|agent/,
      /fs::read\b|read_to_string|fs::write|fs::copy|File::|remove_dir|set_permissions|create_dir(?:_all)?\(|std::env/,
      /use std::fs[^;]*\bas\b|extern crate/,
    ];
    for (const pattern of forbidden) {
      const hit = rust.match(pattern);
      if (hit) violations.push(`${file}: ${hit[0]}`);
    }
    const fsItems = [...rust.matchAll(/std::fs::(\w+)|use std::fs::\{([^}]*)\}/g)].flatMap((m) => (m[1] ? [m[1]] : m[2].split(",").map((x) => x.trim())));
    for (const item of fsItems) if (!(fsAllowed[file] ?? []).includes(item)) violations.push(`${file}: std::fs::${item}`);
    if (/use std::fs(?:;|::\*)/.test(rust)) violations.push(`${file}: imports all of std::fs`);
    if (!["projects/registry.rs", "store.rs"].includes(file) && /OpenOptions|DirBuilder|remove_file|rename\(|hard_link/.test(rust)) {
      violations.push(`${file}: file operation outside the registry`);
    }
    for (const m of rust.matchAll(/libc::(\w+)/g)) if (!libcAllowed.has(m[1])) violations.push(`${file}: libc::${m[1]}`);
  }
  return violations;
}

// The production files of the provider catalog and evidence vocabulary (issue #109), as listed (and compiler-verified)
// above, keyed by path under src-tauri/src.
function agentSources(): Record<string, string> {
  const production = new Set(
    ["default", "release", "dev-runtime"].flatMap((c) => nativeSources.configurations[c]).filter((f) => f.startsWith("src/agents")),
  );
  return Object.fromEntries([...production].map((f) => [f.slice("src/".length), read(`src-tauri/${f}`)]));
}

// The production files of the execution lifecycle model (issue #112), keyed the same way.
function lifecycleSources(): Record<string, string> {
  const production = new Set(
    ["default", "release", "dev-runtime"].flatMap((c) => nativeSources.configurations[c]).filter((f) => f.startsWith("src/lifecycle")),
  );
  return Object.fromEntries([...production].map((f) => [f.slice("src/".length), read(`src-tauri/${f}`)]));
}

// Rust code with comments removed and every string and character literal emptied, so that documentation text (URLs,
// flag names in notes) is never mistaken for code and code is never hidden in a comment or behind a quote character.
// Path separators lose their surrounding spaces (`crate :: x` is `crate::x`). Raw strings are refused separately.
const CHAR_LITERAL = /'(?:\\u\{[0-9A-Fa-f]{1,6}\}|\\x[0-9A-Fa-f]{2}|\\.|[^\\'\n])'/y;
// Memoized: it is pure, and the bypass fixtures run every rule over every file while changing one file at a time.
const codeOnlyCache = new Map<string, string>();
function rustCodeOnly(text: string): string {
  const cached = codeOnlyCache.get(text);
  if (cached !== undefined) return cached;
  const code = stripRust(text);
  codeOnlyCache.set(text, code);
  return code;
}
function stripRust(text: string): string {
  let out = "";
  for (let i = 0; i < text.length; ) {
    CHAR_LITERAL.lastIndex = i;
    if (text.startsWith("//", i)) {
      const end = text.indexOf("\n", i);
      i = end < 0 ? text.length : end;
    } else if (text.startsWith("/*", i)) {
      // Block comments nest in Rust: the comment ends at the */ that closes the first /*.
      let depth = 0;
      do {
        if (text.startsWith("/*", i)) {
          depth++;
          i += 2;
        } else if (text.startsWith("*/", i)) {
          depth--;
          i += 2;
        } else {
          i++;
        }
      } while (depth > 0 && i < text.length);
      out += " ";
    } else if (text[i] === "'" && CHAR_LITERAL.test(text)) {
      // A character or byte literal; a lifetime such as 'static has no closing quote and is kept as code.
      out += "''";
      i = CHAR_LITERAL.lastIndex;
    } else if (text[i] === '"') {
      let j = i + 1;
      while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      out += '""';
      i = j + 1;
    } else {
      out += text[i++];
    }
  }
  return out.replace(/\s*::\s*/g, "::");
}

// Content rules for the provider catalog and evidence vocabulary: pure data and pure functions. No process, file,
// network, environment, clock, thread, global state, FFI, unsafe code or included file; no Tauri, serde or libc; no
// command; nothing visible outside the crate; no path into the rest of the crate (agents.rs names no `super`, and its
// two modules reach only each other); no identifier that could hold a path, a search location, an argument vector or
// a command; and no macro other than matches!.
function agentContentViolations(files: Record<string, string>): string[] {
  return pureContentViolations(files, (file) => (file === "agents.rs" ? /\bsuper\b/ : /\bsuper\b(?!::(?:catalog|evidence)\b)/));
}

// The rules above, shared with the lifecycle model. `superRule` says how a file may not name `super`.
function pureContentViolations(files: Record<string, string>, superRule: (file: string) => RegExp): string[] {
  const violations: string[] = [];
  for (const [file, text] of Object.entries(files)) {
    if (/(?<![A-Za-z0-9_])[bc]?r#*"/.test(text)) violations.push(`${file}: raw string`);
    const code = rustCodeOnly(text);
    const forbidden = [
      /\b(?:std|core|alloc)::/,
      /\b(?:Command|process|spawn|fork|execve?|execvp|kill|signal|Child|Stdio|Output)\b/,
      /\b(?:fs|File|OpenOptions|DirBuilder|Path|PathBuf|read_dir|read_to_string|metadata|canonicalize|symlink_metadata)\b/,
      /\b(?:net|TcpStream|TcpListener|UdpSocket|UnixStream|SocketAddr|IpAddr)\b/,
      /\b(?:env|var_os|SystemTime|Instant|UNIX_EPOCH|now|thread|sleep)\b/,
      /\bstatic\s+mut\b|\b(?:thread_local|OnceLock|OnceCell|LazyLock|LazyCell|lazy_static|Cell|RefCell|Mutex|RwLock|Arc|Rc)\b|\bAtomic\w*/,
      /\b(?:unsafe|extern)\b|#\s*!?\s*\[\s*(?:link|no_mangle|export_name|path|used|link_section)\b/,
      /\b(?:tauri|serde|serde_json|libc|rfd)\b|invoke_handler|generate_handler/,
      // The rest of the crate: by `crate`, by the sibling modules' names, or through `super` from a module root (where
      // it is the crate root). Submodules may name only their reviewed siblings through `super`.
      /(?<!pub\()\bcrate\b|\b(?:runtime_bridge|store|projects|tasks|lib|main)\b/,
      superRule(file),
      /\bpub\s+(?!\((?:crate|super)\))/,
      /\b\w*(?:path|argv|args|arguments|dirs?(?![a-z])|director|search|location|candidate|command|cmd|launch|invoke|program|exe(?!cutable)|binar|template|flag)\w*\b/i,
    ];
    for (const pattern of forbidden) {
      const hit = code.match(pattern);
      if (hit) violations.push(`${file}: ${hit[0]}`);
    }
    for (const m of code.matchAll(/\b(\w+)\s*!\s*[([{]/g)) if (m[1] !== "matches") violations.push(`${file}: ${m[1]}!`);
  }
  return violations;
}

// The only references to the rest of the crate that the lifecycle model may make (D3.2b decision 12.8): these two
// imports, as whole lines, adjacent and in this order, as the first lines of code of lifecycle/model.rs (only line
// comments come before them, so nothing such as `pub(crate)` can prefix them). Anything else, including another copy,
// is held to the agents rules, with no name of the agents module and no identifier that could mean authorized,
// dispatched, enforced or terminated anywhere in it. The module root names no `super`; a submodule names `super` only
// to reach a sibling (`super::model`, `super::reduce`, `super::live`, `super::reconcile`; decision 12.8(b), D3.2b-2 and
// D3.2b-3), and no file imports with a glob.
const LIFECYCLE_IMPORTS = ["use crate::agents::catalog::ProviderId;", "use crate::agents::evidence::{Sha256, Token};"];
function withoutLifecycleImports(file: string, text: string): string {
  if (file !== "lifecycle/model.rs") return text;
  const lines = text.split("\n");
  const first = lines.findIndex((line) => !line.startsWith("//"));
  if (lines[first] !== LIFECYCLE_IMPORTS[0] || lines[first + 1] !== LIFECYCLE_IMPORTS[1]) return text;
  lines.splice(first, 2);
  return lines.join("\n");
}
// D3.2b-3 (#116): the one reviewed exception to the `terminat` rule. Design r5 names an inert reconciliation
// recommendation `termination_proposal`. It exists as exactly the identifier `TerminationProposal`, only in
// lifecycle/reconcile.rs, exactly twice: as a variant of `Recommendation` whose whole declaration is pinned below (plain
// data: an attempt number and two one-value enums, the derives fixed), and in the one place that value is built and
// pushed. Only then are those two occurrences set aside before the rules run. Any other spelling, letter case, prefix,
// suffix, raw identifier, file, declaration or count is held to the unchanged rule and fails; nothing else about
// termination is relaxed. reconciliationViolations pins that nothing consumes a recommendation.
const REVIEWED_PROPOSAL = /(?<![\w#])TerminationProposal(?!\w)/g;
const RECOMMENDATION_DECLARATION =
  "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub(crate) enum Recommendation {\n    ReviewAttempt(AttemptNo),\n" +
  "    TerminationProposal { attempt: AttemptNo, basis: ProposalBasis, policy: PolicyRequirement },\n    RetryPossible(AttemptNo),\n}\n";
const PROPOSAL_DATA = [
  "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub(crate) enum ProposalBasis {\n    PidStartTokenAndHash,\n}\n",
  "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub(crate) enum PolicyRequirement {\n    SeparatePolicy,\n}\n",
];
const PROPOSAL_CONSTRUCTION =
  /\brecommendations\.push\(Recommendation::TerminationProposal \{\s*attempt,\s*basis: ProposalBasis::PidStartTokenAndHash,\s*policy: PolicyRequirement::SeparatePolicy,\s*\}\);/g;
function withoutReviewedProposal(file: string, text: string): string {
  if (file !== "lifecycle/reconcile.rs") return text;
  const code = rustCodeOnly(text);
  const reviewed =
    (code.match(REVIEWED_PROPOSAL) ?? []).length === 2 &&
    code.split(RECOMMENDATION_DECLARATION).length === 2 &&
    PROPOSAL_DATA.every((data) => code.split(data).length === 2) &&
    (code.match(PROPOSAL_CONSTRUCTION) ?? []).length === 1;
  return reviewed ? text.replace(REVIEWED_PROPOSAL, "ReviewedProposal") : text;
}
// D3.2b-4 (#118, decisions 12.8(c) and 12.9): the journal codec is the only lifecycle file that may name serde, and only
// in these forms, which are set aside before the rules run: the import below as its first line of code (only line
// comments before it), the pure in-memory serde_json functions listed here, and the `deny_unknown_fields` attribute.
// Any other serde or serde_json item (a reader, a writer, a custom (de)serializer, another attribute), a second import,
// an import elsewhere or in another file stays a violation. serializationViolations pins what the derives may be on.
const REVIEWED_SERDE_IMPORT = "use serde::{Deserialize, Serialize};";
const REVIEWED_SERDE_JSON = /\bserde_json::(?:from_slice|to_vec|from_value|Value)\b(?!\s+as\b)/g;
const REVIEWED_SERDE_ATTRIBUTE = /^[ \t]*#\[serde\(deny_unknown_fields\)\]\n/gm;
function withoutReviewedSerde(file: string, text: string): string {
  if (file !== "lifecycle/codec.rs") return text;
  const lines = text.split("\n");
  const first = lines.findIndex((line) => !line.startsWith("//"));
  if (lines[first] !== REVIEWED_SERDE_IMPORT) return text;
  lines.splice(first, 1);
  return lines.join("\n").replace(REVIEWED_SERDE_JSON, "reviewed_json").replace(REVIEWED_SERDE_ATTRIBUTE, "");
}
// Which siblings each lifecycle file may name through `super` (decision 12.8(b), D3.2b-2 to D3.2b-4). The codec names
// only the model: it never reaches the reducer, the live layer or reconciliation. Replay reaches the codec and the
// reducer; reconciliation reaches replay only to build a recorded history from one. The live layer reaches the codec
// (D3.2c-2, #122) to encode the frames it stages and to judge an uncertain write against the re-read bytes; never replay.
const LIFECYCLE_SIBLINGS: Record<string, string[]> = {
  "lifecycle/model.rs": [],
  "lifecycle/reduce.rs": ["model"],
  "lifecycle/live.rs": ["model", "reduce", "reconcile", "codec"],
  "lifecycle/reconcile.rs": ["model", "reduce", "live", "replay"],
  "lifecycle/codec.rs": ["model"],
  "lifecycle/replay.rs": ["model", "reduce", "codec"],
};
function lifecycleContentViolations(files: Record<string, string>): string[] {
  const stripped = Object.fromEntries(
    Object.entries(files).map(([file, text]) => [file, withoutReviewedSerde(file, withoutReviewedProposal(file, withoutLifecycleImports(file, text)))]),
  );
  const violations = pureContentViolations(stripped, (file) =>
    file === "lifecycle.rs" ? /\bsuper\b/ : /\bsuper\b(?!::(?:model|reduce|live|reconcile|codec|replay)\b)/,
  );
  for (const [file, text] of Object.entries(stripped)) {
    const code = rustCodeOnly(text);
    // Code is printable ASCII (review of #116): a look-alike letter must not slip a word past the rules above.
    for (const pattern of [/\bagents\b/, /\w*(?:authori[sz]|dispatch|enforc|terminat|execute_now)\w*/i, /\buse\b[^;]*\*/, /[^\t\n\r -~]/]) {
      const hit = code.match(pattern);
      if (hit) violations.push(`${file}: ${hit[0]}`);
    }
    if (file === "lifecycle.rs") continue;
    const allowed = LIFECYCLE_SIBLINGS[file];
    if (!allowed) violations.push(`${file}: no reviewed sibling list`);
    for (const m of code.matchAll(/\bsuper::(\w+)/g)) if (!allowed?.includes(m[1])) violations.push(`${file}: names super::${m[1]}`);
  }
  return violations;
}

// D3.2b-4 (#118): serialization lives in the codec only, on its own wire types. No other lifecycle file names a serde
// trait; nothing implements one by hand; and every derive that names one is on a private codec struct named for the
// wire (`W…`, `WireOut`, `WireRecord`) or the version peek (`Peek`), so no model, live, replay or reconciliation type
// can be written or read by serde. Returns what breaks those rules.
function serializationViolations(files: Record<string, string>): string[] {
  const violations: string[] = [];
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    if (file !== "lifecycle/codec.rs" && /\b(?:Serialize|Deserialize|Serializer|Deserializer|serde|serde_json)\b/.test(code)) violations.push(`${file}: names serialization`);
    if (/\bimpl\b[^{;]*\b(?:Serialize|Deserialize|Serializer|Deserializer|Visitor)\b/.test(code)) violations.push(`${file}: implements serialization by hand`);
    // A conditional attribute could derive or rename anything the rules below never see (review of #118).
    if (/#\s*\[\s*cfg_attr\b/.test(code)) violations.push(`${file}: a conditional attribute`);
  }
  const codec = rustCodeOnly(files["lifecycle/codec.rs"] ?? "");
  for (const m of codec.matchAll(/#\[derive\(([^)]*)\)\]\s*/g)) {
    if (!/\b(?:Serialize|Deserialize)\b/.test(m[1])) continue;
    const target = codec.slice((m.index ?? 0) + m[0].length).match(/^(?:#\[serde\(deny_unknown_fields\)\]\s*)?struct (\w+)/);
    if (!target || !/^(?:W[A-Z]\w*|WireOut|WireRecord|Peek)$/.test(target[1])) violations.push(`codec.rs: serialization derived for ${target?.[1] ?? "something other than a wire struct"}`);
  }
  return violations;
}

// D3.2b-4 (#118): replayed history is read-only and recovered bytes never feed it. Decoded frames, decoded streams and
// unverified frames are built only by the codec, with private fields; an unverified frame holds no event, and nothing
// but the codec names it or the scan that makes it. A replayed history is built once, by `replay`, has no derive, no
// visible field, no trait, no mutable access, and is named only by replay.rs and reconcile.rs. A recorded history is
// built from a replay only: `new` is test-only, the private `assemble` is called exactly four times (by `new` and by
// `from_replay`), only those three functions return one, and `Verified` is named once, in `from_replay`.
const CODEC_TYPES = ["Decoded", "DecodedFrame", "UnverifiedFrame"];
const RECORDED_NEW =
  "    #[cfg(test)]\n    pub(crate) fn new(projection: RunProjection, status: HistoryStatus, unrecorded_repairs: Vec<JournalRepair>) -> RecordedHistory {\n";
function replayViolations(files: Record<string, string>): string[] {
  const violations: string[] = [];
  const codec = rustCodeOnly(files["lifecycle/codec.rs"] ?? "");
  const replay = rustCodeOnly(files["lifecycle/replay.rs"] ?? "");
  const reconcile = files["lifecycle/reconcile.rs"] ?? "";
  const reconcileCode = rustCodeOnly(reconcile);
  for (const name of CODEC_TYPES) {
    const declared = [...codec.matchAll(new RegExp(`((?:#\\[[^\\]]*\\]\\s*)*)pub\\(crate\\) struct ${name} \\{([^}]*)\\}`, "g"))];
    if (declared.length !== 1) violations.push(`codec.rs: ${name} declared ${declared.length} times`);
    for (const [, attributes, body] of declared) {
      const allowed = name === "UnverifiedFrame" ? "#[derive(Clone, Debug, PartialEq, Eq)]" : "";
      if (attributes.trim() !== allowed) violations.push(`codec.rs: ${name} has ${attributes.trim()}`);
      if (/\bpub\b/.test(body)) violations.push(`codec.rs: ${name} has a visible field`);
      if (name === "UnverifiedFrame" && /\b(?:Event|EventBody|Decoded|DecodedFrame|RunProjection)\b/.test(body)) violations.push("codec.rs: an unverified frame holds an event");
    }
    if ((codec.match(structLiteral(name)) ?? []).length !== 1) violations.push(`codec.rs: ${name} built ${(codec.match(structLiteral(name)) ?? []).length} times`);
  }
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    for (const name of [...CODEC_TYPES, "ReplayedHistory"]) {
      if (new RegExp(`\\bimpl\\b[^{;]*\\bfor\\s+[(\\s]*(?:[\\w:]*::)?${name}\\b`).test(code)) violations.push(`${file}: a trait is implemented for ${name}`);
      if (new RegExp(`\\btype\\s+\\w+[^=;]*=[^;]*\\b${name}\\b`).test(code)) violations.push(`${file}: an alias of ${name}`);
      if (new RegExp(`\\b${name}\\s+as\\b`).test(code)) violations.push(`${file}: ${name} renamed`);
    }
    if (file !== "lifecycle/codec.rs") {
      for (const name of CODEC_TYPES) if (structLiteral(name).test(code)) violations.push(`${file}: builds a ${name}`);
      if (/\b(?:UnverifiedFrame|scan_after_corruption)\b/.test(code)) violations.push(`${file}: reads unverified frames`);
    }
    if (file !== "lifecycle/replay.rs" && structLiteral("ReplayedHistory").test(code)) violations.push(`${file}: builds a ReplayedHistory`);
    if (!["lifecycle/replay.rs", "lifecycle/reconcile.rs"].includes(file) && /\bReplayedHistory\b/.test(code)) violations.push(`${file}: names ReplayedHistory`);
    if (file !== "lifecycle/reconcile.rs" && /\bHistoryStatus::Verified\b/.test(code)) violations.push(`${file}: names a verified status`);
  }
  const replayed = [...replay.matchAll(/((?:#\[[^\]]*\]\s*)*)pub\(crate\) struct ReplayedHistory \{([^}]*)\}/g)];
  if (replayed.length !== 1 || replayed[0][1].trim() || /\bpub\b/.test(replayed[0][2])) violations.push("replay.rs: ReplayedHistory is not declared once, plain, with private fields");
  if ((replay.match(structLiteral("ReplayedHistory")) ?? []).length !== 1) violations.push("replay.rs: ReplayedHistory built more than once");
  if (/&\s*mut\s+self\b/.test(replay)) violations.push("replay.rs: mutable access");
  // No history is changed after it is built (review of #118): no mutable reference to one or mutable binding of one
  // anywhere, no assignment to their fields in their files, and only `replay` returns a replayed history.
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    if (/&\s*mut\s+(?:[\w:]*::)?(?:ReplayedHistory|RecordedHistory)\b/.test(code)) violations.push(`${file}: a mutable history`);
    if (/\bmut\s+\w+\s*:\s*(?:[\w:]*::)?(?:ReplayedHistory|RecordedHistory)\b/.test(code)) violations.push(`${file}: a mutable history binding`);
  }
  for (const [file, fields] of [
    ["lifecycle/replay.rs", "projection|fault|damage|boundaries|tail"],
    ["lifecycle/reconcile.rs", "projection|status|unrecorded_repairs|bytes"],
  ]) {
    // Spacing around `.` and parentheses around a field path do not hide a field (re-review of #118).
    const code = rustCodeOnly(files[file] ?? "")
      .replace(/\s*\.\s*/g, ".")
      .replace(/\(\s*((?:&\s*mut\s+)?[\w.]+)\s*\)/g, "$1");
    if (new RegExp(`\\.(?:${fields})\\s*(?:[-+*/|&^]|<<|>>)?=(?!=)`).test(code)) violations.push(`${file}: a history field assigned`);
    // Re-review of #118: nor changed through a mutating method, a mutable borrow of a field, or a mutable `self`, which
    // an untyped `let mut` binding would otherwise allow.
    const mutating = "take|replace|clear|push|pop|insert|remove|retain|truncate|drain|extend|append|get_or_insert\\w*|as_mut|iter_mut|sort\\w*|dedup\\w*|swap\\w*|fill|split_off|resize";
    if (new RegExp(`\\.(?:${fields})\\s*\\.\\s*(?:${mutating})\\b`).test(code)) violations.push(`${file}: a history field changed by a method`);
    if (new RegExp(`&\\s*mut\\s+[\\w.]*\\.(?:${fields})\\b`).test(code)) violations.push(`${file}: a history field borrowed mutably`);
    if (/\bmut\s+self\b/.test(code)) violations.push(`${file}: a mutable self`);
    if (/\bref\s+mut\b/.test(code)) violations.push(`${file}: a mutable reference by pattern`);
  }
  for (const m of replay.matchAll(/\bfn\s+(\w+)([^{;]*)[{;]/g)) {
    const arrow = m[2].lastIndexOf("->");
    if (arrow >= 0 && /\bReplayedHistory\b/.test(m[2].slice(arrow)) && m[1] !== "replay") violations.push(`replay.rs: ${m[1]} returns a replayed history`);
  }
  if (reconcile.split(RECORDED_NEW).length !== 2) violations.push("reconcile.rs: RecordedHistory::new is not test-only");
  if ((reconcileCode.match(/\bRecordedHistory::assemble\(/g) ?? []).length !== 4) violations.push("reconcile.rs: assemble called other than by new and from_replay");
  if (!/\n {4}fn assemble\(/.test(reconcileCode) || /\bpub(?:\([^)]*\))?\s+fn assemble\b/.test(reconcileCode)) violations.push("reconcile.rs: assemble is not private");
  for (const m of reconcileCode.matchAll(/\bfn\s+(\w+)([^{;]*)[{;]/g)) {
    const arrow = m[2].lastIndexOf("->");
    if (arrow >= 0 && /\bRecordedHistory\b/.test(m[2].slice(arrow)) && !["new", "assemble", "from_replay"].includes(m[1])) violations.push(`reconcile.rs: ${m[1]} returns a recorded history`);
  }
  if ((reconcileCode.match(/\bHistoryStatus::Verified\b/g) ?? []).length !== 1) violations.push("reconcile.rs: a verified status named outside from_replay");
  return violations;
}

// D3.2b-2 (#114): live authority exists only in memory, in values that cannot be copied, formatted, serialized or
// built outside live.rs, and a draft's producer is fixed by its typed constructor. Returns what breaks those rules.
// D3.2c-2 (#122): a write outcome and torn-tail evidence are live values too: built once each, by `verify_write` and
// `discard`, never copied, formatted or named outside live.rs.
const LIVE_TYPES = ["LiveGrant", "LiveSupervision", "PendingSupervision", "Staged", "Abandoned", "WriteOutcome", "TornTailEvidence"];
const EVIDENCE_IMPLS: [string, string][] = [
  ["WriteOutcome", "impl WriteOutcome {\n    pub(crate) const fn check(&self) -> PersistCheck {\n        self.check\n    }\n}"],
  [
    "TornTailEvidence",
    "impl TornTailEvidence {\n    pub(crate) const fn len(&self) -> u32 {\n        self.owed.len\n    }\n\n    pub(crate) const fn crc32c(&self) -> u32 {\n        self.owed.crc32c\n    }\n}",
  ],
];
const REVIEWED_FAULT_ASSIGNMENTS = [
  "capacity_fault = live.stated(size).exhausted()",
  "capacity_fault = true",
  "clock_fault = true",
  "poisoned = true",
  "uncertain = 0",
  "uncertain = self.uncertain.saturating_add(1)",
  "owed = None",
  "owed = Some(owed)",
  "checked = None",
  "checked = Some(check)",
];
// `Name {` that builds a value: not a declaration, an impl block or a return type.
const structLiteral = (name: string) => new RegExp(`(?<!(?:\\bstruct|\\bimpl|->|\\blet)\\s*)\\b${name}\\s*\\{`, "g");
function liveAuthorityViolations(files: Record<string, string>): string[] {
  const violations: string[] = [];
  const live = rustCodeOnly(files["lifecycle/live.rs"] ?? "");
  for (const name of LIVE_TYPES) {
    // Declared once, with no attribute other than #[must_use] in front, no crate-visible field, and no trait impl.
    const declared = [...live.matchAll(new RegExp(`((?:#\\[[^\\]]*\\]\\s*)*)pub\\(crate\\) struct ${name}\\b([^{;]*)\\{([^}]*)\\}`, "g"))];
    if (declared.length !== 1) violations.push(`live.rs: ${name} declared ${declared.length} times`);
    for (const [, attributes, , body] of declared) {
      if (attributes.replace(/#\[must_use\]/g, "").trim()) violations.push(`live.rs: ${name} has ${attributes.trim()}`);
      if (/\bpub\b/.test(body)) violations.push(`live.rs: ${name} has a visible field`);
    }
    for (const [file, text] of Object.entries(files)) {
      const code = rustCodeOnly(text);
      if (new RegExp(`\\bimpl\\b[^{;]*\\bfor\\s+[(\\s]*(?:[\\w:]*::)?${name}\\b`).test(code)) violations.push(`${file}: a trait is implemented for ${name}`);
      if (file !== "lifecycle/live.rs" && new RegExp(`\\b${name}\\b`).test(code)) violations.push(`${file}: names ${name}`);
    }
  }
  // A grant is built in one place (mint), a supervision and a pending supervision in one place each (the private
  // `supervision` and `pending` helpers that begin_new and begin_reconciled share).
  for (const name of LIVE_TYPES) {
    const literals = live.match(structLiteral(name)) ?? [];
    if (literals.length !== 1) violations.push(`live.rs: ${name} built ${literals.length} times`);
  }
  // No other name for these types or for EventDraft (an alias could be built, returned or given a trait), and no
  // value built through `Self`: every construction site is a named literal counted below (review of #114).
  const guarded = [...LIVE_TYPES, "EventDraft"].join("|");
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    if (new RegExp(`\\btype\\s+\\w+[^=;]*=[^;]*\\b(?:${guarded})\\b`).test(code)) violations.push(`${file}: an alias of a guarded type`);
    if (new RegExp(`\\b(?:${guarded})\\s+as\\b`).test(code)) violations.push(`${file}: a guarded type renamed`);
    if (/\bSelf\s*\{/.test(code)) violations.push(`${file}: a value built through Self`);
  }
  // Only stage (and its private check) takes a grant; only confirm returns grants, through its private settle and mint.
  for (const m of live.matchAll(/\bfn\s+(\w+)([^{;]*)[{;]/g)) {
    const [, fn, signature] = m;
    const arrow = signature.lastIndexOf("->");
    const params = arrow < 0 ? signature : signature.slice(0, arrow);
    const returns = arrow < 0 ? "" : signature.slice(arrow);
    // D3.2c-2: `stage` hands its grant to the private `stage_frame`, which `stage_repair` calls with none.
    if (/LiveGrant/.test(params) && !["stage", "stage_frame", "check_grant"].includes(fn)) violations.push(`live.rs: ${fn} takes a grant`);
    if (/LiveGrant/.test(returns) && !["confirm", "settle", "mint"].includes(fn)) violations.push(`live.rs: ${fn} returns a grant`);
    // D3.2c-2: only verify_write makes an outcome, which only adopt and discard (and their private check) take; only
    // discard makes torn-tail evidence, which only stage_repair takes.
    if (/\bWriteOutcome\b/.test(returns) && fn !== "verify_write") violations.push(`live.rs: ${fn} returns a write outcome`);
    if (/\bWriteOutcome\b/.test(params) && !["adopt", "discard", "outcome_for"].includes(fn)) violations.push(`live.rs: ${fn} takes a write outcome`);
    if (/\bTornTailEvidence\b/.test(returns) && fn !== "discard") violations.push(`live.rs: ${fn} returns torn-tail evidence`);
    if (/\bTornTailEvidence\b/.test(params) && fn !== "stage_repair") violations.push(`live.rs: ${fn} takes torn-tail evidence`);
  }
  // Review of #122: every mention of stage_frame is its private declaration or one of the two reviewed calls, so no
  // path (`Self::stage_frame`, a visible wrapper) stages a journal draft as a repair.
  const frameMentions = (live.match(/\bstage_frame\b/g) ?? []).length;
  const frameDeclaration = (live.match(/\n {4}fn stage_frame\(&mut self, drafts: &\[EventDraft\], grant: Option<LiveGrant>, mono: MonoMs, size: JournalSize, repair: bool\)/g) ?? []).length;
  if (frameMentions !== 3 || frameDeclaration !== 1 || !live.includes("self.stage_frame(drafts, grant, mono, size, false)") || !live.includes("self.stage_frame(&[draft], None, mono, size, true)")) {
    violations.push("live.rs: stage_frame is not private, or is called other than by stage and, without a grant, by stage_repair");
  }
  // The evidence types expose read-only accessors only: one reviewed impl block each, so no setter can turn an Absent
  // outcome into Landed.
  for (const [name, block] of EVIDENCE_IMPLS) {
    if ((live.match(new RegExp(`\\bimpl\\b[^{;]*\\b${name}\\b[^{;]*\\{`, "g")) ?? []).length !== 1 || live.split(block).length !== 2) {
      violations.push(`live.rs: ${name} has methods other than the reviewed accessors`);
    }
  }
  // Fault states only ever enter (review of #122): poisoned, clock_fault and capacity_fault are only set, the repair
  // owed and the uncertain count change only at their reviewed sites, and nothing takes, swaps or borrows them.
  const assignments = [...live.matchAll(/\.(poisoned|capacity_fault|clock_fault|uncertain|owed|checked)\s*=(?![=>])\s*([^;]*);/g)].map((m) => `${m[1]} = ${m[2].trim()}`);
  for (const assignment of assignments) if (!REVIEWED_FAULT_ASSIGNMENTS.includes(assignment)) violations.push(`live.rs: ${assignment}`);
  if (/\.(?:poisoned|capacity_fault|clock_fault|uncertain|owed|checked)\s*\.\s*(?:take|replace|get_or_insert\w*|as_mut|insert)\b/.test(live) || /&\s*mut\s+[\w.]*\.(?:poisoned|capacity_fault|clock_fault|uncertain|owed|checked)\b/.test(live) || /\bmem::/.test(live)) {
    violations.push("live.rs: a fault state changed other than by assignment");
  }
  // SupervisedRun (review of #116) borrows an opened supervision: declared once in live.rs with no attribute and no
  // visible field, built once (by `held`), with no trait.
  const supervisedRuns = [...live.matchAll(/((?:#\[[^\]]*\]\s*)*)pub\(crate\) struct SupervisedRun<'a> \{([^}]*)\}/g)];
  if (supervisedRuns.length !== 1 || supervisedRuns[0][1].trim() || /\bpub\b/.test(supervisedRuns[0][2])) violations.push("live.rs: SupervisedRun is not the reviewed declaration");
  if ((live.match(structLiteral("SupervisedRun")) ?? []).length !== 1) violations.push("live.rs: SupervisedRun built more than once");
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    if (/\bimpl\b[^{;]*\bfor\s+[(\s]*(?:[\w:]*::)?SupervisedRun\b/.test(code)) violations.push(`${file}: a trait is implemented for SupervisedRun`);
    if (file !== "lifecycle/live.rs" && structLiteral("SupervisedRun").test(code)) violations.push(`${file}: builds a SupervisedRun`);
  }
  // Opening (D3.2b-3) holds a supervision: declared once in live.rs with no attribute, no trait, named nowhere else.
  const openings = [...live.matchAll(/((?:#\[[^\]]*\]\s*)*)pub\(crate\) enum Opening\b/g)];
  if (openings.length !== 1 || openings[0][1].trim()) violations.push("live.rs: Opening is not declared once without attributes");
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    if (/\bimpl\b[^{;]*\bfor\s+[(\s]*(?:[\w:]*::)?Opening\b/.test(code)) violations.push(`${file}: a trait is implemented for Opening`);
    if (file !== "lifecycle/live.rs" && /\bOpening\b/.test(code)) violations.push(`${file}: names Opening`);
  }
  // EventDraft: private fields, built by one private constructor, which the typed constructors (and the test-only
  // forging helper) call.
  const model = rustCodeOnly(files["lifecycle/model.rs"] ?? "");
  const draft = model.match(/pub\(crate\) struct EventDraft \{([^}]*)\}/);
  if (!draft || /\bpub\b/.test(draft[1])) violations.push("model.rs: EventDraft fields are visible");
  if ((model.match(structLiteral("EventDraft")) ?? []).length !== 1) violations.push("model.rs: EventDraft built outside its constructor");
  if (!/#\[cfg\(test\)\]\s*pub\(crate\) fn forged\b/.test(model)) violations.push("model.rs: forged is not test-only");
  for (const [file, text] of Object.entries(files)) {
    if (file !== "lifecycle/model.rs" && structLiteral("EventDraft").test(rustCodeOnly(text))) violations.push(`${file}: builds an EventDraft`);
  }
  return violations;
}

// D3.2b-3 (#116): reconciliation output is inert history. A recommendation is plain data that is built and pushed,
// never consumed: no method, no trait beyond its pinned derives, no alias, no match, no function taking one, and no
// other file naming it. The list that holds them is a private field read only by a test-only accessor, and no other
// production file names `recommendations` at all (review of #116: `r.recommendations.len()` would otherwise be a
// consumer that names no recommendation). The opening frame and the recorded history are built in one place each, with private fields,
// no derive and no trait, so a frame cannot be forged or reused by the live layer. Returns what breaks those rules.
const RECONCILE_TYPES = ["ReconciledFrame", "RecordedHistory"];
// D3.2c-2 (#122): a never-created proof is built once, by `never_created`, from a replay, a mark and the marker; its
// derives are the ones a summary needs, and its field is private.
const NEVER_CREATED_DECLARATION = "#[derive(Clone, Debug, PartialEq, Eq)]\npub(crate) struct NeverCreatedRun {\n    run: RunId,\n}";
const RECOMMENDATIONS_ACCESSOR =
  "#[cfg(test)]\n    pub(crate) fn recommendations(&self) -> &[Recommendation] {\n        &self.recommendations\n    }";
const INERT_TYPES = ["Recommendation", "ProposalBasis", "PolicyRequirement"];
function reconciliationViolations(files: Record<string, string>): string[] {
  const violations: string[] = [];
  const reconcile = rustCodeOnly(files["lifecycle/reconcile.rs"] ?? "");
  if (reconcile.split(RECOMMENDATION_DECLARATION).length !== 2) violations.push("reconcile.rs: Recommendation is not the reviewed declaration");
  for (const data of PROPOSAL_DATA) if (reconcile.split(data).length !== 2) violations.push("reconcile.rs: a proposal type is not the reviewed declaration");
  // Every other mention of Recommendation is the one private field that holds them, a value pushed onto it, or the
  // test-only accessor; every mention of that field is its declaration, the local list, a push, the struct literal or
  // the accessor.
  const mentions = (reconcile.match(/\bRecommendation\b/g) ?? []).length;
  const pushes = (reconcile.match(/\brecommendations\.push\(Recommendation::\w+\b/g) ?? []).length;
  const field = (reconcile.match(/\n {4}recommendations: Vec<Recommendation>,\n/g) ?? []).length;
  const accessor = reconcile.split(RECOMMENDATIONS_ACCESSOR).length - 1;
  const local = (reconcile.match(/\blet mut recommendations = Vec::new\(\);/g) ?? []).length;
  const literal = (reconcile.match(/\bReconciliation \{ frame, findings, recommendations, blocking, informational \}/g) ?? []).length;
  const uses = (reconcile.match(/\brecommendations\b/g) ?? []).length;
  if (field !== 1 || accessor !== 1 || local !== 1 || literal !== 1 || mentions !== 3 + pushes || uses !== 5 + pushes) {
    violations.push("reconcile.rs: a recommendation is used other than built and pushed");
  }
  for (const m of reconcile.matchAll(/\bfn\s+(\w+)([^{;]*)[{;]/g)) {
    const arrow = m[2].lastIndexOf("->");
    const params = arrow < 0 ? m[2] : m[2].slice(0, arrow);
    if (/\b(?:Recommendation|ProposalBasis|PolicyRequirement)\b/.test(params)) violations.push(`reconcile.rs: ${m[1]} takes a recommendation`);
  }
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    for (const name of [...INERT_TYPES, ...RECONCILE_TYPES]) {
      if (new RegExp(`\\bimpl\\b[^{;]*\\bfor\\s+[(\\s]*(?:[\\w:]*::)?${name}\\b`).test(code)) violations.push(`${file}: a trait is implemented for ${name}`);
      if (new RegExp(`\\btype\\s+\\w+[^=;]*=[^;]*\\b${name}\\b`).test(code)) violations.push(`${file}: an alias of ${name}`);
      if (new RegExp(`\\b${name}\\s+as\\b`).test(code)) violations.push(`${file}: ${name} renamed`);
    }
    if (file !== "lifecycle/reconcile.rs" && /\brecommendations\b/.test(code)) violations.push(`${file}: reads recommendations`);
    for (const name of INERT_TYPES) {
      if (new RegExp(`\\bimpl\\s+${name}\\b`).test(code)) violations.push(`${file}: a method on ${name}`);
      if (file !== "lifecycle/reconcile.rs" && new RegExp(`\\b${name}\\b`).test(code)) violations.push(`${file}: names ${name}`);
    }
    if (file !== "lifecycle/reconcile.rs") {
      for (const name of RECONCILE_TYPES) if (structLiteral(name).test(code)) violations.push(`${file}: builds a ${name}`);
    }
  }
  if (reconcile.split(NEVER_CREATED_DECLARATION).length !== 2) violations.push("reconcile.rs: NeverCreatedRun is not the reviewed declaration");
  if ((reconcile.match(structLiteral("NeverCreatedRun")) ?? []).length !== 1) violations.push("reconcile.rs: NeverCreatedRun built more than once");
  for (const m of reconcile.matchAll(/\bfn\s+(\w+)([^{;]*)[{;]/g)) {
    if (/\bNeverCreatedRun\b/.test(m[2])) violations.push(`reconcile.rs: ${m[1]} takes or returns a never-created proof`);
  }
  for (const [file, text] of Object.entries(files)) {
    const code = rustCodeOnly(text);
    if (new RegExp(`\\bimpl\\b[^{;]*\\bfor\\s+[(\\s]*(?:[\\w:]*::)?NeverCreatedRun\\b`).test(code)) violations.push(`${file}: a trait is implemented for NeverCreatedRun`);
    if (/\btype\s+\w+[^=;]*=[^;]*\bNeverCreatedRun\b/.test(code) || /\bNeverCreatedRun\s+as\b/.test(code)) violations.push(`${file}: NeverCreatedRun renamed`);
    if (file !== "lifecycle/reconcile.rs" && /\bNeverCreatedRun\b/.test(code)) violations.push(`${file}: names NeverCreatedRun`);
  }
  for (const name of RECONCILE_TYPES) {
    const declared = [...reconcile.matchAll(new RegExp(`((?:#\\[[^\\]]*\\]\\s*)*)pub\\(crate\\) struct ${name}\\b([^{;]*)\\{([^}]*)\\}`, "g"))];
    if (declared.length !== 1) violations.push(`reconcile.rs: ${name} declared ${declared.length} times`);
    for (const [, attributes, , body] of declared) {
      if (attributes.trim()) violations.push(`reconcile.rs: ${name} has ${attributes.trim()}`);
      if (/\bpub\b/.test(body)) violations.push(`reconcile.rs: ${name} has a visible field`);
    }
    if ((reconcile.match(structLiteral(name)) ?? []).length !== 1) violations.push(`reconcile.rs: ${name} built more than once`);
  }
  return violations;
}

describe("desktop trust boundary", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json"));

  it("grants the main window exactly ten app permissions and no other", () => {
    const dir = join(root, "src-tauri", "capabilities");
    const files = readdirSync(dir).filter((f) => f.endsWith(".json"));
    expect(files).toEqual(["default.json"]);
    const cap = JSON.parse(readFileSync(join(dir, "default.json"), "utf8"));
    // No shell, fs, network, opener or core plugin permission, and no remote origin.
    expect(cap.permissions).toEqual([
      "allow-ipc-boundary-status",
      "allow-get-runtime-status",
      "allow-list-projects",
      "allow-add-project",
      "allow-remove-project",
      "allow-reset-projects",
      "allow-list-tasks",
      "allow-get-task",
      "allow-create-task",
      "allow-update-task",
    ]);
    expect(cap.windows).toEqual(["main"]);
    expect(cap.remote).toBeUndefined();
    expect(cap.webviews).toBeUndefined();
  });

  it("uses a CSP with no remote origin and no eval", () => {
    const csp: string = conf.app.security.csp;
    expect(csp).toContain("default-src 'self'");
    // Tauri's own IPC alias is the one http origin Tauri requires; it is not a remote host.
    expect(csp.replace("http://ipc.localhost", "")).not.toMatch(/https?:|wss?:|\*|unsafe-eval|unsafe-inline/);
    expect(csp).toContain("object-src 'none'");
  });

  it("does not expose the Tauri API globally or load remote pages", () => {
    expect(conf.app.withGlobalTauri ?? false).toBe(false);
    expect(conf.build.devUrl).toMatch(/^http:\/\/localhost:/);
  });

  it("pins the Cargo dependencies structurally, including target-specific ones", () => {
    const cargo = read("src-tauri/Cargo.toml");
    expect(cargo).not.toMatch(/tauri-plugin|sidecar/i);
    // The exact set of direct dependencies (every table, quoted or dotted key, rename, target cfg, features) is
    // pinned from `cargo metadata` by the Rust tests in src-tauri/src/lib.rs, together with mutation fixtures for
    // the spellings a line-based check misses. They must stay in the suite.
    const lib = read("src-tauri/src/lib.rs");
    expect(lib).toContain("fn direct_dependencies_are_exactly_the_reviewed_set()");
    expect(lib).toContain("fn the_dependency_check_sees_every_manifest_spelling()");
    // The native folder panel, macOS only, without its Linux-only default features (xdg-portal, wayland).
    expect(cargo).toMatch(/^rfd = \{ version = "0\.17\.2", default-features = false \}$/m);
    expect(read("src-tauri/Cargo.lock")).toMatch(/^name = "rfd"\nversion = "0\.17\.2"$/m);
  });

  it("registers no Tauri plugins or sidecars and exactly ten declared app commands", () => {
    const cargo = read("src-tauri/Cargo.toml");
    expect(cargo).not.toMatch(/tauri-plugin|sidecar/i);
    // The development bridge is opt-in: dev-runtime is the only feature and nothing enables it by default.
    expect(cargo.split("[features]")[1].match(/^[a-z-]+(?= *=)/gm)).toEqual(["dev-runtime"]);
    expect(cargo.split("[features]")[1]).toMatch(/^dev-runtime = \[\]$/m);
    // Production code is everything before the Rust test module.
    const lib = read("src-tauri/src/lib.rs");
    const rust = lib.slice(0, lib.indexOf("#[cfg(test)]"));
    // The only process-related call here is the blocking-worker hop; starting Python lives in the
    // cfg-gated runtime_bridge/process.rs.
    expect(rust.replaceAll("spawn_blocking", "")).not.toMatch(/\.plugin\(|Command::|std::process|std::fs|std::env|std::net|tokio|reqwest|spawn/);
    expect(rust.match(/invoke_handler\(/g)).toHaveLength(1);
    const names = (list: string | undefined) => list?.split(",").map((n) => n.trim().replaceAll('"', "")).filter(Boolean);
    expect(names(rust.match(/generate_handler!\[([^\]]*)\]/)?.[1])).toEqual(COMMANDS);
    expect(rust.match(/#\[tauri::command(?:\([^)]*\))?\]/g)).toHaveLength(10);
    expect(rust.match(/#\[tauri::command\([^)]*\)\]/g)).toEqual(['#[tauri::command(rename_all = "snake_case")]']);
    // The project commands take no path: add_project, list_projects and reset_projects take no argument from
    // the webview, and remove_project takes only the registration id.
    expect(rust).toMatch(/async fn list_projects\(projects: State<'_, Projects>\)/);
    expect(rust).toMatch(/async fn add_project<R: Runtime>\(\s*window: WebviewWindow<R>,\s*projects: State<'_, Projects>,\s*picker: State<'_, Picker<R>>,\s*\)/);
    expect(rust).toMatch(/async fn remove_project\(id: String, projects: State<'_, Projects>\)/);
    expect(rust).toMatch(/async fn reset_projects\(projects: State<'_, Projects>\)/);
    // The task commands take text, ids and a revision, never a path; create_task keeps the documented snake_case keys.
    expect(rust).toMatch(/async fn list_tasks\(tasks: State<'_, Tasks>\)/);
    expect(rust).toMatch(/async fn get_task\(id: String, tasks: State<'_, Tasks>\)/);
    expect(rust).toMatch(
      /#\[tauri::command\(rename_all = "snake_case"\)\]\nasync fn create_task\(\s*project_id: String,\s*expected_registered_at_ms: u64,\s*title: String,\s*description: String,\s*tasks: State<'_, Tasks>,\s*\)/,
    );
    expect(rust).toMatch(/async fn update_task\(\s*id: String,\s*revision: u64,\s*title: String,\s*description: String,\s*status: String,\s*tasks: State<'_, Tasks>,\s*\)/);
    const bridge = read("src-tauri/src/runtime_bridge.rs");
    expect(bridge).not.toMatch(/Command::|std::process|std::fs|std::env/);
    expect(bridge).toMatch(/#\[cfg\(any\(test, all\(feature = "dev-runtime", target_os = "macos"\)\)\)\]\s*mod process;/);
    // The app manifest is what makes Tauri enforce the capability for local-origin app commands.
    const build = read("src-tauri/build.rs");
    expect(names(build.match(/AppManifest::new\(\)\s*\.commands\(&\[([^\]]*)\]\)/)?.[1])).toEqual(COMMANDS);
    expect(conf.bundle.externalBin).toBeUndefined();
  });

  // Registration is a bookmark: the registry module looks at folder metadata and its own files, nothing else.
  it("keeps the project registry away from folder contents, processes and the network", () => {
    const files = projectSources();
    expect(Object.keys(files).sort()).toEqual(["projects.rs", "projects/folder.rs", "projects/registry.rs", "store.rs", "tasks.rs", "tasks/records.rs"]);
    expect(projectContentViolations(files)).toEqual([]);
    const production = (p: string) => {
      const text = read(p);
      // These files end with their test module declaration; tests live in their own files.
      return /(?:registry|records|store)\.rs$/.test(p) ? text.slice(0, text.lastIndexOf("#[cfg(test)]\nmod tests;")) : text;
    };
    // The registry's files are opened only in the store's four functions, and only under the registry's own fixed
    // names: the store holds no name of its own and receives only FILES. The registry itself only links the
    // corrupt file to its backup name and then removes the registry name (reset).
    const registry = production("src-tauri/src/projects/registry.rs");
    const store = production("src-tauri/src/store.rs");
    const functions = [...registry.matchAll(/^(?:pub\(super\) )?fn (\w+)[\s\S]*?^\}/gm)];
    const storeFunctions = [...store.matchAll(/^(?:pub\(crate\) )?fn (\w+)[\s\S]*?^\}/gm)];
    expect(storeFunctions.filter((f) => /OpenOptions/.test(f[0])).map((f) => f[1])).toEqual(["open_read", "open_lock", "create_temp", "open_dir"]);
    expect(store.match(/OpenOptions::new\(\)/g)).toHaveLength(4);
    expect(store.match(/remove_file\([^)]*\)/g)).toEqual(["remove_file(&temp)", "remove_file(&temp)"]);
    expect(store.match(/rename\([^)]*\)/g)).toEqual(["rename(&temp, dir.join(files.file)"]);
    expect(store.match(/hard_link/g)).toBeNull();
    expect(store.match(/DirBuilder::new\(\)[^\n]*/g)).toEqual(["DirBuilder::new().mode(0o700).create(dir) {"]);
    expect(store.match(/\bjoin\([^)]*\)/g)).toEqual(["join(files.file)", "join(files.lock)", "join(files.temp)", "join(files.file)"]);
    expect(store).not.toMatch(/const \w+: &str|"[^"]*\.(?:json|lock|tmp)"/);
    expect(registry).toMatch(/^static FILES: Files = Files \{ file: REGISTRY, lock: LOCK, temp: TEMP, writer: &WRITER \};$/m);
    expect(registry).toMatch(/^impl Store for Registry \{\n {4}fn files\(\) -> &'static Files \{\n {8}&FILES\n {4}\}\n\}$/m);
    expect(registry.match(/store::\w+(?:::<\w+>)?\([^)]*\)/g)).toEqual([
      "store::read::<Registry>(dir, MAX_BYTES)",
      "store::lock::<Registry>(dir, create)",
      "store::replace(locked, &bytes)",
    ]);
    expect(registry).toMatch(/^fn commit\(locked: &store::Locked<Registry>, stored: &Stored\)/m);
    // The registry never depends on the task store; the dependency runs the other way, through registrations().
    expect(registry).not.toMatch(/crate::tasks|TaskStore|tasks::/);
    // The guard: only lock() builds one, its fields are private, and a replace takes the guard, not a directory.
    expect(store).toMatch(/^pub\(crate\) struct Locked<S: Store> \{\n {4}dir: PathBuf,\n {4}_file: File,\n {4}_writer: MutexGuard<'static, \(\)>,\n {4}_store: PhantomData<fn\(\) -> S>,\n\}$/m);
    expect(store.match(/\bLocked \{/g)).toEqual(["Locked {"]);
    expect(store).toMatch(/return Ok\(Some\(Locked \{ dir: dir\.to_path_buf\(\), _file: file, _writer: writer, _store: PhantomData \}\)\);/);
    expect(store).toMatch(/^pub\(crate\) fn replace<S: Store>\(locked: &Locked<S>, bytes: &\[u8\]\) -> Result<\(\), Unavailable> \{\n {4}let \(dir, files\) = \(locked\.dir\.as_path\(\), S::files\(\)\);/m);
    expect(store).not.toMatch(/#\[derive\([^)]*\)\]\npub\(crate\) struct Locked/);
    expect(registry.match(/OpenOptions|DirBuilder|rename\(/g)).toBeNull();
    expect(registry.match(/remove_file\([^)]*\)/g)).toEqual(["remove_file(&registry)"]);
    expect(registry.match(/hard_link\([^)]*\)/g)).toEqual(["hard_link(registry, &backup)"]);
    // Criterion 21's exception: one listing, of the directory backup_present was given, reading entry names only
    // and comparing them with the fixed backup prefix. It opens, stats, reads, renames and removes no entry, and
    // its only caller is the import's precedence check.
    expect(registry.match(/read_dir/g)).toEqual(["read_dir"]);
    expect(functions.filter((f) => /read_dir/.test(f[0])).map((f) => f[1])).toEqual(["backup_present"]);
    const scan = functions.find((f) => f[1] === "backup_present")?.[0] ?? "";
    expect(scan).toContain(REVIEWED_LISTING);
    expect(scan).toContain("entry.file_name().as_bytes().starts_with(BACKUP_PREFIX.as_bytes())");
    expect(scan).not.toMatch(/OpenOptions|open_\w+\(|\.metadata\(|file_type|\.path\(\)|remove_|rename|hard_link|read_to|fs::read\b/);
    expect(registry.match(/\bbackup_present\([^)]*\)/g)).toEqual(["backup_present(dir)", "backup_present(dir: &Path)"]);
    expect(functions.filter((f) => /\bbackup_present\(dir\)/.test(f[0])).map((f) => f[1])).toEqual(["occupied"]);
    expect(registry).toMatch(/^const REGISTRY: &str = "projects\.json";$/m);
    expect(registry).toMatch(/^const LOCK: &str = "projects\.lock";$/m);
    expect(registry).toMatch(/^const TEMP: &str = "projects\.json\.tmp";$/m);
    expect(registry).toMatch(/^const BACKUP_PREFIX: &str = "projects\.json\.corrupt-";$/m);
    // Failure injection and the pre-commit test hook are compiled out of production builds.
    expect(store).toMatch(/#\[cfg\(not\(test\)\)\]\npub\(crate\) fn fault\(_: Step\) -> std::io::Result<\(\)> \{\n {4}Ok\(\(\)\)\n\}/);
    expect(store.match(/^#\[cfg\(test\)\]\n(?:thread_local!|pub\(crate\) fn \w+)/gm)).toEqual([
      "#[cfg(test)]\nthread_local!",
      "#[cfg(test)]\npub(crate) fn arm",
      "#[cfg(test)]\npub(crate) fn fault",
    ]);
    expect(store.match(/FAULTS/g)).toHaveLength(3);
    expect(store.match(/tests::/g)).toBeNull();
    expect(registry.match(/tests::\w+\(/g)).toEqual(["tests::before_recheck(", "tests::after_backup_scan("]);
    expect(registry).toMatch(/ {4}#\[cfg\(test\)\]\n {4}tests::after_backup_scan\(\);\n/);
    expect(registry).toMatch(/ {4}#\[cfg\(test\)\]\n {4}tests::before_recheck\(\);\n {4}folder::recheck\(&candidate\)\?;/);
    // The registry and the panel are macOS-only; other targets answer unsupported_platform.
    const projects = read("src-tauri/src/projects.rs");
    expect(projects).toMatch(/#\[cfg\(target_os = "macos"\)\]\nmod folder;\n#\[cfg\(target_os = "macos"\)\]\nmod registry;/);
    expect(read("src-tauri/src/lib.rs")).toMatch(/\nmod runtime_bridge;\n#\[cfg\(target_os = "macos"\)\]\nmod store;\n/);
    expect(projects.match(/rfd::/g)).toEqual(["rfd::"]);
    expect(projects).toMatch(/\.set_can_create_directories\(false\)/);
  });

  // A saved task is a record about work, not work: the task store reads the registry's ids and times, keeps its own
  // fixed files through the shared protocol, and has no file operation, folder access, recovery or deletion of its own.
  it("keeps the task store to its own files, the registry's ids and the approved schema", () => {
    const files = projectSources();
    const records = files["tasks/records.rs"].slice(0, files["tasks/records.rs"].lastIndexOf("#[cfg(test)]\nmod tests;"));
    const tasks = files["tasks.rs"];
    expect(records).toMatch(/^const TASKS: &str = "tasks\.json";$/m);
    expect(records).toMatch(/^const LOCK: &str = "tasks\.lock";$/m);
    expect(records).toMatch(/^const TEMP: &str = "tasks\.json\.tmp";$/m);
    expect(records).toMatch(/^static WRITER: Mutex<\(\)> = Mutex::new\(\(\)\);$/m);
    expect(records).toMatch(/^static FILES: Files = Files \{ file: TASKS, lock: LOCK, temp: TEMP, writer: &WRITER \};$/m);
    expect(records).toMatch(/^impl Store for TaskStore \{\n {4}fn files\(\) -> &'static Files \{\n {8}&FILES\n {4}\}\n\}$/m);
    expect(records.match(/store::\w+(?:::<\w+>)?\([^)]*\)/g)).toEqual([
      "store::read::<TaskStore>(dir, MAX_BYTES)",
      "store::lock::<TaskStore>(dir, create)",
      "store::replace(locked, &bytes)",
    ]);
    expect(records).toMatch(/^fn commit\(locked: &store::Locked<TaskStore>, stored: &Stored\)/m);
    // Exactly two stores in the whole crate, each implemented by the module that owns its files.
    const stores = Object.entries(files).flatMap(([f, t]) => [...t.matchAll(/impl Store for (\w+)/g)].map((m) => `${f}: ${m[1]}`));
    expect(stores.sort()).toEqual(["projects/registry.rs: Registry", "tasks/records.rs: TaskStore"]);
    // The registry is reached only through its read-only accessor; no folder, backup or recovery code is used.
    expect([...new Set([...records.matchAll(/crate::projects::(\w+)/g)].map((m) => m[1]))]).toEqual(["registrations"]);
    expect(records + tasks).not.toMatch(/folder::|registry::|symlink_metadata|canonicalize|hard_link|remove_file|BACKUP|corrupt-|fault\(|sync_dir|trusted_dir|std::fs|libc::/);
    expect([...records.matchAll(/^pub\(super\) fn (\w+)/gm)].map((m) => m[1])).toEqual(["list", "get", "create", "update"]);
    expect(records + tasks).not.toMatch(/fn (?:reset|delete|remove)\w*/);
    // Schema version 1: exactly the approved fields, with no path, plan, run or authority field, and two statuses.
    const fields = (text: string, name: string) =>
      text.match(new RegExp(`^(?:pub )?struct ${name} \\{\\n([\\s\\S]*?)^\\}`, "m"))?.[1].match(/^\s+(?:#\[[^\]]*\]\s+)?(\w+):/gm)?.map((f) => f.trim().replace(/^#\[[^\]]*\]\s+/, "").replace(":", ""));
    expect(fields(records, "Stored")).toEqual(["schema_version", "next_id", "tasks"]);
    expect(fields(records, "Record")).toEqual(["id", "project", "title", "description", "status", "created_at_ms", "updated_at_ms", "revision"]);
    expect(fields(records, "ProjectRef")).toEqual(["id", "registered_at_ms"]);
    expect(records.match(/#\[serde\(deny_unknown_fields\)\]\nstruct (\w+)/g)).toEqual([
      "#[serde(deny_unknown_fields)]\nstruct Stored",
      "#[serde(deny_unknown_fields)]\nstruct Record",
      "#[serde(deny_unknown_fields)]\nstruct ProjectRef",
    ]);
    expect(tasks).toMatch(/^pub enum Status \{\n {4}Draft,\n {4}Archived,\n\}$/m);
    expect(tasks).toMatch(/#\[cfg\(target_os = "macos"\)\]\nmod records;/);
    expect(read("src-tauri/src/lib.rs")).toMatch(/\nmod store;\nmod tasks;\n/);
  });

  // Which files compile is verified from rustc's dep-info by scripts/verify-native-boundary.mjs (with its own
  // regression fixtures in scripts/test-verify-native-boundary.sh); these fixtures cover the content rules.
  it("finds prohibited operations in any compiled project registry file", () => {
    const actual = projectSources();
    const add = (file: string, text: string) => ({ ...actual, [file]: actual[file] + "\n" + text });
    const bypasses: Record<string, Record<string, string>> = {
      "directory listing": add("projects/folder.rs", "pub fn f(p: &Path) { let _ = std::fs::read_dir(p); }"),
      "Path::read_dir": add("projects.rs", "fn f(p: &std::path::Path) { let _ = p.read_dir(); }"),
      "process": add("projects/registry.rs", 'fn g() { let _ = std::process::Command::new("git"); }'),
      "File::options outside the registry": add("projects/folder.rs", "fn h(p: &Path) { let _ = std::fs::File::options().read(true).open(p); }"),
      "raw libc open": add("projects/folder.rs", "fn i() { unsafe { libc::opendir(std::ptr::null()) }; }"),
      "aliased import": { ...actual, "projects/registry.rs": actual["projects/registry.rs"].replace("use std::collections::HashSet;", "use std::collections::HashSet;\nuse std::fs::OpenOptions as Opener;") },
      "network": add("projects.rs", 'fn j() { let _ = std::net::TcpStream::connect("127.0.0.1:1"); }'),
      "reading contents": add("projects/folder.rs", "fn k(p: &Path) { let _ = std::fs::read_to_string(p); }"),
      // The reviewed listing is exempt once, in registry.rs, with that exact argument; nothing else is.
      "a second listing in the registry": add("projects/registry.rs", "fn l(dir: &Path) { let _ = std::fs::read_dir(dir); }"),
      "the reviewed listing of another directory": {
        ...actual,
        "projects/registry.rs": actual["projects/registry.rs"].replace(REVIEWED_LISTING, "std::fs::read_dir(dir.parent().unwrap())"),
      },
      "the reviewed listing outside the registry": add("projects/folder.rs", "fn m(dir: &Path) { let _ = std::fs::read_dir(dir); }"),
      "a listing through Path in the registry": add("projects/registry.rs", "fn n(dir: &Path) { let _ = dir.read_dir(); }"),
      // The shared store is held to the same rules, and the exemption does not reach it.
      "the reviewed listing in the store": add("store.rs", "fn o(dir: &Path) { let _ = std::fs::read_dir(dir); }"),
      "a hard link in the store": add("store.rs", "fn p(a: &Path, b: &Path) { let _ = std::fs::hard_link(a, b); }"),
      "a process in the store": add("store.rs", 'fn q() { let _ = std::process::Command::new("sh"); }'),
      "an environment read in the store": add("store.rs", 'fn r() { let _ = std::env::var("HOME"); }'),
      // The task store has no file operation of its own, and no listing, folder look-up, process or network.
      "a file operation in the task store": add("tasks/records.rs", "fn s(p: &Path) { let _ = std::fs::remove_file(p); }"),
      "a folder look-up in the task store": add("tasks/records.rs", "fn t(p: &Path) { let _ = std::fs::symlink_metadata(p); }"),
      "a listing in the task store": add("tasks.rs", "fn u(p: &std::path::Path) { let _ = std::fs::read_dir(p); }"),
      "a process in the task store": add("tasks.rs", 'fn v() { let _ = std::process::Command::new("sh"); }'),
      "a network call in the task store": add("tasks/records.rs", 'fn w() { let _ = std::net::TcpStream::connect("127.0.0.1:1"); }'),
    };
    for (const [name, files] of Object.entries(bypasses)) {
      expect(projectContentViolations(files), name).not.toEqual([]);
    }
  });

  // D3.2a (#109, design #108): cataloguing a provider authorizes nothing. The catalog and evidence vocabulary are
  // compiled in, but no command, module or initializer reaches them, and they can neither find nor run anything.
  it("keeps the provider catalog and evidence vocabulary pure and unreachable from the interface", () => {
    const files = agentSources();
    expect(Object.keys(files).sort()).toEqual(["agents.rs", "agents/catalog.rs", "agents/evidence.rs"]);
    expect(agentContentViolations(files)).toEqual([]);
    // Each file ends with its test module declaration; tests live in their own files.
    for (const file of ["agents/catalog.rs", "agents/evidence.rs"]) expect(files[file]).toMatch(/\n#\[cfg\(test\)\]\nmod tests;\n$/);
    // lib.rs declares the module, silences dead-code warnings outside tests only, and names it nowhere else.
    const lib = read("src-tauri/src/lib.rs");
    const production = rustCodeOnly(lib.slice(0, lib.indexOf("#[cfg(test)]")));
    expect(production.match(/\bagents\b/g)).toEqual(["agents"]);
    expect(lib).toMatch(/\n#\[cfg_attr\(not\(test\), allow\(dead_code\)\)\]\nmod agents;\n#\[cfg_attr\(not\(test\), allow\(dead_code\)\)\]\nmod lifecycle;\nmod projects;\n/);
    // No other compiled production file uses it, except the lifecycle model's two reviewed imports (decision 12.8).
    const others = ["default", "release", "dev-runtime"]
      .flatMap((c) => nativeSources.configurations[c])
      .filter((f) => !f.startsWith("src/agents") && f !== "src/lib.rs");
    for (const file of new Set(others)) {
      const text = withoutLifecycleImports(file.slice("src/".length), read(`src-tauri/${file}`));
      expect(rustCodeOnly(text), file).not.toMatch(/\bagents\b/);
    }
    // The vocabulary has no field that could mean authorized, dispatched or enforced.
    const evidence = rustCodeOnly(files["agents/evidence.rs"]);
    expect(evidence).not.toMatch(/\b\w*(?:authori[sz]ed?|dispatch\w*|enforced|Enforced)\b/);
  });

  it("finds prohibited operations in the provider catalog and evidence vocabulary", () => {
    const actual = agentSources();
    const add = (file: string, text: string) => ({ ...actual, [file]: actual[file] + "\n" + text });
    const bypasses: Record<string, Record<string, string>> = {
      "a process": add("agents/catalog.rs", 'fn a() { let _ = std::process::Command::new("codex"); }'),
      "an imported process type": add("agents/evidence.rs", "use std::process::Command;"),
      "a file read": add("agents/catalog.rs", 'fn b() { let _ = std::fs::read("/opt/homebrew/bin/codex"); }'),
      "a path type": add("agents/catalog.rs", "struct C { at: PathBuf }"),
      "a network call": add("agents/evidence.rs", 'fn d() { let _ = std::net::TcpStream::connect("127.0.0.1:1"); }'),
      "an environment read": add("agents/catalog.rs", 'fn e() { let _ = std::env::var("PATH"); }'),
      "an environment macro": add("agents/catalog.rs", 'const F: &str = env!("HOME");'),
      "an included file": add("agents/catalog.rs", 'const G: &str = include_str!("/etc/paths");'),
      "a clock": add("agents/evidence.rs", "fn h() { let _ = std::time::SystemTime::now(); }"),
      "global mutable state": add("agents/evidence.rs", "static mut I: u8 = 0;"),
      "lazy initialization": add("agents/catalog.rs", "static J: OnceLock<u8> = OnceLock::new();"),
      "unsafe code": add("agents/evidence.rs", "fn k() { unsafe {} }"),
      "foreign code": add("agents/evidence.rs", 'extern "C" { fn getpid() -> i32; }'),
      "libc": add("agents/evidence.rs", "fn l() -> i32 { libc::getpid() }"),
      "a command": add("agents/catalog.rs", "#[tauri::command]\nfn m() {}"),
      "serialization for IPC": add("agents/catalog.rs", "#[derive(serde::Serialize)]\nstruct N;"),
      "another module of the crate": add("agents/evidence.rs", "use crate::store;"),
      "a public item": add("agents/catalog.rs", "pub fn o() {}"),
      "an argument vector": add("agents/catalog.rs", "struct P { argv: &'static [&'static str] }"),
      "a search location": add("agents/catalog.rs", "const SEARCH_PATHS: [&str; 0] = []; struct Q { candidates: u8 }"),
      "a raw string": add("agents/catalog.rs", 'const R: &str = r"--version";'),
      "another macro": add("agents/evidence.rs", 'fn s() { println!("x"); }'),
      "a module from another file": add("agents.rs", '#[path = "../store.rs"]\nmod t;'),
      // Independent review of #110: escapes into the rest of the crate, code behind a quote character, byte and C raw
      // strings, and compound identifiers.
      "the crate root through super in agents.rs": add("agents.rs", "pub(crate) fn v() { let _ = super::runtime_bridge::query(); }"),
      "a sibling module imported through super": add("agents.rs", "use super::store;"),
      "a crate path with spaces": add("agents/catalog.rs", "fn w() { let _ = crate :: runtime_bridge :: query(); }"),
      "the crate renamed": add("agents/catalog.rs", "use crate as root;"),
      "super of super renamed": add("agents/evidence.rs", "use super::super as root;"),
      "a root import through super": add("agents/evidence.rs", "#[derive(super::Serialize)]\nstruct X;"),
      "a root type through super": add("agents/evidence.rs", "fn y(_: super::State<'_, u8>) {}"),
      "code between quote characters": add("agents/evidence.rs", "fn z() { let _q = '\"'; let _ = std::process::Command::new(\"sh\"); let _r = '\"'; }"),
      "code between escaped quote characters": add("agents/catalog.rs", "fn aa() { let _q = b'\\\"'; let _ = std::fs::read(\"x\"); let _r = '\\\"'; }"),
      "a byte raw string": add("agents/catalog.rs", 'const AB: &[u8] = br"\\"; fn ac() { let _ = std::fs::read(br"\\"); }'),
      "a C raw string": add("agents/catalog.rs", 'const AD: &str = cr"x";'),
      "compound identifiers": add("agents/catalog.rs", "struct AE { executable_path: &'static str, launch_args: &'static [&'static str], search_dirs: u8 }"),
      "a command template": add("agents/catalog.rs", "const VERSION_COMMAND: [&str; 0] = [];"),
      // Re-review of #110: nested block comments, and other names for something to run.
      "code after a nested block comment": add("agents/evidence.rs", 'fn af() { /* /* */ " */ let _ = std::env::var(""); /* " */ }'),
      "a cmd field": add("agents/catalog.rs", "struct AG { cmd: u8 }"),
      "a program field": add("agents/catalog.rs", "struct AH { program: u8 }"),
      "an exec constant": add("agents/catalog.rs", 'const EXEC: [&str; 2] = ["exec", "--json"];'),
      "a binary field": add("agents/catalog.rs", "struct AI { binary: u8 }"),
      "a version flag": add("agents/catalog.rs", 'const VERSION_FLAG: &str = "--version";'),
      "an argument template": add("agents/catalog.rs", "struct AJ { template: u8 }"),
    };
    for (const [name, files] of Object.entries(bypasses)) {
      expect(agentContentViolations(files), name).not.toEqual([]);
    }
    // Documentation text is data: a string or a comment that mentions a process or a path is not code.
    const text = add("agents/catalog.rs", 'const U: &str = "std::process::Command::new(\\"codex\\") in /usr/bin"; // std::fs::read');
    expect(agentContentViolations(text)).toEqual([]);
  });

  // D3.2b-1 (#112, design #108): the execution lifecycle model is history, never authority. It is compiled in, but no
  // command, module or initializer reaches it, it can neither find, start nor observe anything, and it names the rest
  // of the crate only through two reviewed imports.
  it("keeps the lifecycle model pure, unreachable from the interface and limited to its reviewed imports", () => {
    const files = lifecycleSources();
    expect(Object.keys(files).sort()).toEqual([
      "lifecycle.rs",
      "lifecycle/codec.rs",
      "lifecycle/live.rs",
      "lifecycle/model.rs",
      "lifecycle/reconcile.rs",
      "lifecycle/reduce.rs",
      "lifecycle/replay.rs",
    ]);
    expect(lifecycleContentViolations(files)).toEqual([]);
    expect(liveAuthorityViolations(files)).toEqual([]);
    expect(reconciliationViolations(files)).toEqual([]);
    expect(serializationViolations(files)).toEqual([]);
    expect(replayViolations(files)).toEqual([]);
    // The reviewed termination-proposal exception applies: without it, the reconciliation file fails the rule.
    expect(withoutReviewedProposal("lifecycle/reconcile.rs", files["lifecycle/reconcile.rs"])).not.toEqual(files["lifecycle/reconcile.rs"]);
    expect(pureContentViolations({ "lifecycle/reconcile.rs": files["lifecycle/reconcile.rs"] }, () => /\bsuper\b(?!::(?:model|reduce|live|replay)\b)/)).toEqual([]);
    expect(rustCodeOnly(files["lifecycle/reconcile.rs"])).toMatch(/\w*terminat\w*/i);
    // The reviewed serde forms apply to the codec only: without them it fails the rules, and they change no other file.
    expect(pureContentViolations({ "lifecycle/codec.rs": files["lifecycle/codec.rs"] }, () => /\bsuper\b(?!::model\b)/)).not.toEqual([]);
    expect(withoutReviewedSerde("lifecycle/codec.rs", files["lifecycle/codec.rs"])).not.toEqual(files["lifecycle/codec.rs"]);
    for (const file of Object.keys(files).filter((f) => f !== "lifecycle/codec.rs")) expect(withoutReviewedSerde(file, files[file])).toEqual(files[file]);
    for (const file of ["lifecycle/model.rs", "lifecycle/live.rs", "lifecycle/replay.rs"]) expect(files[file]).toMatch(/\n#\[cfg\(test\)\]\nmod tests;\n$/);
    // The reducer's, the codec's and reconciliation's test fixtures (D3.2c-2: a never-created summary built through
    // `never_created`) are shared with other lifecycle tests, in test builds only.
    for (const file of ["lifecycle/reduce.rs", "lifecycle/codec.rs", "lifecycle/reconcile.rs"]) expect(files[file]).toMatch(/\n#\[cfg\(test\)\]\npub\(crate\) mod tests;\n$/);
    for (const line of LIFECYCLE_IMPORTS) expect(files["lifecycle/model.rs"].split("\n").filter((l) => l === line)).toHaveLength(1);
    // lib.rs declares the module once (pinned in the agents test above) and no other compiled production file names
    // it, the agents module included.
    const lib = read("src-tauri/src/lib.rs");
    expect(rustCodeOnly(lib.slice(0, lib.indexOf("#[cfg(test)]"))).match(/\blifecycle\b/g)).toEqual(["lifecycle"]);
    const others = ["default", "release", "dev-runtime"]
      .flatMap((c) => nativeSources.configurations[c])
      .filter((f) => !f.startsWith("src/lifecycle") && f !== "src/lib.rs");
    for (const file of new Set(others)) expect(rustCodeOnly(read(`src-tauri/${file}`)), file).not.toMatch(/\blifecycle\b/);
  });

  it("finds prohibited operations and unreviewed references in the lifecycle model", () => {
    const actual = lifecycleSources();
    const add = (file: string, text: string) => ({ ...actual, [file]: actual[file] + "\n" + text });
    const model = (from: string, to: string) => ({ ...actual, "lifecycle/model.rs": actual["lifecycle/model.rs"].replace(from, to) });
    const replace = (file: string, from: string, to: string) => {
      expect(actual[file]).toContain(from);
      return { ...actual, [file]: actual[file].replace(from, to) };
    };
    const bypasses: Record<string, Record<string, string>> = {
      "a process": add("lifecycle/model.rs", 'fn a() { let _ = std::process::Command::new("codex"); }'),
      "a file read": add("lifecycle/model.rs", 'fn b() { let _ = std::fs::read("/tmp/r1.ordj"); }'),
      "a clock": add("lifecycle/model.rs", "fn c() { let _ = std::time::Instant::now(); }"),
      "an environment read": add("lifecycle.rs", 'fn d() { let _ = std::env::var("HOME"); }'),
      "a map from std": add("lifecycle/model.rs", "struct E { by_key: std::collections::BTreeMap<u8, u8> }"),
      "serialization": add("lifecycle/model.rs", "#[derive(serde::Serialize)]\nstruct F;"),
      "a command": add("lifecycle/model.rs", "#[tauri::command]\nfn g() {}"),
      "a macro other than matches!": add("lifecycle/model.rs", "fn h() -> Vec<u8> { vec![1] }"),
      "unsafe code": add("lifecycle/model.rs", "fn i() { unsafe {} }"),
      "a public item": add("lifecycle/model.rs", "pub fn j() {}"),
      "another module of the crate": add("lifecycle/model.rs", "use crate::store;"),
      "super in the module root": add("lifecycle.rs", "use super::store;"),
      "the crate root through super": add("lifecycle/reduce.rs", "use super::super::store;"),
      "super of super with spaces": add("lifecycle/live.rs", "fn s0() { let _ = super :: super :: tasks::list(); }"),
      "a module of the crate through super": add("lifecycle/live.rs", "use super::store;"),
      "the root through super": add("lifecycle/reduce.rs", "use super as root;"),
      "a glob of a sibling": add("lifecycle/live.rs", "use super::model::*;"),
      "a glob inside braces": add("lifecycle/live.rs", "use super::model::{*};"),
      "a glob in a nested tree": add("lifecycle/reduce.rs", "use super::{model::{Event, *}};"),
      "a prefix of a sibling name": add("lifecycle/reduce.rs", "use super::models;"),
      "the agents root renamed": add("lifecycle/model.rs", "use crate::agents as a;"),
      "an agents module renamed": add("lifecycle/model.rs", "use crate::agents::catalog as c;"),
      "a glob import": add("lifecycle/model.rs", "use crate::agents::evidence::*;"),
      "a grouped import naming more": model(LIFECYCLE_IMPORTS[1], "use crate::agents::evidence::{Sha256, Token, Record};"),
      "a re-export": model(LIFECYCLE_IMPORTS[0], "pub(crate) use crate::agents::catalog::ProviderId;"),
      "a re-export split over two lines": model(LIFECYCLE_IMPORTS[0], `pub(crate)\n${LIFECYCLE_IMPORTS[0]}`),
      "a reviewed import after other code": model(LIFECYCLE_IMPORTS[0], `const A0: u8 = 0;\n${LIFECYCLE_IMPORTS[0]}`),
      "the reviewed imports swapped": model(`${LIFECYCLE_IMPORTS[0]}\n${LIFECYCLE_IMPORTS[1]}`, `${LIFECYCLE_IMPORTS[1]}\n${LIFECYCLE_IMPORTS[0]}`),
      "a spaced path": model(LIFECYCLE_IMPORTS[0], "use crate :: agents :: catalog :: ProviderId;"),
      "a second copy of a reviewed import": add("lifecycle/model.rs", LIFECYCLE_IMPORTS[0]),
      "a reviewed import in the module root": add("lifecycle.rs", LIFECYCLE_IMPORTS[0]),
      "a full path to the agents module": add("lifecycle/model.rs", "fn k(_: crate::agents::evidence::Record) {}"),
      "the agents module by name": add("lifecycle/model.rs", "use self::agents;"),
      "an authority field": add("lifecycle/model.rs", "struct L { authorized: bool }"),
      "an authority prefix": add("lifecycle/model.rs", "enum L2 { AuthorizedStart }"),
      "an authority word inside a name": add("lifecycle/model.rs", "struct L3 { pre_authorized_grant: bool, authorized_by: u8 }"),
      "an authorization type": add("lifecycle/model.rs", "struct Authorization;"),
      "a dispatch state": add("lifecycle/model.rs", "enum M { Dispatched }"),
      "an enforced limit": add("lifecycle/model.rs", "struct EnforcedLimit;"),
      "a termination method": add("lifecycle/model.rs", "fn terminate_child() {}"),
      "a terminating state": add("lifecycle/model.rs", "enum M2 { Terminating }"),
      "a search location": add("lifecycle/model.rs", "struct N { executable_path: u8 }"),
      "a process in the reducer": add("lifecycle/reduce.rs", 'fn sp() { let _ = std::process::Command::new("codex"); }'),
      "a clock in the live layer": add("lifecycle/live.rs", "fn cl() -> u64 { let _ = std::time::Instant::now(); 0 }"),
      "a sleep in the live layer": add("lifecycle/live.rs", "fn sl() { thread::sleep(d); }"),
      "a launch word in the live layer": add("lifecycle/live.rs", "fn launch_grant() {}"),
      "serialization in the reducer": add("lifecycle/reduce.rs", "#[derive(serde::Serialize)]\nstruct Wire;"),
      // D3.2b-3 (#116): the termination-proposal exception is the exact identifier, in its file, twice, nowhere else.
      "a proposal in snake case": add("lifecycle/reconcile.rs", "fn p0() -> u8 { let termination_proposal = 0; termination_proposal }"),
      "a proposal in upper case": add("lifecycle/reconcile.rs", "const TERMINATION_PROPOSAL: u8 = 0;"),
      "a proposal in lower camel case": add("lifecycle/reconcile.rs", "fn p1() -> u8 { let terminationProposal = 0; terminationProposal }"),
      "a proposal with a prefix": add("lifecycle/reconcile.rs", "struct XTerminationProposal;"),
      "a proposal with a suffix": add("lifecycle/reconcile.rs", "struct TerminationProposals;"),
      "a proposal with a digit": add("lifecycle/reconcile.rs", "struct TerminationProposal2;"),
      "a proposal with an underscore suffix": add("lifecycle/reconcile.rs", "struct TerminationProposal_;"),
      "a proposal as a raw identifier": add("lifecycle/reconcile.rs", "struct r#TerminationProposal;"),
      "a third mention of the proposal": add("lifecycle/reconcile.rs", "fn p2(r: Recommendation) -> bool { matches!(r, Recommendation::TerminationProposal { .. }) }"),
      "a proposal imported under an alias": add("lifecycle/reconcile.rs", "use self::Recommendation::TerminationProposal as Proposal;"),
      "a proposal in the live layer": add("lifecycle/live.rs", "fn p3() { let _ = TerminationProposal; }"),
      "a proposal in the reducer": add("lifecycle/reduce.rs", "enum P4 { TerminationProposal }"),
      "a proposal in the module root": add("lifecycle.rs", "enum P5 { TerminationProposal }"),
      "a proposal in the model": add("lifecycle/model.rs", "struct TerminationProposal;"),
      "a proposal assembled by a macro": add("lifecycle/reconcile.rs", "fn p6() { concat_idents!(Termination, Proposal); }"),
      "a proposal carrying an action": replace("lifecycle/reconcile.rs", "policy: PolicyRequirement },", "policy: PolicyRequirement, act: fn(u32) },"),
      "a proposal carrying a process id": replace("lifecycle/reconcile.rs", "policy: PolicyRequirement },", "policy: PolicyRequirement, pid: u32 },"),
      "a proposal built a second time": add("lifecycle/reconcile.rs", "fn p7(attempt: AttemptNo) -> Recommendation { Recommendation::TerminationProposal { attempt, basis: ProposalBasis::PidStartTokenAndHash, policy: PolicyRequirement::SeparatePolicy } }"),
      "a termination method beside the proposal": add("lifecycle/reconcile.rs", "fn terminate_attempt() {}"),
      "a terminating state beside the proposal": add("lifecycle/reconcile.rs", "enum P8 { Terminating }"),
      "a kill beside the proposal": add("lifecycle/reconcile.rs", "fn p9() { kill(4242); }"),
      "a signal beside the proposal": add("lifecycle/reconcile.rs", "fn p10() { let _ = signal::SIGTERM; }"),
      "a process in reconciliation": add("lifecycle/reconcile.rs", 'fn p11() { let _ = std::process::Command::new("kill"); }'),
      "a clock in reconciliation": add("lifecycle/reconcile.rs", "fn p12() { let _ = Instant::now(); }"),
      "a process table read in reconciliation": add("lifecycle/reconcile.rs", 'fn p13() { let _ = std::fs::read_dir("/proc"); }'),
      "a command in reconciliation": add("lifecycle/reconcile.rs", "#[tauri::command]\nfn p14() {}"),
      "serialization in reconciliation": add("lifecycle/reconcile.rs", "#[derive(serde::Serialize)]\nstruct P15;"),
      "super escaping reconciliation": add("lifecycle/reconcile.rs", "use super::super::store;"),
      // Review of #116: a look-alike letter (Cyrillic i) in a forbidden word.
      "a look-alike letter": add("lifecycle/reconcile.rs", "fn term\u0456nate_attempt() {}"),
    };
    for (const [name, files] of Object.entries(bypasses)) {
      expect(lifecycleContentViolations(files), name).not.toEqual([]);
    }
    // The live authority and draft construction rules (D3.2b-2).
    const live = (from: string, to: string) => ({ ...actual, "lifecycle/live.rs": actual["lifecycle/live.rs"].replace(from, to) });
    const modelText = (from: string, to: string) => ({ ...actual, "lifecycle/model.rs": actual["lifecycle/model.rs"].replace(from, to) });
    const authority: Record<string, Record<string, string>> = {
      "a cloneable grant": live("#[must_use]\npub(crate) struct LiveGrant", "#[must_use]\n#[derive(Clone)]\npub(crate) struct LiveGrant"),
      "a formattable supervision": live("pub(crate) struct LiveSupervision", "#[derive(Debug)]\npub(crate) struct LiveSupervision"),
      "a trait implemented by hand": add("lifecycle/live.rs", "impl Clone for LiveGrant { fn clone(&self) -> Self { todo() } }"),
      "a default staged frame": add("lifecycle/live.rs", "impl Default for Staged { fn default() -> Self { todo() } }"),
      "a visible grant field": live("    use_by: MonoMs,\n}", "    pub(crate) use_by: MonoMs,\n}"),
      "a grant built twice": add("lifecycle/live.rs", "fn again(g: &LiveGrant) -> LiveGrant { LiveGrant { ..todo() } }"),
      "a grant named by the reducer": add("lifecycle/reduce.rs", "fn holds(_: &super::live::LiveGrant) {}"),
      "a grant returned by another function": add("lifecycle/live.rs", "fn keep(&self) -> Option<LiveGrant> { None }"),
      "a grant taken by another function": add("lifecycle/live.rs", "fn take(grant: LiveGrant) {}"),
      "a supervision built twice": add("lifecycle/live.rs", "fn other() -> LiveSupervision { LiveSupervision { ..todo() } }"),
      "a visible draft field": modelText("pub(crate) struct EventDraft {\n    producer: Producer,", "pub(crate) struct EventDraft {\n    pub(crate) producer: Producer,"),
      "a draft built elsewhere": add("lifecycle/reduce.rs", "fn forge() -> EventDraft { EventDraft { producer: Producer::User } }"),
      "a forging helper in production": modelText("    #[cfg(test)]\n    pub(crate) fn forged", "    pub(crate) fn forged"),
      // Independent review of #114: construction through Self, generic signatures, aliases and paths.
      "a grant twin through Self": add("lifecycle/live.rs", "impl LiveGrant { pub(crate) fn twin(&self) -> Self { Self { epoch: self.epoch, use_by: self.use_by } } }"),
      "a staged frame through Self": add("lifecycle/live.rs", "impl Staged { fn fake(frame: u64) -> Self { Self { frame } } }"),
      "a labelled draft through Self": add("lifecycle/model.rs", "impl EventDraft { pub(crate) fn labelled(producer: Producer, body: EventBody) -> Self { Self { producer, body } } }"),
      "a generic function returning a grant": add("lifecycle/live.rs", "fn keep<'a>(&'a self) -> Option<LiveGrant> { None }"),
      "a generic function taking a grant": add("lifecycle/live.rs", "fn take<T>(grant: LiveGrant, _t: T) {}"),
      "a trait for an alias": add("lifecycle/live.rs", "type G = LiveGrant;\nimpl Clone for G { fn clone(&self) -> G { todo() } }"),
      "a trait for a path": add("lifecycle/live.rs", "impl Clone for self::LiveGrant where Self: Sized { fn clone(&self) -> Self { todo() } }"),
      "a default for an alias": add("lifecycle/live.rs", "type S = Staged;\nimpl Default for S { fn default() -> S { todo() } }"),
      "a renamed grant": add("lifecycle/live.rs", "use self::LiveGrant as G;"),
      "an alias of a draft": add("lifecycle/reduce.rs", "type D = EventDraft;"),
      // Re-review of #114: tuple parameters, nested generics, a parenthesized impl target, another staged literal.
      "a grant after a tuple parameter": add("lifecycle/live.rs", "fn take(_t: (u8, u8), grant: LiveGrant) {}"),
      "a grant returned under nested generics": add("lifecycle/live.rs", "fn keep<T: Into<u8>>(_t: T) -> Option<LiveGrant> { None }"),
      "a trait for a parenthesized type": add("lifecycle/live.rs", "impl Default for (Staged) { fn default() -> Staged { todo() } }"),
      "a second staged frame": add("lifecycle/live.rs", "fn fake() -> Staged { Staged { frame: 0 } }"),
      "a second pending supervision": add("lifecycle/live.rs", "fn fake() -> PendingSupervision { PendingSupervision { events: todo() } }"),
      // D3.2c-2 (#122): write outcomes and torn-tail evidence.
      "a cloneable outcome": live("#[must_use]\npub(crate) struct WriteOutcome", "#[must_use]\n#[derive(Clone)]\npub(crate) struct WriteOutcome"),
      "a visible outcome field": live("    expected: Vec<u8>,\n    check: PersistCheck,\n}", "    expected: Vec<u8>,\n    pub(crate) check: PersistCheck,\n}"),
      "a landed outcome built elsewhere": add("lifecycle/live.rs", "fn landed(o: WriteOutcome) -> WriteOutcome { WriteOutcome { check: PersistCheck::Landed, ..o } }"),
      "an outcome returned by another function": add("lifecycle/live.rs", "fn keep(&self) -> Option<WriteOutcome> { None }"),
      "an outcome taken by stage": add("lifecycle/live.rs", "fn stage2(outcome: WriteOutcome) {}"),
      "an outcome named by reconciliation": add("lifecycle/reconcile.rs", "fn o(_: &super::live::WriteOutcome) {}"),
      "a default outcome": add("lifecycle/live.rs", "impl Default for WriteOutcome { fn default() -> WriteOutcome { todo() } }"),
      "copyable evidence": live("#[must_use]\npub(crate) struct TornTailEvidence", "#[must_use]\n#[derive(Clone, Copy)]\npub(crate) struct TornTailEvidence"),
      "evidence built from figures": add("lifecycle/live.rs", "fn shown(&self, len: u32, crc32c: u32) -> TornTailEvidence { TornTailEvidence { epoch: self.epoch, run: self.projection.run(), owed: OwedRepair { frame: 0, len, crc32c } } }"),
      "evidence taken by stage": add("lifecycle/live.rs", "fn stage3(evidence: &TornTailEvidence) {}"),
      "a repair frame staged with a grant": live("self.stage_frame(&[draft], None, mono, size, true)", "self.stage_frame(&[draft], grant, mono, size, true)"),
      "a plain frame staged as a repair": live("self.stage_frame(drafts, grant, mono, size, false)", "self.stage_frame(drafts, grant, mono, size, true)"),
      // Review of #122 (G2 to G5).
      "a repair staged through Self": add("lifecycle/live.rs", "impl LiveSupervision { fn sneak(&mut self, d: EventDraft, mono: MonoMs, size: JournalSize) { let _ = Self::stage_frame(self, &[d], None, mono, size, true); } }"),
      "a visible stage_frame": live("    fn stage_frame(&mut self", "    pub(crate) fn stage_frame(&mut self"),
      "an outcome setter": add("lifecycle/live.rs", "impl WriteOutcome { pub(crate) fn set(&mut self, check: PersistCheck) { self.check = check; } }"),
      "an outcome setter in the reviewed block": live("impl WriteOutcome {\n", "impl WriteOutcome {\n    pub(crate) fn set(&mut self, check: PersistCheck) { self.check = check; }\n"),
      "an evidence setter": add("lifecycle/live.rs", "impl TornTailEvidence { pub(crate) fn grow(&mut self) { self.owed.len += 1; } }"),
      "an unpoisoned supervision": add("lifecycle/live.rs", "impl LiveSupervision { pub(crate) fn unpoison(&mut self) { self.poisoned = false; self.uncertain = 0; self.owed = None; } }"),
      "a capacity fault cleared": add("lifecycle/live.rs", "impl LiveSupervision { fn clear(&mut self) { self.capacity_fault = false; } }"),
      "a repair owed taken": add("lifecycle/live.rs", "impl LiveSupervision { fn forget(&mut self) { let _ = self.owed.take(); } }"),
      "a poison flag swapped": add("lifecycle/live.rs", "impl LiveSupervision { fn swap(&mut self, other: &mut bool) { std::mem::swap(&mut self.poisoned, other); } }"),
      "an alias of the outcome": add("lifecycle/live.rs", "type O = WriteOutcome;"),
    };
    for (const [name, files] of Object.entries(authority)) {
      expect(liveAuthorityViolations(files), name).not.toEqual([]);
    }
    // The opening of a reconciled run (D3.2b-3).
    const opening: Record<string, Record<string, string>> = {
      "a cloneable opening": replace("lifecycle/live.rs", "pub(crate) enum Opening", "#[derive(Clone)]\npub(crate) enum Opening"),
      "a trait for an opening": add("lifecycle/live.rs", "impl Default for Opening { fn default() -> Opening { todo() } }"),
      "an opening named by reconciliation": add("lifecycle/reconcile.rs", "fn o(_: super::live::Opening) {}"),
      "a supervised run claimed without a supervision": add("lifecycle/reconcile.rs", "fn claim(live: &'static u8) -> SupervisedRun<'static> { SupervisedRun { live } }"),
      "a supervised run built twice": add("lifecycle/live.rs", "fn claim(live: &LiveSupervision) -> SupervisedRun<'_> { SupervisedRun { live } }"),
      "a cloneable supervised run": replace("lifecycle/live.rs", "pub(crate) struct SupervisedRun<'a>", "#[derive(Clone, Copy)]\npub(crate) struct SupervisedRun<'a>"),
      "a visible supervised field": replace("lifecycle/live.rs", "    live: &'a LiveSupervision,\n}", "    pub(crate) live: &'a LiveSupervision,\n}"),
      "a supervision built by begin_reconciled": add("lifecycle/live.rs", "fn again(live: LiveSupervision) -> LiveSupervision { LiveSupervision { ..live } }"),
    };
    for (const [name, files] of Object.entries(opening)) {
      expect(liveAuthorityViolations(files), name).not.toEqual([]);
    }
    // Recommendations stay inert; the opening frame and history are built only in reconcile.rs.
    const inert: Record<string, Record<string, string>> = {
      "a method on Recommendation": add("lifecycle/reconcile.rs", "impl Recommendation { pub(crate) fn act(&self) {} }"),
      "a trait for Recommendation": add("lifecycle/reconcile.rs", "impl Default for Recommendation { fn default() -> Recommendation { todo() } }"),
      "a trait for a proposal basis": add("lifecycle/reconcile.rs", "impl Default for ProposalBasis { fn default() -> ProposalBasis { todo() } }"),
      "another derive on Recommendation": replace("lifecycle/reconcile.rs", "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub(crate) enum Recommendation", "#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub(crate) enum Recommendation"),
      "another recommendation": replace("lifecycle/reconcile.rs", "    RetryPossible(AttemptNo),\n}", "    RetryPossible(AttemptNo),\n    StopNow(AttemptNo),\n}"),
      "a function taking a recommendation": add("lifecycle/reconcile.rs", "fn act_on(_r: &Recommendation) {}"),
      "a match on a recommendation": add("lifecycle/reconcile.rs", "fn p0(r: Vec<u8>) -> u8 { match r.len() { 0 => { let _ = Recommendation::RetryPossible; 1 } _ => 0 } }"),
      "a recommendation read back": add("lifecycle/reconcile.rs", "fn p1(r: &Reconciliation) -> bool { r.recommendations.iter().any(|x| matches!(x, Recommendation::ReviewAttempt(_))) }"),
      "recommendations counted in the live layer": add("lifecycle/live.rs", "use super::reconcile::Reconciliation;\nfn proposes_stop(r: &Reconciliation) -> bool { r.recommendations.len() > 1 }"),
      "recommendations counted in reconciliation": add("lifecycle/reconcile.rs", "fn proposes_stop(r: &Reconciliation) -> bool { r.recommendations.len() > 1 }"),
      "recommendations counted through the accessor": add("lifecycle/live.rs", "fn proposes_stop(r: &super::reconcile::Reconciliation) -> usize { r.recommendations().len() }"),
      "a recommendation pushed into another list": add("lifecycle/reconcile.rs", "fn sink() { let mut other = Vec::new(); other.push(Recommendation::RetryPossible); }"),
      "a production accessor": replace("lifecycle/reconcile.rs", "    #[cfg(test)]\n    pub(crate) fn recommendations", "    pub(crate) fn recommendations"),
      "a visible recommendations field": replace("lifecycle/reconcile.rs", "\n    recommendations: Vec<Recommendation>,\n", "\n    pub(crate) recommendations: Vec<Recommendation>,\n"),
      "a recommendation named by the live layer": add("lifecycle/live.rs", "use super::reconcile::Recommendation;"),
      "a recommendation named by the reducer": add("lifecycle/reduce.rs", "fn p2(_: &[super::reconcile::Recommendation]) {}"),
      "an alias of Recommendation": add("lifecycle/reconcile.rs", "type R = Recommendation;"),
      "Recommendation renamed": add("lifecycle/reconcile.rs", "use self::Recommendation as R;"),
      "a cloneable frame": replace("lifecycle/reconcile.rs", "pub(crate) struct ReconciledFrame", "#[derive(Clone)]\npub(crate) struct ReconciledFrame"),
      "a cloneable history": replace("lifecycle/reconcile.rs", "pub(crate) struct RecordedHistory", "#[derive(Clone)]\npub(crate) struct RecordedHistory"),
      "a visible frame field": replace("lifecycle/reconcile.rs", "    drafts: Vec<EventDraft>,\n    /// The size", "    pub(crate) drafts: Vec<EventDraft>,\n    /// The size"),
      "a frame built twice": add("lifecycle/reconcile.rs", "fn fake(f: ReconciledFrame) -> ReconciledFrame { ReconciledFrame { ..f } }"),
      "a frame built by the live layer": add("lifecycle/live.rs", "fn fake(f: ReconciledFrame) -> ReconciledFrame { ReconciledFrame { ..f } }"),
      "a history built twice": add("lifecycle/reconcile.rs", "fn fake(h: RecordedHistory) -> RecordedHistory { RecordedHistory { ..h } }"),
      "a trait for a frame": add("lifecycle/live.rs", "impl Clone for ReconciledFrame { fn clone(&self) -> ReconciledFrame { todo() } }"),
      "an alias of a frame": add("lifecycle/live.rs", "type F = ReconciledFrame;"),
      // D3.2c-2 (#122): never-created proofs.
      "a never-created proof from a run id": add("lifecycle/reconcile.rs", "fn assume(run: RunId) -> RunSummary { RunSummary::NeverCreated(NeverCreatedRun { run }) }"),
      "a never-created proof built by the live layer": add("lifecycle/live.rs", "fn fake(run: RunId) -> super::reconcile::NeverCreatedRun { super::reconcile::NeverCreatedRun { run } }"),
      "a visible never-created field": replace("lifecycle/reconcile.rs", "pub(crate) struct NeverCreatedRun {\n    run: RunId,", "pub(crate) struct NeverCreatedRun {\n    pub(crate) run: RunId,"),
      "a default never-created proof": add("lifecycle/reconcile.rs", "impl Default for NeverCreatedRun { fn default() -> NeverCreatedRun { todo() } }"),
      "a function returning a never-created proof": add("lifecycle/reconcile.rs", "fn proof(s: RunSummary) -> Option<NeverCreatedRun> { None }"),
    };
    for (const [name, files] of Object.entries(inert)) {
      expect(reconciliationViolations(files), name).not.toEqual([]);
    }
    // The codec and replay (D3.2b-4): serde only in the codec and only in the reviewed forms, the sibling table, and the
    // usual process, file, clock and environment cases in the new files.
    const codecFile = "lifecycle/codec.rs";
    const content: Record<string, Record<string, string>> = {
      "serde in replay": add("lifecycle/replay.rs", "#[derive(serde::Serialize)]\nstruct R0;"),
      "serde_json in the reducer": add("lifecycle/reduce.rs", "fn r1(b: &[u8]) { let _ = serde_json::from_slice::<u8>(b); }"),
      "a reader in the codec": add(codecFile, "fn r2<R>(r: R) { let _ = serde_json::from_reader::<R, u8>(r); }"),
      "a writer in the codec": add(codecFile, "fn r3(v: &u8) { let _ = serde_json::to_writer(Vec::new(), v); }"),
      "a serde path in the codec": add(codecFile, "fn r4<'de, D: serde::Deserializer<'de>>(_d: D) {}"),
      "serde_json renamed in the codec": add(codecFile, "use serde_json as j;"),
      "a reviewed serde_json item renamed": add(codecFile, "use serde_json::Value as Grant;"),
      "another serde attribute": add(codecFile, '#[derive(Deserialize)]\n#[serde(rename = "x")]\nstruct WOther { a: u8 }'),
      "a custom field deserializer": add(codecFile, '#[derive(Deserialize)]\nstruct WFn { #[serde(deserialize_with = "f")] a: u8 }'),
      "a second serde import": add(codecFile, REVIEWED_SERDE_IMPORT),
      "a serde import naming more": replace(codecFile, REVIEWED_SERDE_IMPORT, "use serde::{Deserialize, Serialize, Serializer};"),
      "the serde import after other code": { ...actual, [codecFile]: actual[codecFile].replace(`${REVIEWED_SERDE_IMPORT}\n`, "") + `\n${REVIEWED_SERDE_IMPORT}` },
      "the serde import in replay": add("lifecycle/replay.rs", REVIEWED_SERDE_IMPORT),
      "a file in the codec": add(codecFile, 'fn r5() { let _ = std::fs::read("/tmp/r1.ordj"); }'),
      "a file type in the codec": add(codecFile, "fn r6(_: File) {}"),
      "a clock in the codec": add(codecFile, "fn r7() { let _ = std::time::SystemTime::now(); }"),
      "an environment read in replay": add("lifecycle/replay.rs", 'fn r8() { let _ = std::env::var("HOME"); }'),
      "a process in replay": add("lifecycle/replay.rs", 'fn r9() { let _ = std::process::Command::new("codex"); }'),
      "a launch word in the codec": add(codecFile, "fn launch_frame() {}"),
      "a path in the codec": add(codecFile, "struct R10 { journal_path: u8 }"),
      "a macro in the codec": add(codecFile, 'fn r11() -> String { format!("{}", 1) }'),
      "the codec naming the reducer": add(codecFile, "use super::reduce::apply_recorded;"),
      "the codec naming the live layer": add(codecFile, "use super::live::begin_new;"),
      "the codec naming reconciliation": add(codecFile, "use super::reconcile::RecordedHistory;"),
      "the codec naming replay": add(codecFile, "use super::replay::replay;"),
      "the live layer naming replay": add("lifecycle/live.rs", "use super::replay::replay;"),
      "the reducer naming replay": add("lifecycle/reduce.rs", "use super::replay::ReplayedHistory;"),
      "the model naming the codec": add("lifecycle/model.rs", "use super::codec::encode_frame;"),
      "replay naming the live layer": add("lifecycle/replay.rs", "use super::live::LiveSupervision;"),
      "replay naming reconciliation": add("lifecycle/replay.rs", "use super::reconcile::RecordedHistory;"),
      "reconciliation naming the codec": add("lifecycle/reconcile.rs", "use super::codec::decode_stream;"),
      "the codec escaping through super": add(codecFile, "use super::super::store;"),
      "an unreviewed lifecycle file": { ...actual, "lifecycle/journal.rs": "use super::model::Event;\n" },
    };
    for (const [name, files] of Object.entries(content)) {
      expect(lifecycleContentViolations(files), name).not.toEqual([]);
    }
    const serialization: Record<string, Record<string, string>> = {
      "a model type deriving serde": add("lifecycle/model.rs", "#[derive(Serialize)]\npub(crate) struct S0;"),
      "a live type deriving serde": add("lifecycle/live.rs", "#[derive(Deserialize)]\nstruct S1;"),
      "serialization named by the reducer": add("lifecycle/reduce.rs", "fn s2<T: Serialize>(_: T) {}"),
      "serde named by replay": add("lifecycle/replay.rs", "fn s3(v: &serde_json::Value) {}"),
      "a derive on a codec enum": add(codecFile, "#[derive(Serialize)]\nenum WKind { A }"),
      "a derive on a codec struct not named for the wire": add(codecFile, "#[derive(Serialize, Deserialize)]\nstruct Grant { a: u8 }"),
      "a derive on a visible codec struct": add(codecFile, "#[derive(Serialize)]\npub(crate) struct WVisible;"),
      "a derive on a decoded frame": replace(codecFile, "pub(crate) struct DecodedFrame {", "#[derive(Serialize)]\npub(crate) struct DecodedFrame {"),
      "a hand-written Serialize": add(codecFile, "impl Serialize for DecodedFrame { fn serialize<S>(&self, s: S) {} }"),
      "a hand-written Deserialize": add(codecFile, "impl<'de> Deserialize<'de> for Decoded {}"),
      "a hand-written visitor": add(codecFile, "struct V;\nimpl<'de> Visitor<'de> for V {}"),
    };
    serialization["a derive behind a conditional attribute"] = add(codecFile, "#[cfg_attr(all(), derive(Deserialize))]\nstruct Forged { a: u8 }");
    serialization["a conditional attribute in the model"] = add("lifecycle/model.rs", "#[cfg_attr(test, derive(Clone))]\nstruct C0;");
    for (const [name, files] of Object.entries(serialization)) {
      expect(serializationViolations(files), name).not.toEqual([]);
    }
    const replayed: Record<string, Record<string, string>> = {
      "a cloneable replayed history": replace("lifecycle/replay.rs", "pub(crate) struct ReplayedHistory {", "#[derive(Clone)]\npub(crate) struct ReplayedHistory {"),
      "a visible replay field": replace("lifecycle/replay.rs", "    fault: Option<JournalFault>,\n    damage", "    pub(crate) fault: Option<JournalFault>,\n    damage"),
      "a replayed history built twice": add("lifecycle/replay.rs", "fn twin(h: ReplayedHistory) -> ReplayedHistory { ReplayedHistory { ..h } }"),
      "a replayed history built by reconciliation": add("lifecycle/reconcile.rs", "fn forge(h: ReplayedHistory) -> ReplayedHistory { ReplayedHistory { ..h } }"),
      "a mutable replay method": add("lifecycle/replay.rs", "impl ReplayedHistory { pub(crate) fn clear(&mut self) { self.fault = None; } }"),
      "a trait for a replayed history": add("lifecycle/replay.rs", "impl Default for ReplayedHistory { fn default() -> ReplayedHistory { todo() } }"),
      "a replayed history named by the live layer": add("lifecycle/live.rs", "fn from_disk(_: &ReplayedHistory) {}"),
      "an alias of a replayed history": add("lifecycle/replay.rs", "type R = ReplayedHistory;"),
      "a recorded history stated in production": replace("lifecycle/reconcile.rs", "    #[cfg(test)]\n    pub(crate) fn new(", "    pub(crate) fn new("),
      "a visible assemble": replace("lifecycle/reconcile.rs", "    fn assemble(", "    pub(crate) fn assemble("),
      "another assemble call": add("lifecycle/reconcile.rs", "fn trust(p: RunProjection) { let _ = RecordedHistory::assemble(p, HistoryStatus::Quarantined(QuarantineReason::Marker), Vec::new()); }"),
      "another function returning a recorded history": add("lifecycle/reconcile.rs", "fn relabel(h: RecordedHistory) -> RecordedHistory { h }"),
      "a verified status elsewhere in reconciliation": add("lifecycle/reconcile.rs", "fn v0() -> HistoryStatus { HistoryStatus::Verified }"),
      "a verified status in the live layer": add("lifecycle/live.rs", "fn v1() -> HistoryStatus { HistoryStatus::Verified }"),
      "a decoded frame built in replay": add("lifecycle/replay.rs", "fn f0() -> DecodedFrame { DecodedFrame { offset: 0, end: 0, events: Vec::new() } }"),
      "a decoded stream built twice": add(codecFile, "fn f1() -> Decoded { Decoded { frames: Vec::new(), end: StreamEnd::Clean } }"),
      "a visible decoded frame field": replace(codecFile, "    events: Vec<Event>,\n}\n\nimpl DecodedFrame", "    pub(crate) events: Vec<Event>,\n}\n\nimpl DecodedFrame"),
      "an unverified frame carrying events": replace(codecFile, "    kinds: Vec<EventKind>,\n}", "    kinds: Vec<EventKind>,\n    events: Vec<Event>,\n}"),
      "another derive on an unverified frame": replace(codecFile, "#[derive(Clone, Debug, PartialEq, Eq)]\npub(crate) struct UnverifiedFrame", "#[derive(Clone, Debug, PartialEq, Eq, Default)]\npub(crate) struct UnverifiedFrame"),
      "an unverified frame read by replay": add("lifecycle/replay.rs", "fn u0(_: &super::codec::UnverifiedFrame) {}"),
      "the audit scan called by reconciliation": add("lifecycle/reconcile.rs", "fn u1(b: &[u8]) { let _ = super::codec::scan_after_corruption(b, 0); }"),
      // D3.2c-2: the live layer names the codec to encode and judge frames, never to read recovered bytes.
      "the audit scan called by the live layer": add("lifecycle/live.rs", "fn u3(b: &[u8]) { let _ = super::codec::scan_after_corruption(b, 0); }"),
      "a cloneable decoded stream": replace(codecFile, "pub(crate) struct Decoded {", "#[derive(Clone)]\npub(crate) struct Decoded {"),
      "a trait for a decoded frame": add(codecFile, "impl Clone for DecodedFrame { fn clone(&self) -> DecodedFrame { todo() } }"),
      "a decoded frame renamed": add("lifecycle/replay.rs", "use super::codec::DecodedFrame as Frame;"),
      // Review of #118: histories changed after they are built.
      "a replayed history cleared through a reference": add("lifecycle/replay.rs", "fn clear(h: &mut ReplayedHistory) { h.fault = None; }"),
      "a replayed history cleared by value": add("lifecycle/replay.rs", "fn clear(mut h: ReplayedHistory) -> ReplayedHistory { h.fault = None; h.tail = None; h }"),
      "a replayed history relabelled": add("lifecycle/replay.rs", "fn clear(h: ReplayedHistory) -> ReplayedHistory { h }"),
      "a recorded history relabelled": add("lifecycle/reconcile.rs", "fn relabel(h: &mut RecordedHistory, v: &RecordedHistory) { h.status = v.status; }"),
      "a recorded history rebound": add("lifecycle/reconcile.rs", "fn relabel(h: RecordedHistory) { let mut x: RecordedHistory = h; }"),
      "a recorded status assigned": add("lifecycle/reconcile.rs", "fn relabel(mut h: Box<RecordedHistory>) { h.status = HistoryStatus::Quarantined(QuarantineReason::Marker); }"),
      "repairs dropped through an untyped binding": add("lifecycle/reconcile.rs", "fn quiet(h: RecordedHistory, i: ReconcileInput) -> bool { let mut x = h; x.unrecorded_repairs.clear(); reconcile(&x, &[], i).is_ok() }"),
      "repairs dropped through a mutable self": add("lifecycle/reconcile.rs", "impl RecordedHistory { pub(crate) fn reconcile_quietly(mut self, i: ReconcileInput) -> Result<Reconciliation, ReconcileRefused> { self.unrecorded_repairs.clear(); reconcile(&self, &[], i) } }"),
      "a fault taken through an untyped binding": add("lifecycle/replay.rs", "pub(crate) fn with_clean(h: ReplayedHistory, f: impl FnOnce(ReplayedHistory)) { let mut r = h; r.fault.take(); f(r) }"),
      "a parenthesised receiver": add("lifecycle/replay.rs", "fn s0(h: ReplayedHistory) { let mut r = h; (r.fault).take(); }"),
      "a parenthesised borrow": add("lifecycle/reconcile.rs", "fn s1(h: RecordedHistory) { let mut x = h; Vec::clear(&mut (x.unrecorded_repairs)); }"),
      "spaces around the dots": add("lifecycle/replay.rs", "fn s2(h: ReplayedHistory) { let mut r = h; r . fault . take(); }"),
      "a ref mut match": add("lifecycle/replay.rs", "fn s3(h: ReplayedHistory) { let mut r = h; match r.fault { ref mut x => *x = None } }"),
      "a ref mut destructure": add("lifecycle/reconcile.rs", "fn s4(x: RecordedHistory) { let RecordedHistory { ref mut unrecorded_repairs, .. } = x; unrecorded_repairs.clear(); }"),
      "a field borrowed mutably": add("lifecycle/replay.rs", "fn clean(h: ReplayedHistory) { let mut r = h; wipe(&mut r.tail); }"),
      "a replay field assigned in place": add("lifecycle/replay.rs", "fn grow(h: &mut Vec<u8>, r: ReplayedHistory) { let mut r = r; r.boundaries = Vec::new(); }"),
    };
    for (const [name, files] of Object.entries(replayed)) {
      expect(replayViolations(files), name).not.toEqual([]);
    }
    // The agents module may never name the lifecycle model.
    const agents = agentSources();
    expect(rustCodeOnly(agents["agents/evidence.rs"] + "\nuse crate::lifecycle;")).toMatch(/\blifecycle\b/);
    for (const [file, text] of Object.entries(agents)) expect(rustCodeOnly(text), file).not.toMatch(/\blifecycle\b/);
  });

  // Reviewed in issue #40: the frontend may depend on @tauri-apps/api, and only to call get_runtime_status.
  it("depends only on @tauri-apps/api, pinned to the Rust tauri crate's version", () => {
    const pkg = JSON.parse(read("package.json"));
    const deps = Object.keys({ ...pkg.dependencies });
    expect(deps.filter((d) => d.startsWith("@tauri-apps"))).toEqual(["@tauri-apps/api"]);
    // Everything the production bundle may take from node_modules comes from these packages.
    expect(deps).toEqual(["@tauri-apps/api", "lucide-react", "react", "react-dom"]);
    expect(Object.values(pkg.dependencies).every((v) => typeof v === "string" && /^\^?\d/.test(v))).toBe(true);
    const crate = read("src-tauri/Cargo.lock").match(/name = "tauri"\nversion = "([^"]+)"/)?.[1];
    expect(pkg.dependencies["@tauri-apps/api"]).toBe(crate);
  });

  it("calls get_runtime_status from the runtime adapter, with no arguments", () => {
    // What the production bundle may contain, including which module may import @tauri-apps, is checked against a
    // real build in frontend-boundary.test.ts.
    // index.html loads the app's one entry module, and there is no public/ directory to serve other scripts from.
    expect(read("index.html").match(/<script[^>]*>/g)).toEqual(['<script type="module" src="/src/main.tsx">']);
    expect(existsSync(join(root, "public"))).toBe(false);

    const adapter = read("src/app/runtimeStatus.ts");
    expect(adapter.match(/from "@tauri-apps[^"]*"/g)).toEqual(['from "@tauri-apps/api/core"']);
    expect(adapter).toMatch(/^import \{ invoke, isTauri \} from "@tauri-apps\/api\/core";$/m);
    expect(adapter).toMatch(/^const COMMAND = "get_runtime_status";$/m);
    // One call site, with the fixed name and nothing else; invoke itself is never handed to other code.
    expect(adapter.match(/\binvoke\b/g)).toHaveLength(2);
    expect(adapter).toMatch(/\binvoke\(COMMAND\)/);
    expect(adapter).not.toMatch(/export[^\n]*\binvoke\b(?!\()|export \{|export \*/);
  });

  // Issue #74: the project registry commands are reachable from one adapter, and the reset command from nowhere.
  it("names the project commands in one module only, sends no path, and never names the reset command", () => {
    const sources: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const p = join(dir, name);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.[cm]?[jt]sx?$/.test(name) && !/\.(test|spec)\./.test(name)) sources.push(p.slice(join(root, "src").length + 1));
      }
    };
    walk(join(root, "src"));
    const adapter = "app/projects.ts";
    expect(sources).toContain(adapter);
    for (const file of sources.filter((f) => f !== adapter)) {
      expect(read(`src/${file}`), file).not.toMatch(/\b(?:list_projects|add_project|remove_project|reset_projects)\b/);
    }
    // Exactly the three reviewed adapters load the Tauri API.
    expect(sources.filter((f) => /@tauri-apps/.test(read(`src/${f}`))).sort()).toEqual(["app/projects.ts", "app/runtimeStatus.ts", "app/tasks.ts"]);

    const code = read(`src/${adapter}`);
    expect(code).not.toMatch(/reset_projects/);
    expect(code.match(/from "@tauri-apps[^"]*"/g)).toEqual(['from "@tauri-apps/api/core"']);
    expect(code).toMatch(/^import \{ invoke, isTauri \} from "@tauri-apps\/api\/core";$/m);
    // Three call sites. The first two take nothing; the third takes only a registration id.
    expect(code.match(/\binvoke\(/g)).toHaveLength(3);
    expect(code).toMatch(/\binvoke\("list_projects"\)/);
    expect(code).toMatch(/\binvoke\("add_project"\)/);
    expect(code).toMatch(/\binvoke\("remove_project", \{ id \}\)/);
    expect(code).toMatch(/export function listProjects\(\): /);
    expect(code).toMatch(/export function addProject\(\): /);
    expect(code).toMatch(/export function removeProject\(id: string\): /);
    expect(code).not.toMatch(/export[^\n]*\binvoke\b(?!\()|export \{|export \*/);
  });

  // Issue #96: the task adapter is the only module that names a task command. No call takes a path: list_tasks takes
  // nothing, get_task a task id, create_task a registration (id and registration time) and text, update_task an id,
  // revision, text and status.
  it("names the task commands in one module only, with the reviewed arguments and no path", () => {
    const sources: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const p = join(dir, name);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.[cm]?[jt]sx?$/.test(name) && !/\.(test|spec)\./.test(name)) sources.push(p.slice(join(root, "src").length + 1));
      }
    };
    walk(join(root, "src"));
    const adapter = "app/tasks.ts";
    expect(sources).toContain(adapter);
    for (const file of sources.filter((f) => f !== adapter)) {
      expect(read(`src/${file}`), file).not.toMatch(/\b(?:list_tasks|get_task|create_task|update_task)\b/);
    }
    const code = read(`src/${adapter}`);
    expect(code.match(/from "@tauri-apps[^"]*"/g)).toEqual(['from "@tauri-apps/api/core"']);
    expect(code).toMatch(/^import \{ invoke, isTauri \} from "@tauri-apps\/api\/core";$/m);
    expect(code.match(/\binvoke\(/g)).toHaveLength(4);
    expect(code).toMatch(/\binvoke\("list_tasks"\)/);
    expect(code).toMatch(/\binvoke\("get_task", \{ id \}\)/);
    expect(code).toMatch(/\binvoke\("create_task", \{ project_id: projectId, expected_registered_at_ms: registeredAtMs, title, description \}\)/);
    expect(code).toMatch(/\binvoke\("update_task", \{ id, revision, title, description, status \}\)/);
    expect(code).not.toMatch(/\b(?:list_projects|add_project|remove_project|reset_projects|get_runtime_status)\b/);
    expect(code).not.toMatch(/export[^\n]*\binvoke\b(?!\()|export \{|export \*/);
  });

  it("references no remote URL from the app sources", () => {
    const hits: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const p = join(dir, name);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.(tsx?|css|html)$/.test(name) && !name.endsWith(".test.ts") && !name.endsWith(".test.tsx")) {
          for (const m of readFileSync(p, "utf8").matchAll(/https?:\/\/[^\s"')]+/g)) hits.push(`${p}: ${m[0]}`);
        }
      }
    };
    walk(join(root, "src"));
    hits.push(...(read("index.html").match(/https?:\/\/[^\s"')]+/g) ?? []));
    expect(hits).toEqual([]);
  });

  it("bundles the font copyright notice and OFL license text", () => {
    const license = "src/assets/fonts/FONT-LICENSES.txt";
    expect(conf.bundle.resources).toEqual({ [`../${license}`]: "FONT-LICENSES.txt" });
    expect(existsSync(join(root, license))).toBe(true);
    const notice = read(license);
    expect(notice).toContain("Copyright 2020 The JetBrains Mono Project Authors");
    expect(notice).toContain("SIL OPEN FONT LICENSE Version 1.1");
  });

  it("bundles only the fonts the notice covers; the interface text is the system font", () => {
    const css = read("src/styles/index.css");
    const families = [...css.matchAll(/@font-face\s*{[^}]*?font-family:\s*"([^"]+)"/g)].map((m) => m[1]);
    expect(families).toEqual(["JetBrains Mono"]);
    expect(readdirSync(join(root, "src/assets/fonts")).filter((f) => f.endsWith(".woff2"))).toEqual(["jetbrains-mono-latin.woff2"]);
    expect(css).toMatch(/--font-sans:\s*-apple-system/);
  });

  it("removes control animation when reduced motion is requested", () => {
    const css = read("src/styles/index.css");
    expect(css).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)/);
    expect(css).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)[\s\S]*?transition:\s*none/);
  });
});
