#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: bash scripts/check-architecture.sh [BASE]

Run the layer, core-module, renderer-host, active-perf, migration/identity,
and crate-private export guardrails against the current working tree.
BASE defaults to origin/master, is printed and resolved once, and must exist
locally. Pass HEAD explicitly for working-tree-only checks. No fetch, merge-base
selection, HEAD^ fallback, dependency download or compilation is performed.
Requires Bash, Git, Python 3.10+, Cargo (repository toolchain), grep, sed, wc and tr.
EOF
}
if [[ "${1:-}" == -h || "${1:-}" == --help ]]; then
  usage
  exit 0
fi
if (( $# > 1 )); then
  usage >&2
  exit 2
fi
for tool in git python3 cargo grep sed wc tr mktemp; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "architecture gate: required tool is unavailable: $tool" >&2
    exit 2
  fi
done

base_ref="${1-origin/master}"
if ! base="$(git rev-parse --verify --end-of-options "${base_ref}^{commit}" 2>/dev/null)"; then
  echo "architecture gate: comparison base is unavailable: $base_ref" >&2
  echo "Supply an existing BASE or fetch the intended history explicitly, then rerun." >&2
  exit 2
fi
printf 'architecture gate: base %s (%s); candidate is the current working tree\n' "$base_ref" "$base"

# Existing guards use Git's tracked-source inventory. Give them a private index
# containing intent-to-add entries for staged additions and nonignored untracked
# files, so their structural scans see the same candidate as their diff scans.
# Never refresh, stage, stash, reset or otherwise change the user's real index.
# Read its inventory first; this also retains explicitly staged ignored files.
export GIT_OPTIONAL_LOCKS=0
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
git ls-files --cached --others --exclude-standard -z > "$TMP/paths"
while IFS= read -r -d '' path; do
  if [[ -e "$path" || -L "$path" ]]; then
    printf '%s\0' "$path"
  fi
done < "$TMP/paths" > "$TMP/present-paths"
export GIT_INDEX_FILE="$TMP/index"
git -c core.splitIndex=false read-tree HEAD
if [[ -s "$TMP/present-paths" ]]; then
  GIT_LITERAL_PATHSPECS=1 git -c core.splitIndex=false add --intent-to-add --force \
    --ignore-removal --pathspec-from-file="$TMP/present-paths" --pathspec-file-nul
fi
export NOON_ROOT="$ROOT"

run_guard() {
  local script="$1"
  shift
  if [[ ! -r "scripts/$script" ]]; then
    echo "architecture gate: missing or unreadable guard: scripts/$script" >&2
    exit 2
  fi
  TIMEFORMAT="architecture gate: $script: %3R s"
  time bash "scripts/$script" "$@"
}

run_guard layer-dependency-ratchet.sh
run_guard noon-core-module-ownership-ratchet.sh
run_guard renderer-host-boundary-ratchet.sh
run_guard active-perf-frontend-ratchet.sh
run_guard architecture-ratchet.sh "$base"

# Keep the existing CI-only export check on the same local path as the ratchets.
if grep -En '^[[:space:]]*pub[[:space:]]+use[[:space:]]+legacy::(.*ScenePlayer|\*)' crates/noon-web/src/lib.rs; then
  echo "architecture gate: ScenePlayer must remain crate-private" >&2
  exit 1
else
  status=$?
  if (( status != 1 )); then
    echo "architecture gate: crate-private export scan failed" >&2
    exit 2
  fi
fi

echo "architecture gate passed"
