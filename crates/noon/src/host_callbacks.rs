//! Direct Rust host callables over the canonical callback publication barrier.

use std::collections::BTreeMap;
use std::error::Error;

use crate::{
    CallbackAdvance, CallbackPhaseOverlay, CallbackReadRequest, CallbackReadValue,
    EffectiveObjectProperties, ExecutionSegment, ExecutionSegmentAdvanceError, ExecutionSession,
    ExecutionSessionCallbackError, FrameState, HostCallbackId, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticStore, Style, Transform2D,
};

type BoxedCallbackError = Box<dyn Error + 'static>;
type RustHostCallback =
    dyn for<'a> FnMut(&mut RustHostCallbackContext<'a>) -> Result<(), BoxedCallbackError> + 'static;

#[derive(Clone, Copy)]
enum CallbackAdvanceTarget {
    Time(f64),
    Segment {
        segment: ExecutionSegment,
        requested_time: f64,
        target_time: f64,
    },
}

impl CallbackAdvanceTarget {
    fn advance<'a>(
        self,
        session: &'a mut ExecutionSession,
    ) -> Result<CallbackAdvance<'a>, RustHostCallbackError> {
        match self {
            Self::Time(time) => session
                .advance_to_callback_barrier(time)
                .map_err(Into::into),
            Self::Segment {
                segment,
                requested_time,
                ..
            } => session
                .advance_segment_to_callback_barrier(segment, requested_time)
                .map_err(Into::into),
        }
    }

    fn reached(self, ready_time: f64) -> bool {
        match self {
            Self::Time(time) => ready_time == time,
            // A callback activation boundary with no active invocation can yield
            // Ready before the requested/clamped segment target, so keep driving.
            Self::Segment { target_time, .. } => ready_time == target_time,
        }
    }
}

/// Thin callback-local view over one revision-pinned ordered phase overlay.
///
/// Reads observe writes made by earlier callbacks in the phase. Setters produce
/// effective driver writes; they do not mutate authored semantic state.
pub struct RustHostCallbackContext<'a> {
    callback_id: HostCallbackId,
    occurrence_index: usize,
    target: SemanticNodeId,
    overlay: &'a mut CallbackPhaseOverlay,
    session: &'a ExecutionSession,
}

impl RustHostCallbackContext<'_> {
    pub const fn callback_id(&self) -> HostCallbackId {
        self.callback_id
    }

    pub const fn occurrence_index(&self) -> usize {
        self.occurrence_index
    }

    pub const fn target(&self) -> SemanticNodeId {
        self.target
    }

    pub const fn time(&self) -> f64 {
        self.overlay.time()
    }

    pub const fn delta_time(&self) -> f64 {
        self.overlay.delta_time()
    }

    pub fn target_state(&self) -> &EffectiveObjectProperties {
        self.overlay
            .object(self.target)
            .expect("session-selected callback target must retain its phase row")
    }

    pub fn object(&self, object: SemanticNodeId) -> Option<&EffectiveObjectProperties> {
        self.overlay.object(object)
    }

    /// Read an arbitrary live object through this exact unpublished phase.
    pub fn read_object(
        &self,
        object: SemanticNodeId,
    ) -> Result<EffectiveObjectProperties, ExecutionSessionCallbackError> {
        if let Some(staged) = self.overlay.object(object) {
            return Ok(staged.clone());
        }
        match self
            .session
            .required_callback_read(self.overlay.token(), CallbackReadRequest::Object(object))
            .map_err(ExecutionSessionCallbackError::from)?
        {
            CallbackReadValue::Object(value) => Ok(value),
            CallbackReadValue::Scalar(_) => unreachable!("object request returns object value"),
        }
    }

    /// Read an arbitrary scalar signal through this exact unpublished phase.
    pub fn scalar_signal(
        &self,
        signal: SemanticNodeId,
    ) -> Result<f32, ExecutionSessionCallbackError> {
        match self
            .session
            .required_callback_read(
                self.overlay.token(),
                CallbackReadRequest::ScalarSignal(signal),
            )
            .map_err(ExecutionSessionCallbackError::from)?
        {
            CallbackReadValue::Scalar(value) => Ok(value),
            CallbackReadValue::Object(_) => unreachable!("scalar request returns scalar value"),
        }
    }

    pub fn set_target_transform(
        &mut self,
        transform: Transform2D,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.overlay.set_transform(self.target, transform)
    }

    pub fn set_transform(
        &mut self,
        object: SemanticNodeId,
        transform: Transform2D,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.overlay.set_transform(object, transform)
    }

    pub fn set_target_style(&mut self, style: Style) -> Result<(), ExecutionSessionCallbackError> {
        self.overlay.set_style(self.target, style)
    }

    pub fn set_style(
        &mut self,
        object: SemanticNodeId,
        style: Style,
    ) -> Result<(), ExecutionSessionCallbackError> {
        self.overlay.set_style(object, style)
    }
}

