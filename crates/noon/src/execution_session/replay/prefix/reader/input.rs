//! Forward presentation-only input through the existing shared session policy.
//! No frontend picking, gesture synthesis, frame retagging or causal mutation.
use super::PrefixReader;
use crate::execution_session::{
    ExecutionSessionInputError, ExecutionViewportQuery, InspectionNavigationError,
    NativePointerInputPublication, NativePointerInputToken, PointerFillQuery, PointerFrameError,
    PointerFrameSnapshot, PointerFrameView, PointerHoverTransition, PointerSelectionHighlight,
    PointerSelectionPresentation,
};
use noon_core::{Camera2DState, NativePointerId, NativePointerInput, Rect, SemanticNodeId, Vec2};
use noon_runtime::SpatialIndexUpdateStats;

impl PrefixReader {
    pub fn has_native_pointer_subscribers(&self) -> bool {
        self.session.has_native_pointer_subscribers()
    }
    pub fn enable_pointer_fill_selection(
        &mut self,
        max_movement: f32,
    ) -> Result<(), ExecutionSessionInputError> {
        self.session.enable_pointer_fill_selection(max_movement)
    }
    pub fn disable_pointer_fill_selection(&mut self) -> Result<(), ExecutionSessionInputError> {
        self.session.disable_pointer_fill_selection()
    }
    pub fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, ExecutionSessionInputError> {
        self.session
            .configure_native_pointer_input(pointer, view_revision)
    }
    pub fn native_pointer_input_token(
        &self,
    ) -> Result<NativePointerInputToken, ExecutionSessionInputError> {
        self.session.native_pointer_input_token()
    }
    pub fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        self.session.submit_native_pointer_input(token, input)
    }
    pub fn pick_native_pointer_fill(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        eligible: impl FnMut(SemanticNodeId) -> bool,
    ) -> Result<PointerFillQuery, ExecutionSessionInputError> {
        self.session
            .pick_native_pointer_fill(token, input, eligible)
    }
    pub fn selected_pointer_target(&self) -> Option<SemanticNodeId> {
        self.session.selected_pointer_target()
    }
    pub fn pointer_selection_highlight(&self) -> Option<PointerSelectionHighlight> {
        self.session.pointer_selection_highlight()
    }
    pub fn pointer_selection_presentation(&self) -> Option<PointerSelectionPresentation> {
        self.session.pointer_selection_presentation()
    }
    pub fn hovered_pointer_target(&self) -> Option<SemanticNodeId> {
        self.session.hovered_pointer_target()
    }
    pub fn refresh_pointer_hover(
        &mut self,
        displayed: &PointerFrameSnapshot,
        view: PointerFrameView,
    ) -> Result<Option<PointerHoverTransition>, PointerFrameError> {
        self.session.refresh_pointer_hover(displayed, view)
    }
    pub fn capture_pointer_frame(
        &self,
        view: PointerFrameView,
    ) -> Result<PointerFrameSnapshot, PointerFrameError> {
        self.session.capture_pointer_frame(view)
    }
    /// Validate the caller's actually presented snapshot, never replace it with
    /// the producer frontier or a newer unpresented reader publication.
    pub fn pointer_input_token_for_frame(
        &self,
        displayed: &PointerFrameSnapshot,
        view: PointerFrameView,
    ) -> Result<NativePointerInputToken, PointerFrameError> {
        displayed.input_token(&self.session, view)
    }
    pub fn inspection_camera(&self) -> Result<Camera2DState, PointerFrameError> {
        self.session.inspection_camera()
    }
    pub fn inspection_pointer_view(
        &self,
        revision: u64,
        viewport: Vec2,
    ) -> Result<PointerFrameView, PointerFrameError> {
        self.session.inspection_pointer_view(revision, viewport)
    }
    pub fn inspection_view_revision(&self) -> u64 {
        self.session.inspection_view_revision()
    }
    pub fn zoom_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        view: PointerFrameView,
        surface: Vec2,
        factor: f64,
    ) -> Result<bool, InspectionNavigationError> {
        self.session
            .zoom_inspection_view(displayed, view, surface, factor)
    }
    pub fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, InspectionNavigationError> {
        self.session
            .scroll_inspection_view(displayed, view, surface, delta_pixels)
    }
    pub fn reset_inspection_view(&mut self) -> Result<bool, InspectionNavigationError> {
        self.session.reset_inspection_view()
    }
    pub fn query_viewport(&mut self, bounds: Rect) -> ExecutionViewportQuery {
        self.session.query_viewport(bounds)
    }
    /// Candidate membership is qualified by both reader incarnation and frame.
    pub fn viewport_query_is_current(&self, query: &ExecutionViewportQuery) -> bool {
        query.is_current_for(&self.session)
    }
    pub fn last_spatial_update_stats(&self) -> SpatialIndexUpdateStats {
        self.session.last_spatial_update_stats()
    }
}
