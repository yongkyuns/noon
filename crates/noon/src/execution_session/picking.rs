//! Publication-qualified, candidate-local picking of analytic fill interiors.
//!
//! This first C5 query observes the existing session/runtime/index. It owns no
//! scene, input queue, geometry cache, selection state, or platform mechanics.

use noon_core::{
    GeometryRef, GeometryResource, GeometryResourceLookup, NativePointerInput, PublicationContext,
    SemanticNodeId, StrokeWidthMode, Transform2D, Vec2,
};
use noon_runtime::{FrameState, SpatialQueryStats};

use super::{ExecutionSession, ExecutionSessionInputError, NativePointerInputToken};

mod identity;

/// Why a potentially filled candidate cannot be decided by the initial picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerFillUnsupported {
    /// Curved paths, lines, text, images and unsupported external content do not
    /// have a precise policy here; their bounds are not a precise fill hit.
    Content,
    /// Partial reveal and morph progress are not approximated with a full shape.
    PartialGeometry,
    /// The inverse affine mapping is undefined or the nominal shape has no area.
    DegenerateGeometry,
}

/// Result in topmost-first painter order, after the supplied eligibility filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerFillOutcome {
    /// Cancellation has no position. This differs from a positioned miss.
    NoPosition,
    Miss,
    Hit(SemanticNodeId),
    /// Do not silently select an object behind an eligible, undecidable fill.
    Unsupported {
        target: SemanticNodeId,
        reason: PointerFillUnsupported,
    },
}

/// Immutable observation, not input acceptance or authority for a later action.
/// Revalidate the input token at the session boundary before publishing actions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerFillQuery {
    input: NativePointerInput,
    publication: PublicationContext,
    outcome: PointerFillOutcome,
    spatial_stats: SpatialQueryStats,
    precise_tests: usize,
}

impl PointerFillQuery {
    pub const fn input(&self) -> NativePointerInput {
        self.input
    }
    pub const fn publication(&self) -> PublicationContext {
        self.publication
    }
    pub const fn outcome(&self) -> PointerFillOutcome {
        self.outcome
    }
    pub const fn spatial_stats(&self) -> SpatialQueryStats {
        self.spatial_stats
    }
    pub const fn precise_tests(&self) -> usize {
        self.precise_tests
    }
}

impl ExecutionSession {
    /// Pick the topmost eligible analytic fill at an occurrence's scene position.
    ///
    /// This deliberately named *fill* policy supports complete Circle/Rectangle
    /// interiors under the effective affine transform, including reflection and
    /// nonuniform scale. Fill alpha, object opacity, appearance and presence must
    /// be positive. Strokes are not hit targets, including stroke-only objects.
    /// The nominal contour is inclusive; raster AA fringes, alpha quantization,
    /// minimum raster proxy sizes, and pixel-perfect coverage are not modelled.
    ///
    /// Line-only vector paths use the renderer-neutral non-zero polygon fill rule;
    /// curves, text/image masks, lines, partial reveals/morphs and degenerate
    /// geometry return `Unsupported` when encountered as an eligible potentially
    /// filled candidate. The query never calls their bounding box a precise hit.
    /// Groups are traversed by the existing lowering/index; returned identities
    /// are semantic leaves, never execution slots or frontend wrapper IDs. During
    /// sealed replay they belong to the selected historical revision, even when
    /// the corresponding object has since been deleted from the authored store.
    /// A historical hit grants no permission to mutate a deleted source object.
    ///
    /// The eligibility predicate is native, candidate-local filtering for a
    /// caller's declared targets. Returning false permits picking through that
    /// candidate. It must not stand in for host-language scene traversal/policy.
    ///
    /// The token must belong to this runtime, current binding/view and effective
    /// publication. Stale input is rejected, never retagged. This does not pin a
    /// historical displayed frame or prove that a current frame was presented.
    /// The host must preserve that association; delayed displayed-frame picking
    /// is not implemented by this current-publication query.
    ///
    /// No sequence is acknowledged and no reactive input is published here.
    /// Input submission and any later native action still use the existing
    /// session boundary. Required callback barriers are not bypassed by a query.
    pub fn pick_native_pointer_fill(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
        mut eligible: impl FnMut(SemanticNodeId) -> bool,
    ) -> Result<PointerFillQuery, ExecutionSessionInputError> {
        let publication = self.preflight_native_pointer_input(token, input)?;
        let mut result = PointerFillQuery {
            input,
            publication,
            outcome: PointerFillOutcome::NoPosition,
            spatial_stats: SpatialQueryStats::default(),
            precise_tests: 0,
        };
        let Some(position) = input.position() else {
            return Ok(result);
        };
        (result.outcome, result.spatial_stats, result.precise_tests) =
            self.pick_effective_fill(position.scene(), &mut eligible);
        Ok(result)
    }

