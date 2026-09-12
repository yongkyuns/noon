from pathlib import Path


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    p = Path(path)
    text = p.read_text()
    actual = text.count(old)
    if actual != count:
        raise SystemExit(
            f"{path}: expected {count} occurrences, found {actual}: {old[:120]!r}"
        )
    p.write_text(text.replace(old, new, count))


def require(path: str, token: str) -> None:
    text = Path(path).read_text()
    if token not in text:
        raise SystemExit(f"{path}: required token missing: {token!r}")


# Runtime publication: name the localized seam accurately and let sessions use the
# generic publication contract rather than the B3 migration spelling.
replace(
    "crates/noon-runtime/src/lib.rs",
    "take_renderer_publication_with_followup_invalidation",
    "take_renderer_publication_with_followup_presentation_redraw",
    1,
)
replace(
    "crates/noon/src/execution_session.rs",
    "take_renderer_publication_with_followup_invalidation",
    "take_renderer_publication_with_followup_presentation_redraw",
    1,
)
replace(
    "crates/noon/src/execution_session.rs",
    ".with_derived_display_objects(&self.derived_display_objects)",
    ".with_transient_presentations(&self.derived_display_objects)",
    1,
)

# Make presentation-only dirtiness an explicit predicate. It remains non-empty so
# hosts redraw, while retained preparation can prove there is no stable dirtiness.
replace(
    "crates/noon-runtime/src/frame.rs",
    """    pub const fn requires_presentation_redraw(&self) -> bool {
        self.presentation_redraw
    }

    pub fn object_indices(&self) -> &[usize] {
""",
    """    pub const fn requires_presentation_redraw(&self) -> bool {
        self.presentation_redraw
    }

    /// True when this change set requests presentation work but carries no stable
    /// frame, structure, painter-order, or spatial dirtiness.
    pub fn is_presentation_only(&self) -> bool {
        self.presentation_redraw
            && !self.all
            && self.object_indices.is_empty()
            && self.added_indices.is_empty()
            && self.removed_indices.is_empty()
            && self.painter_order_range.is_none()
    }

    pub fn object_indices(&self) -> &[usize] {
""",
)
replace(
    "crates/noon-runtime/src/frame.rs",
    """        assert!(changes.requires_presentation_redraw());
        assert!(!changes.is_empty());
""",
    """        assert!(changes.requires_presentation_redraw());
        assert!(changes.is_presentation_only());
        assert!(!changes.is_empty());
""",
)
replace(
    "crates/noon-runtime/src/frame.rs",
    """        assert!(changes.is_all());
        assert!(!changes.requires_presentation_redraw());
""",
    """        assert!(changes.is_all());
        assert!(!changes.requires_presentation_redraw());
        assert!(!changes.is_presentation_only());
""",
)

