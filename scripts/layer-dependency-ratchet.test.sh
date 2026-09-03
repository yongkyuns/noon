#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/layer-dependency-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p \
  "$TMP/crates/noon-core" \
  "$TMP/crates/noon-compile" \
  "$TMP/crates/noon-runtime" \
  "$TMP/crates/noon-render-wgpu"

write_clean_fixture() {
  cat >"$TMP/crates/noon-core/Cargo.toml" <<'EOF'
[package]
name = "noon-core"
[dependencies]
serde = "1"
EOF

  cat >"$TMP/crates/noon-compile/Cargo.toml" <<'EOF'
[package]
name = "noon-compile"
[dependencies]
noon-core = { path = "../noon-core" }
EOF

  cat >"$TMP/crates/noon-runtime/Cargo.toml" <<'EOF'
[package]
name = "noon-runtime"
[dependencies]
noon-core = { path = "../noon-core" }
noon-compile = { path = "../noon-compile" }
EOF

  cat >"$TMP/crates/noon-render-wgpu/Cargo.toml" <<'EOF'
[package]
name = "noon-render-wgpu"
[dependencies]
noon-core = { path = "../noon-core" }
noon-runtime = { path = "../noon-runtime" }
EOF
}

expect_failure() {
  label="$1"
  if NOON_ROOT="$TMP" bash "$RATCHET" >"$TMP/output" 2>&1; then
    echo "layer dependency ratchet self-test unexpectedly passed: $label" >&2
    cat "$TMP/output" >&2
    exit 1
  fi
}

write_clean_fixture
NOON_ROOT="$TMP" bash "$RATCHET" >/dev/null

write_clean_fixture
echo 'noon-runtime = { path = "../noon-runtime" }' >>"$TMP/crates/noon-core/Cargo.toml"
expect_failure "noon-core -> noon-runtime"

write_clean_fixture
echo 'noon-render-wgpu = { path = "../noon-render-wgpu" }' >>"$TMP/crates/noon-compile/Cargo.toml"
expect_failure "noon-compile -> noon-render-wgpu"

write_clean_fixture
echo '"noon-web" = { path = "../noon-web" }' >>"$TMP/crates/noon-runtime/Cargo.toml"
expect_failure "noon-runtime -> noon-web with quoted TOML key"

write_clean_fixture
cat >>"$TMP/crates/noon-render-wgpu/Cargo.toml" <<'EOF'

[target.'cfg(target_arch = "wasm32")'.dependencies.noon]
path = "../noon"
EOF
expect_failure "noon-render-wgpu -> noon through dependency table"

echo "architecture layer dependency ratchet self-test passed"
