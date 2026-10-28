//! Genuine worker input envelope; the collection receipt is never refreshed here.
#[cfg(test)]
use crate::browser_pointer_input::BrowserPointerInput;
use crate::browser_pointer_input::BrowserPointerTarget;
use noon::integration::{
    NativePointerInputPublication, NativePointerInputToken, PointerFrameSnapshot, PointerFrameView,
};
use noon::ExecutionSession;
use noon_core::{NativePointerId, NativePointerInput, Vec2};

/// Borrowed input adapter for one player-owned semantic store/session pair.
/// It owns no pointer policy, target set, or source state: all occurrences
/// enter the existing session drag policy, whose empty target set preserves
/// ordinary native pointer behavior.
pub(super) struct PlayerPointerTarget<'a> {
    pub(super) session: &'a mut ExecutionSession,
    pub(super) semantics: &'a std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    pub(super) translation_drag_undo: &'a mut Option<noon::TranslationDragUndo>,
}

impl BrowserPointerTarget for PlayerPointerTarget<'_> {
    fn observe_pointer_frame(
        &mut self,
        frame: &noon::integration::PointerFrameSnapshot,
    ) -> Result<(), noon::integration::PointerFrameError> {
        self.session
            .refresh_pointer_hover(frame, frame.view())
            .map(|_| ())
    }
    fn has_native_pointer_subscribers(&self) -> bool {
        self.session.has_native_pointer_subscribers()
    }
    fn translation_drag_active(&self) -> bool {
        self.session.translation_drag_active()
    }
    fn inspection_camera(
        &self,
    ) -> Result<noon_core::Camera2DState, noon::integration::PointerFrameError> {
        self.session.inspection_camera()
    }
    fn capture_pointer_frame(
        &self,
        view: noon::integration::PointerFrameView,
    ) -> Result<noon::integration::PointerFrameSnapshot, noon::integration::PointerFrameError> {
        self.session.capture_pointer_frame(view)
    }
    fn validate_pointer_frame(
        &self,
        frame: &noon::integration::PointerFrameSnapshot,
        view: noon::integration::PointerFrameView,
        validation: crate::browser_pointer_input::PointerFrameValidation,
    ) -> Result<(), noon::integration::PointerFrameError> {
        match validation {
            crate::browser_pointer_input::PointerFrameValidation::Input => {
                frame.validate_current(&*self.session, view)
            }
            crate::browser_pointer_input::PointerFrameValidation::Presentation => {
                frame.validate_presentation(&*self.session, view)
            }
            crate::browser_pointer_input::PointerFrameValidation::CapturedGesture => {
                frame.validate_captured_gesture(&*self.session, view)
            }
        }
    }
    fn pointer_input_token_for_frame(
        &self,
        frame: &noon::integration::PointerFrameSnapshot,
        view: noon::integration::PointerFrameView,
    ) -> Result<NativePointerInputToken, noon::integration::PointerFrameError> {
        frame.input_token(&*self.session, view)
    }

    fn configure_pointer(
        &mut self,
        pointer: NativePointerId,
        view: u64,
    ) -> Result<NativePointerInputToken, String> {
        self.session
            .configure_native_pointer_input(pointer, view)
            .map_err(|error| error.to_string())
    }

    fn pointer_token(&self) -> Result<NativePointerInputToken, String> {
        self.session
            .native_pointer_input_token()
            .map_err(|error| error.to_string())
    }

    fn scroll_inspection(
        &mut self,
        frame: &PointerFrameSnapshot,
        view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, String> {
        self.session
            .scroll_inspection_view(frame, view, surface, delta_pixels)
            .map_err(|error| error.to_string())
    }

    fn submit_pointer(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, String> {
        self.session
            .submit_translation_drag_input(&mut self.semantics.borrow_mut(), token, input)
            .map(|receipt| {
                if let Some(undo) = receipt.undo {
                    *self.translation_drag_undo = Some(undo);
                }
                receipt.input
            })
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
use super::SemanticExecutionPlayer;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tests;

// These existing source/sequence/reactive tests exercise the shared normalizer
// with an explicitly supplied, current test frame. Production worker receipt
// issuance, collection and rejection are tested separately in presentation_tests.
#[cfg(test)]
fn submit_test_frame(
    target: &mut impl crate::browser_pointer_input::BrowserPointerTarget,
    binding: &mut Option<crate::browser_pointer_input::BrowserPointerBinding>,
    sequence: &mut u64,
    input: BrowserPointerInput,
) -> Result<(), String> {
    use crate::browser_pointer_input::{
        submit_browser_pointer_input, submit_presented_browser_pointer_input,
    };
    use noon::integration::PointerFrameView;
    let Some((_, viewport)) = input.coordinates()? else {
        return submit_browser_pointer_input(target, binding, sequence, input).map(|_| ());
    };
    let view = PointerFrameView::new(
        input.view_revision,
        viewport,
        target.inspection_camera().map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let frame = target
        .capture_pointer_frame(view)
        .map_err(|e| e.to_string())?;
    submit_presented_browser_pointer_input(target, binding, sequence, input, &frame)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
impl SemanticExecutionPlayer {
    fn submit_test_frame_input(&mut self, input: BrowserPointerInput) -> Result<(), String> {
        let mut target = PlayerPointerTarget {
            session: &mut self.session,
            semantics: self
                .semantics
                .as_ref()
                .ok_or("execution player lost its semantic store")?,
            translation_drag_undo: &mut self.translation_drag_undo,
        };
        submit_test_frame(
            &mut target,
            &mut self.browser_pointer_binding,
            &mut self.next_native_event_sequence,
            input,
        )
    }
    fn submit_test_frame_json(&mut self, json: &str) -> Result<(), String> {
        self.submit_test_frame_input(serde_json::from_str(json).map_err(|e| e.to_string())?)
    }
}
