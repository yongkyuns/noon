from pathlib import Path

BASE = "9c373a41fe509941bf20dd0ec04cfb2c952fb20c"


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    file = Path(path)
    text = file.read_text()
    actual = text.count(old)
    if actual != count:
        raise SystemExit(
            f"{path}: expected {count} occurrence(s), found {actual}: {old[:120]!r}"
        )
    file.write_text(text.replace(old, new, count))


# Retire the misleading full-invalidation helper name without changing the
# presentation-only behavior already present on the current #1478 parent.
replace(
    "crates/noon-runtime/src/lib.rs",
    "take_renderer_publication_with_followup_invalidation",
    "take_renderer_publication_with_followup_presentation_redraw",
)
replace(
    "crates/noon/src/execution_session.rs",
    "take_renderer_publication_with_followup_invalidation",
    "take_renderer_publication_with_followup_presentation_redraw",
)
replace(
    "crates/noon/src/execution_session.rs",
    ".with_derived_display_objects(&self.derived_display_objects)",
    ".with_transient_presentations(&self.derived_display_objects)",
)

# Direct hosts consume the generic retained transient-presentation boundary.
for path in ("crates/noon-native/src/lib.rs", "crates/noon-web/src/execution_canvas.rs"):
    replace(
        path,
        ".encode_retained_with_derived(",
        ".encode_retained_with_transient_presentations(",
    )

# Keep runtime publication documentation feature-neutral while migration aliases
# remain available to the stacked B3 implementation.
replace(
    "crates/noon-runtime/src/renderer_publication.rs",
    "including the current unequal-family Transform padding copies. Content still",
    "including plan-local copies and other transient effects. Content still",
)
replace(
    "crates/noon-runtime/src/renderer_publication.rs",
    """/// The `DerivedDisplay*` names remain temporarily because the active unequal-family
/// Transform stack already uses them. New renderer/runtime consumers should use the
""",
    """/// The `DerivedDisplay*` names remain temporarily as compatibility aliases for the
/// active B3 stack. New renderer/runtime consumers should use the
""",
)
replace(
    "crates/noon-runtime/src/renderer_publication.rs",
    "/// Migration accessor for the currently stacked unequal-family Transform work.",
    "/// Migration accessor for the currently stacked B3 work.",
)
replace(
    "crates/noon-runtime/src/renderer_publication.rs",
    """    /// Migration entry point for the currently stacked unequal-family Transform
    /// implementation. It delegates to the feature-neutral publication contract.
""",
    """    /// Migration entry point for the currently stacked B3 implementation. It
    /// delegates to the feature-neutral publication contract.
""",
)

# Retained rendering gets a feature-neutral public spelling. Keep the old entry as
# a migration delegate so the change is API convergence rather than a second path.
replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    """pub enum RetainedDerivedDisplayError {
    MixedTextUnsupported,
}

impl std::fmt::Display for RetainedDerivedDisplayError {
""",
    """pub enum RetainedDerivedDisplayError {
    MixedTextUnsupported,
}

/// Feature-neutral spelling for the retained transient-presentation boundary.
pub type RetainedTransientPresentationError = RetainedDerivedDisplayError;

impl std::fmt::Display for RetainedDerivedDisplayError {
""",
)
replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    '"derived family Transform display rows are not yet interleaved with retained glyph painter items",',
    '"transient presentation rows are not yet interleaved with retained glyph painter items",',
)
replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    """    /// Encode a retained geometry-only frame with identity-free derived analytic
    /// occurrences. Mixed glyph frames fail closed until the retained text stream can
    /// represent plan-local occurrence ordinals without manufacturing object IDs.
    pub fn encode_retained_with_derived(
""",
    """    /// Encode a retained geometry-only frame with identity-free transient analytic
    /// occurrences. Mixed glyph frames fail closed until the retained text stream can
    /// represent plan-local occurrence ordinals without manufacturing object IDs.
    pub fn encode_retained_with_transient_presentations(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        prepared: &PreparedRetainedGpuFrame<'_>,
        transient: &crate::PreparedDerivedDisplay,
        clear_color: wgpu::Color,
        query_set: Option<&wgpu::QuerySet>,
    ) -> Result<RetainedDrawStats, RetainedTransientPresentationError> {
        self.encode_retained_with_derived(
            encoder,
            view,
            prepared,
            transient,
            clear_color,
            query_set,
        )
    }

    /// Migration entry point for the current B3 stack. New hosts should use
    /// `encode_retained_with_transient_presentations`.
    pub fn encode_retained_with_derived(
""",
)

