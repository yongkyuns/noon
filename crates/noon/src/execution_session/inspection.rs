//! Session-owned inspection navigation over the existing camera and input path.
//!
//! No authored camera mutation, timeline, picking index or platform policy lives
//! here. A caller presents the returned shared view and retains its snapshot only
//! after successful presentation, exactly as for ordinary pointer frames.

use noon_core::{Camera2DState, Vec2};

use super::{
    ExecutionSession, ExecutionSessionInputError, PointerFrameError, PointerFrameSnapshot,
    PointerFrameView,
};
use crate::{InspectionView2D, InspectionViewError};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SessionInspectionView {
    adjustment: InspectionView2D,
    revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InspectionNavigationError {
    Frame(PointerFrameError),
    Input(ExecutionSessionInputError),
    Composition(InspectionViewError),
    RevisionExhausted,
    CameraChangedDuringCancellation,
}

impl std::fmt::Display for InspectionNavigationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frame(error) => error.fmt(f),
            Self::Input(error) => error.fmt(f),
            Self::Composition(error) => error.fmt(f),
            Self::RevisionExhausted => f.write_str("inspection view revision is exhausted"),
            Self::CameraChangedDuringCancellation => f.write_str(
                "inspection cannot cancel a pointer driver that changes the effective camera",
            ),
        }
    }
}

impl std::error::Error for InspectionNavigationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            Self::Input(error) => Some(error),
            Self::Composition(error) => Some(error),
            _ => None,
        }
    }
}

impl ExecutionSession {
    /// Navigation revision scoped to this runtime incarnation. It never resets
    /// when zoom returns to its original value; old displayed snapshots stay stale.
    pub const fn inspection_view_revision(&self) -> u64 {
        self.inspection.revision
    }

    /// The composed presentation camera. `camera()` remains the authored/effective
    /// camera for export and replay; inspection never changes that camera object.
    pub fn inspection_camera(&self) -> Result<Camera2DState, PointerFrameError> {
        self.inspection
            .adjustment
            .resolve(self.camera().map_err(PointerFrameError::Camera)?)
            .map_err(PointerFrameError::Inspection)
    }

    /// Resolve one view for rendering, coordinate mapping and frame association.
    /// The host owns the surface revision and logical content size, not camera
    /// composition. Render with this value's camera, then capture/retain that same
    /// view at the successful presentation boundary. No presentation is implied.
    pub fn inspection_pointer_view(
        &self,
        surface_revision: u64,
        viewport: Vec2,
    ) -> Result<PointerFrameView, PointerFrameError> {
        PointerFrameView::new(surface_revision, viewport, self.inspection_camera()?)
    }

    /// Apply cursor-anchored inspection zoom to a still-current displayed frame.
    ///
    /// `surface` uses the snapshot's logical content pixels; wheel-unit conversion
    /// and canvas opt-in belong at the platform boundary. True requests one new
    /// presentation, not advancement of authored time. False is an exact no-op.
    ///
    /// Held native buttons are cancelled through sparse runtime input preparation
    /// before committing either state. A cancellation that changes the camera is
    /// explicitly rejected: camera-driver rebasing is not implemented here. Old
    /// pointer bindings are retired on success, and a fresh binding is required
    /// before another contact can form a click. No release event is fabricated.
    /// Required-callback gates remain enforced; ordinary active animations do not
    /// constitute a scene-wide lock on this independent session view.
    pub fn zoom_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        factor: f64,
    ) -> Result<bool, InspectionNavigationError> {
        displayed
            .validate_current(self, current_view)
            .map_err(InspectionNavigationError::Frame)?;
        let anchor = displayed
            .position(surface)
            .map_err(InspectionNavigationError::Frame)?
            .scene();
        let camera = self
            .camera()
            .map_err(PointerFrameError::Camera)
            .map_err(InspectionNavigationError::Frame)?;
        let next = self
            .inspection
            .adjustment
            .zoom_about(camera, anchor, factor)
            .map_err(InspectionNavigationError::Composition)?;
        self.commit_inspection_view(next)
    }

    /// Reset viewer navigation explicitly, without changing authored time or the
    /// scene. Same-session seeks and surface recovery preserve the adjustment;
    /// a newly created/replaced session starts at identity. Clones preserve the
    /// numerical adjustment but have a fresh runtime identity and no old receipts.
    /// Restart adapters must call this or install a new session, not reset a host
    /// camera alone. This operation can recover from an unrepresentable composed
    /// view as long as the authored/effective camera itself remains valid.
    pub fn reset_inspection_view(&mut self) -> Result<bool, InspectionNavigationError> {
        self.ensure_direct_input_ingress_available()
            .map_err(InspectionNavigationError::Input)?;
        self.commit_inspection_view(InspectionView2D::default())
    }

    fn commit_inspection_view(
        &mut self,
        next: InspectionView2D,
    ) -> Result<bool, InspectionNavigationError> {
        if next == self.inspection.adjustment {
            return Ok(false);
        }
        let revision = self
            .inspection
            .revision
            .checked_add(1)
            .ok_or(InspectionNavigationError::RevisionExhausted)?;
        let camera = self
            .camera()
            .map_err(PointerFrameError::Camera)
            .map_err(InspectionNavigationError::Frame)?;
        next.resolve(camera)
            .map_err(InspectionNavigationError::Composition)?;
        if let Some(prepared) = self
            .prepare_pointer_view_cancellation()
            .map_err(InspectionNavigationError::Input)?
        {
            if let Some(object) = self.camera_object {
                let index = self
                    .runtime
                    .frame_index_for_object(object)
                    .ok_or(InspectionNavigationError::CameraChangedDuringCancellation)?;
                let properties = self
                    .runtime
                    .prepared_properties_at(&prepared.frame, index, None)
                    .ok_or(InspectionNavigationError::CameraChangedDuringCancellation)?;
                let geometry = self
                    .runtime
                    .frame()
                    .render_geometry(index)
                    .ok_or(InspectionNavigationError::CameraChangedDuringCancellation)?;
                if Camera2DState::from_frame_object(geometry, properties.transform) != Some(camera)
                {
                    return Err(InspectionNavigationError::CameraChangedDuringCancellation);
                }
            }
            self.commit_reactive_input_batch(prepared)
                .map_err(InspectionNavigationError::Input)?;
        }
        // All fallible arithmetic, native evaluation and publication precedes
        // this bounded commit. Neither identity allocator nor semantic state changes.
        self.retire_pointer_view_binding();
        self.inspection = SessionInspectionView {
            adjustment: next,
            revision,
        };
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
