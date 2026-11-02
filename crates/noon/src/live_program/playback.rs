//! One typed player over bounded preparation and the original observed source.
//! Hosts own clocks, device work and physical receipts; admission stays here.
use super::{
    LiveContinuation, LiveProgram, LiveProgramError, LiveProgramStatus, PreparedSource,
    PreparedSourceError, PreparedSourceStatus,
};
use crate::integration::{
    ExecutionViewportQuery, ForwardSample, ForwardSampleError, ForwardSampleStatus,
    NativePointerInputPublication, NativePointerInputToken, PointerFrameSnapshot, PointerFrameView,
};
use crate::{ExecutionSession, PrefixError, PrefixReader, RustHostCallbackTable};
use noon_core::{
    NativeEventOccurrence, NativeInputValue, NativePointerId, NativePointerInput,
    NativeStateSource, PublicationContext, Rect, Vec2,
};
use noon_runtime::{RendererPublication, ReplayLimits, RuntimeWakeState, TimelineWakeState};

#[derive(Debug)]
pub enum PlaybackProgramError<E> {
    Preparation(PreparedSourceError<E>),
    Prefix(PrefixError),
    Program(LiveProgramError<E>),
    Sample(ForwardSampleError<E>),
    Unavailable,
}
impl<E: std::fmt::Display> std::fmt::Display for PlaybackProgramError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Preparation(e) => e.fmt(f),
            Self::Prefix(e) => e.fmt(f),
            Self::Program(e) => e.fmt(f),
            Self::Sample(e) => e.fmt(f),
            Self::Unavailable => f.write_str("playback presentation is not available"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PlaybackProgramError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Preparation(error) => Some(error),
            Self::Prefix(error) => Some(error),
            Self::Program(error) => Some(error),
            Self::Sample(error) => Some(error),
            Self::Unavailable => None,
        }
    }
}

// Both variants contain the same large canonical program. Boxing just the
// preparing variant adds an allocation while leaving the program-sized enum.
#[allow(clippy::large_enum_variant)]
enum Source<C> {
    Preparing(PreparedSource<C>),
    Observed(LiveProgram<C>),
}