# Strengthen the existing 100k locality regression so the endpoint publication
# carries a real identity-free occurrence, while preserving #1478's newer
# has_stable_changes() semantics.
replace(
    "crates/noon-runtime/tests/geometry_patch_locality.rs",
    "use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2};\nuse noon_runtime::{RuntimePatchStats, SceneInstance};",
    """use noon_core::{GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2};
use noon_runtime::{
    RuntimePatchStats, SceneInstance, TransientPresentationOccurrence, TransientPresentationState,
};""",
)
replace(
    "crates/noon-runtime/tests/geometry_patch_locality.rs",
    """    {
        let endpoint = live.take_renderer_publication_with_followup_invalidation();
        assert!(endpoint.changes().is_empty());
    }

    let retirement = live.take_renderer_publication();
    let changes = retirement.changes();
    assert!(!changes.is_all());
    assert!(changes.requires_presentation_redraw());
    assert!(!changes.is_structural());
    assert!(!changes.has_painter_order_change());
    assert!(changes.object_indices().is_empty());
    assert!(changes.added_indices().is_empty());
    assert!(changes.removed_indices().is_empty());
    assert_eq!(retirement.frame().objects.len(), OBJECT_COUNT);
    assert!(live.take_spatial_changes().is_empty());
""",
    """    let transient = [TransientPresentationOccurrence::new(
        0,
        7,
        TransientPresentationState {
            z_index: 0.0,
            content: ObjectContentRef::Geometry(GeometryRef::circle(0.5)),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        },
    )];
    {
        let endpoint = live
            .take_renderer_publication_with_followup_presentation_redraw()
            .with_transient_presentations(&transient)
            .expect("valid transient presentation");
        assert!(endpoint.changes().is_empty());
        assert_eq!(endpoint.transient_presentations().len(), 1);
    }

    let retirement = live.take_renderer_publication();
    let changes = retirement.changes();
    assert!(!changes.is_all());
    assert!(changes.requires_presentation_redraw());
    assert!(!changes.has_stable_changes());
    assert!(!changes.is_structural());
    assert!(!changes.has_painter_order_change());
    assert!(changes.object_indices().is_empty());
    assert!(changes.added_indices().is_empty());
    assert!(changes.removed_indices().is_empty());
    assert_eq!(retirement.frame().objects.len(), OBJECT_COUNT);
    assert!(retirement.transient_presentations().is_empty());
    assert!(live.take_spatial_changes().is_empty());
""",
)

# Browser host contract test follows the generic retained entry point.
replace(
    "web/gpu-timestamp-host.test.mjs",
    '  ".encode_retained_with_derived(",',
    '  ".encode_retained_with_transient_presentations(",',
)

# Add the guard to the canonical architecture entrypoint, its orchestration test,
# and the architecture workflow self-test inventory.
replace(
    "scripts/check-architecture.sh",
    """Run the layer, core-module, renderer-host, active-perf, migration/identity,
and crate-private export guardrails against the current working tree.
""",
    """Run the layer, core-module, renderer-host, transient-presentation,
active-perf, migration/identity, and crate-private export guardrails against the
current working tree.
""",
)
replace(
    "scripts/check-architecture.sh",
    """run_guard renderer-host-boundary-ratchet.sh
run_guard active-perf-frontend-ratchet.sh
""",
    """run_guard renderer-host-boundary-ratchet.sh
run_guard transient-presentation-ratchet.sh
run_guard active-perf-frontend-ratchet.sh
""",
)
replace(
    "scripts/test_check_entrypoint.py",
    """GUARDS = ["layer-dependency-ratchet.sh", "noon-core-module-ownership-ratchet.sh",
          "renderer-host-boundary-ratchet.sh", "active-perf-frontend-ratchet.sh",
          "architecture-ratchet.sh"]
""",
    """GUARDS = ["layer-dependency-ratchet.sh", "noon-core-module-ownership-ratchet.sh",
          "renderer-host-boundary-ratchet.sh", "transient-presentation-ratchet.sh",
          "active-perf-frontend-ratchet.sh", "architecture-ratchet.sh"]
""",
)
replace(
    ".github/workflows/architecture-ratchets.yml",
    """            scripts/noon-core-module-ownership-ratchet.test.sh \\
            scripts/renderer-host-boundary-ratchet.sh \\
            scripts/active-perf-frontend-ratchet.sh \\
""",
    """            scripts/noon-core-module-ownership-ratchet.test.sh \\
            scripts/renderer-host-boundary-ratchet.sh \\
            scripts/transient-presentation-ratchet.sh \\
            scripts/transient-presentation-ratchet.test.sh \\
            scripts/active-perf-frontend-ratchet.sh \\
""",
)
replace(
    ".github/workflows/architecture-ratchets.yml",
    """          bash scripts/noon-core-module-ownership-ratchet.test.sh
          bash scripts/active-perf-frontend-ratchet.test.sh
""",
    """          bash scripts/noon-core-module-ownership-ratchet.test.sh
          bash scripts/transient-presentation-ratchet.test.sh
          bash scripts/active-perf-frontend-ratchet.test.sh
""",
)

