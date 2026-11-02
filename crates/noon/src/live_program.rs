//! Target-neutral ownership for realtime Rust authoring continuations.
//!
//! A [`LiveProgram`] drives the one execution component owned by its consumed
//! semantic [`Scene`]. It owns only the continuation control-plane state
//! needed to await one existing [`ExecutionSegment`]. Timeline evaluation,
//! callback ordering, semantic publication, and renderer-facing invalidation
//! remain in their existing owners.

use std::{error::Error, rc::Rc};

mod inspection;
#[cfg(feature = "prepared-prefix")]
mod playback;
#[cfg(feature = "prepared-prefix")]
pub use playback::{PlaybackProgram, PlaybackProgramError};
#[cfg(feature = "prepared-prefix")]
mod prepared;
#[cfg(feature = "prepared-prefix")]
pub use prepared::{PreparedSource, PreparedSourceError, PreparedSourceStatus};

use noon_core::{
    NativeEventOccurrence, NativeInputValue, NativePointerId, NativePointerInput,
    NativeStateSource, PublicationContext, Rect,
};

use crate::execution_session::{
    ExecutionViewportQuery, NativePointerInputPublication, NativePointerInputToken,
    TranslationDragError, TranslationDragReceipt,
};
use crate::{
    ExecutionSegment, ExecutionSegmentAdvanceError, ExecutionSegmentState, ExecutionSession,
    ExecutionSessionInputError, LiveSession, RustHostCallbackError, RustHostCallbackTable, Scene,
};
use noon_runtime::{FrameState, RendererPublication};

/// One application-authored continuation result.
///
/// The continuation owns ordinary Rust locals and semantic handles. It supplies
/// an existing segment created through the borrowed [`LiveSession`], never a
/// host-defined interval, clock, scheduler entry, or semantic identity.
#[derive(Debug, PartialEq)]
pub enum ContinuationStep {
    Await(ExecutionSegment),
    /// Publish this existing transaction only after the host prepares its resources.
    Publish(crate::StagedEffectPublication),
    Finished,
}

/// Resumable direct Rust authoring over one shared live scene/session pair.
pub trait LiveContinuation {
    type Error;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error>;
}

/// Observable control-plane state for one live Rust program.
///
/// `PublicationPending` means endpoint completion has coherently published, but
/// the host has not yet admitted that exact publication. The program cannot
/// resume authoring until [`LiveProgram::admit_publication`] receives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LiveProgramStatus {
    ReadyToResume,
    PreparingResources,
    Awaiting(ExecutionSegmentState),
    PublicationPending(PublicationContext),
    Finished,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum LiveProgramPhase {
    ReadyToResume,
    PreparingResources,
    Awaiting(ExecutionSegment),
    PublicationPending(PublicationContext),
    Finished,
    Terminal,
}

/// Failure while driving or resuming one live Rust program.
#[derive(Debug)]
pub enum LiveProgramError<E> {
    InvalidState {
        operation: &'static str,
        state: LiveProgramStatus,
    },
    PublicationMismatch {
        expected: PublicationContext,
        actual: PublicationContext,
    },
    PublicationStillPending {
        expected: PublicationContext,
    },
    CompletedSegment(ExecutionSegment),
    Callback(RustHostCallbackError),
    Input(ExecutionSessionInputError),
    TranslationDrag(TranslationDragError),
    Inspection(crate::InspectionNavigationError),
    Segment(ExecutionSegmentAdvanceError),
    Completion(crate::LiveSessionError),
    Continuation(E),
}

impl<E: std::fmt::Display> std::fmt::Display for LiveProgramError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidState { operation, state } => {
                write!(
                    formatter,
                    "cannot {operation} while live program is {state:?}"
                )
            }
            Self::PublicationMismatch { expected, actual } => write!(
                formatter,
                "cannot resume from publication {actual:?}; expected {expected:?}"
            ),
            Self::PublicationStillPending { expected } => write!(
                formatter,
                "cannot resume before publication {expected:?} is admitted by the host"
            ),
            Self::CompletedSegment(segment) => write!(
                formatter,
                "continuation returned an already completed segment ending at {}",
                segment.end_time()
            ),
            Self::Callback(error) => error.fmt(formatter),
            Self::Input(error) => error.fmt(formatter),
            Self::TranslationDrag(error) => error.fmt(formatter),
            Self::Inspection(error) => error.fmt(formatter),
            Self::Segment(error) => error.fmt(formatter),
            Self::Completion(error) => error.fmt(formatter),
            Self::Continuation(error) => error.fmt(formatter),
        }
    }
}

impl<E: Error + 'static> Error for LiveProgramError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Callback(error) => Some(error),
            Self::Input(error) => Some(error),
            Self::TranslationDrag(error) => Some(error),
            Self::Inspection(error) => Some(error),
            Self::Segment(error) => Some(error),
            Self::Completion(error) => Some(error),
            Self::Continuation(error) => Some(error),
            Self::InvalidState { .. }
            | Self::CompletedSegment(_)
            | Self::PublicationMismatch { .. }
            | Self::PublicationStillPending { .. } => None,
        }
    }
}