# Runtime terminology remains migration-compatible, but production comments no
# longer give the generic lane feature-specific semantic meaning.
replace(
    "crates/noon-runtime/src/renderer_publication.rs",
    """/// plan-local visual occurrence whose lifetime is bounded by execution/publication,
/// including the current unequal-family Transform padding copies. Content still
""",
    """/// plan-local visual occurrence whose lifetime is bounded by execution/publication,
/// including plan-local copies and other transient effects. Content still
""",
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
    """    /// Migration accessor for the currently stacked unequal-family Transform work.
""",
    """    /// Migration accessor for the currently stacked B3 work.
""",
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

# The 100k locality regression now carries a real transient occurrence at the
# endpoint and proves the next publication removes it without touching stable rows.
replace(
    "crates/noon-runtime/tests/geometry_patch_locality.rs",
    "use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2};\nuse noon_runtime::{RuntimePatchStats, SceneInstance};",
    """use noon_core::{GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2};
use noon_runtime::{
    RuntimePatchStats, SceneInstance, TransientPresentationOccurrence,
    TransientPresentationState,
};""",
)
replace(
    "crates/noon-runtime/tests/geometry_patch_locality.rs",
    """    {
        let endpoint = live.take_renderer_publication_with_followup_invalidation();
        assert!(endpoint.changes().is_empty());
    }

    let retirement = live.take_renderer_publication();
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
""",
)
replace(
    "crates/noon-runtime/tests/geometry_patch_locality.rs",
    """    assert!(changes.requires_presentation_redraw());
    assert!(!changes.is_structural());
""",
    """    assert!(changes.requires_presentation_redraw());
    assert!(changes.is_presentation_only());
    assert!(!changes.is_structural());
""",
)
replace(
    "crates/noon-runtime/tests/geometry_patch_locality.rs",
    """    assert_eq!(retirement.frame().objects.len(), OBJECT_COUNT);
    assert!(live.take_spatial_changes().is_empty());
""",
    """    assert_eq!(retirement.frame().objects.len(), OBJECT_COUNT);
    assert!(retirement.transient_presentations().is_empty());
    assert!(live.take_spatial_changes().is_empty());
""",
)

# Existing end-to-end family Transform regression now checks the localized generic
# retirement contract rather than the temporary full invalidation seam.
replace(
    "crates/noon/src/execution_session/family_transform_tests.rs",
    """    // Completion grants the endpoint one coherent publication, then queues one
    // renderer-only follow-up so the synthetic alignment occurrence is actually
    // removed from a presented surface.
    assert!(session.wake_state().frame_pending());
    let removal = session.take_renderer_publication();
    assert!(removal.changes().is_all());
    assert!(removal.derived_display_objects().is_empty());
""",
    """    // Completion grants the endpoint one coherent publication, then queues one
    // presentation-only follow-up. Stable rows and spatial state remain resident
    // while the transient occurrence is removed from the presented surface.
    assert!(session.wake_state().frame_pending());
    let removal = session.take_renderer_publication();
    assert!(!removal.changes().is_all());
    assert!(removal.changes().is_presentation_only());
    assert!(removal.changes().object_indices().is_empty());
    assert!(removal.changes().added_indices().is_empty());
    assert!(removal.changes().removed_indices().is_empty());
    assert!(!removal.changes().has_painter_order_change());
    assert!(removal.transient_presentations().is_empty());
""",
)

# Retained preparation treats presentation-only dirtiness as zero stable dirtiness.
# The publication remains non-empty outside this function, so hosts still redraw.
retained_path = Path("crates/noon-render-wgpu/src/gpu/retained_text.rs")
retained = retained_path.read_text()
function = "fn prepare_with_changes_inner<'a>("
start = retained.find(function)
if start < 0:
    raise SystemExit("retained_text.rs: prepare_with_changes_inner missing")
result = ") -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {"
body = retained.find(result, start)
if body < 0:
    raise SystemExit("retained_text.rs: prepare_with_changes_inner return signature changed")
body += len(result)
insert = """
        // Presentation-only dirtiness wakes a new draw so transient pixels can be
        // erased, but it must not invalidate or rebuild stable retained state.
        let stable_changes = changes
            .is_presentation_only()
            .then(FrameChanges::default);
        let changes = stable_changes.as_ref().unwrap_or(changes);
"""
if insert.strip() in retained:
    raise SystemExit("retained_text.rs: presentation-only normalization already present")
retained = retained[:body] + insert + retained[body:]
retained_path.write_text(retained)

replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    '"derived family Transform display rows are not yet interleaved with retained glyph painter items",',
    '"transient presentation rows are not yet interleaved with retained glyph painter items",',
)

# Keep the legacy error/method for stacked callers, but provide a generic public
# renderer entry point and move the direct hosts onto it.
replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    """pub enum RetainedDerivedDisplayError {
    MixedTextUnsupported,
}
""",
    """pub enum RetainedDerivedDisplayError {
    MixedTextUnsupported,
}

/// Feature-neutral spelling for the retained transient-presentation boundary.
pub type RetainedTransientPresentationError = RetainedDerivedDisplayError;
""",
)
old_doc = """    /// Encode a retained geometry-only frame with identity-free derived analytic
    /// occurrences. Mixed glyph frames fail closed until the retained text stream can
    /// represent plan-local occurrence ordinals without manufacturing object IDs.
    pub fn encode_retained_with_derived(
"""
generic = """    /// Encode a retained geometry-only frame with identity-free transient analytic
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
"""
replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    old_doc,
    generic,
)
replace(
    "crates/noon-native/src/lib.rs",
    ".encode_retained_with_derived(",
    ".encode_retained_with_transient_presentations(",
    1,
)
replace(
    "crates/noon-web/src/execution_canvas.rs",
    ".encode_retained_with_derived(",
    ".encode_retained_with_transient_presentations(",
    1,
)

