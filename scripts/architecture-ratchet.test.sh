#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/architecture-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/src"
cp "$RATCHET" "$TMP/scripts/architecture-ratchet.sh"

cd "$TMP"
git init -q
git config user.name "Noon Architecture Ratchet Test"
git config user.email "ratchet-test@example.invalid"
printf 'pub fn visible_module_tree() {}\n' > src/lib.rs
git add scripts/architecture-ratchet.sh src/lib.rs
git commit -qm "baseline"

bash scripts/architecture-ratchet.sh HEAD >/dev/null

expect_rejected() {
  local label="$1"
  if bash scripts/architecture-ratchet.sh HEAD >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted $label" >&2
    exit 1
  fi
}

cat > src/lib.rs <<'EOF'
#[path = "hidden.rs"]
mod hidden;
EOF
expect_rejected '#[path] module indirection'

git checkout -q -- src/lib.rs
cat > src/lib.rs <<'EOF'
include!("hidden.rs");
EOF
expect_rejected 'include!(...) module indirection'

cat > src/lib.rs <<'EOF'
include! { "hidden.rs" }
EOF
expect_rejected 'include! {...} module indirection'

cat > src/lib.rs <<'EOF'
include! [ "hidden.rs" ]
EOF
expect_rejected 'include! [...] module indirection'

echo "architecture ratchet self-test passed"
