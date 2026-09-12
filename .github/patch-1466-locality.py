from pathlib import Path


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    p = Path(path)
    text = p.read_text()
    actual = text.count(old)
    if actual != count:
        raise SystemExit(f"{path}: expected {count} occurrences, found {actual}: {old[:100]!r}")
    p.write_text(text.replace(old, new, count))


# Name the runtime seam by its actual semantics now that it no longer performs a
# full invalidation, and route the session through the generic publication API.
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
replace(
    "crates/noon-runtime/tests/geometry_patch_locality.rs",
    "take_renderer_publication_with_followup_invalidation",
    "take_renderer_publication_with_followup_presentation_redraw",
)

# Make presentation-only dirtiness explicit so stable preparers can distinguish it
# from actual object/structure/painter changes without heuristics.
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

    /// True when this publication requests presentation work but carries no stable
    /// frame, structural, painter-order, or spatial dirtiness.
    pub const fn is_presentation_only(&self) -> bool {
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

# Stable retained preparation treats presentation-only work as an empty stable
# change set. The publication still remains wake-worthy and advances context; only
# the stable cache preparation sees zero dirtiness.
replace(
    "crates/noon-render-wgpu/src/gpu/retained_text.rs",
    """    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        if changes.is_all() || changes.is_structural() {
""",
    """    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedPrepareError> {
        let stable_changes = changes.is_presentation_only().then(FrameChanges::default);
        let changes = stable_changes.as_ref().unwrap_or(changes);
        if changes.is_all() || changes.is_structural() {
""",
)

# Add a focused renderer regression beside the existing retained incremental tests.
path = Path("crates/noon-render-wgpu/src/gpu/retained_text.rs")
text = path.read_text()
anchor = """    #[test]\n    fn new_or_changed_text_generation_requires_gpu_upload() {\n"""
if text.count(anchor) != 1:
    raise SystemExit("retained_text.rs: expected unique insertion anchor")
regression = r'''    #[test]
    fn presentation_only_redraw_reuses_retained_preparation() {
        let (frame, texts, fonts, geometries) = mixed_text_frame();
        let metrics = TextDeviceMetrics::uniform(100.0).unwrap();
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();

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
            assert_eq!(prepared.geometry_stats().full_rebuilds, 1);
        }
        let baseline = preparer.incremental_stats();
        let reuse_before = preparer.prepared_generation_reuses;

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
        }

        let after = preparer.incremental_stats();
        assert_eq!(after.scratch_rebuilds, baseline.scratch_rebuilds);
        assert_eq!(after.scratch_reuses, baseline.scratch_reuses + 1);
        assert_eq!(after.text_snapshot_copies, baseline.text_snapshot_copies);
        assert_eq!(after.mixed_order_rebuilds, baseline.mixed_order_rebuilds);
        assert_eq!(preparer.prepared_generation_reuses, reuse_before + 1);
    }

'''
path.write_text(text.replace(anchor, regression + anchor, 1))

# Tighten the large-scene regression to assert the semantic helper name and the
# explicit presentation-only classification.
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

# Architecture ratchet: keep transient presentation identity-free and keep endpoint
# retirement out of the full-invalidation path.
Path("scripts/transient-presentation-ratchet.sh").write_text(r'''#!/usr/bin/env bash
set -euo pipefail

ROOT="${NOON_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$ROOT"

runtime='crates/noon-runtime/src/lib.rs'
publication='crates/noon-runtime/src/renderer_publication.rs'
session='crates/noon/src/execution_session.rs'
retained='crates/noon-render-wgpu/src/gpu/retained_text.rs'

for path in "$runtime" "$publication" "$session" "$retained"; do
  [[ -r "$path" ]] || { echo "transient presentation ratchet: missing $path" >&2; exit 2; }
done

if git grep -n 'take_renderer_publication_with_followup_invalidation' -- "$runtime" "$session"; then
  echo 'transient presentation ratchet: retired full-invalidation helper name returned' >&2
  exit 1
fi

helper="$(python3 - "$runtime" <<'PY'
from pathlib import Path
import sys
text = Path(sys.argv[1]).read_text()
name = 'pub fn take_renderer_publication_with_followup_presentation_redraw'
start = text.find(name)
if start < 0:
    raise SystemExit(2)
brace = text.find('{', start)
depth = 0
for i in range(brace, len(text)):
    c = text[i]
    if c == '{': depth += 1
    elif c == '}':
        depth -= 1
        if depth == 0:
            print(text[start:i+1])
            break
else:
    raise SystemExit(2)
PY
)" || { echo 'transient presentation ratchet: cannot parse follow-up redraw helper' >&2; exit 2; }

[[ "$helper" == *'FrameChanges::presentation_redraw()'* ]] || {
  echo 'transient presentation ratchet: follow-up redraw must use presentation_redraw' >&2
  exit 1
}
[[ "$helper" != *'invalidate_all('* ]] || {
  echo 'transient presentation ratchet: transient retirement must not invalidate all stable rows' >&2
  exit 1
}

if ! grep -q 'with_transient_presentations(&self.derived_display_objects)' "$session"; then
  echo 'transient presentation ratchet: execution session must publish through generic transient vocabulary' >&2
  exit 1
fi

if ! grep -q 'changes.is_presentation_only().then(FrameChanges::default)' "$retained"; then
  echo 'transient presentation ratchet: retained preparation must normalize presentation-only dirtiness to zero stable changes' >&2
  exit 1
fi

struct="$(python3 - "$publication" <<'PY'
from pathlib import Path
import sys
text = Path(sys.argv[1]).read_text()
name = 'pub struct DerivedDisplayObject {'
start = text.find(name)
if start < 0:
    raise SystemExit(2)
end = text.find('\n}', start)
if end < 0:
    raise SystemExit(2)
print(text[start:end+2])
PY
)" || { echo 'transient presentation ratchet: cannot parse transient occurrence representation' >&2; exit 2; }

if [[ "$struct" == *'ObjectId'* || "$struct" == *'SemanticNodeId'* || "$struct" == *'TrackId'* ]]; then
  echo 'transient presentation ratchet: transient occurrence acquired synthetic stable identity' >&2
  exit 1
fi

for required in \
  'pub type TransientPresentationState = DerivedDisplayObjectState;' \
  'pub type TransientPresentationOccurrence = DerivedDisplayObject;' \
  'pub const fn transient_presentations(&self)'; do
  grep -Fq "$required" "$publication" || {
    echo "transient presentation ratchet: missing generic publication contract: $required" >&2
    exit 1
  }
done

echo 'transient presentation locality and identity ratchet passed'
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
  "$TMP/crates/noon-render-wgpu/src/gpu"
cp "$RATCHET" "$TMP/scripts/transient-presentation-ratchet.sh"

cat > "$TMP/crates/noon-runtime/src/lib.rs" <<'EOF'
pub fn take_renderer_publication_with_followup_presentation_redraw(&mut self) {
    self.changes = FrameChanges::presentation_redraw();
}
EOF
cat > "$TMP/crates/noon-runtime/src/renderer_publication.rs" <<'EOF'
pub struct DerivedDisplayObject {
    anchor_object_index: u32,
    occurrence_index: u32,
}
pub type TransientPresentationState = DerivedDisplayObjectState;
pub type TransientPresentationOccurrence = DerivedDisplayObject;
pub const fn transient_presentations(&self) {}
EOF
cat > "$TMP/crates/noon/src/execution_session.rs" <<'EOF'
.with_transient_presentations(&self.derived_display_objects)
EOF
cat > "$TMP/crates/noon-render-wgpu/src/gpu/retained_text.rs" <<'EOF'
let stable_changes = changes.is_presentation_only().then(FrameChanges::default);
EOF

cd "$TMP"
git init -q
git add .
git commit -qm baseline -c user.name=test -c user.email=test@example.invalid
NOON_ROOT="$TMP" bash scripts/transient-presentation-ratchet.sh >/dev/null

expect_rejected() {
  local label="$1"
  if NOON_ROOT="$TMP" bash scripts/transient-presentation-ratchet.sh >/dev/null 2>&1; then
    echo "transient presentation ratchet self-test accepted $label" >&2
    exit 1
  fi
  git checkout -q -- .
}

python3 - <<'PY'
from pathlib import Path
p = Path('crates/noon-runtime/src/lib.rs')
p.write_text(p.read_text().replace('FrameChanges::presentation_redraw()', 'self.changes.invalidate_all()'))
PY
expect_rejected 'full invalidation retirement'

python3 - <<'PY'
from pathlib import Path
p = Path('crates/noon-runtime/src/renderer_publication.rs')
p.write_text(p.read_text().replace('occurrence_index: u32,', 'occurrence_index: u32,\n    object_id: ObjectId,'))
PY
expect_rejected 'synthetic ObjectId'

python3 - <<'PY'
from pathlib import Path
p = Path('crates/noon-render-wgpu/src/gpu/retained_text.rs')
p.write_text('let changed = changes.is_empty();\n')
PY
expect_rejected 'retained presentation-only normalization removal'

NOON_ROOT="$TMP" bash scripts/transient-presentation-ratchet.sh >/dev/null
echo 'transient presentation ratchet self-test passed'
''')

# Wire the new guard into the canonical local architecture entrypoint and its CI
# self-test inventory.
replace(
    "scripts/check-architecture.sh",
    """run_guard active-perf-frontend-ratchet.sh
run_guard architecture-ratchet.sh \"$base\"
""",
    """run_guard active-perf-frontend-ratchet.sh
run_guard transient-presentation-ratchet.sh
run_guard architecture-ratchet.sh \"$base\"
""",
)
replace(
    ".github/workflows/architecture-ratchets.yml",
    """            scripts/active-perf-frontend-ratchet.sh \\
            scripts/active-perf-frontend-ratchet.test.sh; do
""",
    """            scripts/active-perf-frontend-ratchet.sh \\
            scripts/active-perf-frontend-ratchet.test.sh \\
            scripts/transient-presentation-ratchet.sh \\
            scripts/transient-presentation-ratchet.test.sh; do
""",
)
replace(
    ".github/workflows/architecture-ratchets.yml",
    """          bash scripts/active-perf-frontend-ratchet.test.sh
          bash scripts/layer-dependency-ratchet.test.sh
""",
    """          bash scripts/active-perf-frontend-ratchet.test.sh
          bash scripts/transient-presentation-ratchet.test.sh
          bash scripts/layer-dependency-ratchet.test.sh
""",
)
