from pathlib import Path
import runpy

path = Path("crates/noon-runtime/src/lib.rs")
text = path.read_text()
current = """    /// Consume one renderer publication while queueing a presentation-only redraw
    /// for the immediately following publication.
    ///
    /// This supports identity-free transient occurrences whose exact endpoint must
    /// be presented once and then removed from the next presented surface. Stable
    /// frame rows, painter order, resources, and spatial state remain resident and
    /// are not marked dirty merely to erase the transient occurrence.
    pub fn take_renderer_publication_with_followup_invalidation(
        &mut self,
    ) -> RendererPublication<'_> {
        let changes = self.take_frame_changes();
        self.changes = FrameChanges::presentation_redraw();
        RendererPublication::new(
"""
old = """    /// Consume one renderer publication while queueing a renderer-only full
    /// invalidation for the immediately following publication.
    ///
    /// This supports transient overlays whose exact endpoint must be presented once
    /// and then removed on the next redraw without mutating scene or spatial state.
    pub fn take_renderer_publication_with_followup_invalidation(
        &mut self,
    ) -> RendererPublication<'_> {
        let changes = self.take_frame_changes();
        self.changes.invalidate_all();
        RendererPublication::new(
"""
if text.count(current) != 1:
    raise SystemExit("current concurrent locality helper shape changed; refusing stale patch")
path.write_text(text.replace(current, old, 1))
runpy.run_path(".github/patch-1478.py", run_name="__main__")
