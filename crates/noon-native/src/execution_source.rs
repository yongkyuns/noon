use noon::integration::{
    ExecutionViewportQuery, ForwardSample, ForwardSampleStatus, RendererPublication, TimelineWakeState,
};
use noon::integration::{NativePointerInputToken, PointerFrameSnapshot, PointerFrameView};
use noon::{
    ExecutionSession, LiveContinuation, LiveProgram, LiveProgramStatus, RustHostCallbackTable,
};
use noon_core::{
    Camera2DState, Inset2DViewState, NativeEventOccurrence, NativeInputValue, NativePointerId,
    NativePointerInput, NativeStateSource, PublicationContext, Rect, Vec2,
};

use crate::NativeHostError;

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
    ) -> Result<(), NativeHostError>;
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
}

pub(crate) struct StaticExecutionSource {
    session: ExecutionSession,
    callbacks: RustHostCallbackTable,
}

impl StaticExecutionSource {
    pub(crate) const fn new(session: ExecutionSession, callbacks: RustHostCallbackTable) -> Self {
        Self { session, callbacks }
    }
}

impl NativeExecutionSource for StaticExecutionSource {
    fn advance_interactions(&mut self, wall_time_seconds: f64) -> Result<(), NativeHostError> {
        self.session
            .advance_interactions(wall_time_seconds)
            .map(|_| ())
            .map_err(|error| NativeHostError::Program(error.to_string()))
    }

    fn frame_time(&self) -> f64 {
        self.session.frame().time
    }

    fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, NativeHostError> {
        self.session
            .scroll_inspection_view(displayed, current_view, surface, delta_pixels)
            .map_err(Into::into)
    }

    fn query_viewports(&mut self, bounds: &[Rect]) -> ExecutionViewportQuery {
        let query = self.session.query_viewports(bounds);
        self.session
            .renderer_viewport_query(query)
            .expect("fresh viewport query shares this publication")
    }

    fn inset_2d_views(&self) -> Result<Vec<Inset2DViewState>, NativeHostError> {
        self.session
            .inset_2d_views()
            .map_err(|error| NativeHostError::Gpu(error.to_string()))
    }

    fn timeline(&self) -> TimelineWakeState {
        self.session.wake_state().timeline()
    }

    fn frame_pending(&self) -> bool {
        self.session.wake_state().frame_pending()
    }

    fn advance_to(&mut self, requested_time: f64) -> Result<(), NativeHostError> {
        self.callbacks
            .advance_to(&mut self.session, requested_time)
            .map(|_| ())
            .map_err(NativeHostError::from)
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
            .configure_native_pointer_input(pointer, view_revision)
            .map_err(Into::into)
    }
    fn native_pointer_input_token(&self) -> Result<NativePointerInputToken, NativeHostError> {
        self.session
            .native_pointer_input_token()
            .map_err(Into::into)
    }
    fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<(), NativeHostError> {
        self.session
            .submit_native_pointer_input(token, input)
            .map(|_| ())
            .map_err(Into::into)
    }

    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError> {
        self.session
            .set_native_state_input(source, value)
            .map(|_| ())
            .map_err(Into::into)
    }

    fn emit_native_event(&mut self, event: NativeEventOccurrence) -> Result<(), NativeHostError> {
        self.session
            .emit_native_event(event)
            .map(|_| ())
            .map_err(Into::into)
    }

    fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        self.session.take_renderer_publication()
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
            .refresh_pointer_hover(frame, frame.view())
            .map(|_| ())
            .map_err(Into::into)
    }

    fn session(&self) -> &ExecutionSession {
        &self.session
    }

    #[cfg(test)]
    fn static_session_mut(&mut self) -> Option<&mut ExecutionSession> {
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
    ) -> Result<(), NativeHostError> {
        self.program
            .submit_native_pointer_input(token, input)
            .map(|_| ())
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
