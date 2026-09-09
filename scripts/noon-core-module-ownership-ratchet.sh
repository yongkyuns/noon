#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Semantic store and reactive declarations use ordinary module ownership.
# No organizational path/include indirection remains permitted.
# Use the same baseline grep dependency as the other architecture guards.
# Scan the working tree (including untracked Rust files), and distinguish an
# empty match set from a tool/read failure so the guard cannot pass unchecked.
if module_indirections="$(
  grep -rnE --include='*.rs' \
    '^[[:space:]]*#\[[[:space:]]*path[[:space:]]*=|(^|[^[:alnum:]_])include![[:space:]]*(\(|\{|\[)' \
    crates/noon-core/src
)"; then
  :
else
  scan_status=$?
  if (( scan_status != 1 )); then
    echo "noon-core module ownership ratchet: source scan failed" >&2
    exit "$scan_status"
  fi
fi

if [[ -n "$module_indirections" ]]; then
  printf 'noon-core module ownership ratchet: unexpected indirection:\n%s\n' "$module_indirections" >&2
  echo 'noon-core ownership requires ordinary modules, without #[path] or include! indirection.' >&2
  exit 1
fi

echo "noon-core module ownership ratchet passed"
