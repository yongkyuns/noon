#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: bash scripts/check.sh [fast|full|rust|fmt-lint|test|web|architecture] [BASE]

  fast          Architecture gate, format/check/clippy, workspace library tests.
  full          Architecture gate, full Rust gate, browser build/validation.
  rust          Architecture gate, format/check/clippy, all workspace tests.
  fmt-lint      Architecture gate, format/check/clippy only.
  test          Architecture gate, all workspace tests.
  web           Architecture gate, browser build/validation.
  architecture  All cheap architecture guardrails, without compilation.

BASE defaults to origin/master and must exist locally; no automatic fetch or
fallback. Pass HEAD explicitly to check only the current working-tree changes.
The candidate includes staged edits, unstaged edits and nonignored untracked
files at their current working-tree contents; the real Git index is unchanged.

See .github/ci/README.md for prerequisites, focused iteration commands and timing.
Extended GitHub browser, parity, golden, differential, performance and platform
checks remain required where appropriate; this local gate does not replace them.
EOF
}

fmt_lint() {
  cargo fmt --all -- --check
  cargo check --workspace --all-targets --all-features
  cargo clippy --workspace --all-targets --all-features -- -D warnings
}

fast_tests() {
  cargo test --workspace --all-features --lib --no-fail-fast
}

all_tests() {
  cargo test --workspace --all-features --no-fail-fast
}

web_check() {
  bash scripts/build-web-demo.sh
}

mode="${1:-fast}"
case "$mode" in
  -h|--help|help)
    usage
    exit 0
    ;;
  fast|full|rust|fmt-lint|test|web|architecture)
    ;;
  *)
    echo "unknown check mode: $mode" >&2
    usage >&2
    exit 2
    ;;
esac
if (( $# > 2 )); then
  usage >&2
  exit 2
fi

# Every public validation mode runs the same guards before compiling anything.
bash scripts/check-architecture.sh "${2-origin/master}"
case "$mode" in
  fast) fmt_lint; fast_tests ;;
  full) fmt_lint; all_tests; web_check ;;
  rust) fmt_lint; all_tests ;;
  fmt-lint) fmt_lint ;;
  test) all_tests ;;
  web) web_check ;;
  architecture) : ;;
esac
