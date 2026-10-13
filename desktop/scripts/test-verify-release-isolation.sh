#!/usr/bin/env bash
# Negative and positive tests for verify-release-isolation.sh, using controlled fixture bundles.
# Usage: test-verify-release-isolation.sh [<dev-runtime Synapse.app>]
# With a bundle built with --features dev-runtime, also requires the verifier to reject that real artifact
# through each independent check.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd -P)"
verify="$here/verify-release-isolation.sh"
work="$(mktemp -d)"
trap 'chmod -R u+rwx "$work" 2>/dev/null; rm -rf "$work"' EXIT
failures=0

# Expect <code> from the verifier on <app>, optionally with <text> in its output.
expect() {
  local label="$1" code="$2" app="$3" text="${4:-}" output status=0
  output="$("$verify" "$app" 2>&1)" || status=$?
  if [ "$status" -ne "$code" ]; then
    echo "not ok - $label: exit $status, expected $code"; echo "$output" | sed 's/^/    /'; failures=$((failures + 1))
  elif [ -n "$text" ] && ! grep -q -F -- "$text" <<<"$output"; then
    echo "not ok - $label: output lacks '$text'"; echo "$output" | sed 's/^/    /'; failures=$((failures + 1))
  else
    echo "ok - $label"
  fi
}

printf '#include <stdio.h>\nint main(void) { return puts("clean") < 0; }\n' > "$work/clean.c"
printf '#include <signal.h>\nint main(void) { return killpg(0, 0); }\n' > "$work/killpg.c"
cc -o "$work/clean" "$work/clean.c"
cc -o "$work/killpg" "$work/killpg.c"

# bundle <dir> [<executable fixture>] [<CFBundleExecutable>]
bundle() {
  local app="$work/$1.app" binary="${2:-}" name="${3:-synapse-desktop}"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  /usr/libexec/PlistBuddy -c "Add :CFBundleExecutable string $name" "$app/Contents/Info.plist" >/dev/null
  if [ -n "$binary" ]; then cp "$binary" "$app/Contents/MacOS/synapse-desktop"; fi
  echo "$app"
}

expect "clean fixture passes and names the inspected executable" 0 "$(bundle clean "$work/clean")" \
  "inspected Contents/MacOS/synapse-desktop (sha256 "

# Layout and executable problems are inspection errors (2), never success.
expect "missing executable" 2 "$(bundle missing)" "executable missing"
empty="$(bundle empty)"; rm -f "$empty/Contents/MacOS/"*
expect "empty Contents/MacOS" 2 "$empty" "executable missing"
mkdir -p "$work/nocontents.app"
expect "no Contents directory" 2 "$work/nocontents.app" "missing Contents/"
noplist="$(bundle noplist "$work/clean")"; rm "$noplist/Contents/Info.plist"
expect "no Info.plist" 2 "$noplist" "missing Contents/Info.plist"
expect "unexpected CFBundleExecutable" 2 "$(bundle renamed "$work/clean" other-binary)" "expected 'synapse-desktop'"
text="$(bundle text)"; printf '#!/bin/sh\n' > "$text/Contents/MacOS/synapse-desktop"; chmod 755 "$text/Contents/MacOS/synapse-desktop"
expect "executable is not Mach-O" 2 "$text" "not a Mach-O"
unreadable="$(bundle unreadable "$work/clean")"; chmod 000 "$unreadable/Contents/MacOS/synapse-desktop"
expect "unreadable executable" 2 "$unreadable" "not readable"
link="$(bundle link)"; ln -s "$work/clean" "$link/Contents/MacOS/synapse-desktop"
expect "symlinked executable" 2 "$link" "symlink"

# A failing inspection tool must stop the script, not read as "nothing found": a tool that fails silently,
# one that prints plausible output and then fails, and nm succeeding with no symbols at all.
mkdir -p "$work/shim"
shim() {
  local label="$1" tool="$2" body="$3" status=0 output
  printf '#!/bin/sh\n%s\n' "$body" > "$work/shim/$tool"; chmod 755 "$work/shim/$tool"
  output="$(PATH="$work/shim:$PATH" "$verify" "$(bundle "shim-$label" "$work/clean")" 2>&1)" || status=$?
  if [ "$status" -eq 2 ]; then echo "ok - $label is an inspection error"
  else echo "not ok - $label: exit $status, expected 2"; echo "$output" | sed 's/^/    /'; failures=$((failures + 1)); fi
  rm "$work/shim/$tool"
}
shim "nm failure" nm 'exit 1'
shim "nm output then failure" nm 'echo _puts; exit 1'
shim "nm with no symbols" nm 'exit 0'
shim "otool failure" otool 'case "$1" in -L) exit 1;; *) exec /usr/bin/otool "$@";; esac'
shim "otool output then failure" otool 'case "$1" in -L) echo "x:"; echo " /usr/lib/libSystem.B.dylib"; exit 1;; *) exec /usr/bin/otool "$@";; esac'

# A file the search cannot read is an inspection error, not a clean result.
unsearchable="$(bundle unsearchable "$work/clean")"; printf 'x' > "$unsearchable/Contents/Resources/locked"
chmod 000 "$unsearchable/Contents/Resources/locked"
expect "unreadable file inside the bundle" 2 "$unsearchable" "grep failed"

# Each kind of development evidence is rejected (1) on its own.
expect "imports _killpg" 1 "$(bundle killpg "$work/killpg")" "imports _killpg"
marker="$(bundle marker "$work/clean")"; printf 'src/runtime_bridge/process.rs' > "$marker/Contents/Resources/blob"
expect "bridge source marker" 1 "$marker" "runtime_bridge/process.rs"
checkout="$(cd "$here/../.." && pwd -P)"
path="$(bundle path "$work/clean")"; printf '%s' "$checkout" > "$path/Contents/Resources/blob"
expect "checkout path" 1 "$path" "bundle contains '$checkout'"
python="$(bundle python "$work/clean")"; touch "$python/Contents/Resources/status.pyc"
expect "Python file" 1 "$python" "Python files"

# The negative control on a real artifact: a bundle built with --features dev-runtime must be rejected by
# the same verifier, and by more than one independent check.
if [ "$#" -ge 1 ]; then
  dev_app="$1"
  expect "real dev-runtime bundle: imports _killpg" 1 "$dev_app" "imports _killpg"
  expect "real dev-runtime bundle: bridge source marker" 1 "$dev_app" "contains 'runtime_bridge/process.rs'"
  expect "real dev-runtime bundle: checkout path" 1 "$dev_app" "bundle contains '$checkout'"
fi

if [ "$failures" -ne 0 ]; then echo "$failures verifier test(s) failed"; exit 1; fi
echo "all verifier tests passed"
