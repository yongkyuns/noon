//! Constant-size input association with one coherent frame and platform view.
//!
//! Capture before rendering and retain only after that exact frame/view is
//! successfully presented. A snapshot is not a GPU or OS presentation receipt:
//! the host owns that acknowledgement. It retains no geometry, index or history.

use noon_core::{Camera2DState, NativePointerPosition, PublicationContext, Vec2};
use noon_runtime::RuntimeIdentity;

use super::{ExecutionSession, ExecutionSessionInputError, NativePointerInputToken};
use crate::ExecutionSessionCameraError;

/// The actual camera and logical content viewport used for one presentation.
///
/// The host must change `revision` on mapping/lifecycle changes, including resize,
/// device scale, viewport placement and surface replacement. Size and camera are
/// compared as well, so an unchanged revision cannot conceal either change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerFrameView {
    revision: u64,
    viewport: Vec2,
    camera: Camera2DState,
}

impl PointerFrameView {
    pub fn new(
        revision: u64,
        viewport: Vec2,
        camera: Camera2DState,
    ) -> Result<Self, PointerFrameError> {
        if !viewport.x.is_finite()
            || !viewport.y.is_finite()
            || viewport.x <= 0.0
            || viewport.y <= 0.0
            || !camera.center.x.is_finite()
            || !camera.center.y.is_finite()
            || !camera.height.is_finite()
            || camera.height <= 0.0
        {
            return Err(PointerFrameError::InvalidView);
        }
        Ok(Self {
            revision,
            viewport,
            camera,
        })
    }

    pub const fn revision(self) -> u64 {
        self.revision
    }
    pub const fn viewport(self) -> Vec2 {
        self.viewport
    }
    pub const fn camera(self) -> Camera2DState {
        self.camera
    }
}

/// Immutable metadata captured from the execution that produced a rendered frame.
///
/// This value cannot be constructed from caller-supplied publication/runtime IDs.
/// Capturing it does not consume renderer dirtiness, acknowledge input, configure
/// a pointer, advance authored time or claim that anything has been displayed.
/// Hosts retain only their last successfully presented snapshot and invalidate it
/// when their surface is lost/replaced. Worker hosts must additionally associate
/// each occurrence with the receipt visible at collection time; they must not
/// substitute a newer receipt when draining a delayed occurrence.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerFrameSnapshot {
    runtime: RuntimeIdentity,
    publication: PublicationContext,
    view: PointerFrameView,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PointerFrameError {
    InvalidView,
    ViewChanged,
    CameraMismatch,
    PositionOutOfRange,
    Camera(ExecutionSessionCameraError),
    Input(ExecutionSessionInputError),
}

impl std::fmt::Display for PointerFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidView => {
                f.write_str("pointer frame requires a finite camera and positive finite viewport")
            }
            Self::ViewChanged => {
                f.write_str("pointer view no longer matches the captured presentation")
            }
            Self::CameraMismatch => f.write_str(
                "pointer presentation camera does not match the effective execution camera",
            ),
            Self::PositionOutOfRange => {
                f.write_str("pointer frame projection is nonfinite or outside the coordinate range")
            }
            Self::Camera(error) => error.fmt(f),
            Self::Input(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for PointerFrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Camera(error) => Some(error),
            Self::Input(error) => Some(error),
            _ => None,
        }
    }
}

impl ExecutionSession {
    /// Capture the current coherent frame with the view that the host will draw.
    ///
    /// The caller retains this value only after presenting the same publication
    /// with this view. Failed acquisition, encoding, submission or presentation
    /// must not replace the last presented snapshot. This is a read-only O(1)
    /// operation, independent of the number of scene objects or input subscribers.
    pub fn capture_pointer_frame(
        &self,
        view: PointerFrameView,
    ) -> Result<PointerFrameSnapshot, PointerFrameError> {
        if view.camera != self.camera().map_err(PointerFrameError::Camera)? {
            return Err(PointerFrameError::CameraMismatch);
        }
        Ok(PointerFrameSnapshot {
            runtime: self.runtime_identity(),
            publication: self.publication_context(),
            view,
        })
    }
}

impl PointerFrameSnapshot {
    pub const fn publication(&self) -> PublicationContext {
        self.publication
    }
    pub const fn view(&self) -> PointerFrameView {
        self.view
    }

    /// Map one occurrence-local logical position using the captured view only.
    /// Outside-viewport positions are valid for platform capture. Intermediate
    /// arithmetic is widened so extreme finite inputs do not overflow before a
    /// representable final coordinate is computed. No current camera is consulted.
    pub fn position(&self, surface: Vec2) -> Result<NativePointerPosition, PointerFrameError> {
        let camera = self.view.camera;
        // world-units per logical y pixel; the normalized 2D camera is isotropic.
        let scale = f64::from(camera.height) / f64::from(self.view.viewport.y);
        let x = f64::from(camera.center.x)
            + (f64::from(surface.x) - f64::from(self.view.viewport.x) * 0.5) * scale;
        let y = f64::from(camera.center.y)
            + (f64::from(self.view.viewport.y) * 0.5 - f64::from(surface.y)) * scale;
        NativePointerPosition::new(Vec2::new(x as f32, y as f32), surface)
            .map_err(|_| PointerFrameError::PositionOutOfRange)
    }

    /// Obtain the existing input token only while this captured frame is current.
    ///
    /// This never refreshes a stale snapshot to the session's newer publication.
    /// The returned token goes through ordinary precise-picking and atomic input
    /// admission. If execution advances after this call, that admission rejects
    /// the positional occurrence again. The configured source/binding and callback
    /// rules are unchanged. Use `position` for the occurrence's coordinates.
    ///
    /// Cancellation does NOT require this positional check: use the contact's
    /// previously captured token with `NativePointerInputKind::Cancel`. The normal
    /// ingress permits its older publication while still enforcing runtime,
    /// binding, sequence and callback barriers. No automatic replay is performed.
    pub fn input_token(
        &self,
        session: &ExecutionSession,
        current_view: PointerFrameView,
    ) -> Result<NativePointerInputToken, PointerFrameError> {
        session
            .ensure_direct_input_ingress_available()
            .map_err(PointerFrameError::Input)?;
        if self.runtime != session.runtime_identity() {
            return Err(PointerFrameError::Input(
                ExecutionSessionInputError::ForeignPointerRuntime,
            ));
        }
        if self.view != current_view {
            return Err(PointerFrameError::ViewChanged);
        }
        let current = session.publication_context();
        if self.publication != current {
            return Err(PointerFrameError::Input(
                ExecutionSessionInputError::StalePointerPublication {
                    expected: current,
                    actual: self.publication,
                },
            ));
        }
        if self.view.camera != session.camera().map_err(PointerFrameError::Camera)? {
            return Err(PointerFrameError::CameraMismatch);
        }
        let token = session
            .native_pointer_input_token()
            .map_err(PointerFrameError::Input)?;
        if token.context().view_revision != self.view.revision {
            return Err(PointerFrameError::ViewChanged);
        }
        Ok(token)
    }
}

#[cfg(test)]
mod tests;