/// Owns one original continuation and, while admissible, its derived reader.
/// Preparation advances at most one source call per `prepare_ready`, with one
/// second of lookahead (one unknown segment may overshoot). No wall clock lives
/// here. Missing data retains the last exact sample; it never samples an epsilon
/// before the frontier. Observed execution uses the ordinary ForwardSample path.
///
/// Handoff is one-way. Once observed execution is needed, earlier cached history
/// is released and seek is unavailable; callbacks/causal input are not replayed.
/// This integration is part of the temporary #2211 qualification gate.
pub struct PlaybackProgram<C> {
    source: Source<C>,
    reader: Option<PrefixReader>,
    buffering: bool,
    reanchor: bool,
}
impl<C: LiveContinuation> LiveProgram<C> {
    pub fn into_playback(mut self) -> Result<PlaybackProgram<C>, PlaybackProgramError<C::Error>> {
        if self.status() == LiveProgramStatus::Terminal {
            return Err(PlaybackProgramError::Program(
                self.invalid_state("start playback"),
            ));
        }
        self.require_effect_resource_preparation();
        // An attached, already-started program has no promised past history.
        // Continue that exact owner instead of rewinding or replaying its body.
        let source = if self.status() != LiveProgramStatus::ReadyToResume
            || self.session().frame().time > 0.0
        {
            Source::Observed(self)
        } else {
            Source::Preparing(
                self.into_prepared_source(ReplayLimits::default())
                    .map_err(PlaybackProgramError::Preparation)?,
            )
        };
        Ok(PlaybackProgram {
            source,
            reader: None,
            buffering: false,
            reanchor: false,
        })
    }
}
impl<C: LiveContinuation> PlaybackProgram<C> {
    /// The active runtime for read-only metadata. Before initial readiness this
    /// is the canonical bootstrap projection, not permission to present it. A
    /// failure before bootstrap can leave no runtime; it cannot be resumed.
    pub fn session(&self) -> Option<&ExecutionSession> {
        if let Some(reader) = &self.reader {
            return Some(reader.session());
        }
        match &self.source {
            Source::Preparing(source) => source.program.as_ref().map(LiveProgram::session),
            Source::Observed(program) => Some(program.session()),
        }
    }
    pub fn presentation_ready(&self) -> bool {
        self.reader.is_some() || matches!(self.source, Source::Observed(_))
    }
    pub fn buffering(&self) -> bool {
        self.buffering
    }
    /// One notification to reanchor the HOST's existing clock after a data stall.
    pub fn take_clock_reanchor(&mut self) -> bool {
        std::mem::take(&mut self.reanchor)
    }
    pub fn end_time(&self) -> Option<f64> {
        self.reader.as_ref().and_then(PrefixReader::end_time)
    }
    pub fn can_seek_to(&self, time: f64) -> bool {
        self.reader
            .as_ref()
            .is_some_and(|reader| reader.contains_time(time))
    }
    pub fn seek(&mut self, time: f64) -> Result<(), PlaybackProgramError<C::Error>> {
        self.reader
            .as_mut()
            .ok_or(PlaybackProgramError::Unavailable)?
            .seek(time)
            .map_err(PlaybackProgramError::Prefix)?;
        self.buffering = false;
        self.reanchor = true;
        Ok(())
    }
    pub fn source_active(&self) -> bool {
        match &self.source {
            Source::Preparing(source) => !matches!(
                source.status(),
                PreparedSourceStatus::Finished
                    | PreparedSourceStatus::Failed
                    | PreparedSourceStatus::Cancelled
            ),
            Source::Observed(program) => !matches!(
                program.status(),
                LiveProgramStatus::Finished | LiveProgramStatus::Terminal
            ),
        }
    }
    pub fn finished(&self) -> bool {
        match &self.source {
            Source::Observed(program) => program.status() == LiveProgramStatus::Finished,
            Source::Preparing(_) => self
                .end_time()
                .is_some_and(|end| self.reader.as_ref().unwrap().frame().time >= end),
        }
    }
    pub fn wake_state(&self) -> Option<RuntimeWakeState> {
        match &self.source {
            Source::Observed(program) => Some(program.wake_state()),
            Source::Preparing(source) => {
                let state = self
                    .reader
                    .as_ref()
                    .map(PrefixReader::wake_state)
                    .or_else(|| self.session().map(ExecutionSession::wake_state))?;
                if (self.reader.is_none() || self.buffering)
                    && source.status() == PreparedSourceStatus::Ready
                {
                    Some(state.with_additional_timeline(TimelineWakeState::Continuous))
                } else {
                    Some(state)
                }
            }
        }
    }
    pub fn pending_endpoint(&self) -> Option<PublicationContext> {
        match &self.source {
            Source::Observed(program) => match program.status() {
                LiveProgramStatus::PublicationPending(context) => Some(context),
                _ => None,
            },
            Source::Preparing(_) => None,
        }
    }
    fn handoff(&mut self) {
        let Source::Preparing(source) = &mut self.source else {
            return;
        };
        let Some(mut program) = source.take_execution_program() else {
            return;
        };
        if let Some(reader) = &self.reader {
            program
                .scene
                .owned_execution_mut()
                .inherit_prepared_inspection(reader.session());
        }
        self.source = Source::Observed(program);
        self.reader = None;
        if self.buffering {
            self.reanchor = true;
        }
        self.buffering = false;
    }
    /// One cooperative source step and ordered borrowed batch admission. At a
    /// late requirement, keep the reader until an actual request reaches its
    /// frontier. No source reconstruction or synthetic callback sample occurs.
    pub fn prepare_ready(
        &mut self,
        callbacks: &mut RustHostCallbackTable,
    ) -> Result<bool, PlaybackProgramError<C::Error>> {
        let target = self
            .reader
            .as_ref()
            .map_or(0.0, |reader| reader.frame().time + 1.0);
        self.prepare_until(callbacks, target)
    }

