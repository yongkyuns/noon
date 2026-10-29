//! The experimental historical reader uses the ordinary browser receipt and
//! normalization path. Its restricted API never exposes the owned session.
use super::*;
use noon::PrefixReader;

impl BrowserPointerTarget for PrefixReader {
    fn has_native_pointer_subscribers(&self) -> bool {
        PrefixReader::has_native_pointer_subscribers(self)
    }
    fn translation_drag_active(&self) -> bool {
        // Causal mutation/dragging has not been admitted for prepared history.
        false
    }
    fn inspection_camera(&self) -> Result<noon_core::Camera2DState, PointerFrameError> {
        PrefixReader::inspection_camera(self)
    }
    fn capture_pointer_frame(
        &self,
        view: PointerFrameView,
    ) -> Result<PointerFrameSnapshot, PointerFrameError> {
        PrefixReader::capture_pointer_frame(self, view)
    }
    fn validate_pointer_frame(
        &self,
        frame: &PointerFrameSnapshot,
        view: PointerFrameView,
        validation: PointerFrameValidation,
    ) -> Result<(), PointerFrameError> {
        match validation {
            PointerFrameValidation::Input => {
                PrefixReader::validate_pointer_frame(self, frame, view)
            }
            PointerFrameValidation::Presentation => {
                PrefixReader::validate_pointer_presentation(self, frame, view)
            }
            PointerFrameValidation::CapturedGesture => Err(PointerFrameError::Input(
                ExecutionSessionInputError::Interaction(
                    "prepared playback does not admit causal captured gestures".into(),
                ),
            )),
        }
    }
    fn pointer_input_token_for_frame(
        &self,
        frame: &PointerFrameSnapshot,
        view: PointerFrameView,
    ) -> Result<NativePointerInputToken, PointerFrameError> {
        PrefixReader::pointer_input_token_for_frame(self, frame, view)
    }
    fn observe_pointer_frame(
        &mut self,
        frame: &PointerFrameSnapshot,
    ) -> Result<(), PointerFrameError> {
        self.refresh_pointer_hover(frame, frame.view()).map(|_| ())
    }
    fn configure_pointer(
        &mut self,
        pointer: NativePointerId,
        view: u64,
    ) -> Result<NativePointerInputToken, String> {
        self.configure_native_pointer_input(pointer, view)
            .map_err(|e| e.to_string())
    }
    fn pointer_token(&self) -> Result<NativePointerInputToken, String> {
        self.native_pointer_input_token().map_err(|e| e.to_string())
    }
    fn scroll_inspection(
        &mut self,
        frame: &PointerFrameSnapshot,
        view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, String> {
        self.scroll_inspection_view(frame, view, surface, delta_pixels)
            .map_err(|e| e.to_string())
    }
    fn submit_pointer(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, String> {
        self.submit_native_pointer_input(token, input)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
