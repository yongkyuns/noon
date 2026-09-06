use noon::{
    ExecutionSession, ExecutionViewportQuery, LiveContinuation, LiveProgram, LiveProgramStatus,
    RendererPublication, RustHostCallbackTable, TimelineWakeState,
};
use noon_core::{
    Camera2DState, NativeEventOccurrence, NativeInputValue, NativeStateSource, PublicationContext,
    Rect,
};

use crate::NativeHostError;

/// The narrow execution surface consumed by the native platform loop.
///
/// Both implementations retain their canonical runtime owner. This trait only
/// lets the common event loop drive time, deliver normalized input, query
/// visibility, and acknowledge a publication after successful presentation.
pub(crate) trait NativeExecutionSource {
    fn frame_time(&self) -> f64;
    fn camera(&self) -> Result<Camera2DState, NativeHostError>;
    fn query_viewport(&mut self, bounds: Rect) -> ExecutionViewportQuery;
    fn timeline(&self) -> TimelineWakeState;
    fn frame_pending(&self) -> bool;
    /// Advance canonical execution and report whether this call committed at
    /// least one opaque host callback phase.
    ///
    /// The bit is platform timing metadata only. The execution session remains
    /// the sole callback schedule and authored-time authority.
    fn advance_to(&mut self, requested_time: f64) -> Result<bool, NativeHostError>;
    /// Resume one authoring continuation when its shared program is ready.
    ///
    /// The return value lets the platform clock reanchor only when application
    /// code actually supplied the next segment.
    fn resume_ready(&mut self) -> Result<bool, NativeHostError>;
    fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), NativeHostError>;
    fn emit_native_event(&mut self, event: NativeEventOccurrence) -> Result<(), NativeHostError>;
    fn take_renderer_publication(&mut self) -> RendererPublication<'_>;
    fn admit_presented_publication(
        &mut self,
        publication: PublicationContext,
    ) -> Result<(), NativeHostError>;

    #[cfg(test)]
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
    fn frame_time(&self) -> f64 {
        self.session.frame().time
    }

    fn camera(&self) -> Result<Camera2DState, NativeHostError> {
        self.session.camera().map_err(Into::into)
    }

    fn query_viewport(&mut self, bounds: Rect) -> ExecutionViewportQuery {
        self.session.query_viewport(bounds)
    }

    fn timeline(&self) -> TimelineWakeState {
        self.session.wake_state().timeline()
    }

    fn frame_pending(&self) -> bool {
        self.session.wake_state().frame_pending()
    }

    fn advance_to(&mut self, requested_time: f64) -> Result<bool, NativeHostError> {
        self.callbacks
            .advance_to(&mut self.session, requested_time)
            .map_err(NativeHostError::from)?;
        Ok(self.callbacks.last_advance_completed_callback_phase())
    }

    fn resume_ready(&mut self) -> Result<bool, NativeHostError> {
        Ok(false)
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

    fn admit_presented_publication(
        &mut self,
        _publication: PublicationContext,
    ) -> Result<(), NativeHostError> {
        Ok(())
    }

    #[cfg(test)]
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
    fn frame_time(&self) -> f64 {
        self.program.session().frame().time
    }

    fn camera(&self) -> Result<Camera2DState, NativeHostError> {
        self.program.session().camera().map_err(Into::into)
    }

    fn query_viewport(&mut self, bounds: Rect) -> ExecutionViewportQuery {
        self.program.query_viewport(bounds)
    }

    fn timeline(&self) -> TimelineWakeState {
        self.program.wake_state().timeline()
    }

    fn frame_pending(&self) -> bool {
        self.program.session().wake_state().frame_pending()
    }

    fn advance_to(&mut self, requested_time: f64) -> Result<bool, NativeHostError> {
        self.program
            .drive_to(&mut self.callbacks, requested_time)
            .map_err(|error| NativeHostError::Program(error.to_string()))?;
        Ok(self.callbacks.last_advance_completed_callback_phase())
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

    fn admit_presented_publication(
        &mut self,
        publication: PublicationContext,
    ) -> Result<(), NativeHostError> {
        if let LiveProgramStatus::PublicationPending(expected) = self.program.status() {
            if publication != expected {
                return Err(NativeHostError::Program(format!(
                    "presented publication {publication:?} does not match live endpoint {expected:?}"
                )));
            }
            self.program
                .admit_publication(publication)
                .map_err(|error| NativeHostError::Program(error.to_string()))?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn session(&self) -> &ExecutionSession {
        self.program.session()
    }
}
