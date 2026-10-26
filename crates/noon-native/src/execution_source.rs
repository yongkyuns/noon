use noon::integration::{
    ExecutionViewportQuery, ForwardSample, ForwardSampleStatus, RendererPublication,
    TimelineWakeState,
};
use noon::integration::{
    NativePointerInputPublication, NativePointerInputToken, PointerFrameSnapshot, PointerFrameView,
};
use noon::{
    ExecutionSession, LiveContinuation, LiveProgram, LiveProgramStatus, RustHostCallbackTable,
    SceneExecution,
};
use noon_core::{
    Camera2DState, Inset2DViewState, NativeEventOccurrence, NativeInputValue, NativePointerId,
    NativePointerInput, NativeStateSource, PublicationContext, Rect, Vec2,
};

use crate::NativeHostError;

#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod viewport_tests;

/// The narrow execution surface consumed by the native platform loop.
///
/// Both implementations retain their canonical runtime owner. This trait only
/// lets the common event loop drive time, deliver normalized input, query
/// visibility, and acknowledge a publication after the renderer has retained it.
/// Physical presentation is a separate, potentially coalesced operation.
pub(crate) trait NativeExecutionSource {
    fn frame_time(&self) -> f64;
    fn advance_interactions(&mut self, wall_time_seconds: f64) -> Result<(), NativeHostError>;
    fn retire_execution_source(&mut self) -> Result<(), NativeHostError>;
    fn camera(&self) -> Result<Camera2DState, NativeHostError> {
        self.session().inspection_camera().map_err(Into::into)
    }
    fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, NativeHostError>;

    /// Validate the displayed view as well as the authored publication before
    /// releasing a live continuation. A view-only edit need not change the latter.
    fn admit_presented_frame(
        &mut self,
        frame: &PointerFrameSnapshot,
    ) -> Result<(), NativeHostError> {
        frame.validate_presentation(self.session(), frame.view())?;
        self.refresh_pointer_hover(frame)?;
        self.admit_retained_publication(frame.publication())
    }
    fn refresh_pointer_hover(
        &mut self,
        frame: &PointerFrameSnapshot,
    ) -> Result<(), NativeHostError>;
    fn query_viewports(&mut self, bounds: &[Rect]) -> ExecutionViewportQuery;
    fn inset_2d_views(&self) -> Result<Vec<Inset2DViewState>, NativeHostError>;
    fn timeline(&self) -> TimelineWakeState;
    fn frame_pending(&self) -> bool;
    /// An unfinished source is still playing while an internal publication
    /// barrier temporarily suppresses its timeline wake. This is not user idle.
    fn source_active(&self) -> bool {
        false
    }
    /// A coherent endpoint that the renderer must retain before source resumes.
    fn pending_endpoint(&self) -> Option<PublicationContext> {
        None
    }
    /// Advance through the existing shared runtime/source protocol. Work is
    /// bounded per call; the caller retains its absolute wall-time target when
    /// more source work or a renderer publication is required.
    fn advance_to(&mut self, requested_time: f64) -> Result<(), NativeHostError>;
    /// The engine-admitted finite immutable playback range. Live continuations
    /// keep their source/completion protocol and do not acquire transport control
    /// merely because their current runtime has a finite endpoint.
    fn finite_transport_end(&self) -> Option<f64> {
        None
    }
    /// Apply an explicit viewer seek through the existing shared runtime. The
    /// platform host retains play/pause intent and resets its existing clock only
    /// after this succeeds; this source owns no alternate playhead or schedule.
    fn seek_to(&mut self, _time: f64) -> Result<(), NativeHostError> {
        Err(NativeHostError::PlaybackControlsUnavailable)
    }
    /// Resume one authoring continuation when its shared program is ready.
    /// Source execution latency does not start a new playback epoch.
    fn resume_ready(&mut self) -> Result<bool, NativeHostError>;
    fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, NativeHostError>;
    fn native_pointer_input_token(&self) -> Result<NativePointerInputToken, NativeHostError>;
    fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, NativeHostError>;
    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError>;
    fn emit_native_event(&mut self, event: NativeEventOccurrence) -> Result<(), NativeHostError>;
    fn take_renderer_publication(&mut self) -> RendererPublication<'_>;
    fn admit_retained_publication(
        &mut self,
        publication: PublicationContext,
    ) -> Result<(), NativeHostError>;

    fn session(&self) -> &ExecutionSession;

    #[cfg(test)]
    fn static_session_mut(&mut self) -> Option<&mut ExecutionSession> {
        None
    }

    #[cfg(test)]
    fn static_execution_mut(&mut self) -> Option<&mut SceneExecution> {
        None
    }
}

pub(crate) struct StaticExecutionSource {
    session: SceneExecution,
    callbacks: RustHostCallbackTable,
    finite_transport_end: Option<f64>,
}

