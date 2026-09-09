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

# Protect the current Phase A root ownership, not a permanent target crate map.
# A reviewed owner change must update this guard and its relocation regression;
# unrelated domains must not return to reactive even through ordinary paths.
# Keep implementation modules private; intentional crate-root exports are separate.
core_src=crates/noon-core/src
for owner in animation publication reactive resources semantic_store; do
  if grep -Eq "^[[:space:]]*mod[[:space:]]+$owner[[:space:]]*;[[:space:]]*(//.*)?$" "$core_src/lib.rs"; then
    :
  else
    scan_status=$?
    if (( scan_status != 1 )); then
      echo "noon-core module ownership ratchet: root ownership scan failed" >&2
      exit "$scan_status"
    fi
    echo "noon-core module ownership ratchet: $owner must remain an ordinary private root module" >&2
    exit 1
  fi
  if [[ -f "$core_src/$owner.rs" && -f "$core_src/$owner/mod.rs" ]] ||
     [[ ! -f "$core_src/$owner.rs" && ! -f "$core_src/$owner/mod.rs" ]]; then
    echo "noon-core module ownership ratchet: $owner must resolve to exactly one ordinary module file" >&2
    exit 1
  fi
done

reactive_sources=()
if [[ -f "$core_src/reactive.rs" ]]; then
  reactive_sources+=("$core_src/reactive.rs")
fi
if [[ -d "$core_src/reactive" ]]; then
  reactive_sources+=("$core_src/reactive")
fi
if reactive_owners="$(
  grep -rnE --include='*.rs' \
    '(^|[;{}])[[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+(animation|publication|resources|semantic_store)([[:space:];{]|$)' \
    "${reactive_sources[@]}"
)"; then
  printf 'noon-core module ownership ratchet: unrelated domain declared under reactive:\n%s\n' "$reactive_owners" >&2
  exit 1
else
  scan_status=$?
  if (( scan_status != 1 )); then
    echo "noon-core module ownership ratchet: reactive ownership scan failed" >&2
    exit "$scan_status"
  fi
fi

echo "noon-core module ownership ratchet passed"
