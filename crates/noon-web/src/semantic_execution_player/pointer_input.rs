//! Genuine worker input envelope; the collection receipt is never refreshed here.
use crate::browser_pointer_input::{BrowserPointerInput, BrowserPointerTarget};
use crate::worker_pointer_presentation::WorkerPointerReceipt;
use noon::integration::{
    NativePointerInputPublication, NativePointerInputToken, PointerFrameSnapshot, PointerFrameView,
};
use noon::ExecutionSession;
use noon_core::{NativePointerId, NativePointerInput, Vec2};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkerPointerInput {
    pub input: BrowserPointerInput,
    pub presentation: Option<WorkerPointerReceipt>,
}

/// Borrowed input adapter for one player-owned semantic store/session pair.
/// It owns no pointer policy, target set, or source state: all occurrences
/// enter the existing session drag policy, whose empty target set preserves
/// ordinary native pointer behavior.
pub(super) struct PlayerPointerTarget<'a> {
    pub(super) session: &'a mut ExecutionSession,
    pub(super) semantics: Option<std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>>,
    pub(super) root: Option<noon_core::SemanticNodeId>,
}

impl BrowserPointerTarget for PlayerPointerTarget<'_> {
    fn session(&self) -> &ExecutionSession {
        self.session
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
        let Some(semantics) = self.semantics.as_ref() else {
            return self
                .session
                .submit_native_pointer_input(token, input)
                .map_err(|error| error.to_string());
        };
        let root = self
            .root
            .ok_or("live semantic pointer input is missing its scene root")?;
        noon::LiveSession::new(semantics, root, self.session)
            .submit_translation_drag_input(token, input)
            .map(|receipt| receipt.input)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
use super::SemanticExecutionPlayer;
#[cfg(test)]
use noon_core::Vec2;
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
        return submit_browser_pointer_input(target, binding, sequence, input);
    };
    let view = PointerFrameView::new(
        input.view_revision,
        viewport,
        target.session().camera().map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let frame = target
        .session()
        .capture_pointer_frame(view)
        .map_err(|e| e.to_string())?;
    submit_presented_browser_pointer_input(target, binding, sequence, input, &frame)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
impl SemanticExecutionPlayer {
    fn submit_test_frame_input(&mut self, input: BrowserPointerInput) -> Result<(), String> {
        submit_test_frame(
            &mut self.session,
            &mut self.browser_pointer_binding,
            &mut self.next_native_event_sequence,
            input,
        )
    }
    fn submit_test_frame_json(&mut self, json: &str) -> Result<(), String> {
        self.submit_test_frame_input(serde_json::from_str(json).map_err(|e| e.to_string())?)
    }
}