impl StaticExecutionSource {
    pub(crate) fn new(session: SceneExecution, callbacks: RustHostCallbackTable) -> Self {
        // This source consumes its complete authored plan. Admitted interaction
        // attachment maintenance preserves its endpoint, so retaining that bound
        // avoids rescanning timeline events at each pause, seek, or pointer occurrence.
        let finite_transport_end = session.session().finite_playback_end_time().filter(|&end| {
            session.session().frame().time.is_finite()
                && session.session().frame().time >= 0.0
                && session.session().frame().time <= end
        });
        Self {
            session,
            callbacks,
            finite_transport_end,
        }
    }
}

impl NativeExecutionSource for StaticExecutionSource {
    fn retire_execution_source(&mut self) -> Result<(), NativeHostError> {
        self.session
            .retire_execution_source()
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn advance_interactions(&mut self, wall_time_seconds: f64) -> Result<(), NativeHostError> {
        self.session
            .advance_interactions(wall_time_seconds)
            .map(|_| ())
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn frame_time(&self) -> f64 {
        self.session.session().frame().time
    }

    fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, NativeHostError> {
        self.session
            .session_mut()
            .scroll_inspection_view(displayed, current_view, surface, delta_pixels)
            .map_err(Into::into)
    }

    fn query_viewports(&mut self, bounds: &[Rect]) -> ExecutionViewportQuery {
        let query = self.session.session_mut().query_viewports(bounds);
        self.session
            .session_mut()
            .renderer_viewport_query(query)
            .expect("fresh viewport query shares this publication")
    }

    fn inset_2d_views(&self) -> Result<Vec<Inset2DViewState>, NativeHostError> {
        self.session
            .session()
            .inset_2d_views()
            .map_err(|error| NativeHostError::Gpu(error.to_string()))
    }

    fn timeline(&self) -> TimelineWakeState {
        self.session.session().wake_state().timeline()
    }

    fn frame_pending(&self) -> bool {
        self.session.session().wake_state().frame_pending()
    }

    fn advance_to(&mut self, requested_time: f64) -> Result<(), NativeHostError> {
        self.callbacks
            .advance_to(self.session.session_mut(), requested_time)
            .map(|_| ())
            .map_err(NativeHostError::from)
    }

    fn finite_transport_end(&self) -> Option<f64> {
        self.session
            .session()
            .supports_immutable_playback()
            .then_some(self.finite_transport_end)
            .flatten()
    }

    fn seek_to(&mut self, time: f64) -> Result<(), NativeHostError> {
        let end = self
            .finite_transport_end()
            .ok_or(NativeHostError::PlaybackControlsUnavailable)?;
        if !time.is_finite() {
            return Err(noon::EvaluationError::InvalidTime(time).into());
        }
        if time < 0.0 || time > end {
            return Err(noon::EvaluationError::ReplayTimeOutsideRange {
                time,
                start: 0.0,
                end,
            }
            .into());
        }
        self.session.seek(time).map(|_| ()).map_err(Into::into)
    }

    fn resume_ready(&mut self) -> Result<bool, NativeHostError> {
        Ok(false)
    }

    fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, NativeHostError> {
        self.session
            .session_mut()
            .configure_native_pointer_input(pointer, view_revision)
            .map_err(Into::into)
    }
    fn native_pointer_input_token(&self) -> Result<NativePointerInputToken, NativeHostError> {
        self.session
            .session()
            .native_pointer_input_token()
            .map_err(Into::into)
    }
    fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, NativeHostError> {
        self.session
            .submit_native_pointer_input(token, input)
            .map_err(Into::into)
    }

    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError> {
        self.session
            .session_mut()
            .set_native_state_input(source, value)
            .map(|_| ())
            .map_err(Into::into)
    }

    fn emit_native_event(&mut self, event: NativeEventOccurrence) -> Result<(), NativeHostError> {
        self.session
            .session_mut()
            .emit_native_event(event)
            .map(|_| ())
            .map_err(Into::into)
    }

    fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.session.session_mut().take_renderer_publication()
    }

    fn admit_retained_publication(
        &mut self,
        _publication: PublicationContext,
    ) -> Result<(), NativeHostError> {
        Ok(())
    }

    fn refresh_pointer_hover(
        &mut self,
        frame: &PointerFrameSnapshot,
    ) -> Result<(), NativeHostError> {
        self.session
            .session_mut()
            .refresh_pointer_hover(frame, frame.view())
            .map(|_| ())
            .map_err(Into::into)
    }

    fn session(&self) -> &ExecutionSession {
        self.session.session()
    }

    #[cfg(test)]
    fn static_session_mut(&mut self) -> Option<&mut ExecutionSession> {
        Some(self.session.session_mut())
    }

    #[cfg(test)]
    fn static_execution_mut(&mut self) -> Option<&mut SceneExecution> {
        Some(&mut self.session)
    }
}

pub(crate) struct LiveProgramExecutionSource<C: LiveContinuation> {
    program: LiveProgram<C>,
    callbacks: RustHostCallbackTable,
}

impl<C> LiveProgramExecutionSource<C>
where
    C: LiveContinuation,
    C::Error: std::fmt::Display,
{
    pub(crate) fn new(
        mut program: LiveProgram<C>,
        callbacks: RustHostCallbackTable,
    ) -> Result<Self, NativeHostError> {
        program
            .resume()
            .map_err(|error| NativeHostError::Program(error.to_string()))?;
        Ok(Self { program, callbacks })
    }
}

impl<C> NativeExecutionSource for LiveProgramExecutionSource<C>
where
    C: LiveContinuation + 'static,
    C::Error: std::fmt::Display,
{
    fn retire_execution_source(&mut self) -> Result<(), NativeHostError> {
        self.program
            .retire_execution_source()
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn advance_interactions(&mut self, wall_time_seconds: f64) -> Result<(), NativeHostError> {
        self.program
            .advance_interactions(wall_time_seconds)
            .map(|_| ())
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn frame_time(&self) -> f64 {
        self.program.session().frame().time
    }

    fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, NativeHostError> {
        self.program
            .scroll_inspection_view(displayed, current_view, surface, delta_pixels)
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn query_viewports(&mut self, bounds: &[Rect]) -> ExecutionViewportQuery {
        let queries = bounds
            .iter()
            .copied()
            .map(|bounds| self.program.query_viewport(bounds))
            .collect::<Vec<_>>();
        self.program
            .session()
            .renderer_viewport_query_union(queries)
            .expect("fresh viewport queries share this publication")
    }

    fn inset_2d_views(&self) -> Result<Vec<Inset2DViewState>, NativeHostError> {
        self.program
            .session()
            .inset_2d_views()
            .map_err(|error| NativeHostError::Gpu(error.to_string()))
    }

    fn timeline(&self) -> TimelineWakeState {
        self.program.wake_state().timeline()
    }

    fn frame_pending(&self) -> bool {
        self.program.session().wake_state().frame_pending()
    }

    fn source_active(&self) -> bool {
        !matches!(
            self.program.status(),
            LiveProgramStatus::Finished | LiveProgramStatus::Terminal
        )
    }

    fn pending_endpoint(&self) -> Option<PublicationContext> {
        match self.program.status() {
            LiveProgramStatus::PublicationPending(context) => Some(context),
            _ => None,
        }
    }

    fn advance_to(&mut self, requested_time: f64) -> Result<(), NativeHostError> {
        // Reuse the same shared forward-sampling protocol as offline Rust
        // output. It alone orders callbacks, clipped endpoints, continuations,
        // and same-time edits. No host-side segment schedule is introduced.
        const MAX_STEPS: u32 = 64;
        let mut sample = ForwardSample::new(
            &mut self.program,
            &mut self.callbacks,
            requested_time,
            MAX_STEPS,
        )
        .map_err(|error| NativeHostError::Program(error.to_string()))?;
        for _ in 0..MAX_STEPS {
            match sample
                .advance()
                .map_err(|error| NativeHostError::Program(error.to_string()))?
            {
                ForwardSampleStatus::Progress => {}
                ForwardSampleStatus::PublicationPending(_)
                | ForwardSampleStatus::Ready(_)
                | ForwardSampleStatus::SourceFinished(_) => return Ok(()),
            }
        }
        // Yield before ForwardSample's transition limit would be exceeded.
        // The canonical program keeps its phase; the next host wake continues
        // toward the SAME epoch's elapsed time rather than discarding lateness.
        Ok(())
    }

    fn resume_ready(&mut self) -> Result<bool, NativeHostError> {
        if self.program.status() == LiveProgramStatus::ReadyToResume {
            self.program
                .resume()
                .map_err(|error| NativeHostError::Program(error.to_string()))?;
            return Ok(true);
        }
        Ok(false)
    }

    fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, NativeHostError> {
        self.program
            .configure_native_pointer_input(pointer, view_revision)
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }
    fn native_pointer_input_token(&self) -> Result<NativePointerInputToken, NativeHostError> {
        self.program
            .native_pointer_input_token()
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }
    fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, NativeHostError> {
        self.program
            .submit_native_pointer_input(token, input)
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError> {
        self.program
            .set_native_state_input(source, value)
            .map(|_| ())
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn emit_native_event(&mut self, event: NativeEventOccurrence) -> Result<(), NativeHostError> {
        self.program
            .emit_native_event(event)
            .map(|_| ())
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.program.take_renderer_publication()
    }

    fn admit_retained_publication(
        &mut self,
        publication: PublicationContext,
    ) -> Result<(), NativeHostError> {
        if let LiveProgramStatus::PublicationPending(expected) = self.program.status() {
            if publication != expected {
                return Err(NativeHostError::Program(format!(
                    "retained publication {publication:?} does not match live endpoint {expected:?}"
                )));
            }
            self.program
                .admit_publication(publication)
                .map_err(|error| NativeHostError::Program(error.to_string()))?;
        }
        Ok(())
    }

    fn refresh_pointer_hover(
        &mut self,
        frame: &PointerFrameSnapshot,
    ) -> Result<(), NativeHostError> {
        self.program
            .refresh_pointer_hover(frame)
            .map(|_| ())
            .map_err(Into::into)
    }

    fn session(&self) -> &ExecutionSession {
        self.program.session()
    }
}
