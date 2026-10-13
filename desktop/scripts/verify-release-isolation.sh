#!/usr/bin/env bash
# Rejects a macOS app bundle that could reach, or points at, the development Python runtime.
# Usage: verify-release-isolation.sh <path to Synapse.app>
# Exit 0: inspected and clean. Exit 1: development-runtime evidence found. Exit 2: the bundle could not be
# inspected (bad layout, missing or invalid executable, a tool failed). Anything but 0 must fail CI.
# These checks are evidence, not proof: see docs/implementation/desktop-runtime-bridge.md#release-isolation.
set -euo pipefail

EXPECTED_EXECUTABLE="synapse-desktop"

die() { echo "ERROR: $1" >&2; exit 2; }
failures=0
fail() { echo "FAIL: $1"; failures=$((failures + 1)); }

[ "$#" -eq 1 ] || die "usage: $0 <Synapse.app>"
app="$1"
desktop="$(cd "$(dirname "$0")/.." && pwd -P)"
checkout="$(cd "$desktop/.." && pwd -P)"

# The executable is the one Info.plist names, and it must be the one Tauri builds; nothing is guessed.
[ -d "$app/Contents" ] || die "not an app bundle: missing Contents/"
plist="$app/Contents/Info.plist"
[ -f "$plist" ] || die "missing Contents/Info.plist"
name="$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$plist" 2>/dev/null)" \
  || die "Info.plist has no CFBundleExecutable"
[ "$name" = "$EXPECTED_EXECUTABLE" ] || die "CFBundleExecutable is '$name', expected '$EXPECTED_EXECUTABLE'"
exe="$app/Contents/MacOS/$name"
[ ! -L "$exe" ] || die "executable is a symlink"
[ -f "$exe" ] || die "executable missing: Contents/MacOS/$name"
[ -r "$exe" ] && [ -x "$exe" ] || die "executable is not readable and executable"
# otool exits 0 on non-Mach-O input; lipo does not.
archs="$(lipo -archs "$exe" 2>/dev/null)" && [ -n "$archs" ] || die "executable is not a Mach-O file"

# Each tool's output is captured first, so a tool failure stops the script instead of looking like "no match".
libraries="$(otool -L "$exe")" || die "otool -L failed"
imports="$(nm -u "$exe")" || die "nm -u failed"
[ -n "$imports" ] || die "nm -u listed no imported symbols"

# 1. Development paths and bridge markers anywhere in the bundle.
needles=("$checkout" ".dev-python" "runtime-bridge" "status.py" "SYNAPSE_DEV_PYTHON" "SYNAPSE_DEV_CHECKOUT"
  "runtime_bridge/process.rs")
if [ -f "$desktop/src-tauri/.dev-python" ]; then
  needles+=("$(tr -d '\n' < "$desktop/src-tauri/.dev-python")")
fi
for needle in "${needles[@]}"; do
  status=0
  grep -r -a -q -F -- "$needle" "$app" || status=$?
  case "$status" in
    0) fail "bundle contains '$needle'" ;;
    1) ;;
    *) die "grep failed while searching for '$needle'" ;;
  esac
done

# 2. Process-group termination exists only in the bridge's spawn code; std and Tauri do not import killpg.
if grep -q -x -F "_killpg" <<<"$imports"; then fail "executable imports _killpg (bridge process code)"; fi

# 3. No Python shipped.
python_files="$(find "$app" \( -iname '*python*' -o -name '*.py' -o -name '*.pyc' \) -print)" || die "find failed"
[ -z "$python_files" ] || fail "bundle contains Python files"

# 4. Only system libraries; a Homebrew, @rpath or checkout path would be a new dependency.
foreign="$(tail -n +2 <<<"$libraries" | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/Library/)' || true)"
[ -z "$foreign" ] || fail "executable links non-system libraries: $foreign"

if [ "$failures" -ne 0 ]; then exit 1; fi
digest="$(shasum -a 256 "$exe" | awk '{print $1}')" || die "shasum failed"
echo "OK: inspected Contents/MacOS/$name (sha256 $digest, $(tail -n +2 <<<"$libraries" | wc -l | tr -d ' ') libraries, $(wc -l <<<"$imports" | tr -d ' ') imported symbols): no development runtime evidence"
