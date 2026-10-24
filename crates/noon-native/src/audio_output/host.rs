//! Device lifetime integration with the existing native presentation clock.

use std::time::Instant;

use crate::{NativeApp, NativeHostError};

use super::{NativeAudioOutput, HEALTH_POLL};

impl NativeApp {
    pub(crate) fn prepare_audio(&mut self) -> Result<(), NativeHostError> {
        if !self.playback_paused && self.audio_output.is_none() && self.audio_remaining() {
            self.audio_output = Some(NativeAudioOutput::prepare(
                self.execution.session(),
                self.viewer_gain(),
            )?);
            self.audio_picture_ready = false;
        }
        Ok(())
    }

    pub(crate) fn audio_remaining(&self) -> bool {
        self.execution
            .session()
            .audio_end_time()
            .is_some_and(|end| self.execution.frame_time() < end)
    }

    pub(crate) fn release_audio(&mut self) {
        if let Some(output) = self.audio_output.take() {
            output.release();
            if self.releasing_audio_output.is_none() {
                self.releasing_audio_output = Some(output);
            }
            // A replacement never admits while the original release exists.
            // Repeated seeks discard only their still-silent prepared stream,
            // retaining the first tail and its original finite deadline.
        }
        self.audio_picture_ready = false;
    }

    pub(crate) fn stop_audio(&mut self) {
        // Lifecycle/failure cancellation is immediate, unlike viewer transport.
        self.audio_output = None;
        self.releasing_audio_output = None;
        self.audio_picture_ready = false;
    }

    pub(crate) fn suspend_audio(&mut self, now: Instant) -> Result<(), NativeHostError> {
        if self.audio_output.is_some() || self.releasing_audio_output.is_some() {
            self.advance_realtime_timeline(now)?;
            self.stop_audio();
            self.realtime_clock = None;
        }
        Ok(())
    }

    pub(crate) fn check_audio(&mut self, now: Instant) -> Result<(), NativeHostError> {
        if let Some(output) = self.releasing_audio_output.as_ref() {
            output.check(now)?;
            if output.released(now) {
                self.releasing_audio_output = None;
            }
        }
        if let Some(output) = self.audio_output.as_ref() {
            output.check(now)?;
            if self.audio_picture_ready
                && !self.playback_paused
                && !self.playback_picture_pending
                && self.releasing_audio_output.is_none()
            {
                output.admit();
            }
            // Preserve the same anchor even if a late host first observes it
            // after the final device block. Only the current stream can anchor
            // Runtime; a releasing stream is disposable output, never a clock.
            if self.realtime_clock.is_none() && !self.playback_paused {
                self.realtime_clock = output.clock();
            }
            if output.drained(now) {
                self.audio_output = None;
            }
        }
        Ok(())
    }

    pub(crate) fn audio_control_flow(
        &self,
        flow: winit::event_loop::ControlFlow,
    ) -> winit::event_loop::ControlFlow {
        use winit::event_loop::ControlFlow;
        if self.audio_output.is_none() && self.releasing_audio_output.is_none() {
            return flow;
        }
        let health_deadline = Instant::now() + HEALTH_POLL;
        match flow {
            ControlFlow::Poll => ControlFlow::Poll,
            ControlFlow::WaitUntil(deadline) => {
                ControlFlow::WaitUntil(deadline.min(health_deadline))
            }
            ControlFlow::Wait => ControlFlow::WaitUntil(health_deadline),
        }
    }
}