/// One consumed semantic scene, its matching runtime, and one application continuation.
///
/// The program has no public constructor from independent parts, so a host cannot
/// accidentally pair a semantic store with a foreign runtime. It retains at most
/// one existing segment and never creates another timeline or cursor.
pub struct LiveProgram<C> {
    scene: Scene,
    continuation: C,
    phase: LiveProgramPhase,
    pending_effects: Option<crate::StagedEffectPublication>,
}

impl Scene {
    /// Consume this scene into one paired live program.
    ///
    /// Lowering happens once before the continuation starts. All later semantic
    /// work is available only through the program's temporary [`LiveSession`].
    pub fn into_live_program<C: LiveContinuation>(
        mut self,
        continuation: C,
    ) -> Result<LiveProgram<C>, noon_compile::SemanticExecutionLoweringError> {
        let execution = self.execution_session()?;
        if execution.has_audio() {
            return Err(
                noon_compile::SemanticExecutionLoweringError::AudioExecutionUnavailable {
                    operation: "live source continuation",
                },
            );
        }
        if execution.has_playback_controls() {
            return Err(
                noon_compile::SemanticExecutionLoweringError::PlaybackControlsExecutionUnavailable {
                    operation: "live source continuation",
                },
            );
        }
        self.install_execution(execution);
        Ok(LiveProgram {
            scene: self,
            continuation,
            phase: LiveProgramPhase::ReadyToResume,
            pending_effects: None,
        })
    }
}

impl<C: LiveContinuation> LiveProgram<C> {
    /// Borrow the authoritative runtime for frame, wake, camera, and query observations.
    pub const fn session(&self) -> &ExecutionSession {
        self.scene.owned_execution()
    }

    /// Derive host cadence from the runtime and this program's one continuation barrier.
    ///
    /// An already-at-end segment still requests one immediate drive so exact-time
    /// callbacks and shared completion cannot be skipped merely because the
    /// underlying tokenless wait already reports a complete interval.
    pub fn wake_state(&self) -> noon_runtime::RuntimeWakeState {
        let wake = self.session().wake_state();
        if self.phase == LiveProgramPhase::PreparingResources {
            // Poll external preparation without advancing the authoritative frame.
            return wake.with_additional_timeline(noon_runtime::TimelineWakeState::Continuous);
        }
        let LiveProgramPhase::Awaiting(segment) = self.phase else {
            // Only an awaited segment authorizes authored-time advancement.
            // Endpoint presentation, authoring resumption, and a finished source
            // retain runtime/input invalidation without driving unbounded
            // callback registrations beyond the source's logical interval.
            return wake.without_timeline_wake();
        };
        let timeline = if self.session().frame().time >= segment.end_time() {
            noon_runtime::TimelineWakeState::Deadline(self.session().frame().time)
        } else {
            self.session().segment_state(segment).timeline()
        };
        wake.with_additional_timeline(timeline)
    }

    /// Query candidate frame rows through the session-owned derived spatial index.
    pub fn query_viewport(&mut self, bounds: Rect) -> ExecutionViewportQuery {
        self.scene.owned_execution_mut().query_viewport(bounds)
    }

    /// Feed a platform monotonic tick to input-driven animations, including after source completion.
    pub fn advance_interactions(
        &mut self,
        wall_time_seconds: f64,
    ) -> Result<(), LiveProgramError<C::Error>> {
        if !self.session().interactions_active() {
            return Ok(());
        }
        self.ensure_host_input_available("advance interaction")?;
        let store = std::rc::Rc::clone(self.scene.integration_store());
        self.scene
            .owned_execution_mut()
            .advance_interactions(&mut store.borrow_mut(), wall_time_seconds)
            .map_err(LiveProgramError::Input)?;
        self.refresh_pending_publication();
        Ok(())
    }

    /// Retire input-owned state before releasing this source's canonical owner.
    /// This leaves authored time unchanged and cancels unpublished resource work.
    /// Renderer recovery that retains the source must not call this method.
    pub fn retire_execution_source(&mut self) -> Result<(), LiveProgramError<C::Error>> {
        let store = std::rc::Rc::clone(self.scene.integration_store());
        self.scene
            .owned_execution_mut()
            .retire_interactions_for_source_replacement(&mut store.borrow_mut())
            .map_err(LiveProgramError::Input)?;
        if self.pending_effects.take().is_some() {
            self.phase = LiveProgramPhase::Terminal;
        }
        self.refresh_pending_publication();
        Ok(())
    }

    /// Observe a successfully presented frame without advancing the continuation
    /// or opening its input/publication barriers.
    pub fn refresh_pointer_hover(
        &mut self,
        frame: &crate::integration::PointerFrameSnapshot,
    ) -> Result<Option<crate::PointerHoverTransition>, crate::integration::PointerFrameError> {
        self.scene
            .owned_execution_mut()
            .refresh_pointer_hover(frame, frame.view())
    }