# Focused renderer regression: a presentation-only redraw must reuse all stable
# mixed-frame preparation rather than walking/repacking stable content.
replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    """    #[test]
    fn new_or_changed_text_generation_requires_gpu_upload() {
""",
    """    #[test]
    fn presentation_only_redraw_reuses_mixed_retained_preparation() {
        let (frame, texts, fonts, geometries) = geometry_and_fast_text_frame();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let initial_text_generation;

        {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::all(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                )
                .unwrap();
            initial_text_generation = prepared.text_generation;
        }
        let baseline = preparer.incremental_stats();
        let baseline_generation_reuses = preparer.prepared_generation_reuses;

        {
            let prepared = preparer
                .prepare_with_changes(
                    &device,
                    &queue,
                    &frame,
                    &FrameChanges::presentation_redraw(),
                    &texts,
                    &fonts,
                    &geometries,
                    metrics,
                )
                .unwrap();
            assert_eq!(prepared.geometry_stats().full_rebuilds, 0);
            assert_eq!(prepared.geometry_stats().instances_repacked, 0);
            assert_eq!(prepared.geometry_stats().dirty_instance_count, 0);
            assert_eq!(prepared.text_generation, initial_text_generation);
        }

        let after = preparer.incremental_stats();
        assert_eq!(after.scratch_rebuilds, baseline.scratch_rebuilds);
        assert_eq!(after.scratch_reuses, baseline.scratch_reuses + 1);
        assert_eq!(after.text_snapshot_copies, baseline.text_snapshot_copies);
        assert_eq!(after.mixed_order_rebuilds, baseline.mixed_order_rebuilds);
        assert_eq!(
            preparer.prepared_generation_reuses,
            baseline_generation_reuses + 1
        );
    }

    #[test]
    fn new_or_changed_text_generation_requires_gpu_upload() {
""",
)

# Targeted architecture ratchet: generic publication/host entry points, localized
# retirement, no synthetic stable identity, and retained-preparation locality.
ratchet = r'''#!/usr/bin/env bash
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
require(frame, "pub fn is_presentation_only(&self) -> bool", "presentation-only classification disappeared")

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
    raise SystemExit("transient presentation ratchet: execution session regained full-invalidating retirement helper")
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
require(retained, ".is_presentation_only()", "retained preparation no longer recognizes presentation-only locality")
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
'''
Path("scripts/transient-presentation-ratchet.sh").write_text(ratchet)

self_test = r'''#!/usr/bin/env bash
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
    pub fn is_presentation_only(&self) -> bool { true }
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
fn prepare(changes: Changes) { let _ = changes.is_presentation_only(); }
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

sed -i 's/changes.is_presentation_only()/true/' crates/noon-render-wgpu/src/gpu/retained_text.rs
expect_rejected 'stable-preparation locality removal' 'no longer recognizes presentation-only locality'
write_fixture

printf '\n// FamilyTransform feature leak\n' >> crates/noon-render-wgpu/src/gpu/derived_display.rs
expect_rejected 'feature-specific renderer coupling' 'feature-specific renderer coupling'
write_fixture

sed -i 's/encode_retained_with_transient_presentations/encode_retained_with_derived/' crates/noon-native/src/lib.rs
expect_rejected 'feature-specific direct host entry point' 'bypasses generic transient renderer entry point'

echo "transient presentation ratchet self-test passed"
'''
Path("scripts/transient-presentation-ratchet.test.sh").write_text(self_test)

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
    ".github/workflows/architecture-ratchets.yml",
    """            scripts/renderer-host-boundary-ratchet.sh \\
            scripts/active-perf-frontend-ratchet.sh \\
""",
    """            scripts/renderer-host-boundary-ratchet.sh \\
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

# Final fail-closed sanity checks on the staged source shape.
require(
    "crates/noon-runtime/src/lib.rs",
    "take_renderer_publication_with_followup_presentation_redraw",
)
require(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    "presentation_only_redraw_reuses_mixed_retained_preparation",
)