/// Host-owned callable lookup for direct Rust execution.
///
/// The table allocates no semantic identity and contains no activation schedule.
/// Caller-supplied IDs may be referenced by any number of compiler-owned semantic
/// registration occurrences. [`ExecutionSession`] alone selects active occurrences
/// and their order.
#[derive(Default)]
pub struct RustHostCallbackTable {
    callbacks: BTreeMap<HostCallbackId, Box<RustHostCallback>>,
    /// Host-timing observation for the most recent bounded advance.
    ///
    /// This is not callback schedule state: the execution session remains the
    /// only authority for selecting phases and occurrences. Platform hosts use
    /// the bit only to exclude time spent executing an opaque callable from the
    /// next wall-to-authored interval.
    last_advance_completed_callback_phase: bool,
}

impl RustHostCallbackTable {
    pub const fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
            last_advance_completed_callback_phase: false,
        }
    }

    /// Whether the most recent callback-aware advance committed at least one
    /// required host callback phase.
    pub const fn last_advance_completed_callback_phase(&self) -> bool {
        self.last_advance_completed_callback_phase
    }

    pub fn insert<F, E>(
        &mut self,
        id: HostCallbackId,
        mut callback: F,
    ) -> Result<(), RustHostCallbackError>
    where
        F: for<'a> FnMut(&mut RustHostCallbackContext<'a>) -> Result<(), E> + 'static,
        E: Error + 'static,
    {
        if self.callbacks.contains_key(&id) {
            return Err(RustHostCallbackError::DuplicateCallback(id));
        }
        self.callbacks.insert(
            id,
            Box::new(move |context| {
                callback(context).map_err(|error| Box::new(error) as BoxedCallbackError)
            }),
        );
        Ok(())
    }

    pub fn contains(&self, id: HostCallbackId) -> bool {
        self.callbacks.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.callbacks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }

    /// Author one semantic occurrence for an already installed host callable.
    pub fn add_updater(
        &self,
        store: &mut SemanticStore,
        target: SemanticNodeId,
        callback: HostCallbackId,
        active_from: f64,
        position: Option<usize>,
    ) -> Result<SemanticMutationTransactionResult, RustHostCallbackError> {
        if !self.contains(callback) {
            return Err(RustHostCallbackError::UnknownCallback {
                callback,
                occurrence_index: None,
            });
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(target, callback, active_from, position);
        transaction
            .apply(store)
            .map_err(RustHostCallbackError::Semantic)
    }

    /// Advance through every compiler-selected callback barrier up to `time`.
    ///
    /// A large advance may cross activation boundaries, so the session can return
    /// more than one phase before reaching the requested time. Each phase commits
    /// once after every ordered callable succeeds.
    pub fn advance_to<'a>(
        &mut self,
        session: &'a mut ExecutionSession,
        time: f64,
    ) -> Result<&'a FrameState, RustHostCallbackError> {
        self.advance(CallbackAdvanceTarget::Time(time), session)
    }

    /// Advance one existing logical segment through the same ordered callback loop.
    ///
    /// Segment validation, forward clamping, and callback phase selection remain
    /// owned by [`ExecutionSession`]. This table only invokes the host callables
    /// selected by that session and commits their one ordered overlay.
    pub fn advance_segment_to<'a>(
        &mut self,
        session: &'a mut ExecutionSession,
        segment: ExecutionSegment,
        requested_time: f64,
    ) -> Result<&'a FrameState, RustHostCallbackError> {
        let current_time = session.frame().time;
        let target_time = if current_time >= segment.end_time() {
            current_time
        } else {
            requested_time.max(current_time).min(segment.end_time())
        };
        self.advance(
            CallbackAdvanceTarget::Segment {
                segment,
                requested_time,
                target_time,
            },
            session,
        )
    }

    fn advance<'a>(
        &mut self,
        target: CallbackAdvanceTarget,
        session: &'a mut ExecutionSession,
    ) -> Result<&'a FrameState, RustHostCallbackError> {
        self.last_advance_completed_callback_phase = false;
        loop {
            match target.advance(session)? {
                CallbackAdvance::Ready(frame) => {
                    let ready_time = frame.time;
                    if target.reached(ready_time) {
                        return Ok(session.frame());
                    }
                    continue;
                }
                CallbackAdvance::HostRequired {
                    invocations,
                    mut overlay,
                } => {
                    let token = overlay.token();
                    for invocation in invocations {
                        let callback_id = invocation.callback_id();
                        let occurrence_index = invocation.occurrence_index();
                        let Some(callback) = self.callbacks.get_mut(&callback_id) else {
                            session.fail_required_callback_phase(token)?;
                            return Err(RustHostCallbackError::UnknownCallback {
                                callback: callback_id,
                                occurrence_index: Some(occurrence_index),
                            });
                        };
                        let mut context = RustHostCallbackContext {
                            callback_id,
                            occurrence_index,
                            target: invocation.target(),
                            overlay: &mut overlay,
                            session,
                        };
                        if let Err(source) = callback(&mut context) {
                            session.fail_required_callback_phase(token)?;
                            return Err(RustHostCallbackError::CallbackFailed {
                                callback: callback_id,
                                occurrence_index,
                                source,
                            });
                        }
                    }
                    let batch = overlay.finish();
                    if let Err(error) = session.commit_required_callback_phase(batch) {
                        session.fail_required_callback_phase(token)?;
                        return Err(RustHostCallbackError::Session(error));
                    }
                    self.last_advance_completed_callback_phase = true;
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum RustHostCallbackError {
    DuplicateCallback(HostCallbackId),
    UnknownCallback {
        callback: HostCallbackId,
        occurrence_index: Option<usize>,
    },
    CallbackFailed {
        callback: HostCallbackId,
        occurrence_index: usize,
        source: BoxedCallbackError,
    },
    Semantic(SemanticMutationTransactionError),
    Session(ExecutionSessionCallbackError),
    Segment(ExecutionSegmentAdvanceError),
}

impl std::fmt::Display for RustHostCallbackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateCallback(callback) => write!(
                formatter,
                "host callback {} is already installed in this callable table",
                callback.get()
            ),
            Self::UnknownCallback {
                callback,
                occurrence_index,
            } => match occurrence_index {
                Some(index) => write!(
                    formatter,
                    "semantic callback occurrence {index} references missing host callable {}",
                    callback.get()
                ),
                None => write!(
                    formatter,
                    "host callable {} must be installed before authoring its occurrence",
                    callback.get()
                ),
            },
            Self::CallbackFailed {
                callback,
                occurrence_index,
                source,
            } => write!(
                formatter,
                "host callback {} failed at semantic occurrence {occurrence_index}: {source}",
                callback.get()
            ),
            Self::Semantic(error) => error.fmt(formatter),
            Self::Session(error) => error.fmt(formatter),
            Self::Segment(error) => error.fmt(formatter),
        }
    }
}

