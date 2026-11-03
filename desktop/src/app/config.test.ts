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
];
const read = (p: string) => readFileSync(join(root, p), "utf8");

// The files rustc compiles for the desktop library in each macOS configuration. verify-native-boundary.mjs checks
// this list against the compiler's dep-info, so any other compiled file fails CI however it was brought in.
const nativeSources: { configurations: Record<string, string[]> } = JSON.parse(read("scripts/native-sources.json"));

// The production project registry files and the storage protocol they use (src/store.rs), as listed (and
// compiler-verified) above, keyed by path under src-tauri/src.
function projectSources(): Record<string, string> {
  const production = new Set(
    ["default", "release", "dev-runtime"]
      .flatMap((c) => nativeSources.configurations[c])
      .filter((f) => f.startsWith("src/projects") || f === "src/store.rs"),
  );
  return Object.fromEntries([...production].map((f) => [f.slice("src/".length), read(`src-tauri/${f}`)]));
}

// Criterion 21's one reviewed exception (issue #82): registry.rs may list the app data directory it was given, once,
// in backup_present, to see whether a corrupt-registry backup exists. Exactly this call in that file is set aside
// before the rules below; every other listing, in any file, is still a violation. Its scope is pinned below.
const REVIEWED_LISTING = "std::fs::read_dir(dir)";

// Content rules for the project registry: no listing, contents, processes, network, Git, Python or gate, and only
// registry.rs and store.rs touch files. Which files exist is not decided here; the compiler-verified list above decides it.
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

describe("desktop trust boundary", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json"));

  it("grants the main window exactly six app permissions and no other", () => {
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

  it("registers no Tauri plugins or sidecars and exactly six declared app commands", () => {
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
    expect(rust.match(/#\[tauri::command\]/g)).toHaveLength(6);
    // The project commands take no path: add_project, list_projects and reset_projects take no argument from
    // the webview, and remove_project takes only the registration id.
    expect(rust).toMatch(/async fn list_projects\(projects: State<'_, Projects>\)/);
    expect(rust).toMatch(/async fn add_project<R: Runtime>\(\s*window: WebviewWindow<R>,\s*projects: State<'_, Projects>,\s*picker: State<'_, Picker<R>>,\s*\)/);
    expect(rust).toMatch(/async fn remove_project\(id: String, projects: State<'_, Projects>\)/);
    expect(rust).toMatch(/async fn reset_projects\(projects: State<'_, Projects>\)/);
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
    expect(Object.keys(files).sort()).toEqual(["projects.rs", "projects/folder.rs", "projects/registry.rs", "store.rs"]);
    expect(projectContentViolations(files)).toEqual([]);
    const production = (p: string) => {
      const text = read(p);
      // registry.rs ends with its test module declaration; tests live in their own file.
      return p.endsWith("registry.rs") ? text.slice(0, text.lastIndexOf("#[cfg(test)]\nmod tests;")) : text;
    };
    // The registry's files are opened only in the store's four functions, and only under the registry's own fixed
    // names: the store holds no name of its own and receives only FILES. The registry itself only links the
    // corrupt file to its backup name and then removes the registry name (reset).
    const registry = production("src-tauri/src/projects/registry.rs");
    const store = read("src-tauri/src/store.rs");
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
    expect(registry.match(/store::\w+\([^)]*\)/g)).toEqual([
      "store::read(dir, &FILES, MAX_BYTES)",
      "store::lock(dir, &FILES, create)",
      "store::replace(dir, &FILES, &bytes)",
    ]);
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
    };
    for (const [name, files] of Object.entries(bypasses)) {
      expect(projectContentViolations(files), name).not.toEqual([]);
    }
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
    // Exactly the two reviewed adapters load the Tauri API.
    expect(sources.filter((f) => /@tauri-apps/.test(read(`src/${f}`))).sort()).toEqual(["app/projects.ts", "app/runtimeStatus.ts"]);

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
