//! Constant-size input association with one coherent frame and platform view.
//!
//! Capture before rendering and retain only after that exact frame/view is
//! successfully presented. A snapshot is not a GPU or OS presentation receipt:
//! the host owns that acknowledgement. It retains no geometry, index or history.

use noon_core::{Camera2DState, NativePointerPosition, PublicationContext, Vec2};
use noon_runtime::RuntimeIdentity;

use super::{ExecutionSession, ExecutionSessionInputError, NativePointerInputToken};
use crate::ExecutionSessionCameraError;

mod damage;
pub(in crate::execution_session) use damage::PointerFrameDamage;

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
/// Hosts retain bounded successfully presented metadata and invalidate it
/// when their surface is lost/replaced. Worker hosts must additionally associate
/// each occurrence with the receipt visible at collection time; they must not
/// substitute a newer receipt when draining a delayed occurrence.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerFrameSnapshot {
    runtime: RuntimeIdentity,
    publication: PublicationContext,
    view: PointerFrameView,
    inspection_revision: u64,
    damage_stamp: Option<u64>,
}

/// A point-specific proof for one current input transaction, never a new receipt.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PointerPositionProof {
    pub(super) publication: PublicationContext,
    pub(super) position: NativePointerPosition,
    pub(super) damage_stamp: Option<u64>,
    pub(super) inspection_revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PointerFrameError {
    InvalidView,
    ViewChanged,
    CameraMismatch,
    PositionOutOfRange,
    PickingChanged,
    Camera(ExecutionSessionCameraError),
    Input(ExecutionSessionInputError),
    Inspection(crate::InspectionViewError),
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
            Self::PickingChanged => {
                f.write_str("the displayed point no longer has unchanged picking evidence")
            }
            Self::Camera(error) => error.fmt(f),
            Self::Input(error) => error.fmt(f),
            Self::Inspection(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for PointerFrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Camera(error) => Some(error),
            Self::Input(error) => Some(error),
            Self::Inspection(error) => Some(error),
            _ => None,
        }
    }
}

impl ExecutionSession {
    /// Capture the current coherent frame with the view that the host will draw.
    ///
    /// The caller retains this value only after presenting the same publication
    /// with this view. Failed acquisition, encoding, submission or presentation
    /// must not replace the last presented snapshot. Capture checkpoints pending
    /// spatial changes in the existing index and bounded damage journal. Work is
    /// proportional to dirty rows; the returned metadata has constant size.
    pub fn capture_pointer_frame(
        &mut self,
        view: PointerFrameView,
    ) -> Result<PointerFrameSnapshot, PointerFrameError> {
        if view.camera != self.inspection_camera()? {
            return Err(PointerFrameError::CameraMismatch);
        }
        self.sync_spatial_index();
        Ok(PointerFrameSnapshot {
            runtime: self.runtime_identity(),
            publication: self.publication_context(),
            view,
            inspection_revision: self.inspection_view_revision(),
            damage_stamp: if self.derived_display_plan.is_some()
                || !self.derived_display_objects.is_empty()
            {
                None
            } else {
                self.pointer_frame_damage.stamp()
            },
        })
    }
}

impl PointerFrameSnapshot {
    /// Session navigation revision, distinct from the host's surface revision.
    pub const fn inspection_revision(&self) -> u64 {
        self.inspection_revision
    }

    pub const fn publication(&self) -> PublicationContext {
        self.publication
    }
    pub const fn view(&self) -> PointerFrameView {
        self.view
    }

