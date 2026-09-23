//! Inspection input through the existing live owner, never a leased mutable session.

use super::{LiveContinuation, LiveProgram, LiveProgramError};
use crate::integration::{PointerFrameSnapshot, PointerFrameView};
use noon_core::Vec2;

impl<C: LiveContinuation> LiveProgram<C> {
    /// Deliver normalized vertical scrolling against the displayed view.
    ///
    /// Positive logical pixels zoom out. This does not resume a continuation or
    /// advance its authored clock. Button cancellation can publish native state;
    /// in that case the existing endpoint publication fence follows that commit.
    /// The host must also present and acknowledge the new inspection view.
    pub fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("scroll inspection view")?;
        let changed = self.scene.owned_execution_mut()
            .scroll_inspection_view(displayed, current_view, surface, delta_pixels)
            .map_err(LiveProgramError::Inspection)?;
        self.refresh_pending_publication();
        Ok(changed)
    }

    /// Reset the session adjustment without resetting authored execution.
    pub fn reset_inspection_view(&mut self) -> Result<bool, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("reset inspection view")?;
        let changed = self.scene.owned_execution_mut().reset_inspection_view()
            .map_err(LiveProgramError::Inspection)?;
        self.refresh_pending_publication();
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContinuationStep, LiveSession, LiveProgramStatus, Scene};

    struct Finish;
    impl LiveContinuation for Finish {
        type Error = std::convert::Infallible;
        fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
            Ok(ContinuationStep::Finished)
        }
    }

    fn frame(program: &LiveProgram<Finish>) -> PointerFrameSnapshot {
        let session = program.session();
        let view = session.inspection_pointer_view(1, Vec2::new(800.0, 400.0)).unwrap();
        session.capture_pointer_frame(view).unwrap()
    }

    #[test]
    fn finished_program_scrolls_and_resets_without_resuming_or_advancing_time() {
        let mut program = Scene::new().into_live_program(Finish).unwrap();
        program.resume().unwrap();
        program.take_renderer_publication();
        let before = frame(&program);
        let publication = program.session().publication_context();
        assert!(program.scroll_inspection_view(&before, before.view(), Vec2::new(400.0, 200.0), -200.0).unwrap());
        assert_eq!(program.status(), LiveProgramStatus::Finished);
        assert_eq!(program.session().frame().time, 0.0);
        assert_eq!(program.session().publication_context(), publication);
        assert!(program.session().inspection_camera().unwrap().height < 8.0);
        assert!(program.reset_inspection_view().unwrap());
        assert_eq!(program.session().inspection_camera().unwrap().height, 8.0);
        assert_eq!(program.status(), LiveProgramStatus::Finished);
        assert!(before.validate_presentation(program.session(), before.view()).is_err());
    }

    #[test]
    fn terminal_owner_rejects_inspection_even_when_the_frame_is_current() {
        let mut program = Scene::new().into_live_program(Finish).unwrap();
        let before = frame(&program);
        program.phase = super::super::LiveProgramPhase::Terminal;
        assert!(matches!(program.scroll_inspection_view(&before, before.view(), Vec2::ZERO, 0.0), Err(LiveProgramError::InvalidState { .. })));
        assert!(matches!(program.reset_inspection_view(), Err(LiveProgramError::InvalidState { .. })));
        assert_eq!(program.session().inspection_view_revision(), 0);
    }

    #[test]
    fn presentation_validation_does_not_grant_input_through_a_callback_barrier() {
        let mut scene = Scene::new();
        let circle = scene.circle(0.5).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        let view = session.inspection_pointer_view(1, Vec2::new(800.0, 400.0)).unwrap();
        let before = session.capture_pointer_frame(view).unwrap();
        let phase = session.begin_required_callback_phase(0.0, [circle.node_id()]).unwrap();
        assert!(before.validate_presentation(&session, view).is_ok());
        assert!(before.validate_current(&session, view).is_err());
        assert!(session.scroll_inspection_view(&before, view, Vec2::ZERO, 1.0).is_err());
        assert_eq!(session.inspection_view_revision(), 0);
        session.commit_required_callback_phase(phase.finish()).unwrap();
    }
}
