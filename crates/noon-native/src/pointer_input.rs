//! Winit pointer collection against the last successfully presented frame.
//!
//! One window's logical cursor feeds the shared typed session input boundary.
//! Positional input retains its collected view and publication. Shared Rust may
//! prove that later effective frames preserve picking at that exact point.
//! Incompatible input cancels the contact and waits for a fresh presentation and
//! cursor sample; cancellation itself does not depend on a positional receipt.
//! The collector owns no geometry, picking index, gesture recognizer or OS grab.

use noon::integration::{
    NativeInputModifiers, NativePointerCancellation, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerInputToken, PointerFrameError, PointerFrameSnapshot,
    PointerFrameView,
};
use noon::{
    PlanarPickCache, PlanarPickFrame, PlanarViewerPointer, SpatialPickCache, SpatialPickFrame,
    SpatialPointerInput, SpatialPointerPhase,
};
use noon_core::{NativeInputValue, NativeStateSource, Vec2};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton};
use winit::keyboard::ModifiersState;

use super::{native_pointer_button, NativeApp, NativeHostError};

mod inspection;

const WINDOW_CURSOR: NativePointerId = NativePointerId {
    source: 1,
    pointer: 0,
};

pub(super) enum ViewerPickFrame {
    Planar(PlanarPickFrame),
    Spatial(SpatialPickFrame),
}

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
    pub(super) presented_viewer: Option<ViewerPickFrame>,
    spatial_cache: SpatialPickCache,
    spatial_button: Option<u8>,
    viewer_cache: PlanarPickCache,

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
        if !self.execution.presentation_ready() {
            return Ok(false);
        }
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
        let publication = self.execution.submit_native_pointer_input(token, input)?;
        self.next_input_sequence = next;
        if let Some(control) = publication.playback_control() {
            self.dispatch_playback_control(control, std::time::Instant::now())?;
        }
        Ok(())
    }

    pub(super) fn capture_pointer_presentation(
        &mut self,
        size: PhysicalSize<u32>,
        scale: f64,
    ) -> Result<PointerFrameSnapshot, NativeHostError> {
        let view = self.pointer_frame_view(logical_size(size, scale)?)?;
        self.execution.capture_pointer_frame(view)
    }

    fn uses_planar_viewer_pointer(&self) -> bool {
        let session = self.execution.session();
        !session.has_native_pointer_subscribers() && matches!(session.camera_3d(), Ok(None))
    }

    fn uses_spatial_viewer_pointer(&self) -> bool {
        let session = self.execution.session();
        !session.has_native_pointer_subscribers() && matches!(session.camera_3d(), Ok(Some(_)))
    }

    pub(super) fn capture_viewer_pick_frame(
        &mut self,
        mapping: &PointerFrameSnapshot,
        spatial_camera: Option<noon_core::SemanticCamera3D>,
        snapshot: bool,
    ) -> Option<ViewerPickFrame> {
        if self.execution.session().has_native_pointer_subscribers() {
            self.pointer.viewer_cache = PlanarPickCache::default();
            self.pointer.spatial_cache = SpatialPickCache::default();
            return None;
        }
        let publication = self.execution.session().resource_publication();
        if let Some(frame) =
            self.pointer
                .spatial_cache
                .capture(&publication, spatial_camera, snapshot)
        {
            self.pointer.viewer_cache = PlanarPickCache::default();
            return Some(ViewerPickFrame::Spatial(frame));
        }
        let view = mapping.view();
        let world_units_per_pixel = f64::from(view.camera().height) / f64::from(view.viewport().y);
        Some(ViewerPickFrame::Planar(self.pointer.viewer_cache.capture(
            &publication,
            snapshot,
            world_units_per_pixel,
        )))
    }

    fn dispatch_viewer_button(
        &mut self,
        state: ElementState,
        surface: Vec2,
        logical: Vec2,
    ) -> Result<PointerDispatch, NativeHostError> {
        let Some((frame, ViewerPickFrame::Planar(pick))) = self
            .pointer
            .presented
            .as_ref()
            .zip(self.pointer.presented_viewer.as_ref())
        else {
            self.cancel_viewer_pointer()?;
            self.pointer.refresh_pending = true;
            return Ok(PointerDispatch::AwaitingPresentation);
        };
        let view = self.pointer_frame_view(logical)?;
        let current = self.execution.capture_pointer_frame(view)?;
        if !frame.same_inspection_attachment(&current) {
            self.cancel_viewer_pointer()?;
            self.pointer.refresh_pending = true;
            return Ok(PointerDispatch::AwaitingPresentation);
        }
        let input = match state {
            ElementState::Pressed => PlanarViewerPointer::Press {
                surface,
                scene: frame.position(surface)?.scene(),
            },
            ElementState::Released => PlanarViewerPointer::Release(surface),
        };
        self.execution
            .submit_planar_viewer_pointer(WINDOW_CURSOR, input, Some(pick));
        Ok(PointerDispatch::Admitted)
    }

    fn dispatch_spatial_pointer(
        &mut self,
        phase: SpatialPointerPhase,
        surface: Vec2,
        logical: Vec2,
        pan: bool,
    ) -> Result<PointerDispatch, NativeHostError> {
        let input = SpatialPointerInput {
            id: 0,
            phase,
            position: surface * (1.0 / logical.y),
            view_revision: self.pointer.view_revision,
            viewport_height: logical.y,
            timestamp_ms: self.interaction_wall_origin.elapsed().as_secs_f64() * 1_000.0,
            touch: false,
            pan,
        };
        let displayed = if phase == SpatialPointerPhase::Press {
            let Some((mapping, ViewerPickFrame::Spatial(pick))) = self
                .pointer
                .presented
                .as_ref()
                .zip(self.pointer.presented_viewer.as_ref())
            else {
                self.pointer.refresh_pending = true;
                return Ok(PointerDispatch::AwaitingPresentation);
            };
            let view = self.pointer_frame_view(logical)?;
            let current = self.execution.capture_pointer_frame(view)?;
            if !mapping.same_inspection_attachment(&current) {
                self.pointer.refresh_pending = true;
                return Ok(PointerDispatch::AwaitingPresentation);
            }
            Some(pick)
        } else {
            None
        };
        let changed = self.execution.submit_spatial_pointer(
            input,
            displayed,
            f64::from(logical.x) / f64::from(logical.y),
        )?;
        self.pointer.refresh_pending |= changed;
        Ok(PointerDispatch::Admitted)
    }

    fn cancel_viewer_pointer(&mut self) -> Result<(), NativeHostError> {
        self.execution.submit_planar_viewer_pointer(
            WINDOW_CURSOR,
            PlanarViewerPointer::Cancel,
            None,
        );
        if self.pointer.spatial_button.take().is_some()
            || matches!(
                self.pointer.presented_viewer,
                Some(ViewerPickFrame::Spatial(_))
            )
        {
            self.dispatch_spatial_pointer(
                SpatialPointerPhase::Cancel,
                Vec2::ZERO,
                Vec2::new(1.0, 1.0),
                false,
            )?;
        }
        Ok(())
    }

    fn pointer_frame_view(&self, logical: Vec2) -> Result<PointerFrameView, NativeHostError> {
        self.execution
            .session()
            .inspection_pointer_view(self.pointer.view_revision, logical)
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
            | PointerFrameError::PickingChanged
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
        if let Err(error) = self
            .execution
            .validate_pointer_frame_position(&frame, view, surface)
        {
            return match error {
                NativeHostError::PointerFrame(error) => self.reject_pointer_frame(error),
                error => Err(error),
            };
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
        let token = match self
            .execution
            .pointer_frame_input_token(&frame, view, surface)
        {
            Ok(token) => token,
            Err(NativeHostError::PointerFrame(error)) => return self.reject_pointer_frame(error),
            Err(error) => return Err(error),
        };
        self.admit_pointer_kind(&token, kind(position))?;
        // A pointer move over unchanged content needs no repaint. Geometry
        // changes wait for the normal successful-presentation observation.
        if frame
            .validate_presentation(self.execution.session(), view)
            .is_ok()
        {
            self.execution.refresh_pointer_hover(&frame)?;
        }
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
        let viewer_pointer =
            outcome == PointerDispatch::Unsubscribed && self.uses_planar_viewer_pointer();
        if viewer_pointer {
            self.execution.submit_planar_viewer_pointer(
                WINDOW_CURSOR,
                PlanarViewerPointer::Move(surface),
                None,
            );
        }
        let spatial_pointer =
            outcome == PointerDispatch::Unsubscribed && self.uses_spatial_viewer_pointer();
        if spatial_pointer && self.pointer.spatial_button.is_some() {
            self.dispatch_spatial_pointer(SpatialPointerPhase::Move, surface, logical, false)?;
        }
        if outcome == PointerDispatch::Admitted
            || viewer_pointer
            || spatial_pointer
            || (outcome == PointerDispatch::Unsubscribed && self.config.inspection_zoom)
        {
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
        if button <= 2 && self.uses_spatial_viewer_pointer() {
            return match (state, self.pointer.spatial_button) {
                (ElementState::Pressed, None) => {
                    let pan = button != 0 || self.pointer.modifiers.shift_key();
                    let outcome = self.dispatch_spatial_pointer(
                        SpatialPointerPhase::Press,
                        surface,
                        logical,
                        pan,
                    )?;
                    if outcome == PointerDispatch::Admitted {
                        self.pointer.spatial_button = Some(button);
                    }
                    Ok(outcome)
                }
                (ElementState::Released, Some(held)) if held == button => {
                    self.pointer.spatial_button = None;
                    self.dispatch_spatial_pointer(
                        SpatialPointerPhase::Release,
                        surface,
                        logical,
                        false,
                    )
                }
                _ => Ok(PointerDispatch::Unsubscribed),
            };
        }
        if button == 0 && self.uses_planar_viewer_pointer() {
            return self.dispatch_viewer_button(state, surface, logical);
        }
        self.dispatch_presented_pointer(surface, logical, |position| {
            if state == ElementState::Pressed {
                NativePointerInputKind::Press { position, button }
            } else {
                NativePointerInputKind::Release { position, button }
            }
        })
    }

    fn cancel_pointer(&mut self, reason: NativePointerCancellation) -> Result<(), NativeHostError> {
        self.cancel_viewer_pointer()?;
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

    pub(super) fn pointer_source_changed(&mut self) -> Result<(), NativeHostError> {
        self.pointer.configured = false;
        self.rebind_pointer_view()?;
        self.ensure_pointer_input()?;
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
        self.cancel_viewer_pointer()?;
        self.pointer.view_revision = next;
        self.pointer.surface = None;
        self.pointer.presented = None;
        self.pointer.presented_viewer = None;
        self.pointer.viewer_cache = PlanarPickCache::default();
        self.pointer.spatial_cache = SpatialPickCache::default();
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
