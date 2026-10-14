#!/usr/bin/env bash
# Regression fixtures for verify-native-boundary.mjs. On a disposable copy of src-tauri: the legitimate crate
# passes, each known way of compiling or resolving code outside the approved boundary fails, and the restored
# copy passes again. The real crate is never modified.
# Usage: test-verify-native-boundary.sh
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd -P)"
desktop="$(cd "$here/.." && pwd -P)"
verify="$here/verify-native-boundary.mjs"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
copy="$work/src-tauri"
pristine="$work/pristine"
failures=0

# Dependencies come from the real target directory; the copy is another package path, so its artifacts are
# separate and never replace the real crate's.
export CARGO_TARGET_DIR="$desktop/src-tauri/target"
rsync -a --exclude target --exclude gen "$desktop/src-tauri/" "$pristine/"
# What tauri-build and generate_context! read from outside src-tauri.
mkdir -p "$work/dist" "$work/src/assets/fonts"
if [ -d "$desktop/dist" ]; then cp -R "$desktop/dist/." "$work/dist/"; fi
cp "$desktop/src/assets/fonts/FONT-LICENSES.txt" "$work/src/assets/fonts/"
restore() {
  rsync -a --delete "$pristine/" "$copy/"
  rm -f "$work/Cargo.toml" "$work/Cargo.lock"
}

# expect <label> <exit code> [verifier flags...]
expect() {
  local label="$1" code="$2" output status=0
  shift 2
  output="$(node "$verify" "$@" "$copy" 2>&1)" || status=$?
  if [ "$status" -eq "$code" ]; then
    echo "ok - $label (exit $status)"; grep -E '^(OK|FAIL|ERROR)' <<<"$output" | sed 's/^/    /' | head -6
  else
    echo "not ok - $label: exit $status, expected $code"; sed 's/^/    /' <<<"$output"; failures=$((failures + 1))
  fi
}

# append <file> <text>: adds a line to a file of the copy.
append() { printf '%s\n' "$2" >> "$copy/$1"; }
readdir='pub fn f(p: &std::path::Path) -> usize { std::fs::read_dir(p).map(|d| d.count()).unwrap_or(0) }'

restore
expect "baseline: the legitimate crate passes in every configuration" 0

# Re-review N2: a submodule declared with a same-line attribute.
append src/projects.rs '#[cfg(target_os = "macos")] mod scan;'
printf '%s\n' "$readdir" > "$copy/src/projects/scan.rs"
expect "N2: same-line cfg attribute adds a project submodule" 1 --fail-fast
restore

# Re-review N3: cfg_attr(path = ...) compiles a file outside the inspected module tree.
append src/projects.rs '#[cfg_attr(target_os = "macos", path = "evil.rs")] mod quiet;'
printf '\n' > "$copy/src/projects/quiet.rs"
printf '%s\n' "$readdir" > "$copy/src/evil.rs"
expect "N3: cfg_attr(path) substitutes another source file" 1 --fail-fast
restore

# The same mechanism used to swap an approved module for another file.
cp "$copy/src/projects/folder.rs" "$copy/src/projects/folder_alt.rs"
perl -pi -e 's|^mod folder;$|#[cfg_attr(target_os = "macos", path = "projects/folder_alt.rs")] mod folder;|' "$copy/src/projects.rs"
grep -q 'folder_alt' "$copy/src/projects.rs"
expect "cfg_attr(path) replaces an approved module's file" 1 --fail-fast
restore

# Re-review N4: a top-level module in lib.rs declared with a same-line attribute.
perl -pi -e 's|^mod projects;$|mod projects;\n#[cfg(target_os = "macos")] mod evil;|' "$copy/src/lib.rs"
grep -q '^#\[cfg(target_os = "macos")\] mod evil;$' "$copy/src/lib.rs"
printf '%s\n' "$readdir" > "$copy/src/evil.rs"
expect "N4: same-line cfg attribute adds a module to lib.rs" 1 --fail-fast
restore

# include! of an unlisted file, and a module compiled only in release builds.
append src/lib.rs 'include!("smuggled.rs");'
printf '%s\n' "$readdir" > "$copy/src/smuggled.rs"
expect "include! compiles an unlisted file" 1 --fail-fast
restore
append src/lib.rs '#[cfg(not(debug_assertions))] mod release_only;'
printf '%s\n' "$readdir" > "$copy/src/release_only.rs"
expect "a module compiled only in release builds" 1 --fail-fast
restore

# Re-review N5: [patch.crates-io] substitutes vendored code without changing the direct dependency list. The lock
# file is updated the way a contributor would, so the check fails on provenance, not on a stale lock.
mkdir -p "$copy/vendor/libc/src"
printf '[package]\nname = "libc"\nversion = "0.2.999"\nedition = "2021"\n' > "$copy/vendor/libc/Cargo.toml"
printf '\n' > "$copy/vendor/libc/src/lib.rs"
printf '\n[patch.crates-io]\nlibc = { path = "vendor/libc" }\n' >> "$copy/Cargo.toml"
(cd "$copy" && cargo update --offline --quiet -p libc 2>/dev/null)
grep -A1 '^name = "libc"$' "$copy/Cargo.lock" | grep -q '^version = "0.2.999"$'
# Not fail-fast: both the override table and its effect on resolution must be reported.
expect "N5: [patch.crates-io] resolves libc to vendored code" 1
restore

# The same substitution without any override table: a dependency declared from a local path.
mkdir -p "$copy/vendor/libc/src"
printf '[package]\nname = "libc"\nversion = "0.2.999"\nedition = "2021"\n' > "$copy/vendor/libc/Cargo.toml"
printf '\n' > "$copy/vendor/libc/src/lib.rs"
perl -pi -e 's|^libc = "0.2"$|libc = { path = "vendor/libc" }|' "$copy/Cargo.toml"
grep -q '^libc = { path = "vendor/libc" }$' "$copy/Cargo.toml"
(cd "$copy" && cargo update --offline --quiet -p libc 2>/dev/null)
expect "a dependency resolved from a local path" 1 --fail-fast
restore

# Source replacement in a Cargo configuration file, which cargo metadata cannot see.
mkdir -p "$copy/.cargo"
printf '[source.crates-io]\nreplace-with = "vendored"\n\n[source.vendored]\ndirectory = "vendor"\n' > "$copy/.cargo/config.toml"
expect "a .cargo/config.toml source replacement" 1 --fail-fast
restore

# A parent workspace manifest takes over dependency resolution (and could carry its own [patch]).
printf '[workspace]\nmembers = ["src-tauri"]\nresolver = "2"\n' > "$work/Cargo.toml"
cp "$copy/Cargo.lock" "$work/Cargo.lock"
expect "a parent workspace manifest" 1 --fail-fast
restore

expect "restored: the legitimate crate passes again" 0

if [ "$failures" -ne 0 ]; then echo "$failures verifier test(s) failed"; exit 1; fi
echo "all native boundary verifier tests passed"
