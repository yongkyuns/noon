//! Bounded, read-only observations for the ordinary native/browser renderer host.
use super::PrefixReader;
use crate::execution_session::ExecutionViewportQuery;
use noon_core::Rect;
use noon_runtime::{RuntimeWakeState, TimelineWakeState};

impl PrefixReader {
    /// Reuse the historical runtime's active channels and revision deadlines.
    /// The ready boundary adds only a wake, never permission to sample the open
    /// frontier. A host reaching that boundary must buffer until more is adopted.
    pub fn wake_state(&self) -> RuntimeWakeState {
        let state = self.session.wake_state();
        let boundary = self.end_time().unwrap_or_else(|| self.frontier());
        if self.end_time().is_some() && self.frame().time >= boundary {
            state.without_timeline_wake()
        } else {
            state.with_additional_timeline(TimelineWakeState::Deadline(boundary))
        }
    }

    pub fn painter_order(&self) -> &[u32] {
        self.session.painter_order()
    }

    /// Query and qualify only the renderer's candidate set in this reader.
    /// This neither exposes the session nor consumes pending renderer changes.
    pub fn query_renderer_viewports(&mut self, bounds: &[Rect]) -> ExecutionViewportQuery {
        let query = self.session.query_viewports(bounds);
        self.session
            .renderer_viewport_query(query)
            .expect("fresh prefix query shares the selected historical publication")
    }

    /// Explicit O(frame-size) diagnostic codec, never a rendering/input boundary.
    #[cfg(feature = "diagnostics")]
    pub fn diagnostic_frame_value(&self) -> serde_json::Value {
        crate::diagnostics::execution_frame_value(&self.session)
    }
}
