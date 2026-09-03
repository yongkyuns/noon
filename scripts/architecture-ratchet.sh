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

if (( migration_found != 0 || module_indirection_found != 0 )); then
  exit 1
fi

echo "architecture migration-growth and module-growth ratchets passed"
