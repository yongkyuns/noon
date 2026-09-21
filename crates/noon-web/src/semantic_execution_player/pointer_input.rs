//! Genuine worker input envelope; the collection receipt is never refreshed here.
use crate::browser_pointer_input::BrowserPointerInput;
use crate::worker_pointer_presentation::WorkerPointerReceipt;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkerPointerInput {
    pub input: BrowserPointerInput,
    pub presentation: Option<WorkerPointerReceipt>,
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
