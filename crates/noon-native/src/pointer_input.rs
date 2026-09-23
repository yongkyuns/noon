//! Winit pointer collection against the last successfully presented frame.
//!
//! One window's logical cursor feeds the shared typed session input boundary.
//! Positional input never substitutes current execution for an older displayed
//! frame. Stale input cancels the contact and waits for a fresh presentation and
//! cursor sample; cancellation itself does not depend on a positional receipt.
//! The collector owns no geometry, picking index, gesture recognizer or OS grab.

use noon::integration::{
    NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerInputToken, PointerFrameError, PointerFrameSnapshot,
    PointerFrameView,
};
use noon_core::{NativeInputValue, NativeStateSource, Vec2};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton};
use winit::keyboard::ModifiersState;

use super::{native_pointer_button, NativeApp, NativeHostError};

const WINDOW_CURSOR: NativePointerId = NativePointerId {
    source: 1,
    pointer: 0,
};

/// Collector lifetime, the last valid OS cursor sample and one presentation
/// receipt are retained. A pending render is never installed as a receipt.
/// The session owns sampled button values, event counters and publications.
#[derive(Default)]
pub(super) struct PointerCollector {
    configured: bool,
    view_revision: u64,
    surface: Option<Vec2>,
    pub(super) modifiers: ModifiersState,
    pub(super) presented: Option<PointerFrameSnapshot>,
    pub(super) refresh_pending: bool,
}

/// Explicit admission outcome for the platform shell. Stale input is recoverable,
/// unlike malformed input or a failed session/callback transaction.
#[derive(Debug, PartialEq)]
pub(super) enum PointerDispatch {
    Admitted,
    Cancelled,
    Unsubscribed,
    AwaitingPresentation,
    RejectedFrame(PointerFrameError),
}

impl NativeApp {
    fn ensure_pointer_input(&mut self) -> Result<bool, NativeHostError> {
        if !self.pointer.configured {
            // Do not activate an unused input domain during window bootstrap or
            // incidental focus/cursor events in callback-only scenes. Explicit
            // typed session callers do not use this collector-interest filter.
            if !self.execution.session().has_native_pointer_subscribers() {
                return Ok(false);
            }
            self.execution
                .configure_native_pointer_input(WINDOW_CURSOR, self.pointer.view_revision)?;
            self.pointer.configured = true;
        }
        Ok(true)
    }

    fn admit_pointer_kind(
        &mut self,
        token: &NativePointerInputToken,
        kind: NativePointerInputKind,
    ) -> Result<(), NativeHostError> {
        let sequence = self.next_input_sequence;
        let next = sequence.checked_add(1).ok_or_else(|| {
            NativeHostError::Platform("native input event sequence exhausted".to_owned())
        })?;
        let modifiers = self.pointer.modifiers;
        let input = NativePointerInput::new(
            sequence,
            token.pointer(),
            token.context(),
            NativeInputModifiers {
                shift: modifiers.shift_key(),
                control: modifiers.control_key(),
                alt: modifiers.alt_key(),
                meta: modifiers.super_key(),
            },
            kind,
        );
        self.execution.submit_native_pointer_input(token, input)?;
        self.next_input_sequence = next;
        Ok(())
    }

