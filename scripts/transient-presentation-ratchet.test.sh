#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RATCHET="$ROOT/scripts/transient-presentation-ratchet.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts" \
  "$TMP/crates/noon-runtime/src" \
  "$TMP/crates/noon/src" \
  "$TMP/crates/noon-render-wgpu/src/gpu" \
  "$TMP/crates/noon-render-wgpu/src" \
  "$TMP/crates/noon-native/src" \
  "$TMP/crates/noon-web/src"
cp "$RATCHET" "$TMP/scripts/transient-presentation-ratchet.sh"
cd "$TMP"

write_fixture() {
  cat > crates/noon-runtime/src/frame.rs <<'RUST'
pub struct FrameChanges;
impl FrameChanges {
    pub fn presentation_redraw() -> Self { Self }
    pub const fn has_stable_changes(&self) -> bool { false }
}
RUST
  cat > crates/noon-runtime/src/lib.rs <<'RUST'
pub fn take_renderer_publication_with_followup_presentation_redraw() {
    let _changes = FrameChanges::presentation_redraw();
}
/// Consume derived spatial invalidation.
pub fn spatial() {}
RUST
  cat > crates/noon/src/execution_session.rs <<'RUST'
fn publish(self) { let _ = self.with_transient_presentations(&self.derived_display_objects); }
RUST
  cat > crates/noon-runtime/src/renderer_publication.rs <<'RUST'
pub struct DerivedDisplayObject {
    anchor_object_index: u32,
    occurrence_index: u32,
}
impl DerivedDisplayObject {}
pub const fn transient_presentations(&self) {}
pub fn with_transient_presentations(&self) {}
RUST
  cat > crates/noon-render-wgpu/src/gpu/retained_text.rs <<'RUST'
fn prepare(changes: Changes) {
    let _stable = (changes.requires_presentation_redraw() && !changes.has_stable_changes())
        .then(FrameChanges::default);
}
pub fn encode_retained_with_transient_presentations(&self) {}
RUST
  cat > crates/noon-render-wgpu/src/render_order.rs <<'RUST'
pub fn prepare_transient() {}
RUST
  cat > crates/noon-render-wgpu/src/gpu/derived_display.rs <<'RUST'
pub fn upload_transient() {}
RUST
  cat > crates/noon-native/src/lib.rs <<'RUST'
fn draw(renderer: Renderer) { renderer.encode_retained_with_transient_presentations(); }
RUST
  cat > crates/noon-web/src/execution_canvas.rs <<'RUST'
fn draw(renderer: Renderer) { renderer.encode_retained_with_transient_presentations(); }
RUST
}

expect_rejected() {
  local label="$1" diagnostic="$2" output
  if output="$(bash scripts/transient-presentation-ratchet.sh 2>&1)"; then
    echo "transient presentation ratchet test failed: accepted $label" >&2
    exit 1
  fi
  if [[ "$output" != *"$diagnostic"* ]]; then
    printf 'transient presentation ratchet test failed: wrong rejection for %s:\n%s\n' "$label" "$output" >&2
    exit 1
  fi
}

write_fixture
bash scripts/transient-presentation-ratchet.sh >/dev/null

sed -i 's/let _changes = FrameChanges::presentation_redraw();/changes.invalidate_all();/' crates/noon-runtime/src/lib.rs
expect_rejected 'full invalidation retirement' 'regained full invalidation'
write_fixture

sed -i 's/anchor_object_index: u32,/object_id: ObjectId,\n    anchor_object_index: u32,/' crates/noon-runtime/src/renderer_publication.rs
expect_rejected 'synthetic stable identity' 'acquired stable identity via ObjectId'
write_fixture

sed -i 's/!changes.has_stable_changes()/true/' crates/noon-render-wgpu/src/gpu/retained_text.rs
expect_rejected 'coexisting stable dirtiness loss' 'no longer preserves coexisting stable dirtiness'
write_fixture

printf '\n// FamilyTransform feature leak\n' >> crates/noon-render-wgpu/src/gpu/derived_display.rs
expect_rejected 'feature-specific renderer coupling' 'feature-specific renderer coupling'
write_fixture

sed -i 's/encode_retained_with_transient_presentations/encode_retained_with_derived/' crates/noon-native/src/lib.rs
expect_rejected 'feature-specific direct host entry point' 'bypasses generic transient renderer entry point'

echo "transient presentation ratchet self-test passed"
