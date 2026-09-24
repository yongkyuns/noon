//! Direct canvas receipt lifecycle. Constant-size platform metadata only; no
//! geometry, input queue, clock, or semantic state. A capture is not presentation.
use crate::browser_pointer_input::recoverable_frame_error;
use crate::browser_pointer_input::{
    self, BrowserPointerAdmissionError, BrowserPointerBinding, BrowserPointerInput,
    BrowserPointerTarget,
};
use noon::integration::{PointerFrameError, PointerFrameSnapshot, PointerFrameView};
use noon::ExecutionSession;
use noon_core::{Camera2DState, Vec2};

#[derive(Default)]
pub(crate) struct DirectPointerPresentation {
    revision: u64,
    view: Option<Vec2>,
    pub(crate) presented: Option<PointerFrameSnapshot>,
    pub(crate) refresh_pending: bool,
}

impl DirectPointerPresentation {
    pub(crate) fn set_view(&mut self, revision: u64, viewport: Vec2) -> Result<bool, String> {
        if revision > (1_u64 << 53) - 1
            || !viewport.x.is_finite()
            || !viewport.y.is_finite()
            || viewport.x < 0.0
            || viewport.y < 0.0
        {
            return Err(
                "pointer view must have an exact revision and finite nonnegative dimensions".into(),
            );
        }
        if revision < self.revision {
            return Err("pointer view revision has been retired".into());
        }
        let next = (viewport.x > 0.0 && viewport.y > 0.0).then_some(viewport);
        if self.view == next && self.revision == revision {
            return Ok(false);
        }
        self.revision = revision;
        self.view = next;
        self.invalidate();
        Ok(true)
    }

    pub(crate) fn viewport(&self) -> Option<Vec2> {
        self.view
    }

    pub(crate) fn submit(
        &mut self,
        target: &mut (impl BrowserPointerTarget + ?Sized),
        binding: &mut Option<BrowserPointerBinding>,
        sequence: &mut u64,
        input: BrowserPointerInput,
    ) -> Result<bool, String> {
        // Bad records are not recoverable display races, even before first paint.
        input.needs_binding(*binding)?;
        sequence
            .checked_add(1)
            .ok_or("native input event sequence exhausted")?;
        if input.cancellation().is_some() {
            browser_pointer_input::submit_browser_pointer_input(target, binding, sequence, input)?;
            return Ok(true);
        }
        if let Some(frame) = self.presented.as_ref() {
            match browser_pointer_input::submit_presented_browser_pointer_input(
                target, binding, sequence, input, frame,
            ) {
                Ok(()) => return Ok(true),
                Err(BrowserPointerAdmissionError::Frame(error))
                    if recoverable_frame_error(&error) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        // Separate cancellation, not acknowledgement of the rejected occurrence.
        // The caller retires the DOM contact, so held motion/release cannot resume
        // this gesture after the next frame. There is no automatic replay.
        browser_pointer_input::cancel_browser_pointer_input(target, binding, sequence, true)?;
        self.refresh_pending = true;
        Ok(false)
    }

    /// Publication identity can change without dirty renderer rows (for example,
    /// a signal with no visual dependents). Re-present that identity through the
    /// normal host wake rather than making the next pointer occurrence discover
    /// and repair the stale receipt. Callback barriers are not display races.
    pub(crate) fn needs_refresh(&self, session: &ExecutionSession) -> bool {
        self.refresh_pending
            || self.presented.as_ref().is_some_and(|frame| {
                frame
                    .validate_current(session, frame.view())
                    .is_err_and(|error| recoverable_frame_error(&error))
            })
    }

    pub(crate) fn invalidate(&mut self) {
        self.presented = None;
        self.refresh_pending = true;
    }

    pub(crate) fn capture(
        &self,
        session: &ExecutionSession,
        camera: Camera2DState,
    ) -> Result<Option<PointerFrameSnapshot>, PointerFrameError> {
        let Some(viewport) = self.view else {
            return Ok(None);
        };
        let view = PointerFrameView::new(self.revision, viewport, camera)?;
        match session.capture_pointer_frame(view) {
            Ok(frame) => Ok(Some(frame)),
            // A low-level manual camera override may render, but must not grant
            // positional input against a different effective execution camera.
            Err(PointerFrameError::CameraMismatch) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn did_present(&mut self, frame: Option<PointerFrameSnapshot>) {
        self.presented = frame;
        self.refresh_pending = false;
    }
}

#[cfg(test)]
mod tests;
