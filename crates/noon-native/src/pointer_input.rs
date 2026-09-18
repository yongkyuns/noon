//! Winit pointer normalization for the existing synchronous session input path.
//!
//! One window's logical OS mouse cursor is projected into the existing unkeyed
//! pointer signals. This is not a raw-device/multitouch collector. Coordinates
//! are captured at delivery against the current effective camera/publication;
//! historical presented-frame picking and asynchronous replay are not promised.
//! There is no private queue, gesture recognizer, target selection or OS grab.
//! Leaving the surface cancels because portable capture is not implemented here.

use noon::integration::{
    NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerPosition,
};
use noon_core::{Camera2DState, NativeInputValue, NativeStateSource, Vec2};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton};
use winit::keyboard::ModifiersState;

use super::{native_pointer_button, NativeApp, NativeHostError};

const WINDOW_CURSOR: NativePointerId = NativePointerId {
    source: 1,
    pointer: 0,
};

/// Only collector lifetime and the last valid OS cursor sample are retained.
/// The session owns sampled button values, event counters and publications.
#[derive(Default)]
pub(super) struct PointerCollector {
    configured: bool,
    view_revision: u64,
    surface: Option<Vec2>,
    pub(super) modifiers: ModifiersState,
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

    fn dispatch_pointer_kind(
        &mut self,
        kind: NativePointerInputKind,
    ) -> Result<(), NativeHostError> {
        let sequence = self.next_input_sequence;
        let next = sequence.checked_add(1).ok_or_else(|| {
            NativeHostError::Platform("native input event sequence exhausted".to_owned())
        })?;
        if !self.ensure_pointer_input()? {
            return Ok(());
        }
        let token = self.execution.native_pointer_input_token()?;
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
        self.execution.submit_native_pointer_input(&token, input)?;
        self.next_input_sequence = next;
        Ok(())
    }

    pub(super) fn dispatch_pointer_position(
        &mut self,
        physical: PhysicalPosition<f64>,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<(), NativeHostError> {
        let logical_size = logical_size(size, scale)?;
        if size.width == 0 || size.height == 0 {
            return self.pointer_left();
        }
        let surface = Vec2::new((physical.x / scale) as f32, (physical.y / scale) as f32);
        let position = pointer_position(surface, logical_size, self.execution.camera()?)?;
        self.dispatch_pointer_kind(NativePointerInputKind::Move(position))?;
        // Never let a rejected occurrence overwrite the position of a later edge.
        if self.pointer.configured {
            self.pointer.surface = Some(surface);
        }
        Ok(())
    }

    pub(super) fn dispatch_pointer_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<(), NativeHostError> {
        let button = native_pointer_button(button).ok_or_else(|| {
            NativeHostError::Platform(
                "native pointer button is outside the u8 input vocabulary".to_owned(),
            )
        })?;
        let logical_size = logical_size(size, scale)?;
        let Some(surface) = self
            .pointer
            .surface
            .filter(|_| size.width > 0 && size.height > 0)
        else {
            // Winit button edges do not include position. After focus/resize or
            // before the first move, acknowledge an explicit cancellation rather
            // than inventing a press/release at the origin or at a stale sample.
            return self.dispatch_pointer_kind(NativePointerInputKind::Cancel(
                NativePointerCancellation::Cancelled,
            ));
        };
        let position = pointer_position(surface, logical_size, self.execution.camera()?)?;
        self.dispatch_pointer_kind(if state == ElementState::Pressed {
            NativePointerInputKind::Press { position, button }
        } else {
            NativePointerInputKind::Release { position, button }
        })
    }

    fn cancel_pointer(&mut self, reason: NativePointerCancellation) -> Result<(), NativeHostError> {
        if self.pointer.configured {
            self.dispatch_pointer_kind(NativePointerInputKind::Cancel(reason))?;
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
    if !logical.x.is_finite() || !logical.y.is_finite() {
        return Err(NativeHostError::Platform(
            "native logical viewport is not representable".to_owned(),
        ));
    }
    Ok(logical)
}

fn pointer_position(
    surface: Vec2,
    logical_size: Vec2,
    camera: Camera2DState,
) -> Result<NativePointerPosition, NativeHostError> {
    if logical_size.x <= 0.0 || logical_size.y <= 0.0 {
        return Err(NativeHostError::Platform(
            "native pointer viewport must be positive".to_owned(),
        ));
    }
    let scene = Vec2::new(
        camera.center.x
            + (surface.x / logical_size.x - 0.5)
                * camera.height
                * (logical_size.x / logical_size.y),
        camera.center.y + (0.5 - surface.y / logical_size.y) * camera.height,
    );
    NativePointerPosition::new(scene, surface)
        .map_err(|error| NativeHostError::Platform(error.to_string()))
}

#[cfg(test)]
mod tests;