    /// Pick in painter order. Open strokes use their revealed contour, and
    /// the viewer can click inside a closed outline above a solid fill.
    pub(super) fn pick_viewer_target(&mut self, point: Vec2) -> Option<SemanticNodeId> {
        self.sync_spatial_index();
        for &slot in self.spatial_index.hit_test(point).slots() {
            let Some(object) = self.slots.object_for_slot(slot) else {
                continue;
            };
            if Some(object) == self.camera_object {
                continue;
            }
            let Some(index) = self.runtime.frame_index_for_object(object) else {
                continue;
            };
            let frame = self.runtime.frame();
            if !viewer_row_visible(frame, index) {
                continue;
            }
            if ViewerHitGeometry::for_row(frame, index, self.runtime.geometry_resources())
                .and_then(|shape| shape.hit(point, 0.0))
                == Some(false)
            {
                continue;
            }
            if self
                .runtime
                .effective_object_bounds(object)
                .is_some_and(|bounds| {
                    point.x >= bounds.min.x
                        && point.x <= bounds.max.x
                        && point.y >= bounds.min.y
                        && point.y <= bounds.max.y
                })
            {
                if let Some(target) = self.interaction_semantic_object_id(object) {
                    return Some(target);
                }
            }
        }
        None
    }

    /// Shared candidate-local query for admitted input and stationary hover.
    pub(super) fn pick_effective_fill(
        &mut self,
        point: Vec2,
        mut eligible: impl FnMut(SemanticNodeId) -> bool,
    ) -> (PointerFillOutcome, SpatialQueryStats, usize) {
        self.sync_spatial_index();
        let candidates = self.spatial_index.hit_test(point);
        let spatial_stats = candidates.stats();
        let mut outcome = PointerFillOutcome::Miss;
        let mut precise_tests = 0;
        for &slot in candidates.slots() {
            let Some(object) = self.slots.object_for_slot(slot) else {
                continue;
            };
            if Some(object) == self.camera_object {
                continue;
            }
            let Some(index) = self.runtime.frame_index_for_object(object) else {
                continue;
            };
            let Some(target) = self.interaction_semantic_object_id(object) else {
                continue;
            };
            let frame = self.runtime.frame();
            let state = &frame.objects[index];
            if !frame.is_present(index)
                || state.appearance <= 0.0
                || state.style.opacity <= 0.0
                || !state.style.fill.is_some_and(|color| color.alpha > 0.0)
                || !eligible(target)
            {
                continue;
            }
            precise_tests += 1;
            match effective_fill_contains(frame, index, point, self.runtime.geometry_resources()) {
                Ok(false) => continue,
                Ok(true) => outcome = PointerFillOutcome::Hit(target),
                Err(reason) => outcome = PointerFillOutcome::Unsupported { target, reason },
            }
            break;
        }
        (outcome, spatial_stats, precise_tests)
    }
}

