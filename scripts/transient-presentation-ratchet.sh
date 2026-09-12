#!/usr/bin/env bash
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
