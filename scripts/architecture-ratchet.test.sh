#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/architecture-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/src" "$TMP/crates/noon-core/src"
cp "$RATCHET" "$TMP/scripts/architecture-ratchet.sh"

cd "$TMP"
git init -q
git config user.name "Noon Architecture Ratchet Test"
git config user.email "ratchet-test@example.invalid"

# Model the canonical semantic identity authority established by Phase A1.
cat > crates/noon-core/src/semantic_store.rs <<'EOF'
pub struct SemanticNodeId {
    slot: u32,
    generation: u32,
}

impl SemanticNodeId {
    pub const fn new(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }
}

pub struct SemanticStore;
EOF

# Model the repository during Phase A: old module indirection may still exist,
# but the ratchet must prevent it from growing while A5 removes the debt.
cat > src/lib.rs <<'EOF'
#[path = "existing_hidden.rs"]
mod existing_hidden;
include!("existing_impl.rs");
EOF
printf 'pub fn existing_hidden() {}\n' > src/existing_hidden.rs
printf 'pub fn existing_impl() {}\n' > src/existing_impl.rs
git add scripts/architecture-ratchet.sh src crates/noon-core/src/semantic_store.rs
git commit -qm "baseline with semantic authority and known A5 debt"
BASE="$(git rev-parse HEAD)"

printf 'pub fn visible_module_tree() {}\n' > src/visible.rs
git add src/visible.rs
git commit -qm "unrelated clean change"
bash scripts/architecture-ratchet.sh "$BASE" >/dev/null

expect_rejected() {
  label="$1"
  if bash scripts/architecture-ratchet.sh "$BASE" >/dev/null 2>&1; then
    echo "architecture ratchet test failed: accepted $label" >&2
    exit 1
  fi
}

reset_to_base() {
  git reset -q --hard "$BASE"
  rm -f src/new_hidden.rs src/duplicate_identity.rs
}

reset_to_base
cat > src/new_hidden.rs <<'EOF'
#[path = "another_hidden.rs"]
mod another_hidden;
EOF
git add src/new_hidden.rs
git commit -qm "add path indirection"
expect_rejected '#[path] module indirection growth'

reset_to_base
cat > src/new_hidden.rs <<'EOF'
include!("another_hidden.rs");
EOF
git add src/new_hidden.rs
git commit -qm "add include parens"
expect_rejected 'include!(...) module indirection growth'

reset_to_base
cat > src/new_hidden.rs <<'EOF'
include! { "another_hidden.rs" }
EOF
git add src/new_hidden.rs
git commit -qm "add include braces"
expect_rejected 'include! {...} module indirection growth'

reset_to_base
cat > src/new_hidden.rs <<'EOF'
include! [ "another_hidden.rs" ]
EOF
git add src/new_hidden.rs
git commit -qm "add include brackets"
expect_rejected 'include! [...] module indirection growth'

reset_to_base
cat > src/duplicate_identity.rs <<'EOF'
pub struct SemanticNodeId(u64);
EOF
git add src/duplicate_identity.rs
git commit -qm "duplicate semantic node identity"
expect_rejected 'second SemanticNodeId definition'

reset_to_base
cat > src/duplicate_identity.rs <<'EOF'
pub struct SemanticStore;
EOF
git add src/duplicate_identity.rs
git commit -qm "duplicate semantic store"
expect_rejected 'second SemanticStore definition'

reset_to_base
cat > src/duplicate_identity.rs <<'EOF'
impl SemanticNodeId {
    pub fn fabricated() -> Self { unreachable!() }
}
EOF
git add src/duplicate_identity.rs
git commit -qm "move semantic id allocator api"
expect_rejected 'SemanticNodeId inherent implementation outside canonical owner'

echo "architecture ratchet self-test passed"