/// Geometry for the candidate-local portion of a viewer hit. An open path
/// must not intercept clicks through the unpainted area of its bounds.
#[derive(PartialEq)]
pub(super) enum ViewerHitGeometry {
    Fill(GeometryRef, Transform2D),
    Stroke {
        geometry: GeometryRef,
        transform: Transform2D,
        radius: f32,
        width_mode: StrokeWidthMode,
        reveal: f32,
    },
}

impl ViewerHitGeometry {
    pub(super) fn for_row(
        frame: &FrameState,
        index: usize,
        resources: &(impl GeometryResourceLookup + ?Sized),
    ) -> Option<Self> {
        let geometry = match frame.render_geometry(index)? {
            GeometryRef::External(id) => match resources.get(resources.current_handle(*id)?)? {
                GeometryResource::VectorPath(path) => GeometryRef::VectorPath(path.clone()),
                GeometryResource::Mesh(_) => return None,
            },
            geometry => geometry.clone(),
        };
        let style = frame.objects[index].style;
        let transform = frame.render_transform(index);
        let reveal = frame.reveal(index);
        let filled = style.fill.is_some_and(|color| color.alpha > 0.0);
        if frame.morph(index) != 0.0 {
            return None;
        }
        if matches!(geometry, GeometryRef::Line { .. })
            || (!filled && matches!(geometry, GeometryRef::VectorPath(_)))
        {
            let width = if style.stroke.is_some_and(|color| color.alpha > 0.0) {
                style.stroke_width
            } else {
                0.0
            };
            let background = if style
                .background_stroke
                .is_some_and(|color| color.alpha > 0.0)
            {
                style.background_stroke_width
            } else {
                0.0
            };
            Some(Self::Stroke {
                geometry,
                transform,
                radius: width.max(background) * 0.5,
                width_mode: style.stroke_width_mode,
                reveal,
            })
        } else {
            (filled && reveal == 1.0).then_some(Self::Fill(geometry, transform))
        }
    }

    pub(super) fn hit(&self, point: Vec2, slop: f64) -> Option<bool> {
        match self {
            Self::Fill(geometry, transform) => {
                geometry_fill_contains(Some(geometry), *transform, point, None).ok()
            }
            Self::Stroke {
                geometry,
                transform,
                radius,
                width_mode,
                reveal,
            } => {
                if *radius <= 0.0 {
                    return Some(false);
                }
                let Some(mut point) = local_fill_point(*transform, point) else {
                    return Some(false);
                };
                let mut scale = [1.0; 2];
                let mut radius = f64::from(*radius);
                if *width_mode == StrokeWidthMode::ScreenSpace {
                    radius += slop;
                    scale = [f64::from(transform.scale.x), f64::from(transform.scale.y)];
                    point = [point[0] * scale[0], point[1] * scale[1]];
                } else {
                    radius +=
                        slop / f64::from(transform.scale.x.abs().min(transform.scale.y.abs()));
                }
                match geometry {
                    GeometryRef::Line { start, end } => {
                        if *reveal <= 0.0 {
                            return Some(false);
                        }
                        let a = [f64::from(start.x) * scale[0], f64::from(start.y) * scale[1]];
                        let b = [f64::from(end.x) * scale[0], f64::from(end.y) * scale[1]];
                        let t = f64::from(*reveal);
                        let b = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                        Some(noon_geometry::point_near_stroke_segment(
                            point, a, b, radius,
                        ))
                    }
                    GeometryRef::VectorPath(path) => {
                        noon_geometry::open_path_stroke_hit(path, point, scale, radius, *reveal)
                    }
                    _ => unreachable!("only open-capable geometry enters stroke picking"),
                }
            }
        }
    }
}

pub(super) fn viewer_row_visible(frame: &FrameState, index: usize) -> bool {
    let row = &frame.objects[index];
    frame.is_drawable(index)
        && row.appearance > 0.0
        && row.style.opacity > 0.0
        && (row.style.fill.is_some_and(|color| color.alpha > 0.0)
            || (row.style.stroke_width > 0.0
                && row.style.stroke.is_some_and(|color| color.alpha > 0.0))
            || row.content.text().is_some()
            || row.content.image().is_some())
}

