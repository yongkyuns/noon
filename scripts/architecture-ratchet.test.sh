#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/architecture-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" "$TMP/src" "$TMP/web" "$TMP/crates/noon-core/src" "$TMP/crates/noon-runtime/src" "$TMP/crates/noon-web/src"
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

# Model the repository during Phase A: old module indirection may still exist
# outside ownership islands already normalized by A5. The growth ratchet must
# prevent that debt from spreading while cleaned islands stay structurally clean.
cat > src/lib.rs <<'EOF'
#[path = "existing_hidden.rs"]
mod existing_hidden;
include!("existing_impl.rs");
EOF
printf 'pub fn existing_hidden() {}\n' > src/existing_hidden.rs
printf 'pub fn existing_impl() {}\n' > src/existing_impl.rs

# noon-runtime models a post-A5 normalized island: only ordinary module layout.
cat > crates/noon-runtime/src/lib.rs <<'EOF'
mod runtime;
EOF
printf 'pub fn runtime() {}\n' > crates/noon-runtime/src/runtime.rs

# These web tools were detached from the migration player by #991 and #994.
printf 'pub fn deterministic_replay() {}\n' > crates/noon-web/src/determinism.rs
printf 'pub fn semantic_snapshot() {}\n' > crates/noon-web/src/semantic_snapshot.rs

# Model the deliberately shrinking ScenePlayer allowlist that remains during A4.
cat > crates/noon-web/src/legacy.rs <<'EOF'
pub struct ScenePlayer;
EOF
for consumer in execution_transport; do
  cat > "crates/noon-web/src/${consumer}.rs" <<'EOF'
use crate::ScenePlayer;
EOF
done

# Model the canonical playback clock left after #1005 removed its legacy duplicate.
printf 'pub struct PlaybackClock;\n' > crates/noon-web/src/clock.rs

git add scripts/architecture-ratchet.sh src crates/noon-core/src/semantic_store.rs crates/noon-runtime/src crates/noon-web/src web
git commit -qm "baseline with semantic authority, known A5 debt, and normalized islands"
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
  rm -f src/new_hidden.rs src/duplicate_identity.rs src/runtime_structural_probe.rs src/web_tool_structural_probe.rs src/scene_player_spread_probe.rs src/deleted_legacy_web_probe.rs src/duplicate_clock_probe.rs src/legacy_clock_probe.rs
  rm -f web/browser-smoke.js crates/noon-web/src/duplicate_clock.rs crates/noon-web/src/legacy/clock.rs
  rmdir crates/noon-web/src/legacy 2>/dev/null || true
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

# Prove noon-runtime is no longer merely growth-ratcheted. Put hidden ownership
# into the comparison base itself, then make an unrelated later commit. A diff-
# only growth check cannot see the old line; the full-tree normalized-island gate
# still must reject the repository state.
reset_to_base
cat > crates/noon-runtime/src/lib.rs <<'EOF'
#[path = "runtime_hidden.rs"]
mod runtime_hidden;
EOF
printf 'pub fn runtime_hidden() {}\n' > crates/noon-runtime/src/runtime_hidden.rs
git add crates/noon-runtime/src
git commit -qm "model regressed normalized runtime baseline"
RUNTIME_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_runtime_regression() {}\n' > src/runtime_structural_probe.rs
git add src/runtime_structural_probe.rs
git commit -qm "unrelated change after runtime regression"
if bash scripts/architecture-ratchet.sh "$RUNTIME_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted pre-existing noon-runtime module indirection" >&2
  exit 1
fi

