//! Action dispatch and completion on the existing session, with no second clock.
use super::{
    ExecutionSession, ExecutionSessionAnimationError, ExecutionSessionInputError,
    NativePointerInputPublication, SemanticCompositionRequest,
};
use crate::{ExecutionSegment, ExecutionSegmentCompletionError, IndicateOptions};
use noon_compile::CompiledPointerInteractions;
use noon_core::{
    AnimationOptions, NativePointerInput, NativePointerInputKind, ReactiveValue, SemanticStore,
    SemanticVec3, SignalId, Vec2,
};
use noon_runtime::{EvaluationError, FrameState};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PointerActionState {
    bindings: Option<CompiledPointerInteractions>,
    active: Option<ExecutionSegment>,
    last_action_sequence: Option<u64>,
}
impl PointerActionState {
    pub(super) const fn new(bindings: Option<CompiledPointerInteractions>) -> Self {
        Self {
            bindings,
            active: None,
            last_action_sequence: None,
        }
    }
    pub(super) const fn fresh(self) -> Self {
        Self::new(self.bindings)
    }
    pub(super) fn indication(self) -> Option<noon_core::PointerIndicateOptions> {
        self.bindings.and_then(|b| b.indicate)
    }
}
#[derive(Debug)]
pub enum PointerActionAdvanceError {
    Evaluation(EvaluationError),
    Completion(ExecutionSegmentCompletionError),
}
impl std::fmt::Display for PointerActionAdvanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evaluation(e) => e.fmt(f),
            Self::Completion(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for PointerActionAdvanceError {}

impl ExecutionSession {
    pub fn has_pointer_actions(&self) -> bool {
        self.pointer_actions.bindings.is_some()
    }
    pub fn accepts_pointer_wheel(&self) -> bool {
        self.pointer_actions
            .bindings
            .is_some_and(|b| b.zoom.is_some())
    }
    pub fn pointer_action_active(&self) -> bool {
        self.pointer_actions.active.is_some()
    }

    /// Consume the receipt of an already committed occurrence exactly once at the
    /// input integration boundary. Input publication and animation activation are
    /// separate transactions: an activation error must never replay the input.
    /// Busy clicks are intentionally dropped, not queued, so no scale drift or
    /// unbounded animation backlog can accumulate.
    pub fn dispatch_pointer_action(
        &mut self,
        store: &mut SemanticStore,
        publication: NativePointerInputPublication,
    ) -> Result<bool, ExecutionSessionAnimationError> {
        let Some(bindings) = self.pointer_actions.bindings else {
            return Ok(false);
        };
        let Some(indication) = bindings.indicate else {
            return Ok(false);
        };
        let Some(target) = publication.selection_click().and_then(|c| c.target()) else {
            return Ok(false);
        };
        if self
            .pointer_actions
            .last_action_sequence
            .is_some_and(|last| publication.input().sequence() <= last)
        {
            return Ok(false);
        }
        self.pointer_actions.last_action_sequence = Some(publication.input().sequence());
        if self.pending_segment_completion.is_some() {
            return Ok(false);
        }
        // Only the actual accepted current occurrence can activate a target. A
        // retained old receipt cannot start another animation after completion.
        if self.last_native_event_sequence != Some(publication.input().sequence())
            || publication.publication() != self.publication_context()
            || !self.semantic_object_is_reachable(target)
        {
            return Ok(false);
        }
        let Some(object) = self.execution_object_id(target) else {
            return Ok(false);
        };
        let Some(row) = self.runtime.frame_index_for_object(object) else {
            return Ok(false);
        };
        let center = self.frame().render_transform(row).translation;
        // The current picker admits centered analytic circles/rectangles only;
        // their effective transform translation is exactly their layout center.
        let request = SemanticCompositionRequest::Indicate {
            target,
            indication: IndicateOptions::new(indication.scale_factor, indication.color),
            scale_center: SemanticVec3::new(f64::from(center.x), f64::from(center.y), 0.0),
            options: AnimationOptions::new().run_time(indication.duration),
        };
        let segment = self.declare_and_activate_composition(
            store,
            bindings.root,
            &request,
            AnimationOptions::new(),
        )?;
        self.pointer_actions.active = Some(segment);
        Ok(true)
    }

    /// Advance interactive work through the ordinary timeline, stopping exactly at
    /// its existing logical completion barrier before advancing farther. Pausing
    /// belongs to the host's ordinary clock; this method has no wall time.
    pub fn advance_pointer_actions_to(
        &mut self,
        store: &mut SemanticStore,
        time: f64,
    ) -> Result<&FrameState, PointerActionAdvanceError> {
        if !time.is_finite() || time < self.frame().time {
            return Err(PointerActionAdvanceError::Evaluation(
                EvaluationError::InvalidTime(time),
            ));
        }
        if let Some(segment) = self.pointer_actions.active {
            self.evaluate(time.min(segment.end_time()))
                .map_err(PointerActionAdvanceError::Evaluation)?;
            if time >= segment.end_time() {
                self.complete_segment(store, segment)
                    .map_err(PointerActionAdvanceError::Completion)?;
                self.pointer_actions.active = None;
            }
        }
        self.evaluate(time)
            .map_err(PointerActionAdvanceError::Evaluation)
    }

    pub(super) fn prepare_pointer_zoom(
        &self,
        input: NativePointerInput,
        updates: &mut Vec<(SignalId, ReactiveValue)>,
    ) -> Result<(), ExecutionSessionInputError> {
        let NativePointerInputKind::Wheel { position, delta } = input.kind() else {
            return Ok(());
        };
        let Some(zoom) = self.pointer_actions.bindings.and_then(|b| b.zoom) else {
            return Ok(());
        };
        if delta.value().y == 0.0 {
            return Ok(());
        }
        if self.execution_object_id(zoom.camera) != Some(zoom.object)
            || !self.semantic_object_is_reachable(zoom.camera)
        {
            return Err(ExecutionSessionInputError::InvalidPointerAction(
                "zoom camera is no longer reachable",
            ));
        }
        let camera = self.camera().map_err(|_| {
            ExecutionSessionInputError::InvalidPointerAction("zoom camera is invalid")
        })?;
        let row = self.runtime.frame_index_for_object(zoom.object).ok_or(
            ExecutionSessionInputError::InvalidPointerAction("zoom camera row is absent"),
        )?;
        let transform = self.frame().render_transform(row);
        let old_height = f64::from(camera.height);
        let options = zoom.options;
        // Clamp in log space BEFORE exp; finite extreme wheel packets never overflow.
        let height = (old_height.ln() + f64::from(delta.value().y) * options.sensitivity)
            .clamp(options.min_height.ln(), options.max_height.ln())
            .exp();
        let ratio = height / old_height;
        let anchor = position.scene();
        let center = Vec2::new(
            (f64::from(anchor.x) + (f64::from(camera.center.x) - f64::from(anchor.x)) * ratio)
                as f32,
            (f64::from(anchor.y) + (f64::from(camera.center.y) - f64::from(anchor.y)) * ratio)
                as f32,
        );
        let scale = Vec2::new(
            (f64::from(transform.scale.x) * ratio) as f32,
            (f64::from(transform.scale.y) * ratio) as f32,
        );
        if ![center.x, center.y, scale.x, scale.y]
            .iter()
            .all(|x| x.is_finite())
            || scale.x <= 0.0
            || scale.y <= 0.0
        {
            return Err(ExecutionSessionInputError::InvalidPointerAction(
                "zoom result is not representable",
            ));
        }
        updates.push((zoom.center_signal, ReactiveValue::Vec2(center)));
        updates.push((zoom.scale_signal, ReactiveValue::Vec2(scale)));
        Ok(())
    }
}
