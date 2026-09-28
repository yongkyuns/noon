//! Native wheel collection. View policy and composition remain in shared Rust.

use super::{logical_size, NativeApp, NativeHostError, PointerDispatch};
use winit::dpi::PhysicalSize;
use winit::event::MouseScrollDelta;

// Winit line deltas have no OS-provided pixel extent. Use the same logical-pixel
// normalization at all device scales; pixel deltas are physical and are divided
// by the window scale. Positive winit Y scrolls up, unlike DOM deltaY.
const LOGICAL_PIXELS_PER_LINE: f64 = 40.0;

fn vertical_pixels(delta: MouseScrollDelta, scale: f64) -> Result<f64, NativeHostError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(NativeHostError::Platform(
            "invalid wheel device scale".to_owned(),
        ));
    }
    let pixels = match delta {
        MouseScrollDelta::LineDelta(_, y) => -f64::from(y) * LOGICAL_PIXELS_PER_LINE,
        MouseScrollDelta::PixelDelta(point) => -point.y / scale,
    };
    if !pixels.is_finite() {
        return Err(NativeHostError::Platform(
            "wheel delta must be finite".to_owned(),
        ));
    }
    Ok(pixels)
}

impl NativeApp {
    pub(crate) fn dispatch_inspection_scroll(
        &mut self,
        delta: MouseScrollDelta,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<PointerDispatch, NativeHostError> {
        if !self.config.inspection_zoom {
            return Ok(PointerDispatch::Unsubscribed);
        }
        let pixels = vertical_pixels(delta, scale)?;
        if pixels == 0.0 {
            return Ok(PointerDispatch::Unsubscribed);
        }
        let logical = logical_size(size, scale)?;
        if size.width == 0 || size.height == 0 {
            self.pointer_left()?;
            return Ok(PointerDispatch::Cancelled);
        }
        // Wheel occurrences carry no cursor location in winit. A valid OS sample
        // is required; focus/leave/resize/stale-contact cancellation clears it.
        let Some(surface) = self.pointer.surface else {
            return Ok(PointerDispatch::Cancelled);
        };
        let Some(displayed) = self.pointer.presented.clone() else {
            // No accumulation or retagging: this occurrence cannot be associated
            // with an image. Keep the cursor pixel for a later fresh occurrence.
            self.pointer.refresh_pending = true;
            return Ok(PointerDispatch::AwaitingPresentation);
        };
        let view = self.pointer_frame_view(logical)?;
        if let Err(error) = displayed.validate_current(self.execution.session(), view) {
            return self.reject_pointer_frame(error);
        }
        if self
            .execution
            .scroll_inspection_view(&displayed, view, surface, pixels)?
        {
            // Shared navigation has atomically cancelled the gesture and retired
            // its binding. Everything below is infallible platform bookkeeping.
            self.pointer.configured = false;
            self.pointer.presented = None;
            self.pointer.refresh_pending = true;
        }
        Ok(PointerDispatch::Admitted)
    }
}

#[cfg(test)]
mod tests;