    /// Absolute loading demand from the host's existing playback clock, not a
    /// new playhead. This keeps preparation ahead of sleeping static intervals.
    pub fn prepare_until(
        &mut self,
        callbacks: &mut RustHostCallbackTable,
        target: f64,
    ) -> Result<bool, PlaybackProgramError<C::Error>> {
        if !target.is_finite() || target < 0.0 {
            return Err(PlaybackProgramError::Preparation(
                PreparedSourceError::InvalidPreparationTarget(target),
            ));
        }
        if let Source::Observed(program) = &mut self.source {
            if program.status() != LiveProgramStatus::ReadyToResume {
                return Ok(false);
            }
            program.resume().map_err(PlaybackProgramError::Program)?;
            Self::settle_frontier(program, callbacks)?;
            return Ok(true);
        }
        let Source::Preparing(source) = &mut self.source else {
            unreachable!()
        };
        match source.status() {
            PreparedSourceStatus::Finished
            | PreparedSourceStatus::Failed
            | PreparedSourceStatus::Cancelled
            | PreparedSourceStatus::Transferred => return Ok(false),
            PreparedSourceStatus::ExecutionRequired(_)
                if self.reader.is_some() && !self.buffering =>
            {
                return Ok(false)
            }
            _ => {}
        }
        let status = if self.buffering {
            Some(
                source
                    .prepare_next()
                    .map_err(PlaybackProgramError::Preparation)?,
            )
        } else {
            source
                .prepare_until(target)
                .map_err(PlaybackProgramError::Preparation)?
        };
        let mut delivered = false;
        if let Some(reader) = source.take_reader() {
            self.reader = Some(reader);
            delivered = true;
        } else if let Some(batch) = source.pending_batch() {
            let reader = self
                .reader
                .as_mut()
                .expect("ordered source batch follows its reader");
            reader
                .reserve_append_capacity(batch)
                .map_err(PlaybackProgramError::Prefix)?;
            reader
                .preflight_append(batch)
                .map_err(PlaybackProgramError::Prefix)?;
            reader
                .append(source.take_batch().expect("borrowed admitted batch"))
                .map_err(PlaybackProgramError::Prefix)?;
            delivered = true;
        }
        if delivered && self.buffering {
            self.buffering = false;
            self.reanchor = true;
        }
        if (self.reader.is_none() || self.buffering)
            && matches!(source.status(), PreparedSourceStatus::ExecutionRequired(_))
        {
            self.handoff();
            if let Source::Observed(program) = &mut self.source {
                // Only an initially required source is still unresumed. A late
                // requirement retains its already-returned pending segment.
                if program.status() == LiveProgramStatus::ReadyToResume {
                    program.resume().map_err(PlaybackProgramError::Program)?;
                }
                Self::settle_frontier(program, callbacks)?;
            }
        }
        Ok(status.is_some())
    }
    fn settle_frontier(
        program: &mut LiveProgram<C>,
        callbacks: &mut RustHostCallbackTable,
    ) -> Result<(), PlaybackProgramError<C::Error>> {
        if matches!(program.status(), LiveProgramStatus::Awaiting(_)) {
            program
                .drive_to(callbacks, program.session().frame().time)
                .map_err(PlaybackProgramError::Program)?;
        }
        Ok(())
    }
    pub fn advance_to(
        &mut self,
        callbacks: &mut RustHostCallbackTable,
        requested: f64,
    ) -> Result<(), PlaybackProgramError<C::Error>> {
        let current = self
            .session()
            .ok_or(PlaybackProgramError::Unavailable)?
            .frame()
            .time;
        if !requested.is_finite() || requested < 0.0 || requested < current {
            return Err(PlaybackProgramError::Sample(
                ForwardSampleError::InvalidTime { requested, current },
            ));
        }
        if let Source::Preparing(source) = &self.source {
            if matches!(source.status(), PreparedSourceStatus::ExecutionRequired(_))
                && self
                    .reader
                    .as_ref()
                    .is_some_and(|reader| requested >= reader.frontier())
            {
                self.handoff();
            }
        }
        if let Source::Observed(program) = &mut self.source {
            const MAX_STEPS: u32 = 64;
            let mut sample = ForwardSample::new(program, callbacks, requested, MAX_STEPS)
                .map_err(PlaybackProgramError::Sample)?;
            for _ in 0..MAX_STEPS {
                if !matches!(
                    sample.advance().map_err(PlaybackProgramError::Sample)?,
                    ForwardSampleStatus::Progress
                ) {
                    break;
                }
            }
            return Ok(());
        }
        let reader = self
            .reader
            .as_mut()
            .ok_or(PlaybackProgramError::Unavailable)?;
        if !reader.contains_time(requested)
            && requested.is_finite()
            && requested >= reader.frontier()
            && reader.end_time().is_none()
        {
            self.buffering = true;
            return Ok(());
        }
        reader
            .advance_to(requested)
            .map_err(PlaybackProgramError::Prefix)?;
        Ok(())
    }
    pub fn pending_effect_programs(&self) -> Option<&[noon_core::WgslEffectProgram]> {
        match &self.source {
            Source::Observed(program) => program.pending_effect_programs(),
            _ => None,
        }
    }
    pub fn publish_prepared_effects(&mut self) -> Result<(), PlaybackProgramError<C::Error>> {
        match &mut self.source {
            Source::Observed(program) => program
                .publish_prepared_effects()
                .map(|_| ())
                .map_err(PlaybackProgramError::Program),
            _ => Err(PlaybackProgramError::Unavailable),
        }
    }
    pub fn admit_publication(
        &mut self,
        publication: PublicationContext,
    ) -> Result<(), PlaybackProgramError<C::Error>> {
        if let Source::Observed(program) = &mut self.source {
            if matches!(program.status(), LiveProgramStatus::PublicationPending(_)) {
                program
                    .admit_publication(publication)
                    .map_err(PlaybackProgramError::Program)?;
            }
        }
        Ok(())
    }
    pub fn take_renderer_publication(&mut self) -> Option<RendererPublication<'_>> {
        if let Some(reader) = &mut self.reader {
            return Some(reader.take_renderer_publication());
        }
        match &mut self.source {
            Source::Observed(program) => Some(program.take_renderer_publication()),
            _ => None,
        }
    }
    pub fn query_viewports(&mut self, bounds: &[Rect]) -> Option<ExecutionViewportQuery> {
        if let Some(reader) = &mut self.reader {
            return Some(reader.query_renderer_viewports(bounds));
        }
        match &mut self.source {
            Source::Observed(program) => {
                let queries: Vec<_> = bounds
                    .iter()
                    .map(|&bounds| program.query_viewport(bounds))
                    .collect();
                Some(
                    program
                        .session()
                        .renderer_viewport_query_union(queries)
                        .expect("fresh queries share publication"),
                )
            }
            _ => None,
        }
    }
    pub fn advance_interactions(
        &mut self,
        wall_time: f64,
    ) -> Result<(), PlaybackProgramError<C::Error>> {
        if let Some(reader) = &mut self.reader {
            return reader
                .advance_interactions(wall_time)
                .map_err(PlaybackProgramError::Prefix);
        }
        match &mut self.source {
            Source::Observed(program) => program
                .advance_interactions(wall_time)
                .map_err(PlaybackProgramError::Program),
            _ => Ok(()),
        }
    }
    pub fn retire_execution_source(&mut self) -> Result<(), PlaybackProgramError<C::Error>> {
        match &mut self.source {
            Source::Observed(program) => program
                .retire_execution_source()
                .map_err(PlaybackProgramError::Program),
            Source::Preparing(source) => {
                source.cancel();
                Ok(())
            }
        }
    }
    pub fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        view: PointerFrameView,
        surface: Vec2,
        delta: f64,
    ) -> Result<bool, PlaybackProgramError<C::Error>> {
        if let Some(reader) = &mut self.reader {
            return reader
                .scroll_inspection_view(displayed, view, surface, delta)
                .map_err(|e| PlaybackProgramError::Program(LiveProgramError::Inspection(e)));
        }
        match &mut self.source {
            Source::Observed(program) => program
                .scroll_inspection_view(displayed, view, surface, delta)
                .map_err(PlaybackProgramError::Program),
            _ => Err(PlaybackProgramError::Unavailable),
        }
    }
    pub fn refresh_pointer_hover(
        &mut self,
        frame: &PointerFrameSnapshot,
    ) -> Result<(), crate::integration::PointerFrameError> {
        if let Some(reader) = &mut self.reader {
            reader.refresh_pointer_hover(frame, frame.view())?;
        } else if let Source::Observed(program) = &mut self.source {
            program.refresh_pointer_hover(frame)?;
        }
        Ok(())
    }
    pub fn configure_native_pointer_input(
        &mut self,
        pointer: NativePointerId,
        view_revision: u64,
    ) -> Result<NativePointerInputToken, PlaybackProgramError<C::Error>> {
        if let Some(reader) = &mut self.reader {
            return reader
                .configure_native_pointer_input(pointer, view_revision)
                .map_err(|e| PlaybackProgramError::Program(LiveProgramError::Input(e)));
        }
        match &mut self.source {
            Source::Observed(program) => program
                .configure_native_pointer_input(pointer, view_revision)
                .map_err(PlaybackProgramError::Program),
            _ => Err(PlaybackProgramError::Unavailable),
        }
    }
    pub fn native_pointer_input_token(
        &self,
    ) -> Result<NativePointerInputToken, PlaybackProgramError<C::Error>> {
        if let Some(reader) = &self.reader {
            return reader
                .native_pointer_input_token()
                .map_err(|e| PlaybackProgramError::Program(LiveProgramError::Input(e)));
        }
        match &self.source {
            Source::Observed(program) => program
                .native_pointer_input_token()
                .map_err(PlaybackProgramError::Program),
            _ => Err(PlaybackProgramError::Unavailable),
        }
    }
    pub fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, PlaybackProgramError<C::Error>> {
        if let Some(reader) = &mut self.reader {
            return reader
                .submit_native_pointer_input(token, input)
                .map_err(|e| PlaybackProgramError::Program(LiveProgramError::Input(e)));
        }
        match &mut self.source {
            Source::Observed(program) => program
                .submit_native_pointer_input(token, input)
                .map_err(PlaybackProgramError::Program),
            _ => Err(PlaybackProgramError::Unavailable),
        }
    }
    pub fn set_native_state_input(
        &mut self,
        source: NativeStateSource,
        value: NativeInputValue,
    ) -> Result<(), PlaybackProgramError<C::Error>> {
        if let Some(reader) = &mut self.reader {
            return reader
                .set_native_state_input(source, value)
                .map_err(|e| PlaybackProgramError::Program(LiveProgramError::Input(e)));
        }
        let program = match &mut self.source {
            Source::Observed(program) => Some(program),
            Source::Preparing(source) => source.program.as_mut(),
        }
        .ok_or(PlaybackProgramError::Unavailable)?;
        program
            .set_native_state_input(source, value)
            .map(|_| ())
            .map_err(PlaybackProgramError::Program)
    }
    pub fn emit_native_event(
        &mut self,
        event: NativeEventOccurrence,
    ) -> Result<(), PlaybackProgramError<C::Error>> {
        if let Some(reader) = &mut self.reader {
            return reader
                .emit_native_event(event)
                .map_err(|e| PlaybackProgramError::Program(LiveProgramError::Input(e)));
        }
        let program = match &mut self.source {
            Source::Observed(program) => Some(program),
            Source::Preparing(source) => source.program.as_mut(),
        }
        .ok_or(PlaybackProgramError::Unavailable)?;
        program
            .emit_native_event(event)
            .map(|_| ())
            .map_err(PlaybackProgramError::Program)
    }
}

#[cfg(test)]
mod tests;
