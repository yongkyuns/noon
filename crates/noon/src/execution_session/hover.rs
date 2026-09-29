//! Stationary hover is a session observation of a successfully presented frame.
//! It shares picking and never fabricates a new input occurrence or native event.
use noon_core::{NativePointerInput, PublicationContext, SemanticNodeId, Vec2};
use noon_runtime::SpatialQueryStats;

use super::{
    ExecutionSession, PointerFillOutcome, PointerFrameError, PointerFrameSnapshot, PointerFrameView,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct PointerHoverState {
    surface: Option<Vec2>,
    view_revision: Option<u64>,
    target: Option<SemanticNodeId>,
    observed: Option<(PublicationContext, PointerFrameView)>,
}

impl PointerHoverState {
    pub(super) fn cancel(&mut self) {
        self.surface = None;
        self.observed = None;
    }

    pub(super) fn accept(&mut self, input: NativePointerInput) {
        let surface = input.position().map(|position| position.surface());
        if self.surface != surface || self.view_revision != Some(input.context().view_revision) {
            self.observed = None;
        }
        self.surface = surface;
        self.view_revision = Some(input.context().view_revision);
    }
}

/// Exactly one observed enter/leave pair. No authored or effective scene write
/// is implied. Unsupported fills block hover just as they block click selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerHoverTransition {
    pub previous: Option<SemanticNodeId>,
    pub current: Option<SemanticNodeId>,
    pub publication: PublicationContext,
    pub spatial_stats: SpatialQueryStats,
    pub precise_tests: usize,
}

impl ExecutionSession {
    /// Refresh hover after presenting a frame, including geometry moving beneath
    /// a stationary pointer. Uses the last admitted surface position with this
    /// exact frame's camera. Hosts must invalidate/rebind input on view changes.
    ///
    /// Selection must be enabled. A repeated clean frame is O(1) and performs no
    /// query. This API does not schedule frames or require polling when settled.
    /// Stale displayed frames fail before changing the previous hover target.
    pub fn refresh_pointer_hover(
        &mut self,
        displayed: &PointerFrameSnapshot,
        view: PointerFrameView,
    ) -> Result<Option<PointerHoverTransition>, PointerFrameError> {
        displayed.validate_current(self, view)?;
        if !self.pointer_selection.enabled() {
            return Ok(None);
        }
        let hover = self.pointer_selection.hover;
        let publication = self.publication_context();
        if hover.observed == Some((publication, view)) {
            return Ok(None);
        }
        let point = hover
            .surface
            .filter(|_| hover.view_revision == Some(view.revision()))
            .map(|surface| displayed.position(surface))
            .transpose()?;
        let (outcome, spatial_stats, precise_tests) = point.map_or(
            (
                PointerFillOutcome::NoPosition,
                SpatialQueryStats::default(),
                0,
            ),
            |position| self.pick_effective_fill(position.scene(), |_| true),
        );
        let current = match outcome {
            PointerFillOutcome::Hit(target) => Some(target),
            _ => None,
        };
        self.pointer_selection.hover.target = current;
        self.pointer_selection.hover.observed = Some((publication, view));
        Ok((current != hover.target).then_some(PointerHoverTransition {
            previous: hover.target,
            current,
            publication,
            spatial_stats,
            precise_tests,
        }))
    }
}
