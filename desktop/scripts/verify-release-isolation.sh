#!/usr/bin/env bash
# Fails if a release app bundle could reach, or points at, the development Python runtime.
# Usage: verify-release-isolation.sh <path to Synapse.app>
# It inspects the artifact itself, so it holds for the configuration Tauri really ships.
set -euo pipefail

app="${1:?usage: $0 <Synapse.app>}"
desktop="$(cd "$(dirname "$0")/.." && pwd)"
checkout="$(cd "$desktop/.." && pwd -P)"
failures=0
fail() { echo "FAIL: $1"; failures=$((failures + 1)); }

needles=("$checkout" ".dev-python" "runtime-bridge" "status.py" "SYNAPSE_DEV_PYTHON" "SYNAPSE_DEV_CHECKOUT")
# A configured interpreter path must never appear either, whatever it is.
if [ -f "$desktop/src-tauri/.dev-python" ]; then
  needles+=("$(tr -d '\n' < "$desktop/src-tauri/.dev-python")")
fi
for needle in "${needles[@]}"; do
  if grep -r -a -l -F -- "$needle" "$app" >/dev/null; then fail "bundle contains '$needle'"; fi
done

if [ -n "$(find "$app" \( -iname '*python*' -o -name '*.py' -o -name '*.pyc' \) -print -quit)" ]; then
  fail "bundle contains Python files"
fi

# The executable may link only system libraries; a Homebrew, @rpath or checkout path would be a new dependency.
binary="$(find "$app/Contents/MacOS" -type f -perm -u+x | head -n 1)"
if otool -L "$binary" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/Library/)'; then
  fail "executable links a non-system library"
fi

if [ "$failures" -ne 0 ]; then exit 1; fi
echo "OK: release bundle has no development runtime path, Python files or non-system libraries"