    /// Configure transient selection without exposing a mutable execution session.
    pub fn set_pointer_fill_selection(
        &mut self,
        max_movement: Option<f32>,
    ) -> Result<(), LiveProgramError<C::Error>> {
        self.ensure_host_input_available("configure pointer selection")?;
        let session = self.scene.owned_execution_mut();
        match max_movement {
            Some(value) => session.enable_pointer_fill_selection(value),
            None => session.disable_pointer_fill_selection(),
        }
        .map_err(LiveProgramError::Input)
    }

    /// Configure the semantic objects eligible for session-owned native
    /// translation dragging. Native callers still route every occurrence to
    /// [`Self::submit_translation_drag_input`]; they do not select an object
    /// identity for a press or move.
    pub fn set_translation_drag_targets<'target>(
        &mut self,
        targets: impl IntoIterator<Item = &'target crate::Mobject>,
    ) -> Result<(), LiveProgramError<C::Error>> {
        self.ensure_host_input_available("configure translation drag targets")?;
        let mut nodes = Vec::new();
        for target in targets {
            if !Rc::ptr_eq(self.scene.integration_store(), target.integration_store()) {
                return Err(LiveProgramError::TranslationDrag(
                    TranslationDragError::ForeignStore,
                ));
            }
            target.validate().map_err(|error| {
                LiveProgramError::TranslationDrag(TranslationDragError::Semantic(error.to_string()))
            })?;
            nodes.push(target.node_id());
        }
        self.scene
            .owned_execution_mut()
            .set_translation_drag_targets(nodes)
            .map_err(LiveProgramError::TranslationDrag)?;
        self.refresh_pending_publication();
        Ok(())
    }

    /// Configure the platform pointer projected into the existing unkeyed native signals.
    pub fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("configure contextual pointer input")?;
        let token = self
            .scene
            .owned_execution_mut()
            .configure_native_pointer_input(pointer, view_revision)
            .map_err(LiveProgramError::Input)?;
        self.refresh_pending_publication();
        Ok(token)
    }

    /// Capture the current session context without exposing mutable execution authority.
    pub fn native_pointer_input_token(
        &self,
    ) -> Result<NativePointerInputToken, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("capture contextual pointer input")?;
        self.session()
            .native_pointer_input_token()
            .map_err(LiveProgramError::Input)
    }

    /// Publish one occurrence and refresh an outstanding endpoint receipt after success.
    pub fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, LiveProgramError<C::Error>> {
        self.submit_translation_drag_input(token, input)
            .map(|receipt| receipt.input)
    }

    /// Deliver one typed pointer occurrence through the configured translation
    /// drag policy and the existing native pointer publication lane.
    ///
    /// Misses and non-captured occurrences retain ordinary pointer, hover, and
    /// click behavior. A captured primary drag alone publishes its scoped
    /// effective translation and reconciles the authored value on release.
    pub fn submit_translation_drag_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<TranslationDragReceipt, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("deliver translation drag input")?;
        let store = Rc::clone(self.scene.integration_store());
        let publication = self
            .scene
            .owned_execution_mut()
            .submit_translation_drag_input(&mut store.borrow_mut(), token, input)
            .map_err(|error| match error {
                TranslationDragError::Input(error) => LiveProgramError::Input(error),
                error => LiveProgramError::TranslationDrag(error),
            })?;
        self.refresh_pending_publication();
        Ok(publication)
    }

    /// Deliver one normalized sampled native value without exposing mutable session authority.
    pub fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<&FrameState, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("deliver native state input")?;
        self.scene
            .owned_execution_mut()
            .set_native_state_input(source, value)
            .map_err(LiveProgramError::Input)?;
        self.refresh_pending_publication();
        Ok(self.session().frame())
    }

    /// Deliver one ordered native event without exposing mutable session authority.
    pub fn emit_native_event(
        &mut self,
        occurrence: NativeEventOccurrence,
    ) -> Result<&FrameState, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("deliver a native event")?;
        self.scene
            .owned_execution_mut()
            .emit_native_event(occurrence)
            .map_err(LiveProgramError::Input)?;
        self.refresh_pending_publication();
        Ok(self.session().frame())
    }

    /// Consume the runtime's current renderer-facing invalidation without copying state.
    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.scene.owned_execution_mut().take_renderer_publication()
    }

    pub fn status(&self) -> LiveProgramStatus {
        match self.phase {
            LiveProgramPhase::ReadyToResume => LiveProgramStatus::ReadyToResume,
            LiveProgramPhase::PreparingResources => LiveProgramStatus::PreparingResources,
            LiveProgramPhase::Awaiting(segment) => {
                LiveProgramStatus::Awaiting(self.session().segment_state(segment))
            }
            LiveProgramPhase::PublicationPending(publication) => {
                LiveProgramStatus::PublicationPending(publication)
            }
            LiveProgramPhase::Finished => LiveProgramStatus::Finished,
            LiveProgramPhase::Terminal => LiveProgramStatus::Terminal,
        }
    }

    /// Renderer integration enables mandatory preparation for later shader
    /// introductions. Detached/headless semantic construction stays device-free.
    pub fn require_effect_resource_preparation(&mut self) {
        self.scene
            .owned_execution_mut()
            .require_effect_resource_preparation();
    }

    /// Owned immutable requests only; borrowing these is never necessary while
    /// awaiting compilation. The semantic transaction remains unpublished.
    pub fn pending_effect_programs(&self) -> Option<&[noon_core::WgslEffectProgram]> {
        self.pending_effects
            .as_ref()
            .map(|pending| pending.programs())
    }

    /// Host acknowledgement after successful resource preparation. The normal
    /// publisher reruns semantic/compiler/runtime preflight against current state.
    /// The resulting frame must then pass the existing retention barrier before
    /// the authoring continuation can resume.
    pub fn publish_prepared_effects(
        &mut self,
    ) -> Result<LiveProgramStatus, LiveProgramError<C::Error>> {
        if self.phase != LiveProgramPhase::PreparingResources {
            return Err(self.invalid_state("publish prepared effects"));
        }
        let pending = self
            .pending_effects
            .take()
            .expect("preparation owns a transaction");
        let segment = match self.scene.owned_live().publish_staged_effects(pending) {
            Ok(segment) => segment,
            Err(error) => {
                self.phase = LiveProgramPhase::Terminal;
                return Err(LiveProgramError::Completion(error));
            }
        };
        self.phase = match segment {
            Some(segment) => LiveProgramPhase::Awaiting(segment),
            None => LiveProgramPhase::PublicationPending(self.session().publication_context()),
        };
        Ok(self.status())
    }

    /// Invoke the application continuation exactly once from a ready barrier.
    ///
    /// A returned segment becomes the only segment this program can drive. A
    /// continuation error terminally closes the program and is never retried.
    pub fn resume(&mut self) -> Result<LiveProgramStatus, LiveProgramError<C::Error>> {
        if self.phase != LiveProgramPhase::ReadyToResume {
            return Err(self.invalid_state("resume authoring"));
        }
        let result = {
            let continuation = &mut self.continuation;
            let mut live = self.scene.owned_live();
            continuation.resume(&mut live)
        };
        match result {
            Ok(ContinuationStep::Await(segment)) => {
                match self.session().validate_segment_for_advance(segment) {
                    Ok(false) => {}
                    Ok(true) => {
                        self.phase = LiveProgramPhase::Terminal;
                        return Err(LiveProgramError::CompletedSegment(segment));
                    }
                    Err(error) => {
                        self.phase = LiveProgramPhase::Terminal;
                        return Err(LiveProgramError::Segment(error));
                    }
                }
                self.phase = LiveProgramPhase::Awaiting(segment);
                Ok(self.status())
            }
            Ok(ContinuationStep::Publish(pending)) => {
                if !pending.belongs_to(self.session()) {
                    self.phase = LiveProgramPhase::Terminal;
                    return Err(LiveProgramError::Completion(
                        crate::ExecutionSessionPublicationError::ForeignSemanticStore.into(),
                    ));
                }
                self.pending_effects = Some(pending);
                self.phase = LiveProgramPhase::PreparingResources;
                Ok(self.status())
            }
            Ok(ContinuationStep::Finished) => {
                self.phase = LiveProgramPhase::Finished;
                Ok(self.status())
            }
            Err(error) => {
                self.phase = LiveProgramPhase::Terminal;
                Err(LiveProgramError::Continuation(error))
            }
        }
    }

    /// Drive the current segment through shared callback and completion logic.
    ///
    /// Mid-segment calls remain `Awaiting`. At the exact endpoint, completion is
    /// reconciled once through [`LiveSession`]. If that leaves renderer-facing
    /// work pending, the exact publication must be admitted before `resume`;
    /// otherwise, including an unchanged pure wait, authoring is immediately ready.
    pub fn drive_to(
        &mut self,
        callbacks: &mut RustHostCallbackTable,
        requested_time: f64,
    ) -> Result<LiveProgramStatus, LiveProgramError<C::Error>> {
        let LiveProgramPhase::Awaiting(segment) = self.phase else {
            return Err(self.invalid_state("drive a segment"));
        };
        if let Err(error) =
            callbacks.advance_segment_to(self.scene.owned_execution_mut(), segment, requested_time)
        {
            self.phase = LiveProgramPhase::Terminal;
            return Err(LiveProgramError::Callback(error));
        }
        if self.session().frame().time < segment.end_time() {
            return Ok(self.status());
        }

        let completion = {
            let mut live = self.scene.owned_live();
            live.complete_segment(segment)
        };
        if let Err(error) = completion {
            self.phase = LiveProgramPhase::Terminal;
            return Err(LiveProgramError::Completion(error));
        }
        debug_assert!(self.session().segment_state(segment).is_complete());

        self.phase = if self.session().wake_state().frame_pending() {
            LiveProgramPhase::PublicationPending(self.session().publication_context())
        } else {
            LiveProgramPhase::ReadyToResume
        };
        Ok(self.status())
    }

    /// Acknowledge admission of the exact endpoint publication.
    ///
    /// Rendering and presentation stay host responsibilities. This method only
    /// pins the host acknowledgement to the runtime publication that completion
    /// produced; it cannot be used to skip segment advancement or completion.
    pub fn admit_publication(
        &mut self,
        publication: PublicationContext,
    ) -> Result<LiveProgramStatus, LiveProgramError<C::Error>> {
        let LiveProgramPhase::PublicationPending(expected) = self.phase else {
            return Err(self.invalid_state("admit an endpoint publication"));
        };
        if publication != expected {
            return Err(LiveProgramError::PublicationMismatch {
                expected,
                actual: publication,
            });
        }
        if self.session().wake_state().frame_pending() {
            return Err(LiveProgramError::PublicationStillPending { expected });
        }
        self.phase = LiveProgramPhase::ReadyToResume;
        Ok(self.status())
    }

    fn invalid_state(&self, operation: &'static str) -> LiveProgramError<C::Error> {
        LiveProgramError::InvalidState {
            operation,
            state: self.status(),
        }
    }

    fn ensure_host_input_available(
        &self,
        operation: &'static str,
    ) -> Result<(), LiveProgramError<C::Error>> {
        if matches!(self.phase, LiveProgramPhase::Terminal) {
            return Err(self.invalid_state(operation));
        }
        Ok(())
    }

    fn refresh_pending_publication(&mut self) {
        if matches!(self.phase, LiveProgramPhase::PublicationPending(_)) {
            let publication = self.session().publication_context();
            self.phase = LiveProgramPhase::PublicationPending(publication);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, convert::Infallible, rc::Rc};

    use noon_core::{
        AnimationOptions, HostCallbackId, RateFunction, SemanticMutationTransaction, SemanticVec3,
        Vec2,
    };

    use super::*;
    use noon_runtime::TimelineWakeState;

    #[cfg(feature = "wgsl-effects")]
    struct AttachShaders([crate::Mobject; 2], bool);
    #[cfg(feature = "wgsl-effects")]
    impl LiveContinuation for AttachShaders {
        type Error = crate::LiveSessionError;
        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            use crate::effects::{
                ShaderCoordinates, ShaderDependencies, ShaderEffect, ShaderProfile, ShaderSchema,
                WgslEffectProgram,
            };
            if self.1 {
                // Ordinary controls and detached animation targets must still work.
                let target = live.target_editor(&self.0[0])?;
                live.set_translation(&target, 1.0, 0.0)?;
                let definition = target.get_effect("stripes")?.authored_definition()?;
                let detached =
                    live.prepare_effect_attachments([(&target, definition, "detached")])?;
                assert!(
                    detached.programs().is_empty(),
                    "detached authoring needs no GPU work"
                );
                return Ok(ContinuationStep::Finished);
            }
            let material = WgslEffectProgram::prepare(ShaderSchema {
                profile: ShaderProfile::PaintedPointwiseV1,
                dependencies: ShaderDependencies { coordinates: ShaderCoordinates::Scene, authored_time: false, seed: false },
                parameters: Box::new([]),
            }, "fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> { return input.color; }").unwrap();
            assert!(matches!(
                live.add_effect(&self.0[0], ShaderEffect::new(material.clone()), "stripes"),
                Err(crate::LiveSessionError::Publication(
                    crate::ExecutionSessionPublicationError::ExternalEffectPreparationRequired
                ))
            ));
            self.1 = true;
            Ok(ContinuationStep::Publish(live.prepare_effect_attachments(
                self.0.iter().map(|object| {
                    (
                        object,
                        ShaderEffect::new(material.clone()).into(),
                        "stripes",
                    )
                }),
            )?))
        }
    }

    #[cfg(feature = "wgsl-effects")]
    fn shader_attachment_program() -> LiveProgram<AttachShaders> {
        let mut scene = Scene::new();
        let panels = [scene.circle(0.4).unwrap(), scene.circle(0.5).unwrap()];
        for panel in &panels {
            scene.add(panel).unwrap();
        }
        let mut program = scene
            .into_live_program(AttachShaders(panels, false))
            .unwrap();
        program.require_effect_resource_preparation();
        program.take_renderer_publication();
        program
    }

    #[cfg(feature = "wgsl-effects")]
    #[test]
    fn shader_resources_are_staged_before_one_coherent_publication_and_retention() {
        let mut program = shader_attachment_program();
        let store = Rc::clone(program.scene.integration_store());
        let before = program.session().publication_context();
        let revision = store.borrow().scene_revision();
        let count = store.borrow().len();
        assert_eq!(
            program.resume().unwrap(),
            LiveProgramStatus::PreparingResources
        );
        assert_eq!(program.pending_effect_programs().unwrap().len(), 1);
        assert_eq!(program.session().publication_context(), before);
        assert_eq!(store.borrow().scene_revision(), revision);
        assert_eq!(store.borrow().len(), count);
        for panel in &program.continuation.0 {
            assert!(store
                .borrow()
                .effect_by_name(panel.node_id(), "stripes")
                .unwrap()
                .is_none());
        }
        assert!(program.take_renderer_publication().changes().is_empty());
        assert!(program.resume().is_err());
        let LiveProgramStatus::PublicationPending(published) =
            program.publish_prepared_effects().unwrap()
        else {
            panic!("the published frame still needs renderer retention");
        };
        assert_eq!(
            store.borrow().scene_revision(),
            revision.checked_next().unwrap()
        );
        assert_eq!(store.borrow().len(), count + 2);
        assert_ne!(published, before);
        assert!(program.resume().is_err());
        program.take_renderer_publication();
        program.admit_publication(published).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }

    #[cfg(feature = "wgsl-effects")]
    #[test]
    fn shader_preparation_revalidates_current_state_and_source_retirement_cancels_it() {
        let mut program = shader_attachment_program();
        program.resume().unwrap();
        let panels = program.continuation.0.clone();
        program
            .scene
            .owned_live()
            .add_effect(&panels[1], crate::effects::Glow::default(), "stripes")
            .unwrap();
        let store = Rc::clone(program.scene.integration_store());
        let revision = store.borrow().scene_revision();
        let count = store.borrow().len();
        let before = program.session().publication_context();
        assert!(program.publish_prepared_effects().is_err());
        assert_eq!(program.status(), LiveProgramStatus::Terminal);
        assert_eq!(program.session().publication_context(), before);
        assert_eq!(store.borrow().scene_revision(), revision);
        assert_eq!(store.borrow().len(), count);
        assert!(store
            .borrow()
            .effect_by_name(panels[0].node_id(), "stripes")
            .unwrap()
            .is_none());

        let mut retired = shader_attachment_program();
        retired.resume().unwrap();
        let before = retired.session().publication_context();
        retired.retire_execution_source().unwrap();
        assert!(retired.pending_effect_programs().is_none());
        assert!(retired.publish_prepared_effects().is_err());
        assert_eq!(retired.session().publication_context(), before);
    }

    struct AnimatedContinuation {
        source: crate::Mobject,
        target: crate::Mobject,
        resumes: Rc<RefCell<usize>>,
        stage: usize,
    }

    impl LiveContinuation for AnimatedContinuation {
        type Error = crate::LiveSessionError;

        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            *self.resumes.borrow_mut() += 1;
            match self.stage {
                0 => {
                    self.stage = 1;
                    let segment = live.declare_and_activate_transform_to(
                        &self.source,
                        &self.target,
                        AnimationOptions::new()
                            .run_time(2.0)
                            .rate_func(RateFunction::Linear),
                    )?;
                    Ok(ContinuationStep::Await(segment))
                }
                1 => {
                    self.stage = 2;
                    live.set_translation(&self.source, 3.0, -1.0)?;
                    assert_eq!(
                        live.effective(&self.source)?.transform.translation,
                        Vec2::new(3.0, -1.0)
                    );
                    Ok(ContinuationStep::Finished)
                }
                _ => unreachable!("finished continuation must never resume again"),
            }
        }
    }

    #[test]
    fn animated_completion_requires_exact_publication_before_resuming_once() {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        scene.add(&source).unwrap();
        let opacity = scene.control_signal("opacity", 1.0).unwrap();
        scene.bind_opacity(&source, &opacity).unwrap();
        let mut target = source.target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(AnimatedContinuation {
                source: source.clone(),
                target,
                resumes: Rc::clone(&resumes),
                stage: 0,
            })
            .unwrap();
        // Admit the initial frame independently of any continuation barrier.
        let initial = program.take_renderer_publication().context();

        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let mut callbacks = RustHostCallbackTable::new();
        assert!(matches!(
            program.drive_to(&mut callbacks, 1.0).unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let LiveProgramStatus::PublicationPending(endpoint) =
            program.drive_to(&mut callbacks, 9.0).unwrap()
        else {
            panic!("animated endpoint must await its renderer publication")
        };
        assert_eq!(*resumes.borrow(), 1);
        assert!(matches!(
            program.resume(),
            Err(LiveProgramError::InvalidState { .. })
        ));
        assert!(matches!(
            program.admit_publication(initial),
            Err(LiveProgramError::PublicationMismatch { .. })
        ));
        assert!(matches!(
            program.admit_publication(endpoint),
            Err(LiveProgramError::PublicationStillPending { .. })
        ));

        // Unbound platform samples are valid no-ops and retain the exact endpoint receipt.
        program
            .set_native_state_input(
                NativeStateSource::PointerPosition,
                NativeInputValue::Vec2(Vec2::new(12.0, 8.0)),
            )
            .unwrap();
        assert_eq!(
            program.status(),
            LiveProgramStatus::PublicationPending(endpoint)
        );

        // Bound input may publish while authoring remains suspended. The host must
        // admit the new coherent receipt rather than the superseded endpoint receipt.
        program
            .set_native_state_input(
                NativeStateSource::Control {
                    name: "opacity".to_owned(),
                },
                NativeInputValue::Scalar(0.5),
            )
            .unwrap();
        let LiveProgramStatus::PublicationPending(latest) = program.status() else {
            panic!("native input must preserve the endpoint presentation barrier")
        };
        assert_ne!(latest, endpoint);
        assert!(matches!(
            program.admit_publication(endpoint),
            Err(LiveProgramError::PublicationMismatch { .. })
        ));

        let admitted = program.take_renderer_publication().context();
        assert_eq!(admitted, latest);
        assert_eq!(
            program.admit_publication(admitted).unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(*resumes.borrow(), 2);
        assert!(matches!(
            program.resume(),
            Err(LiveProgramError::InvalidState { .. })
        ));
        assert_eq!(*resumes.borrow(), 2);
        assert_eq!(
            source.state().unwrap().transform.translation,
            SemanticVec3::new(3.0, -1.0, 0.0)
        );
    }

    struct WaitContinuation {
        stage: usize,
        resumes: Rc<RefCell<usize>>,
    }

    struct ReturnSegment(ExecutionSegment);

    impl LiveContinuation for ReturnSegment {
        type Error = Infallible;

        fn resume(&mut self, _live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            Ok(ContinuationStep::Await(self.0))
        }
    }

    fn scene_with_pending_segment() -> (Scene, ExecutionSession, ExecutionSegment) {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        scene.add(&source).unwrap();
        let mut target = source.target_editor().unwrap();
        target.set_translation(2.0, 0.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_transform_to(
                &source,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        (scene, session, segment)
    }

    #[test]
    fn returned_foreign_or_stale_segment_is_terminal_before_drive() {
        let (_, _, foreign) = scene_with_pending_segment();
        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let mut foreign_program = scene.into_live_program(ReturnSegment(foreign)).unwrap();
        assert!(matches!(
            foreign_program.resume(),
            Err(LiveProgramError::Segment(
                ExecutionSegmentAdvanceError::ForeignSegment { .. }
            ))
        ));
        assert_eq!(foreign_program.status(), LiveProgramStatus::Terminal);

        let (mut scene, session, pending) = scene_with_pending_segment();
        let pending_token = pending.token().unwrap();
        let stale_sequence = crate::execution_segment::ExecutionSegmentSequence::new(
            pending_token.sequence().get().checked_add(1).unwrap(),
        );
        let stale = ExecutionSegment::from_duration(pending.start_time(), pending.duration())
            .unwrap()
            .with_completion_token(crate::execution_segment::ExecutionSegmentToken::new(
                session.runtime_identity(),
                stale_sequence,
            ));
        scene.install_execution(session);
        let mut stale_program = LiveProgram {
            scene,
            continuation: ReturnSegment(stale),
            phase: LiveProgramPhase::ReadyToResume,
            pending_effects: None,
        };
        assert!(matches!(
            stale_program.resume(),
            Err(LiveProgramError::Segment(
                ExecutionSegmentAdvanceError::StaleSegment { .. }
            ))
        ));
        assert_eq!(stale_program.status(), LiveProgramStatus::Terminal);
    }

    impl LiveContinuation for WaitContinuation {
        type Error = Infallible;

        fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            *self.resumes.borrow_mut() += 1;
            if self.stage == 0 {
                self.stage = 1;
                Ok(ContinuationStep::Await(live.wait_segment(1.0).unwrap()))
            } else {
                Ok(ContinuationStep::Finished)
            }
        }
    }

    #[test]
    fn unchanged_wait_resumes_without_inventing_a_renderer_publication() {
        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(WaitContinuation {
                stage: 0,
                resumes: Rc::clone(&resumes),
            })
            .unwrap();
        program.take_renderer_publication();
        program.resume().unwrap();
        let LiveProgramStatus::Awaiting(waiting) = program.status() else {
            panic!("wait continuation must expose its shared deadline")
        };
        assert_eq!(
            waiting.timeline(),
            noon_runtime::TimelineWakeState::Deadline(1.0)
        );
        program
            .drive_to(&mut RustHostCallbackTable::new(), 0.5)
            .unwrap();
        assert_eq!(*resumes.borrow(), 1);
        assert_eq!(
            program
                .drive_to(&mut RustHostCallbackTable::new(), 1.0)
                .unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(*resumes.borrow(), 2);
    }

    #[test]
    fn zero_wait_requests_one_immediate_drive_before_resuming() {
        struct ZeroWaitContinuation {
            stage: usize,
            resumes: Rc<RefCell<usize>>,
        }
        impl LiveContinuation for ZeroWaitContinuation {
            type Error = Infallible;

            fn resume(
                &mut self,
                live: &mut LiveSession<'_>,
            ) -> Result<ContinuationStep, Self::Error> {
                *self.resumes.borrow_mut() += 1;
                if self.stage == 0 {
                    self.stage = 1;
                    Ok(ContinuationStep::Await(live.wait_segment(0.0).unwrap()))
                } else {
                    Ok(ContinuationStep::Finished)
                }
            }
        }

        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(ZeroWaitContinuation {
                stage: 0,
                resumes: Rc::clone(&resumes),
            })
            .unwrap();
        program.take_renderer_publication();
        let LiveProgramStatus::Awaiting(state) = program.resume().unwrap() else {
            panic!("zero wait must still expose its continuation barrier")
        };
        assert!(state.is_complete());
        assert_eq!(
            program.wake_state().timeline(),
            noon_runtime::TimelineWakeState::Deadline(0.0)
        );
        assert_eq!(*resumes.borrow(), 1);
        assert_eq!(
            program
                .drive_to(&mut RustHostCallbackTable::new(), 0.0)
                .unwrap(),
            LiveProgramStatus::ReadyToResume
        );
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(*resumes.borrow(), 2);
    }

    #[test]
    fn callback_failure_is_terminal_and_never_retries_continuation() {
        const CALLBACK: HostCallbackId = HostCallbackId::new(71);
        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object.node_id(), CALLBACK, 0.0, None);
        registration
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(WaitContinuation {
                stage: 0,
                resumes: Rc::clone(&resumes),
            })
            .unwrap();
        program.take_renderer_publication();
        program.resume().unwrap();

        let error = program
            .drive_to(&mut RustHostCallbackTable::new(), 1.0)
            .unwrap_err();
        assert!(matches!(
            error,
            LiveProgramError::Callback(RustHostCallbackError::UnknownCallback { .. })
        ));
        assert_eq!(program.session().frame().time, 0.0);
        assert_eq!(program.status(), LiveProgramStatus::Terminal);
        assert!(matches!(
            program.resume(),
            Err(LiveProgramError::InvalidState { .. })
        ));
        assert_eq!(*resumes.borrow(), 1);
    }

    #[test]
    fn segment_drive_reuses_ordered_callback_loop_before_publication_admission() {
        const CALLBACK: HostCallbackId = HostCallbackId::new(72);
        let mut scene = Scene::new();
        let object = scene.circle(0.4).unwrap();
        scene.add(&object).unwrap();
        let mut registration = SemanticMutationTransaction::new();
        registration.add_updater(object.node_id(), CALLBACK, 0.0, None);
        registration
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let callback_count = Rc::new(RefCell::new(0));
        let mut callbacks = RustHostCallbackTable::new();
        let observed_count = Rc::clone(&callback_count);
        callbacks
            .insert(CALLBACK, move |context| {
                *observed_count.borrow_mut() += 1;
                let mut transform = context.target_state().transform;
                transform.translation.y = 1.0;
                context.set_target_transform(transform)
            })
            .unwrap();
        let resumes = Rc::new(RefCell::new(0));
        let mut program = scene
            .into_live_program(WaitContinuation {
                stage: 0,
                resumes: Rc::clone(&resumes),
            })
            .unwrap();
        program.take_renderer_publication();
        program.resume().unwrap();

        let LiveProgramStatus::PublicationPending(endpoint) =
            program.drive_to(&mut callbacks, 1.0).unwrap()
        else {
            panic!("callback effective write must await its endpoint publication")
        };
        assert!(*callback_count.borrow() > 0);
        assert_eq!(*resumes.borrow(), 1);
        let admitted = program.take_renderer_publication().context();
        assert_eq!(admitted, endpoint);
        program.admit_publication(admitted).unwrap();
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        assert_eq!(*resumes.borrow(), 2);
    }
    #[test]
    fn callback_program_only_exposes_timeline_wake_while_awaiting() {
        let (mut program, mut callbacks) =
            crate::example_scenes::ordinary_callback_continuation_program().unwrap();
        assert_eq!(
            program.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
        program.resume().unwrap();
        assert_eq!(
            program.wake_state().timeline(),
            TimelineWakeState::Continuous
        );
        program.drive_to(&mut callbacks, 1.0).unwrap();
        assert_eq!(
            program.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
        assert_eq!(program.session().frame().time, 1.0);
        let publication = program.session().publication_context();
        drop(program.take_renderer_publication());
        if matches!(program.status(), LiveProgramStatus::PublicationPending(_)) {
            program.admit_publication(publication).unwrap();
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        // Registration remains authoritative and available to the runtime; the
        // finite source must not authorize another segment drive after finish.
        assert_eq!(
            program.session().wake_state().timeline(),
            TimelineWakeState::Continuous
        );
        assert_eq!(
            program.wake_state().timeline(),
            TimelineWakeState::Quiescent
        );
        assert_eq!(program.session().frame().time, 1.0);
    }
}

#[cfg(test)]
mod pointer_input_tests;
