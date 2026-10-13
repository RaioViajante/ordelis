import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const root = join(__dirname, "..", "..");
const read = (p: string) => readFileSync(join(root, p), "utf8");

describe("desktop trust boundary", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json"));

  it("grants the main window exactly two app permissions and no other", () => {
    const dir = join(root, "src-tauri", "capabilities");
    const files = readdirSync(dir).filter((f) => f.endsWith(".json"));
    expect(files).toEqual(["default.json"]);
    const cap = JSON.parse(readFileSync(join(dir, "default.json"), "utf8"));
    // No shell, fs, network, opener or core plugin permission, and no remote origin.
    expect(cap.permissions).toEqual(["allow-ipc-boundary-status", "allow-get-runtime-status"]);
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

  it("registers no Tauri plugins or sidecars and exactly two declared app commands", () => {
    const cargo = read("src-tauri/Cargo.toml");
    expect(cargo).not.toMatch(/tauri-plugin|sidecar/i);
    // The runtime dependencies are tauri, serde, serde_json (response parsing) and libc (poll and process
    // groups for the bridge); adding another is a deliberate review decision.
    const runtime = cargo.split("[dependencies]")[1].split("[dev-dependencies]")[0];
    expect(runtime.match(/^[a-z_-]+(?= *=)/gm)).toEqual(["tauri", "serde", "serde_json", "libc"]);
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
    expect(rust.match(/generate_handler!\[([^\]]*)\]/)?.[1].trim()).toBe("ipc_boundary_status, get_runtime_status");
    expect(rust.match(/#\[tauri::command\]/g)).toHaveLength(2);
    const bridge = read("src-tauri/src/runtime_bridge.rs");
    expect(bridge).not.toMatch(/Command::|std::process|std::fs|std::env/);
    expect(bridge).toMatch(/#\[cfg\(any\(test, all\(feature = "dev-runtime", target_os = "macos"\)\)\)\]\s*mod process;/);
    // The app manifest is what makes Tauri enforce the capability for local-origin app commands.
    const build = read("src-tauri/build.rs");
    expect(build).toMatch(/AppManifest::new\(\)\s*\.commands\(&\["ipc_boundary_status", "get_runtime_status"\]\)/);
    expect(conf.bundle.externalBin).toBeUndefined();
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