    /// Whether a fresh capture can replace obsolete input evidence at this view.
    ///
    /// Hosts combine this with their normal publication/view freshness check.
    /// Policy or same-time seek barriers may leave publication IDs unchanged.
    /// Unsupported derived presentation and exhausted journals cannot be repaired
    /// by a receipt-only redraw, so they must not create a refresh feedback loop.
    /// This is a wake hint, never positional input or presentation admission.
    pub fn needs_input_refresh(&self, session: &ExecutionSession) -> bool {
        session.derived_display_plan.is_none()
            && session.derived_display_objects.is_empty()
            && session.pointer_frame_damage.stamp().is_some()
            && self.damage_stamp != session.pointer_frame_damage.stamp()
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

    /// Validate the captured publication/view before configuring a pointer source.
    ///
    /// Unlike `input_token`, this does not require a configured pointer binding.
    /// Hosts call it before any binding reset, then call `input_token` afterwards:
    /// configuration may itself change effective state. Neither call refreshes the
    /// captured frame or claims that it was presented.
    pub fn validate_current(
        &self,
        session: &ExecutionSession,
        current_view: PointerFrameView,
    ) -> Result<(), PointerFrameError> {
        session
            .ensure_direct_input_ingress_available()
            .map_err(PointerFrameError::Input)?;
        self.validate_presentation(session, current_view)
    }

    /// Prove that the original displayed point has the same picking inputs now.
    ///
    /// Authored/execution revisions and mapping must match. Effective frames may
    /// advance only when every intervening recorded change excludes this point.
    /// Missing or expired evidence rejects input. This neither picks a target nor
    /// replaces the original publication, and it never claims presentation.
    pub fn validate_position(
        &self,
        session: &mut ExecutionSession,
        current_view: PointerFrameView,
        surface: Vec2,
    ) -> Result<(), PointerFrameError> {
        self.validate_input_mapping(session, current_view)?;
        let current = session.publication_context();
        if self.publication.scene_revision() != current.scene_revision()
            || self.publication.execution_revision() != current.execution_revision()
        {
            return Err(PointerFrameError::Input(
                ExecutionSessionInputError::StalePointerPublication {
                    expected: current,
                    actual: self.publication,
                },
            ));
        }
        let position = self.position(surface)?;
        session.sync_spatial_index();
        if session.derived_display_plan.is_some()
            || !session.derived_display_objects.is_empty()
            || !session
                .pointer_frame_damage
                .unchanged_since(self.damage_stamp, position.scene())
        {
            return Err(PointerFrameError::PickingChanged);
        }
        Ok(())
    }

    /// Mint a point-specific token while preserving the collected publication.
    ///
    /// Hosts validate before binding and call this again after binding, because
    /// resetting an older source can change effective state. The private proof
    /// expires on any subsequent publication, policy change or point substitution.
    /// Precise picking and action/driver admission still use the current runtime.
    pub fn input_token_at(
        &self,
        session: &mut ExecutionSession,
        current_view: PointerFrameView,
        surface: Vec2,
    ) -> Result<NativePointerInputToken, PointerFrameError> {
        self.validate_position(session, current_view, surface)?;
        let mut token = session
            .native_pointer_input_token()
            .map_err(PointerFrameError::Input)?;
        if token.context().view_revision != self.view.revision {
            return Err(PointerFrameError::ViewChanged);
        }
        token.context.publication = self.publication;
        token.position_proof = Some(PointerPositionProof {
            publication: session.publication_context(),
            position: self.position(surface)?,
            damage_stamp: session.pointer_frame_damage.stamp(),
            inspection_revision: session.inspection_view_revision(),
        });
        Ok(token)
    }

    /// Validate publication/view identity without admitting input or evaluating
    /// callbacks. Hosts use this after presentation; positional input separately
    /// enforces callback barriers and validates the exact collected point.
    pub fn validate_presentation(
        &self,
        session: &ExecutionSession,
        current_view: PointerFrameView,
    ) -> Result<(), PointerFrameError> {
        if self.runtime != session.runtime_identity() {
            return Err(PointerFrameError::Input(
                ExecutionSessionInputError::ForeignPointerRuntime,
            ));
        }
        if self.view != current_view
            || self.inspection_revision != session.inspection_view_revision()
        {
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
        if self.view.camera != session.inspection_camera()? {
            return Err(PointerFrameError::CameraMismatch);
        }
        Ok(())
    }

    /// Validate the immutable mapping retained by an already captured gesture.
    ///
    /// This deliberately does not compare publications: captured motion may
    /// publish a local effective write before the host presents that newer
    /// frame. It still requires the same runtime, inspection view, camera and
    /// direct-input admission barriers. Callers must use this only after the
    /// session has already acquired its own gesture capture; it is not an
    /// alternative admission path for a new positional input or a new pick.
    pub fn validate_captured_gesture(
        &self,
        session: &ExecutionSession,
        current_view: PointerFrameView,
    ) -> Result<(), PointerFrameError> {
        self.validate_input_mapping(session, current_view)
    }

    fn validate_input_mapping(
        &self,
        session: &ExecutionSession,
        current_view: PointerFrameView,
    ) -> Result<(), PointerFrameError> {
        session
            .ensure_direct_input_ingress_available()
            .map_err(PointerFrameError::Input)?;
        if self.runtime != session.runtime_identity() {
            return Err(PointerFrameError::Input(
                ExecutionSessionInputError::ForeignPointerRuntime,
            ));
        }
        if self.view != current_view
            || self.inspection_revision != session.inspection_view_revision()
        {
            return Err(PointerFrameError::ViewChanged);
        }
        if self.view.camera != session.inspection_camera()? {
            return Err(PointerFrameError::CameraMismatch);
        }
        Ok(())
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
        self.validate_current(session, current_view)?;
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

#[cfg(test)]
mod compatibility_tests;