pub(super) fn analytic_fill_contains(
    frame: &FrameState,
    index: usize,
    point: Vec2,
) -> Result<bool, PointerFillUnsupported> {
    analytic_fill_contains_with_external_path(frame, index, point, None)
}

fn effective_fill_contains(
    frame: &FrameState,
    index: usize,
    point: Vec2,
    resources: &impl GeometryResourceLookup,
) -> Result<bool, PointerFillUnsupported> {
    let external_path = match frame.render_geometry(index) {
        Some(GeometryRef::External(id)) => resources
            .current_handle(*id)
            .and_then(|handle| resources.get(handle))
            .and_then(|resource| match resource {
                GeometryResource::VectorPath(path) => Some(path.as_ref()),
                GeometryResource::Mesh(_) => None,
            }),
        _ => None,
    };
    analytic_fill_contains_with_external_path(frame, index, point, external_path)
}

fn analytic_fill_contains_with_external_path(
    frame: &FrameState,
    index: usize,
    point: Vec2,
    external_path: Option<&noon_core::VectorPath>,
) -> Result<bool, PointerFillUnsupported> {
    if frame.reveal(index) != 1.0 || frame.morph(index) != 0.0 {
        return Err(PointerFillUnsupported::PartialGeometry);
    }
    geometry_fill_contains(
        frame.render_geometry(index),
        frame.render_transform(index),
        point,
        external_path,
    )
}

pub(super) fn geometry_fill_contains(
    geometry: Option<&GeometryRef>,
    transform: noon_core::Transform2D,
    point: Vec2,
    external_path: Option<&noon_core::VectorPath>,
) -> Result<bool, PointerFillUnsupported> {
    let [x, y] =
        local_fill_point(transform, point).ok_or(PointerFillUnsupported::DegenerateGeometry)?;
    match geometry {
        Some(GeometryRef::Circle { radius }) if radius.is_finite() && *radius != 0.0 => {
            Ok(x.hypot(y) <= f64::from(radius.abs()))
        }
        Some(GeometryRef::Rectangle { size })
            if size.x.is_finite() && size.y.is_finite() && size.x != 0.0 && size.y != 0.0 =>
        {
            Ok(
                x.abs() <= f64::from(size.x.abs()) * 0.5
                    && y.abs() <= f64::from(size.y.abs()) * 0.5,
            )
        }
        Some(GeometryRef::Circle { .. } | GeometryRef::Rectangle { .. }) => {
            Err(PointerFillUnsupported::DegenerateGeometry)
        }
        Some(GeometryRef::VectorPath(path)) => noon_geometry::polygon_fill_contains(path, [x, y])
            .ok_or(PointerFillUnsupported::Content),
        Some(GeometryRef::External(_)) => external_path
            .and_then(|path| noon_geometry::polygon_fill_contains(path, [x, y]))
            .ok_or(PointerFillUnsupported::Content),
        _ => Err(PointerFillUnsupported::Content),
    }
}

fn local_fill_point(transform: Transform2D, point: Vec2) -> Option<[f64; 2]> {
    if transform.scale.x == 0.0
        || transform.scale.y == 0.0
        || !transform.scale.x.is_finite()
        || !transform.scale.y.is_finite()
        || !transform.rotation.is_finite()
        || !transform.translation.x.is_finite()
        || !transform.translation.y.is_finite()
    {
        return None;
    }
    // Widen before subtraction/division so valid f32 coordinates and very small
    // invertible scales do not overflow during geometric observation.
    let dx = f64::from(point.x) - f64::from(transform.translation.x);
    let dy = f64::from(point.y) - f64::from(transform.translation.y);
    let (sin, cos) = f64::from(transform.rotation).sin_cos();
    let x = (cos * dx + sin * dy) / f64::from(transform.scale.x);
    let y = (-sin * dx + cos * dy) / f64::from(transform.scale.y);
    Some([x, y])
}

#[cfg(test)]
mod tests;
