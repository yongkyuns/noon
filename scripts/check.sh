#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: bash scripts/check.sh [fast|full|rust|fmt-lint|test|web]

  fast      Format/check/clippy plus workspace library tests.
  full      Full Rust gate plus the browser build/validation entrypoint.
  rust      Format/check/clippy plus all workspace tests.
  fmt-lint  Format/check/clippy only.
  test      All workspace tests only.
  web       Browser build/validation only.

The repository's extended GitHub workflows additionally run browser, parity,
golden, differential, and platform-specific checks where appropriate.
EOF
}

fmt_lint() {
  cargo fmt --all -- --check
  cargo check --workspace --all-targets --all-features
  cargo clippy --workspace --all-targets --all-features -- -D warnings
}

fast_tests() {
  cargo test --workspace --all-features --lib
}

all_tests() {
  cargo test --workspace --all-features
}

web_check() {
  bash scripts/build-web-demo.sh
}

mode="${1:-fast}"
case "$mode" in
  fast)
    fmt_lint
    fast_tests
    ;;
  full)
    fmt_lint
    all_tests
    web_check
    ;;
  rust)
    fmt_lint
    all_tests
    ;;
  fmt-lint)
    fmt_lint
    ;;
  test)
    all_tests
    ;;
  web)
    web_check
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    echo "unknown check mode: $mode" >&2
    usage >&2
    exit 2
    ;;
esac