# Prove the completed A4.6 tool cutovers are structural, not just growth-ratcheted.
# Put migration-player dependencies into the comparison base itself, then make an
# unrelated later commit. The full-tree guard must still reject both tool paths.
reset_to_base
cat > crates/noon-web/src/determinism.rs <<'EOF'
use crate::ScenePlayer;
pub fn deterministic_replay() {}
EOF
cat > crates/noon-web/src/semantic_snapshot.rs <<'EOF'
use crate::PlayerError;
pub fn semantic_snapshot() {}
EOF
git add crates/noon-web/src
git commit -qm "model regressed web tool player baseline"
WEB_TOOL_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_web_tool_regression() {}\n' > src/web_tool_structural_probe.rs
git add src/web_tool_structural_probe.rs
git commit -qm "unrelated change after web tool regression"
if bash scripts/architecture-ratchet.sh "$WEB_TOOL_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted pre-existing web tool migration-player dependency" >&2
  exit 1
fi

# Prove ScenePlayer cannot spread to another noon-web Rust module. Put a new
# consumer into the comparison base itself, then make an unrelated later commit;
# the structural allowlist must still reject that repository state.
reset_to_base
cat > crates/noon-web/src/new_scene_player_consumer.rs <<'EOF'
use crate::ScenePlayer;
EOF
git add crates/noon-web/src/new_scene_player_consumer.rs
git commit -qm "model ScenePlayer consumer spread"
SCENE_PLAYER_SPREAD_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_scene_player_spread() {}\n' > src/scene_player_spread_probe.rs
git add src/scene_player_spread_probe.rs
git commit -qm "unrelated change after ScenePlayer spread"
if bash scripts/architecture-ratchet.sh "$SCENE_PLAYER_SPREAD_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted ScenePlayer consumer outside migration allowlist" >&2
  exit 1
fi

# Prove the cleaned #1003 primary browser smoke cannot regain the deleted frontend
# even when the regression predates the current diff.
reset_to_base
cat > web/browser-smoke.js <<'EOF'
export class NoonCanvasPlayer {}
export function demoSceneJson() { return "{}"; }
EOF
git add web/browser-smoke.js
git commit -qm "model deleted primary browser frontend returning"
DELETED_FRONTEND_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_deleted_frontend() {}\n' > src/deleted_legacy_web_probe.rs
git add src/deleted_legacy_web_probe.rs
git commit -qm "unrelated change after deleted frontend regression"
if bash scripts/architecture-ratchet.sh "$DELETED_FRONTEND_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted deleted primary NoonCanvasPlayer/demoSceneJson surface" >&2
  exit 1
fi

# Prove playback clock ownership remains singular after #1005.
reset_to_base
printf 'pub struct PlaybackClock;\n' > crates/noon-web/src/duplicate_clock.rs
git add crates/noon-web/src/duplicate_clock.rs
git commit -qm "model duplicate playback clock"
DUPLICATE_CLOCK_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_duplicate_clock() {}\n' > src/duplicate_clock_probe.rs
git add src/duplicate_clock_probe.rs
git commit -qm "unrelated change after duplicate clock regression"
if bash scripts/architecture-ratchet.sh "$DUPLICATE_CLOCK_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted duplicate PlaybackClock authority" >&2
  exit 1
fi

# Prove the exact deleted legacy clock module path cannot be recreated even if it
# does not itself declare another PlaybackClock yet.
reset_to_base
mkdir -p crates/noon-web/src/legacy
printf 'pub fn stale_clock_module() {}\n' > crates/noon-web/src/legacy/clock.rs
git add crates/noon-web/src/legacy/clock.rs
git commit -qm "model deleted legacy clock path returning"
LEGACY_CLOCK_REGRESSION_BASE="$(git rev-parse HEAD)"
printf 'pub fn unrelated_after_legacy_clock() {}\n' > src/legacy_clock_probe.rs
git add src/legacy_clock_probe.rs
git commit -qm "unrelated change after legacy clock regression"
if bash scripts/architecture-ratchet.sh "$LEGACY_CLOCK_REGRESSION_BASE" >/dev/null 2>&1; then
  echo "architecture ratchet test failed: accepted deleted legacy clock module path" >&2
  exit 1
fi

echo "architecture ratchet self-test passed"
