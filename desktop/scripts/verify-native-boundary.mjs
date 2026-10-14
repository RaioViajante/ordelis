#!/usr/bin/env node
// Verifies what the desktop library is compiled from and where its dependencies come from, using Cargo's and
// rustc's own outputs instead of parsing Rust or TOML text.
//
// 1. Provenance: `cargo metadata` with dependency resolution (not --no-deps). Every resolved package other than
//    this crate must come from crates.io, and the crate must be its own workspace root, so a [patch], [replace],
//    path, git or other-registry substitution, here or in a parent manifest, fails. Source replacement in a
//    .cargo/config file is invisible to metadata, so tracked Cargo config files are refused outright.
// 2. Compiled files: rustc --emit=dep-info for each macOS configuration (default, dev-runtime, test, release)
//    lists every file the compiler read, including modules from any declaration form, #[path], cfg_attr and
//    include!/include_str!/include_bytes!. The set must equal scripts/native-sources.json.
//
// Usage: verify-native-boundary.mjs [--fail-fast] [<src-tauri dir>]
// Exit 0: inspected and clean. Exit 1: violations. Exit 2: could not inspect. Anything but 0 must fail CI.
// Scope: the macOS host target. User-level Cargo configuration ($CARGO_HOME, CARGO_* variables) belongs to the
// build machine, not the repository, and is not inspected.
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const failFast = args.includes("--fail-fast");
const crateArg = args.find((a) => !a.startsWith("--")) ?? join(here, "..", "src-tauri");
const CARGO = process.env.CARGO ?? "cargo";
const CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index";
const CONFIGURATIONS = {
  default: [],
  "dev-runtime": ["--features", "dev-runtime"],
  test: ["--profile", "test"],
  release: ["--release"],
};

class InspectionError extends Error {}
const violations = [];

function run(command, commandArgs, cwd) {
  try {
    return execFileSync(command, commandArgs, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], maxBuffer: 256 << 20 });
  } catch (error) {
    const detail = String(error.stderr || error.message).trim().split("\n").slice(-4).join(" | ");
    throw new InspectionError(`${command} ${commandArgs.slice(0, 2).join(" ")} failed: ${detail}`);
  }
}

// Make-style dep-info: "target: dep dep ...", with spaces in names escaped as "\ ". Returns the dependencies of
// the rule whose target is `output`; comment lines (# env-dep:...) are not files.
function dependencies(text, output) {
  for (const line of text.split("\n")) {
    if (line.startsWith("#") || !line.trim()) continue;
    const tokens = line.match(/(?:\\.|[^\s\\])+/g).map((t) => t.replace(/\\(.)/g, "$1"));
    if (tokens[0] === `${output}:`) return tokens.slice(1);
  }
  throw new InspectionError(`dep-info has no rule for ${output}`);
}

// Override tables and Cargo configuration files, checked before resolution because some of them (source
// replacement) change what is fetched without changing what cargo metadata reports.
function overrides(crate) {
  const manifest = readFileSync(join(crate, "Cargo.toml"), "utf8");
  for (const line of manifest.split("\n")) {
    if (/^\s*\[{1,2}\s*["']?(?:patch|replace)\b|^\s*["']?(?:patch|replace)["']?\s*\./.test(line)) violations.push(`Cargo.toml overrides dependencies: ${line.trim()}`);
  }
  // Cargo reads .cargo/config(.toml) from the crate directory and every ancestor; refuse any inside the repository.
  for (let dir = crate; ; dir = dirname(dir)) {
    for (const name of ["config", "config.toml"]) {
      if (existsSync(join(dir, ".cargo", name))) violations.push(`Cargo configuration file ${join(dir, ".cargo", name)}`);
    }
    if (existsSync(join(dir, ".git")) || dirname(dir) === dir) break;
  }
}

function provenance(crate) {
  const host = run("rustc", ["-vV"], crate).match(/^host: (\S+)$/m)?.[1];
  if (!host) throw new InspectionError("rustc -vV reported no host target");
  const metadata = JSON.parse(run(CARGO, ["metadata", "--format-version", "1", "--locked", "--offline", "--filter-platform", host], crate));
  if (realpathSync(metadata.workspace_root) !== crate) {
    violations.push(`workspace root is ${metadata.workspace_root}: a parent manifest controls dependency resolution`);
  }
  if (metadata.packages.length < 2) throw new InspectionError("cargo metadata resolved no dependencies");
  for (const pkg of metadata.packages) {
    if (pkg.source === null) {
      if (realpathSync(dirname(pkg.manifest_path)) !== crate) violations.push(`${pkg.name} ${pkg.version} resolves to local path ${dirname(pkg.manifest_path)}`);
    } else if (pkg.source !== CRATES_IO) {
      violations.push(`${pkg.name} ${pkg.version} comes from ${pkg.source}`);
    }
  }
  return realpathSync(metadata.target_directory);
}

function compiledFiles(crate, target, approved, work) {
  // Hashed outputs of this crate's own build script (tauri-build's embedded assets): no extension, never Rust.
  const buildOutput = new RegExp(`^${target.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/(?:debug|release)/build/synapse-desktop-[0-9a-f]+/out/[0-9a-f]{64}$`);
  for (const [name, flags] of Object.entries(CONFIGURATIONS)) {
    const output = join(work, `${name}.d`);
    run(CARGO, ["rustc", "--lib", "--locked", "--offline", "--quiet", ...flags, "--", `--emit=dep-info=${output}`], crate);
    const found = new Set();
    for (const dep of dependencies(readFileSync(output, "utf8"), output)) {
      const file = resolve(crate, dep);
      if (buildOutput.test(file)) continue;
      if (file.startsWith(crate + sep)) found.add(relative(crate, file));
      else violations.push(`${name}: compiles ${file} from outside the crate`);
    }
    const expected = new Set(approved[name] ?? []);
    if (expected.size === 0) throw new InspectionError(`native-sources.json lists nothing for ${name}`);
    const before = violations.length;
    for (const file of found) if (!expected.has(file)) violations.push(`${name}: compiles unapproved ${file}`);
    for (const file of expected) if (!found.has(file)) violations.push(`${name}: does not compile approved ${file}`);
    if (failFast && violations.length > before) return;
  }
}

try {
  if (!existsSync(join(crateArg, "Cargo.toml"))) throw new InspectionError(`no Cargo.toml in ${crateArg}`);
  const crate = realpathSync(crateArg);
  const approved = JSON.parse(readFileSync(join(here, "native-sources.json"), "utf8")).configurations;
  overrides(crate);
  const target = failFast && violations.length ? null : provenance(crate);
  if (target && !(failFast && violations.length)) {
    const work = mkdtempSync(join(tmpdir(), "synapse-dep-info-"));
    try {
      compiledFiles(crate, target, approved, work);
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  }
} catch (error) {
  if (!(error instanceof InspectionError)) throw error;
  // Fail closed either way: violations already found are reported, and an inspection error alone is exit 2.
  for (const v of violations) console.log(`FAIL: ${v}`);
  console.error(`ERROR: ${error.message}`);
  process.exit(violations.length ? 1 : 2);
}
if (violations.length) {
  for (const v of violations) console.log(`FAIL: ${v}`);
  process.exit(1);
}
console.log(`OK: dependencies resolve to crates.io only; compiled files match native-sources.json for ${Object.keys(CONFIGURATIONS).join(", ")}`);
