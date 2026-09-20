//! Worker codec entry; browser normalization is shared with direct Rust/WASM.
use super::SemanticExecutionPlayer;
use crate::browser_pointer_input::{submit_browser_pointer_input, BrowserPointerInput};

impl SemanticExecutionPlayer {
    pub(super) fn submit_browser_pointer_input(
        &mut self,
        input: BrowserPointerInput,
    ) -> Result<(), String> {
        submit_browser_pointer_input(
            &mut self.session,
            &mut self.browser_pointer_binding,
            &mut self.next_native_event_sequence,
            input,
        )
    }
}

#[cfg(test)]
use noon_core::Vec2;

#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tests;