    pub(super) fn capture_pointer_presentation(
        &self,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<PointerFrameSnapshot, NativeHostError> {
        let view = self.pointer_frame_view(logical_size(size, scale)?)?;
        self.execution
            .session()
            .capture_pointer_frame(view)
            .map_err(|error| NativeHostError::Platform(error.to_string()))
    }

    fn pointer_frame_view(&self, logical: Vec2) -> Result<PointerFrameView, NativeHostError> {
        PointerFrameView::new(
            self.pointer.view_revision,
            logical,
            self.execution.camera()?,
        )
        .map_err(|error| NativeHostError::Platform(error.to_string()))
    }

    fn reject_pointer_frame(
        &mut self,
        error: PointerFrameError,
    ) -> Result<PointerDispatch, NativeHostError> {
        use noon::ExecutionSessionInputError;
        match error {
            PointerFrameError::ViewChanged
            | PointerFrameError::CameraMismatch
            | PointerFrameError::Input(
                ExecutionSessionInputError::ForeignPointerRuntime
                | ExecutionSessionInputError::StalePointerPublication { .. },
            ) => {
                // This is a new cancellation occurrence, never acknowledgement or
                // replay of the rejected position/edge. A failed cancellation is
                // still an error; callback and terminal barriers are not bypassed.
                self.cancel_pointer(NativePointerCancellation::Cancelled)?;
                self.pointer.refresh_pending = true;
                Ok(PointerDispatch::RejectedFrame(error))
            }
            PointerFrameError::Input(error) => Err(error.into()),
            PointerFrameError::Camera(error) => Err(error.into()),
            error => Err(NativeHostError::Platform(error.to_string())),
        }
    }

    fn dispatch_presented_pointer(
        &mut self,
        surface: Vec2,
        logical: Vec2,
        kind: impl FnOnce(noon::integration::NativePointerPosition) -> NativePointerInputKind,
    ) -> Result<PointerDispatch, NativeHostError> {
        if !self.execution.session().has_native_pointer_subscribers() {
            return Ok(PointerDispatch::Unsubscribed);
        }
        let Some(frame) = self.pointer.presented.clone() else {
            self.cancel_pointer(NativePointerCancellation::Cancelled)?;
            self.pointer.refresh_pending = true;
            return Ok(PointerDispatch::AwaitingPresentation);
        };
        let view = self.pointer_frame_view(logical)?;
        // Validate before configuration: resetting a binding can mutate signals.
        if let Err(error) = frame.validate_current(self.execution.session(), view) {
            return self.reject_pointer_frame(error);
        }
        let position = frame
            .position(surface)
            .map_err(|error| NativeHostError::Platform(error.to_string()))?;
        if self.next_input_sequence == u64::MAX {
            return Err(NativeHostError::Platform(
                "native input event sequence exhausted".to_owned(),
            ));
        }
        if !self.ensure_pointer_input()? {
            return Ok(PointerDispatch::Unsubscribed);
        }
        let token = match frame.input_token(self.execution.session(), view) {
            Ok(token) => token,
            Err(error) => return self.reject_pointer_frame(error),
        };
        self.admit_pointer_kind(&token, kind(position))?;
        Ok(PointerDispatch::Admitted)
    }

    pub(super) fn dispatch_pointer_position(
        &mut self,
        physical: PhysicalPosition<f64>,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<PointerDispatch, NativeHostError> {
        let logical = logical_size(size, scale)?;
        if size.width == 0 || size.height == 0 {
            self.pointer_left()?;
            return Ok(PointerDispatch::Cancelled);
        }
        let surface = Vec2::new((physical.x / scale) as f32, (physical.y / scale) as f32);
        if !surface.x.is_finite() || !surface.y.is_finite() {
            return Err(NativeHostError::Platform(
                "native pointer coordinates must be finite".to_owned(),
            ));
        }
        let outcome =
            self.dispatch_presented_pointer(surface, logical, NativePointerInputKind::Move)?;
        if outcome == PointerDispatch::Admitted {
            // Rejected samples must not supply coordinates to a later edge.
            self.pointer.surface = Some(surface);
        }
        Ok(outcome)
    }

    pub(super) fn dispatch_pointer_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<PointerDispatch, NativeHostError> {
        let button = native_pointer_button(button).ok_or_else(|| {
            NativeHostError::Platform(
                "native pointer button is outside the u8 input vocabulary".to_owned(),
            )
        })?;
        let logical = logical_size(size, scale)?;
        let Some(surface) = self
            .pointer
            .surface
            .filter(|_| size.width > 0 && size.height > 0)
        else {
            // Winit edges carry no position. Never manufacture one from the origin
            // or from a sample retired by focus, resize or rejected frame input.
            self.cancel_pointer(NativePointerCancellation::Cancelled)?;
            return Ok(PointerDispatch::Cancelled);
        };
        self.dispatch_presented_pointer(surface, logical, |position| {
            if state == ElementState::Pressed {
                NativePointerInputKind::Press { position, button }
            } else {
                NativePointerInputKind::Release { position, button }
            }
        })
    }

    fn cancel_pointer(&mut self, reason: NativePointerCancellation) -> Result<(), NativeHostError> {
        if self.pointer.configured {
            let token = self.execution.native_pointer_input_token()?;
            self.admit_pointer_kind(&token, NativePointerInputKind::Cancel(reason))?;
        }
        self.pointer.surface = None;
        Ok(())
    }

    pub(super) fn pointer_left(&mut self) -> Result<(), NativeHostError> {
        self.cancel_pointer(NativePointerCancellation::Cancelled)
    }

    pub(super) fn pointer_focus_lost(&mut self) -> Result<(), NativeHostError> {
        self.cancel_pointer(NativePointerCancellation::FocusLost)?;
        self.pointer.modifiers = ModifiersState::empty();
        Ok(())
    }

    pub(super) fn rebind_pointer_view(&mut self) -> Result<(), NativeHostError> {
        let next = self.pointer.view_revision.checked_add(1).ok_or_else(|| {
            NativeHostError::Platform("native pointer view revision exhausted".to_owned())
        })?;
        if self.pointer.configured {
            self.execution
                .configure_native_pointer_input(WINDOW_CURSOR, next)?;
        }
        // Only successful session rebinding invalidates the local coordinate cache.
        self.pointer.view_revision = next;
        self.pointer.surface = None;
        self.pointer.presented = None;
        // A size/scale/surface change needs a new presentation even when no
        // authored object or native viewport signal became dirty.
        self.pointer.refresh_pending = true;
        Ok(())
    }

    pub(super) fn pointer_scale_changed(
        &mut self,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<(), NativeHostError> {
        let logical = logical_size(size, scale)?;
        self.rebind_pointer_view()?;
        self.dispatch_state(
            NativeStateSource::ViewportSize,
            NativeInputValue::Vec2(logical),
        )
    }
}

fn logical_size(size: PhysicalSize<u32>, scale: f64) -> Result<Vec2, NativeHostError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(NativeHostError::Platform(
            "native pointer scale must be finite and positive".to_owned(),
        ));
    }
    let logical = Vec2::new(
        (f64::from(size.width) / scale) as f32,
        (f64::from(size.height) / scale) as f32,
    );
    if !logical.x.is_finite()
        || !logical.y.is_finite()
        || (size.width > 0 && logical.x <= 0.0)
        || (size.height > 0 && logical.y <= 0.0)
    {
        return Err(NativeHostError::Platform(
            "native logical viewport is not representable".to_owned(),
        ));
    }
    Ok(logical)
}

#[cfg(test)]
mod tests;
