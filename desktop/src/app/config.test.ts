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

// Rust code with comments removed and every string and character literal emptied, so that documentation text (URLs,
// flag names in notes) is never mistaken for code and code is never hidden in a comment or behind a quote character.
// Path separators lose their surrounding spaces (`crate :: x` is `crate::x`). Raw strings are refused separately.
const CHAR_LITERAL = /'(?:\\u\{[0-9A-Fa-f]{1,6}\}|\\x[0-9A-Fa-f]{2}|\\.|[^\\'\n])'/y;
function rustCodeOnly(text: string): string {
  let out = "";
  for (let i = 0; i < text.length; ) {
    CHAR_LITERAL.lastIndex = i;
    if (text.startsWith("//", i)) {
      const end = text.indexOf("\n", i);
      i = end < 0 ? text.length : end;
    } else if (text.startsWith("/*", i)) {
      const end = text.indexOf("*/", i + 2);
      i = end < 0 ? text.length : end + 2;
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
      // The rest of the crate: by `crate`, by the sibling modules' names, or through `super` from agents.rs (where it
      // is the crate root). catalog.rs and evidence.rs may name only each other through `super`.
      /(?<!pub\()\bcrate\b|\b(?:runtime_bridge|store|projects|tasks|lib|main)\b/,
      file === "agents.rs" ? /\bsuper\b/ : /\bsuper\b(?!::(?:catalog|evidence)\b)/,
      /\bpub\s+(?!\((?:crate|super)\))/,
      /\b\w*(?:path|argv|args|arguments|dirs?(?![a-z])|director|search|location|candidate|command|launch|invoke)\w*\b/i,
    ];
    for (const pattern of forbidden) {
      const hit = code.match(pattern);
      if (hit) violations.push(`${file}: ${hit[0]}`);
    }
    for (const m of code.matchAll(/\b(\w+)\s*!\s*[([{]/g)) if (m[1] !== "matches") violations.push(`${file}: ${m[1]}!`);
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
    expect(lib).toMatch(/\n#\[cfg_attr\(not\(test\), allow\(dead_code\)\)\]\nmod agents;\nmod projects;\n/);
    // No other compiled production file uses it.
    const others = ["default", "release", "dev-runtime"]
      .flatMap((c) => nativeSources.configurations[c])
      .filter((f) => !f.startsWith("src/agents") && f !== "src/lib.rs");
    for (const file of new Set(others)) expect(rustCodeOnly(read(`src-tauri/${file}`)), file).not.toMatch(/\bagents\b/);
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
    };
    for (const [name, files] of Object.entries(bypasses)) {
      expect(agentContentViolations(files), name).not.toEqual([]);
    }
    // Documentation text is data: a string or a comment that mentions a process or a path is not code.
    const text = add("agents/catalog.rs", 'const U: &str = "std::process::Command::new(\\"codex\\") in /usr/bin"; // std::fs::read');
    expect(agentContentViolations(text)).toEqual([]);
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