impl Error for RustHostCallbackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CallbackFailed { source, .. } => Some(source.as_ref()),
            Self::Semantic(error) => Some(error),
            Self::Session(error) => Some(error),
            Self::Segment(error) => Some(error),
            Self::DuplicateCallback(_) | Self::UnknownCallback { .. } => None,
        }
    }
}

impl From<ExecutionSessionCallbackError> for RustHostCallbackError {
    fn from(value: ExecutionSessionCallbackError) -> Self {
        Self::Session(value)
    }
}

impl From<ExecutionSegmentAdvanceError> for RustHostCallbackError {
    fn from(value: ExecutionSegmentAdvanceError) -> Self {
        Self::Segment(value)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::{AnimationOptions, RateFunction, Scene, Vec2};

    use super::*;

    const SET_Y: HostCallbackId = HostCallbackId::new(1);
    const SET_OPACITY: HostCallbackId = HostCallbackId::new(2);
    const ACCUMULATE_DT: HostCallbackId = HostCallbackId::new(3);

    #[test]
    fn typed_host_context_reads_scoped_signal_and_non_target_object() {
        let mut scene = Scene::new();
        let active = scene.circle(0.5).unwrap();
        let mut anchor = scene.circle(0.25).unwrap();
        anchor.set_translation(2.0, 0.0).unwrap();
        scene.add(&active).unwrap();
        scene.add(&anchor).unwrap();
        let tracker = scene.value_tracker(3.0).unwrap();
        let tracker_id = tracker.node_id();
        let anchor_id = anchor.node_id();
        let mut callbacks = RustHostCallbackTable::new();
        callbacks
            .insert(SET_Y, move |context| {
                let value = context.scalar_signal(tracker_id)?;
                let anchor = context.read_object(anchor_id)?;
                let mut transform = context.target_state().transform;
                transform.translation.x = value + anchor.transform.translation.x;
                context.set_target_transform(transform)
            })
            .unwrap();
        callbacks
            .add_updater(
                &mut scene.store().borrow_mut(),
                active.node_id(),
                SET_Y,
                0.0,
                None,
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        callbacks.advance_to(&mut session, 0.0).unwrap();
        let store = scene.store().borrow();
        let effective = session
            .effective_semantic_object(&store, active.node_id())
            .unwrap();
        assert_eq!(effective.object.transform.translation.x, 5.0);
    }

    #[test]
    fn callbacks_share_ordered_overlay_and_accumulate_from_prior_effective_frame() {
        let mut scene = Scene::new();
        let source = scene.circle(1.0).unwrap();
        let mut drift = scene.circle(0.5).unwrap();
        drift.set_translation(-3.0, 0.0).unwrap();
        scene.add(&source).unwrap();
        scene.add(&drift).unwrap();
        let mut target = source.target_editor().unwrap();
        target.set_translation(2.0, 0.0).unwrap();
        let animation = scene
            .declare_transform_to(
                &source,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        let order = Rc::new(RefCell::new(Vec::new()));
        let mut callbacks = RustHostCallbackTable::new();
        let first_order = Rc::clone(&order);
        callbacks
            .insert(SET_Y, move |context| {
                first_order.borrow_mut().push((SET_Y, context.time()));
                let mut transform = context.target_state().transform;
                transform.translation.y = 1.0;
                context.set_target_transform(transform)
            })
            .unwrap();
        let second_order = Rc::clone(&order);
        callbacks
            .insert(SET_OPACITY, move |context| {
                second_order
                    .borrow_mut()
                    .push((SET_OPACITY, context.time()));
                assert_eq!(context.target_state().transform.translation.y, 1.0);
                let mut style = context.target_state().style;
                style.opacity = 0.5;
                context.set_target_style(style)
            })
            .unwrap();
        let drift_order = Rc::clone(&order);
        callbacks
            .insert(ACCUMULATE_DT, move |context| {
                drift_order
                    .borrow_mut()
                    .push((ACCUMULATE_DT, context.time()));
                let mut transform = context.target_state().transform;
                transform.translation.y += context.delta_time() as f32;
                context.set_target_transform(transform)
            })
            .unwrap();
        {
            let mut store = scene.store().borrow_mut();
            callbacks
                .add_updater(&mut store, source.node_id(), SET_Y, 0.0, None)
                .unwrap();
            callbacks
                .add_updater(&mut store, source.node_id(), SET_OPACITY, 0.0, None)
                .unwrap();
            callbacks
                .add_updater(&mut store, drift.node_id(), ACCUMULATE_DT, 0.0, None)
                .unwrap();
        }

        let mut session = scene.execution_session().unwrap();
        {
            let mut live = scene.live(&mut session);
            live.play_animation(&animation).unwrap();
        }
        callbacks.advance_to(&mut session, 1.0).unwrap();
        assert!(callbacks.last_advance_completed_callback_phase());
        let (source_at_one, drift_at_one) = effective_pair(&scene, &session, &source, &drift);
        assert_eq!(source_at_one.0, Vec2::new(1.0, 1.0));
        assert_eq!(source_at_one.1, 0.5);
        assert_eq!(drift_at_one.0, Vec2::new(-3.0, 1.0));

        callbacks.advance_to(&mut session, 2.0).unwrap();
        let (source_at_two, drift_at_two) = effective_pair(&scene, &session, &source, &drift);
        assert_eq!(source_at_two.0, Vec2::new(2.0, 1.0));
        assert_eq!(source_at_two.1, 0.5);
        assert_eq!(drift_at_two.0, Vec2::new(-3.0, 2.0));
        assert_eq!(
            order.borrow().as_slice(),
            &[
                (SET_Y, 0.0),
                (SET_OPACITY, 0.0),
                (ACCUMULATE_DT, 0.0),
                (SET_Y, 1.0),
                (SET_OPACITY, 1.0),
                (ACCUMULATE_DT, 1.0),
                (SET_Y, 2.0),
                (SET_OPACITY, 2.0),
                (ACCUMULATE_DT, 2.0),
            ]
        );
        assert!(matches!(
            callbacks.advance_to(&mut session, 1.0),
            Err(RustHostCallbackError::Session(
                ExecutionSessionCallbackError::NonMonotonicAdvance { .. }
            ))
        ));
        assert!(!callbacks.last_advance_completed_callback_phase());
    }

    #[test]
    fn missing_callable_terminates_phase_without_publishing() {
        let mut scene = Scene::new();
        let target = scene.circle(1.0).unwrap();
        scene.add(&target).unwrap();
        let mut authoring_table = RustHostCallbackTable::new();
        authoring_table
            .insert(SET_Y, |_context| Ok::<_, std::io::Error>(()))
            .unwrap();
        authoring_table
            .add_updater(
                &mut scene.store().borrow_mut(),
                target.node_id(),
                SET_Y,
                0.0,
                None,
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let frame = session.frame().clone();
        let publication = session.publication_context();

        let mut missing = RustHostCallbackTable::new();
        let error = missing.advance_to(&mut session, 1.0).unwrap_err();
        assert!(matches!(
            error,
            RustHostCallbackError::UnknownCallback {
                callback: SET_Y,
                occurrence_index: Some(0)
            }
        ));
        assert_eq!(session.frame(), &frame);
        assert_eq!(session.publication_context(), publication);
        assert!(session.callback_termination().is_some());
        assert!(!missing.last_advance_completed_callback_phase());
    }

    #[test]
    fn callback_error_terminates_phase_without_publishing() {
        let mut scene = Scene::new();
        let target = scene.circle(1.0).unwrap();
        scene.add(&target).unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        callbacks
            .insert(SET_Y, |_context| {
                Err::<(), _>(std::io::Error::other("callback failed"))
            })
            .unwrap();
        callbacks
            .add_updater(
                &mut scene.store().borrow_mut(),
                target.node_id(),
                SET_Y,
                0.0,
                None,
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let frame = session.frame().clone();
        let publication = session.publication_context();

        assert!(matches!(
            callbacks.advance_to(&mut session, 0.0),
            Err(RustHostCallbackError::CallbackFailed {
                callback: SET_Y,
                occurrence_index: 0,
                ..
            })
        ));
        assert_eq!(session.frame(), &frame);
        assert_eq!(session.publication_context(), publication);
        assert!(session.callback_termination().is_some());
    }

    fn effective_pair(
        scene: &Scene,
        session: &ExecutionSession,
        source: &crate::Mobject,
        drift: &crate::Mobject,
    ) -> ((Vec2, f32), (Vec2, f32)) {
        let store = scene.store().borrow();
        let source = session
            .effective_semantic_object(&store, source.node_id())
            .unwrap()
            .object;
        let drift = session
            .effective_semantic_object(&store, drift.node_id())
            .unwrap()
            .object;
        (
            (source.transform.translation, source.style.opacity),
            (drift.transform.translation, drift.style.opacity),
        )
    }
}
