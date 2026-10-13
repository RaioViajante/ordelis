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

  it("pins every Cargo dependency table, including target-specific ones", () => {
    const cargo = read("src-tauri/Cargo.toml");
    expect(cargo).not.toMatch(/tauri-plugin|sidecar/i);
    // Every table that can hold dependencies, whatever its spelling ([x.dependencies], [x.dependencies.name],
    // [target.'cfg(...)'.dependencies]) and every dotted key mentioning dependencies outside a header.
    const lines = cargo.split("\n").filter((l) => !l.trimStart().startsWith("#"));
    const headers = lines.filter((l) => l.startsWith("["));
    expect(headers.filter((h) => h.includes("dependencies"))).toEqual([
      "[build-dependencies]",
      "[dependencies]",
      `[target.'cfg(target_os = "macos")'.dependencies]`,
      "[dev-dependencies]",
    ]);
    expect(lines.filter((l) => !l.startsWith("[") && l.includes("dependencies"))).toEqual([]);
    const tables: Record<string, string[]> = {};
    let current = "";
    for (const line of lines) {
      if (line.startsWith("[")) current = line;
      else if (/^[A-Za-z0-9_-]+ *=/.test(line)) (tables[current] ??= []).push(line.split("=")[0].trim());
    }
    // tauri, serde, serde_json (response parsing) and libc (poll and process groups for the bridge, file
    // locking and identity for the project registry); adding another is a deliberate review decision.
    expect(tables["[dependencies]"]).toEqual(["tauri", "serde", "serde_json", "libc"]);
    expect(tables["[build-dependencies]"]).toEqual(["tauri-build"]);
    expect(tables["[dev-dependencies]"]).toEqual(["tauri"]);
    // The native folder panel, macOS only, without its Linux-only default features (xdg-portal, wayland).
    expect(tables[`[target.'cfg(target_os = "macos")'.dependencies]`]).toEqual(["rfd"]);
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
    const module = ["src-tauri/src/projects.rs", "src-tauri/src/projects/folder.rs", "src-tauri/src/projects/registry.rs"];
    const production = (p: string) => {
      const text = read(p);
      // registry.rs ends with its test module declaration; tests live in their own file.
      return p.endsWith("registry.rs") ? text.slice(0, text.lastIndexOf("#[cfg(test)]\nmod tests;")) : text;
    };
    for (const file of module) {
      const rust = production(file).replaceAll("spawn_blocking", "");
      expect(rust, file).not.toMatch(
        /read_dir|walkdir|glob|std::process|Command::|\bspawn\b|std::net|TcpStream|UdpSocket|reqwest|hyper|tokio::net|\.plugin\(/,
      );
      expect(rust, file).not.toMatch(/runtime_bridge|python|\bgit\b|[Gg]ate|execute|provider|agent/);
      expect(rust, file).not.toMatch(/fs::read\b|read_to_string|fs::write|fs::copy|File::create|remove_dir|set_permissions|create_dir(?:_all)?\(|std::env/);
    }
    // Only folder.rs looks at chosen or registered folders, through canonicalize and lstat alone.
    expect(production(module[0])).not.toMatch(/std::fs|\bFile\b|OpenOptions|canonicalize|metadata\(/);
    const folder = production(module[1]);
    expect(folder).not.toMatch(/\bFile\b|OpenOptions|\bopen\(|remove_file|rename|hard_link|DirBuilder/);
    expect(folder.match(/std::fs::\w+/g)?.sort()).toEqual([
      "std::fs::Metadata",
      "std::fs::canonicalize",
      "std::fs::canonicalize",
      "std::fs::canonicalize",
      "std::fs::symlink_metadata",
      "std::fs::symlink_metadata",
      "std::fs::symlink_metadata",
    ]);
    // The registry opens files only in its four registry-file functions, and changes only its own fixed names.
    const registry = production(module[2]);
    const functions = [...registry.matchAll(/^(?:pub\(super\) )?fn (\w+)[\s\S]*?^\}/gm)];
    const opening = functions.filter((f) => /OpenOptions|File::open/.test(f[0])).map((f) => f[1]);
    expect(opening).toEqual(["open_registry", "open_lock", "create_temp", "open_dir"]);
    expect(registry.match(/OpenOptions::new\(\)/g)).toHaveLength(4);
    expect(registry).not.toMatch(/File::open/);
    expect(registry.match(/remove_file\([^)]*\)/g)).toEqual(["remove_file(&registry)", "remove_file(&temp)", "remove_file(&temp)"]);
    expect(registry.match(/rename\([^)]*\)/g)).toEqual(["rename(&temp, dir.join(REGISTRY)"]);
    expect(registry.match(/hard_link\([^)]*\)/g)).toEqual(["hard_link(registry, &backup)"]);
    expect(registry.match(/DirBuilder::new\(\)[^\n]*/g)).toEqual(["DirBuilder::new().mode(0o700).create(dir) {"]);
    expect(registry).toMatch(/^const REGISTRY: &str = "projects\.json";$/m);
    expect(registry).toMatch(/^const LOCK: &str = "projects\.lock";$/m);
    expect(registry).toMatch(/^const TEMP: &str = "projects\.json\.tmp";$/m);
    expect(registry).toMatch(/^const BACKUP_PREFIX: &str = "projects\.json\.corrupt-";$/m);
    // Failure injection is compiled out of production builds.
    expect(registry).toMatch(/#\[cfg\(not\(test\)\)\]\nfn fault\(_: Step\) -> std::io::Result<\(\)> \{\n {4}Ok\(\(\)\)\n\}/);
    // The registry and the panel are macOS-only; other targets answer unsupported_platform.
    const projects = read(module[0]);
    expect(projects).toMatch(/#\[cfg\(target_os = "macos"\)\]\nmod folder;\n#\[cfg\(target_os = "macos"\)\]\nmod registry;/);
    expect(projects.match(/rfd::/g)).toEqual(["rfd::"]);
    expect(projects).toMatch(/\.set_can_create_directories\(false\)/);
  });

  // Reviewed in issue #40: the frontend may depend on @tauri-apps/api, and only to call get_runtime_status.
  it("depends only on @tauri-apps/api, pinned to the Rust tauri crate's version", () => {
    const pkg = JSON.parse(read("package.json"));
    const deps = Object.keys({ ...pkg.dependencies });
    expect(deps.filter((d) => d.startsWith("@tauri-apps"))).toEqual(["@tauri-apps/api"]);
    const crate = read("src-tauri/Cargo.lock").match(/name = "tauri"\nversion = "([^"]+)"/)?.[1];
    expect(pkg.dependencies["@tauri-apps/api"]).toBe(crate);
  });

  it("calls exactly one native command, from one module, with no arguments", () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const p = join(dir, name);
        if (statSync(p).isDirectory()) walk(p);
        else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)) files.push(p);
      }
    };
    walk(join(root, "src"));
    const users = files.filter((f) => readFileSync(f, "utf8").includes("@tauri-apps"));
    expect(users).toEqual([join(root, "src", "app", "runtimeStatus.ts")]);

    const adapter = read("src/app/runtimeStatus.ts");
    expect(adapter.match(/from "@tauri-apps[^"]*"/g)).toEqual(['from "@tauri-apps/api/core"']);
    expect(adapter).toMatch(/^import \{ invoke, isTauri \} from "@tauri-apps\/api\/core";$/m);
    expect(adapter).toMatch(/^const COMMAND = "get_runtime_status";$/m);
    // The project commands have no frontend caller yet: the Projects UI, and with it the confirmed reset, is a
    // separate increment.
    for (const file of files) {
      expect(readFileSync(file, "utf8"), file).not.toMatch(/list_projects|add_project|remove_project|reset_projects/);
    }
    // One call site, with the fixed name and nothing else; invoke itself is never handed to other code.
    expect(adapter.match(/\binvoke\b/g)).toHaveLength(2);
    expect(adapter).toMatch(/\binvoke\(COMMAND\)/);
    expect(adapter).not.toMatch(/export[^\n]*\binvoke\b(?!\()|export \{|export \*/);
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

  it("bundles the font copyright notices and OFL license texts", () => {
    const license = "src/assets/fonts/FONT-LICENSES.txt";
    expect(conf.bundle.resources).toEqual({ [`../${license}`]: "FONT-LICENSES.txt" });
    expect(existsSync(join(root, license))).toBe(true);
    const notice = read(license);
    expect(notice).toContain('Copyright © 2017 IBM Corp. with Reserved Font Name "Plex"');
    expect(notice).toContain("Copyright 2020 The JetBrains Mono Project Authors");
    expect(notice).toContain("Copyright 2020 The Newsreader Project Authors");
    expect(notice).toContain("SIL OPEN FONT LICENSE Version 1.1");
  });

  it("removes control animation when reduced motion is requested", () => {
    const css = read("src/styles/index.css");
    expect(css).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)/);
    expect(css).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)[\s\S]*?transition:\s*none/);
  });
});
