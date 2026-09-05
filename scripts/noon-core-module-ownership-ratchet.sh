#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# `reactive.rs -> semantic_store.rs` is a temporary migration seam. This
# guard protects ordinary module ownership without making that seam permanent:
# a reviewed normalization may remove it, but no additional `#[path]` or
# `include!` indirection is allowed.
temporary_owner='crates/noon-core/src/reactive.rs'
temporary_target='semantic_store.rs'
module_indirections="$(
  rg -n --glob '*.rs' \
    '^[[:space:]]*#\[[[:space:]]*path[[:space:]]*=|(^|[^[:alnum:]_])include![[:space:]]*(\(|\{|\[)' \
    crates/noon-core/src || true
)"

unexpected=0
while IFS= read -r reference; do
  [[ -z "$reference" ]] && continue

  reference_file="${reference%%:*}"
  remainder="${reference#*:}"
  reference_line="${remainder#*:}"
  if [[ "$reference_file" != "$temporary_owner" ]] || \
     ! printf '%s\n' "$reference_line" | grep -Eq \
       '^[[:space:]]*#\[[[:space:]]*path[[:space:]]*=[[:space:]]*"semantic_store\.rs"[[:space:]]*\][[:space:]]*$'; then
    printf 'noon-core module ownership ratchet: unexpected indirection: %s\n' "$reference" >&2
    unexpected=1
  fi
done <<< "$module_indirections"

if (( unexpected != 0 )); then
  cat >&2 <<'EOF'

noon-core module ownership must remain explicit. The current
`reactive.rs -> semantic_store.rs` `#[path]` is a temporary migration seam;
it may be removed by a reviewed ownership normalization, but it does not
authorize another organizational `#[path]` or `include!` indirection.
EOF
  exit 1
fi

echo "noon-core module ownership ratchet passed"
