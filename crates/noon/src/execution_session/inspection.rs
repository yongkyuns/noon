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
use crate::{
    inspection_view::InspectionView3D, InspectionView2D, InspectionViewError,
    SpatialInspectionMotion,
};

mod gesture;
mod picking;

pub use gesture::{SpatialPointerInput, SpatialPointerPhase};
pub use picking::{SpatialPickCache, SpatialPickFrame};

use gesture::{SpatialGesture, SpatialGestures};

/// Effective display tint for a selected mesh. The authored fill remains in
/// the execution frame, including any concurrent material animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialInspectionHighlight {
    pub object: noon_core::ObjectId,
    pub fill: noon_core::Color,
}

#[derive(Clone, Copy, Debug)]
enum InspectionStart {
    Planar(InspectionView2D),
    Spatial(InspectionView3D),
}

#[derive(Clone, Copy, Debug)]
struct InspectionReset {
    start: InspectionStart,
    origin: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SessionInspectionView {
    adjustment: InspectionView2D,
    spatial: InspectionView3D,
    reset: Option<InspectionReset>,
    spatial_gestures: SpatialGestures,
    spatial_selected: Option<noon_core::SemanticNodeId>,
    revision: u64,
}

impl SessionInspectionView {
    pub(super) fn without_gestures(self) -> Self {
        Self {
            spatial_gestures: SpatialGestures::default(),
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InspectionNavigationError {
    Frame(PointerFrameError),
    Input(ExecutionSessionInputError),
    Composition(InspectionViewError),
    RevisionExhausted,
    InvalidScroll,
    InvalidSpatialMotion,
    CameraChangedDuringCancellation,
}

impl std::fmt::Display for InspectionNavigationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Frame(error) => error.fmt(f),
            Self::Input(error) => error.fmt(f),
            Self::Composition(error) => error.fmt(f),
            Self::InvalidSpatialMotion => {
                f.write_str("inspection motion is outside the spatial camera domain")
            }
            Self::InvalidScroll => f.write_str("inspection scroll delta must be finite"),
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
    #[cfg(feature = "prepared-prefix")]
    pub(crate) fn inherit_prepared_inspection(&mut self, reader: &Self) {
        // The runtime incarnation still changes. This copies only the viewer's
        // adjustment; no old frame/input receipt or pointer contact is rebound.
        self.inspection = reader.inspection.without_gestures();
        if reader.default_viewer_selection() {
            self.enable_default_viewer_selection();
        }
    }

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
            .resolve(
                self.camera_2d()
                    .map_err(PointerFrameError::Camera)?
                    .unwrap_or_default(),
            )
            .map_err(PointerFrameError::Inspection)
    }

    /// Resolve the viewer adjustment for the existing spatial camera row. An
    /// identity adjustment leaves the authored renderer publication untouched.
    pub fn inspection_spatial_camera(
        &self,
    ) -> Result<Option<(usize, noon_core::SemanticCamera3D)>, InspectionNavigationError> {
        self.resolve_spatial_inspection(self.inspection.spatial)
    }

    fn resolve_spatial_inspection(
        &self,
        adjustment: InspectionView3D,
    ) -> Result<Option<(usize, noon_core::SemanticCamera3D)>, InspectionNavigationError> {
        if adjustment == InspectionView3D::default() {
            return Ok(None);
        }
        let camera = self
            .camera_3d()
            .map_err(PointerFrameError::Camera)
            .map_err(InspectionNavigationError::Frame)?;
        let Some((camera, index)) = camera.zip(
            self.camera_object
                .and_then(|id| self.runtime.frame_index_for_object(id)),
        ) else {
            return Ok(None);
        };
        let focal_distance = self.frame().objects[index]
            .camera_profile()
            .map(|(profile, ..)| profile.focal_distance);
        adjustment
            .resolve(camera, focal_distance)
            .map(|view| Some((index, view)))
            .ok_or(InspectionNavigationError::InvalidSpatialMotion)
    }

    /// The current selected mesh remains bound to its semantic identity as its
    /// authored transform and material advance.
    pub fn spatial_inspection_highlight(&self) -> Option<SpatialInspectionHighlight> {
        let object = self.interaction_execution_object_id(self.inspection.spatial_selected?)?;
        let row = self.runtime.effective_object(object)?;
        let color = row.style.fill?;
        let accent = if color.red + color.green - color.blue > 1.4 {
            noon_core::Color::BLUE_A
        } else {
            noon_core::Color::YELLOW
        };
        let blend = |value, target| value * 0.18 + target * 0.82;
        Some(SpatialInspectionHighlight {
            object,
            fill: noon_core::Color::rgba(
                blend(color.red, accent.red),
                blend(color.green, accent.green),
                blend(color.blue, accent.blue),
                color.alpha,
            ),
        })
    }

    fn select_spatial_mesh(&mut self, object: Option<noon_core::ObjectId>) -> bool {
        let selected = object.and_then(|object| self.interaction_semantic_object_id(object));
        if object.is_some() && selected.is_none() {
            return false;
        }
        let changed = selected != self.inspection.spatial_selected;
        self.inspection.spatial_selected = selected;
        changed
    }

    pub fn cancel_spatial_pointer_gestures(&mut self) {
        self.inspection.spatial_gestures = SpatialGestures::default();
    }

    pub fn submit_spatial_pointer(
        &mut self,
        input: SpatialPointerInput,
        displayed: Option<&SpatialPickFrame>,
        aspect: f64,
    ) -> Result<bool, InspectionNavigationError> {
        match self.inspection.spatial_gestures.submit(input) {
            SpatialGesture::None => Ok(false),
            SpatialGesture::Motion(motion) => self.move_spatial_inspection(motion),
            SpatialGesture::Click(position) => Ok(displayed
                .is_some_and(|frame| self.select_spatial_mesh(frame.pick(position, aspect)))),
            SpatialGesture::Reset => self.move_spatial_inspection(SpatialInspectionMotion {
                reset: true,
                ..SpatialInspectionMotion::default()
            }),
        }
    }

    /// Request presentations while a camera return is active, including at a
    /// paused or otherwise static point on the authored timeline.
    pub fn viewer_inspection_active(&self) -> bool {
        self.inspection.reset.is_some()
    }

    pub fn advance_viewer_inspection(
        &mut self,
        wall_time_seconds: f64,
    ) -> Result<(), InspectionNavigationError> {
        let Some(mut reset) = self.inspection.reset else {
            return Ok(());
        };
        if !wall_time_seconds.is_finite() || wall_time_seconds < 0.0 {
            return Err(InspectionNavigationError::InvalidSpatialMotion);
        }
        let origin = *reset.origin.get_or_insert(wall_time_seconds);
        let progress = ((wall_time_seconds - origin) / 0.35).clamp(0.0, 1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        match reset.start {
            InspectionStart::Planar(start) => {
                self.commit_inspection_view(start.returning_to_authored(eased))?;
            }
            InspectionStart::Spatial(start) => {
                let next = start
                    .returning_to_authored(eased)
                    .ok_or(InspectionNavigationError::InvalidSpatialMotion)?;
                self.resolve_spatial_inspection(next)?;
                self.inspection.spatial = next;
            }
        }
        self.inspection.reset = (progress < 1.0).then_some(reset);
        Ok(())
    }

    /// Apply relative spatial navigation. This command has no picking anchor,
    /// so successive updates can compose while a previous view is presenting.
    /// The planar HUD/pointer mapping and authored publication stay unchanged.
    pub fn move_spatial_inspection(
        &mut self,
        motion: SpatialInspectionMotion,
    ) -> Result<bool, InspectionNavigationError> {
        let spatial = self
            .camera_3d()
            .map_err(PointerFrameError::Camera)
            .map_err(InspectionNavigationError::Frame)?
            .is_some();
        if motion.reset {
            let start = if spatial && self.inspection.spatial != InspectionView3D::default() {
                InspectionStart::Spatial(self.inspection.spatial)
            } else if !spatial && self.inspection.adjustment != InspectionView2D::default() {
                InspectionStart::Planar(self.inspection.adjustment)
            } else {
                return Ok(false);
            };
            self.inspection.reset = Some(InspectionReset {
                start,
                origin: None,
            });
            return Ok(true);
        }
        if !spatial {
            if !motion
                .pan
                .into_iter()
                .chain([motion.zoom])
                .all(f64::is_finite)
            {
                return Err(InspectionNavigationError::InvalidSpatialMotion);
            }
            let mut next = self.inspection.adjustment.panned(motion.pan);
            if motion.zoom != 0.0 {
                let camera = self
                    .camera()
                    .map_err(PointerFrameError::Camera)
                    .map_err(InspectionNavigationError::Frame)?;
                let center = next
                    .resolve(camera)
                    .map_err(InspectionNavigationError::Composition)?
                    .center;
                next = next
                    .zoom_about(camera, center, motion.zoom.clamp(-16.0, 16.0).exp())
                    .map_err(InspectionNavigationError::Composition)?;
            }
            let changed = self.commit_inspection_view(next)?;
            if changed {
                self.inspection.reset = None;
            }
            return Ok(changed);
        }
        let next = self
            .inspection
            .spatial
            .moved(motion)
            .ok_or(InspectionNavigationError::InvalidSpatialMotion)?;
        if next == self.inspection.spatial {
            return Ok(false);
        }
        self.resolve_spatial_inspection(next)?;
        self.inspection.spatial = next;
        self.inspection.reset = None;
        Ok(true)
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
        let changed = self.commit_inspection_view(next)?;
        if changed {
            self.inspection.reset = None;
        }
        Ok(changed)
    }

    /// Scroll by normalized logical pixels (positive means zoom out).
    ///
    /// The response curve is shared by platform adapters, not a host animation.
    /// Extreme finite deltas saturate before exponentiation; relative zoom limits
    /// and cursor anchoring remain those of `zoom_inspection_view`.
    pub fn scroll_inspection_view(
        &mut self,
        displayed: &PointerFrameSnapshot,
        current_view: PointerFrameView,
        surface: Vec2,
        delta_pixels: f64,
    ) -> Result<bool, InspectionNavigationError> {
        if !delta_pixels.is_finite() {
            return Err(InspectionNavigationError::InvalidScroll);
        }
        let factor = (delta_pixels / 500.0).clamp(-16.0, 16.0).exp();
        self.zoom_inspection_view(displayed, current_view, surface, factor)
    }

    /// Reset viewer navigation explicitly, without changing authored time or the
    /// scene. Same-session seeks and surface recovery preserve the adjustment;
    /// a newly created/replaced session starts at identity. Clones preserve the
    /// numerical adjustment but have a fresh runtime identity and no old receipts.
    /// Explicit reset adapters must call this instead of resetting a host camera
    /// alone; same-session playback restart preserves the adjustment. This operation
    /// can recover from an unrepresentable composed view as long as the
    /// authored/effective camera itself remains valid.
    pub fn reset_inspection_view(&mut self) -> Result<bool, InspectionNavigationError> {
        self.ensure_direct_input_ingress_available()
            .map_err(InspectionNavigationError::Input)?;
        let changed = self.commit_inspection_view(InspectionView2D::default())?;
        self.inspection.reset = None;
        Ok(changed)
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
                let mut transform = properties.transform;
                if let Some(cancellation) = prepared
                    .drag_cancellation
                    .filter(|cancellation| cancellation.object == object)
                {
                    transform.translation = cancellation.base;
                }
                if Camera2DState::from_frame_object(geometry, transform) != Some(camera) {
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
            ..self.inspection
        };
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