Path("scripts/transient-presentation-ratchet.sh").write_text(r'''#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 -I -S - <<'PY'
from pathlib import Path


def read(path: str) -> str:
    p = Path(path)
    if not p.is_file():
        raise SystemExit(f"transient presentation ratchet: missing {path}")
    return p.read_text()


def require(text: str, token: str, message: str) -> None:
    if token not in text:
        raise SystemExit(f"transient presentation ratchet: {message}")


frame = read("crates/noon-runtime/src/frame.rs")
require(frame, "pub fn presentation_redraw() -> Self", "presentation redraw signal disappeared")
require(frame, "pub const fn has_stable_changes(&self) -> bool", "stable-dirtiness classification disappeared")

runtime = read("crates/noon-runtime/src/lib.rs")
helper_name = "pub fn take_renderer_publication_with_followup_presentation_redraw"
require(runtime, helper_name, "localized follow-up publication helper disappeared")
start = runtime.index(helper_name)
end = runtime.find("/// Consume derived spatial invalidation", start)
if end < 0:
    raise SystemExit("transient presentation ratchet: could not bound follow-up publication helper")
helper = runtime[start:end]
if "invalidate_all" in helper or "FrameChanges::all" in helper:
    raise SystemExit("transient presentation ratchet: transient retirement regained full invalidation")
require(helper, "FrameChanges::presentation_redraw()", "transient retirement no longer queues presentation-only redraw")

session = read("crates/noon/src/execution_session.rs")
if "take_renderer_publication_with_followup_invalidation" in session:
    raise SystemExit("transient presentation ratchet: execution session regained misleading retirement helper")
require(session, ".with_transient_presentations(&self.derived_display_objects)", "execution session bypasses generic transient publication contract")

publication = read("crates/noon-runtime/src/renderer_publication.rs")
require(publication, "pub const fn transient_presentations(&self)", "generic transient publication accessor disappeared")
require(publication, "pub fn with_transient_presentations(", "generic transient publication attachment disappeared")
start = publication.index("pub struct DerivedDisplayObject {")
end = publication.index("impl DerivedDisplayObject", start)
occurrence = publication[start:end]
for forbidden in ("SemanticNodeId", "ObjectId", "TrackId"):
    if forbidden in occurrence:
        raise SystemExit(f"transient presentation ratchet: transient occurrence acquired stable identity via {forbidden}")

retained = read("crates/noon-render-wgpu/src/gpu/retained_text.rs")
require(retained, "changes.requires_presentation_redraw()", "retained preparation no longer recognizes presentation redraw")
require(retained, "!changes.has_stable_changes()", "retained preparation no longer preserves coexisting stable dirtiness")
require(retained, "pub fn encode_retained_with_transient_presentations(", "generic retained renderer entry point disappeared")

for path in ("crates/noon-native/src/lib.rs", "crates/noon-web/src/execution_canvas.rs"):
    host = read(path)
    require(host, ".encode_retained_with_transient_presentations(", f"{path} bypasses generic transient renderer entry point")
    if ".encode_retained_with_derived(" in host:
        raise SystemExit(f"transient presentation ratchet: {path} regained derived-display host coupling")

for path in (
    "crates/noon-runtime/src/renderer_publication.rs",
    "crates/noon-render-wgpu/src/render_order.rs",
    "crates/noon-render-wgpu/src/gpu/derived_display.rs",
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
):
    text = read(path)
    for forbidden in ("FamilyTransform", "family Transform"):
        if forbidden in text:
            raise SystemExit(f"transient presentation ratchet: feature-specific renderer coupling in {path}: {forbidden}")
PY

echo "transient presentation locality and identity ratchet passed"
''')

Path("scripts/transient-presentation-ratchet.test.sh").write_text(r'''#!/usr/bin/env bash
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
''')

print(f"semantic #1494 restack patch applied on {BASE}")
