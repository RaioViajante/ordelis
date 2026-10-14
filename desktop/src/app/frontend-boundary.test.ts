// @vitest-environment node
// The production bundle may only contain the app's own src/ modules and its pinned packages, and no project
// command may be reachable from it yet (the Projects UI is a later increment).
//
// The inspector below looks at what a production Vite build of the desktop UI actually contains, rather than
// which files happen to exist under src/. It builds without writing anything and reads Rollup's module graph and
// output. It lives in this test file so that the app-source scan in Workspace.test.tsx stays without exceptions.
//
// These are defense-in-depth tripwires for code review. The authority over which native commands may run is the
// Tauri capability ACL (src-tauri/capabilities/default.json and its Rust tests). A command name assembled at run
// time is not detectable here: the literal check below only catches names written out in the bundle.
import { build, type Plugin } from "vite";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, sep } from "node:path";

const PROJECT_COMMANDS = /\b(?:list_projects|add_project|remove_project|reset_projects)\b/;
// Every extension Vite resolves as a script module.
const SCRIPT = /\.(?:[cm]?[jt]sx?)$/;
// Rollup ids of Vite's own helpers; anything else virtual (such as an inline index.html script) is refused.
const VITE_INTERNAL = new Set(["\0vite/modulepreload-polyfill.js", "\0vite/preload-helper.js"]);

type Output = { type: "chunk"; fileName: string; code: string } | { type: "asset"; fileName: string };

function isTestSource(root: string, file: string): boolean {
  return /\.(?:test|spec)\.[cm]?[jt]sx?$/.test(file) || file.startsWith(join(root, "src", "test") + sep);
}

/** Builds `projectRoot` (a desktop/ directory with its own vite.config.ts) and returns every violation found. */
async function frontendBuildViolations(projectRoot: string): Promise<string[]> {
  const root = realpathSync(projectRoot);
  const src = join(root, "src") + sep;
  const packages = realpathSync(join(root, "node_modules")) + sep;
  const tauriApi = `${sep}node_modules${sep}@tauri-apps${sep}`;
  const adapter = join(root, "src", "app", "runtimeStatus.ts");
  const graph = new Map<string, string[]>();
  const outputs: Output[] = [];
  const inspector: Plugin = {
    name: "synapse-boundary-inspector",
    enforce: "post",
    generateBundle(_options, bundle) {
      for (const id of this.getModuleIds()) {
        const info = this.getModuleInfo(id);
        graph.set(id, [...(info?.importers ?? []), ...(info?.dynamicImporters ?? [])]);
      }
      for (const out of Object.values(bundle)) {
        outputs.push(out.type === "chunk" ? { type: "chunk", fileName: out.fileName, code: out.code } : { type: "asset", fileName: out.fileName });
      }
    },
  };
  await build({
    root,
    configFile: join(root, "vite.config.ts"),
    mode: "production",
    logLevel: "silent",
    build: { write: false },
    plugins: [inspector],
  });

  const violations: string[] = [];
  const name = (id: string) => (id.startsWith(root + sep) ? id.slice(root.length + 1) : JSON.stringify(id));
  if (graph.size === 0) violations.push("the build produced no module graph");
  for (const [id, importers] of graph) {
    if (VITE_INTERNAL.has(id)) continue;
    const query = id.indexOf("?");
    const file = query === -1 ? id : id.slice(0, query);
    if (file.startsWith(packages)) {
      // Only the one reviewed adapter may load the Tauri API; packages inside it may load each other.
      if (file.includes(tauriApi)) {
        for (const importer of importers) {
          if (importer !== adapter && !importer.includes(tauriApi)) violations.push(`${name(importer)}: imports the Tauri API`);
        }
      }
      continue;
    }
    if (query !== -1) {
      violations.push(`${name(id)}: imported with a query (raw, url, worker or inline code)`);
      continue;
    }
    if (file === join(root, "index.html")) continue;
    if (!file.startsWith(src)) {
      violations.push(`${name(id)}: bundled from outside src/`);
      continue;
    }
    if (isTestSource(root, file)) violations.push(`${name(id)}: test code in the production bundle`);
    if (SCRIPT.test(file)) {
      const text = readFileSync(file, "utf8");
      if (PROJECT_COMMANDS.test(text)) violations.push(`${name(id)}: names a project command`);
      if (text.includes("__TAURI")) violations.push(`${name(id)}: reaches Tauri internals`);
    }
  }
  for (const out of outputs) {
    // A script emitted as a plain asset (new URL(..., import.meta.url), a worker) bypasses the module graph.
    if (out.type === "asset" && SCRIPT.test(out.fileName)) violations.push(`emits script asset ${out.fileName}`);
    if (out.type === "chunk" && PROJECT_COMMANDS.test(out.code)) violations.push(`${out.fileName}: names a project command`);
  }
  return violations;
}


