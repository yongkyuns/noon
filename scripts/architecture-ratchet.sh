#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

base="${1:-}"
if [[ -z "$base" ]]; then
  if git rev-parse --verify HEAD^ >/dev/null 2>&1; then
    base="HEAD^"
  else
    echo "architecture ratchet needs a base commit" >&2
    exit 2
  fi
fi

if ! git cat-file -e "${base}^{commit}" 2>/dev/null; then
  echo "architecture ratchet base commit is unavailable: $base" >&2
  exit 2
fi

range="${base}...HEAD"
forbidden='SceneDefinition|SceneSpec|SceneDocument|ObjectDefinition|ObjectSnapshot|from_legacy|noon::legacy|_manim_canonical_scene|scene_document|noon-ir'

migration_found=0
module_indirection_found=0
current_file=""
while IFS= read -r line; do
  case "$line" in
    "+++ b/"*)
      current_file="${line#+++ b/}"
      ;;
    +*)
      [[ "$line" == "+++ "* ]] && continue
      added="${line:1}"

      if printf '%s\n' "$added" | grep -Eq "$forbidden"; then
        printf 'architecture ratchet: %s: +%s\n' "${current_file:-unknown}" "$added" >&2
        migration_found=1
      fi

      case "$current_file" in
        *.rs)
          if printf '%s\n' "$added" | grep -Eq '^[[:space:]]*#\[[[:space:]]*path[[:space:]]*='; then
            printf 'architecture ratchet: new hidden Rust module ownership: %s: +%s\n' "$current_file" "$added" >&2
            module_indirection_found=1
          fi
          if printf '%s\n' "$added" | grep -Eq '(^|[^[:alnum:]_])include![[:space:]]*(\(|\{|\[)'; then
            printf 'architecture ratchet: new hidden Rust module ownership: %s: +%s\n' "$current_file" "$added" >&2
            module_indirection_found=1
          fi
          ;;
      esac
      ;;
  esac
done < <(
  git diff --no-color --unified=0 "$range" -- \
    '*.rs' '*.py' '*.js' '*.mjs' '*.ts' '*.tsx' \
    'Cargo.toml' '*/Cargo.toml' 'crates/*/Cargo.toml'
)

if (( migration_found != 0 )); then
  cat >&2 <<'EOF'

New migration-era architecture references are not allowed to grow.
Delete/rewrite the dependency instead of spreading it. If a target architecture
change genuinely requires one of these tokens, update docs/architecture.md and
this ratchet explicitly in the same reviewed PR rather than bypassing the check.
EOF
fi

if (( module_indirection_found != 0 )); then
  cat >&2 <<'EOF'

New Rust #[path] / include! module indirection is not allowed.
Existing A5 migration debt is grandfathered only until its owning cleanup lands;
do not move or duplicate it. Once an area is normalized, tighten this ratchet to
make that absence structural. See #960/A5 and #961/A6.8.
EOF
fi

identity_authority_found=0
canonical_identity_file='crates/noon-core/src/semantic_store.rs'

semantic_node_defs="$(git grep -nE '^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?struct[[:space:]]+SemanticNodeId([[:space:]{(;]|$)' -- '*.rs' || true)"
semantic_store_defs="$(git grep -nE '^[[:space:]]*(pub([[:space:]]*\([^)]*\))?[[:space:]]+)?struct[[:space:]]+SemanticStore([[:space:]{(;]|$)' -- '*.rs' || true)"
semantic_node_impls="$(git grep -nE '^[[:space:]]*impl[[:space:]]+SemanticNodeId([[:space:]{]|$)' -- '*.rs' || true)"

require_canonical_only() {
  label="$1"
  matches="$2"

  count=0
  if [[ -n "$matches" ]]; then
    count="$(printf '%s\n' "$matches" | wc -l | tr -d '[:space:]')"
  fi

  if [[ "$count" != "1" ]] || ! printf '%s\n' "$matches" | grep -q "^${canonical_identity_file}:"; then
    printf 'architecture ratchet: %s must exist exactly once in %s\n' "$label" "$canonical_identity_file" >&2
    if [[ -n "$matches" ]]; then
      printf '%s\n' "$matches" >&2
    else
      printf '(no matches)\n' >&2
    fi
    identity_authority_found=1
  fi
}

require_canonical_only 'SemanticNodeId definition' "$semantic_node_defs"
require_canonical_only 'SemanticStore definition' "$semantic_store_defs"
require_canonical_only 'SemanticNodeId inherent implementation' "$semantic_node_impls"

if (( identity_authority_found != 0 )); then
  cat >&2 <<'EOF'

Semantic author identity has one canonical owner. SemanticNodeId and its inherent
allocator API must stay in crates/noon-core/src/semantic_store.rs, alongside the
single SemanticStore definition. Add behavior through that authority instead of
creating a second identity/store definition elsewhere. See #961/A6.3.
EOF
fi

if (( migration_found != 0 || module_indirection_found != 0 || identity_authority_found != 0 )); then
  exit 1
fi

echo "architecture migration-growth, module-growth, and semantic-identity ratchets passed"
