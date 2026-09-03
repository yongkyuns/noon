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

found=0
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
        found=1
      fi
      ;;
  esac
done < <(
  git diff --no-color --unified=0 "$range" -- \
    '*.rs' '*.py' '*.js' '*.mjs' '*.ts' '*.tsx' \
    'Cargo.toml' '*/Cargo.toml' 'crates/*/Cargo.toml'
)

if (( found != 0 )); then
  cat >&2 <<'EOF'

New migration-era architecture references are not allowed to grow.
Delete/rewrite the dependency instead of spreading it. If a target architecture
change genuinely requires one of these tokens, update docs/architecture.md and
this ratchet explicitly in the same reviewed PR rather than bypassing the check.
EOF
  exit 1
fi

echo "architecture migration-growth ratchet passed"