const root = join(__dirname, "..", "..");
const BUILD_TIMEOUT = 180_000;

// A disposable copy of the frontend in the OS temp directory, sharing node_modules through a symlink.
function fixture(change: (dir: string) => void): string {
  const dir = mkdtempSync(join(tmpdir(), "synapse-frontend-boundary-"));
  for (const entry of ["index.html", "vite.config.ts", "package.json", "tsconfig.json", "src"]) {
    cpSync(join(root, entry), join(dir, entry), { recursive: true });
  }
  symlinkSync(join(root, "node_modules"), join(dir, "node_modules"));
  change(dir);
  return dir;
}

function write(dir: string, file: string, text: string) {
  mkdirSync(dirname(join(dir, file)), { recursive: true });
  writeFileSync(join(dir, file), text);
}

function prepend(dir: string, file: string, text: string) {
  write(dir, file, text + readFileSync(join(dir, file), "utf8"));
}

const RESET = 'window.__TAURI_INTERNALS__.invoke("reset_projects");\n';

describe("production bundle boundary", () => {
  it("accepts the app as it is", async () => {
    expect(await frontendBuildViolations(root)).toEqual([]);
  }, BUILD_TIMEOUT);

  it("rejects every reviewed way of bundling code from elsewhere or reaching a project command", async () => {
    const cases: Record<string, (dir: string) => void> = {
      // Re-review N1: a root-absolute import of a file outside src/.
      "root-absolute import": (d) => {
        write(d, "scripts/boot.js", RESET);
        prepend(d, "src/main.tsx", 'import "/scripts/boot.js";\n');
      },
      "relative import from outside src/": (d) => {
        write(d, "scripts/boot.js", "export const x = 1;\n");
        prepend(d, "src/main.tsx", 'import "../scripts/boot.js";\n');
      },
      "dynamic import": (d) => {
        write(d, "scripts/late.js", "export const x = 1;\n");
        prepend(d, "src/main.tsx", 'void import("/scripts/late.js");\n');
      },
      "import.meta.glob": (d) => {
        write(d, "scripts/glob/a.js", "export const x = 1;\n");
        prepend(d, "src/main.tsx", 'import.meta.glob("/scripts/glob/*.js", { eager: true });\n');
      },
      "new URL(..., import.meta.url) worker": (d) => {
        write(d, "src/app/work.js", "self.postMessage(1);\n");
        prepend(d, "src/main.tsx", 'new Worker(new URL("./app/work.js", import.meta.url));\n');
      },
      "?url script asset": (d) => {
        write(d, "src/app/later.js", "export const x = 1;\n");
        prepend(d, "src/main.tsx", 'import later from "./app/later.js?url";\nconsole.log(later);\n');
      },
      "inline script in index.html": (d) => {
        const html = readFileSync(join(d, "index.html"), "utf8");
        write(d, "index.html", html.replace("</body>", '<script type="module">console.log(1)</script></body>'));
      },
      // The first review's bypass: a startup module inside src/ with another extension.
      "startup .js module calling reset_projects": (d) => {
        write(d, "src/app/boot.js", RESET);
        prepend(d, "src/main.tsx", 'import "./app/boot.js";\n');
      },
      "another module importing @tauri-apps": (d) => {
        write(d, "src/app/boot.mjs", 'import { isTauri } from "@tauri-apps/api/core";\nconsole.log(isTauri());\n');
        prepend(d, "src/main.tsx", 'import "./app/boot.mjs";\n');
      },
      "test code imported at startup": (d) => {
        write(d, "src/app/boot.test.ts", "export const x = 1;\n");
        prepend(d, "src/main.tsx", 'import "./app/boot.test";\n');
      },
      "a project command named in the adapter": (d) => {
        prepend(d, "src/app/runtimeStatus.ts", 'export const NEXT = "add_project";\n');
      },
    };
    const dirs: string[] = [];
    try {
      // The copy itself must be clean, so each failure below is caused by its change alone.
      const control = fixture(() => {});
      dirs.push(control);
      expect(await frontendBuildViolations(control)).toEqual([]);
      for (const [label, change] of Object.entries(cases)) {
        const dir = fixture(change);
        dirs.push(dir);
        expect(await frontendBuildViolations(dir), label).not.toEqual([]);
      }
    } finally {
      for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
    }
  }, BUILD_TIMEOUT);
});
