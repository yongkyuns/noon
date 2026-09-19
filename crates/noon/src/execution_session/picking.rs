//! Publication-qualified, candidate-local picking of analytic fill interiors.
//!
//! This first C5 query observes the existing session/runtime/index. It owns no
//! scene, input queue, geometry cache, selection state, or platform mechanics.

use noon_core::{GeometryRef, NativePointerInput, PublicationContext, SemanticNodeId, Vec2};
use noon_runtime::{FrameState, SpatialQueryStats};

use super::{ExecutionSession, ExecutionSessionInputError, NativePointerInputToken};

/// Why a potentially filled candidate cannot be decided by the initial picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerFillUnsupported {
    /// Paths, lines, text, images and externally supplied content need another
    /// explicit hit policy; their bounds are not a precise fill hit.
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
    /// Paths, text/image masks, lines, partial reveals/morphs and degenerate
    /// geometry return `Unsupported` when encountered as an eligible potentially
    /// filled candidate. The query never calls their bounding box a precise hit.
    /// Groups are traversed by the existing lowering/index; returned identities
    /// are semantic leaves, never execution slots or frontend wrapper IDs.
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
        self.sync_spatial_index();
        let candidates = self.spatial_index.hit_test(position.scene());
        result.spatial_stats = candidates.stats();
        result.outcome = PointerFillOutcome::Miss;
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
            let Some(target) = self.execution_index.semantic_object_id(object) else {
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
            result.precise_tests += 1;
            match analytic_fill_contains(frame, index, position.scene()) {
                Ok(false) => continue,
                Ok(true) => result.outcome = PointerFillOutcome::Hit(target),
                Err(reason) => result.outcome = PointerFillOutcome::Unsupported { target, reason },
            }
            break;
        }
        Ok(result)
    }
}

fn analytic_fill_contains(
    frame: &FrameState,
    index: usize,
    point: Vec2,
) -> Result<bool, PointerFillUnsupported> {
    if frame.reveal(index) != 1.0 || frame.morph(index) != 0.0 {
        return Err(PointerFillUnsupported::PartialGeometry);
    }
    let transform = frame.render_transform(index);
    if transform.scale.x == 0.0
        || transform.scale.y == 0.0
        || !transform.scale.x.is_finite()
        || !transform.scale.y.is_finite()
        || !transform.rotation.is_finite()
        || !transform.translation.x.is_finite()
        || !transform.translation.y.is_finite()
    {
        return Err(PointerFillUnsupported::DegenerateGeometry);
    }
    // Widen before subtraction/division so valid f32 coordinates and very small
    // invertible scales do not overflow during geometric observation.
    let dx = f64::from(point.x) - f64::from(transform.translation.x);
    let dy = f64::from(point.y) - f64::from(transform.translation.y);
    let (sin, cos) = f64::from(transform.rotation).sin_cos();
    let x = (cos * dx + sin * dy) / f64::from(transform.scale.x);
    let y = (-sin * dx + cos * dy) / f64::from(transform.scale.y);
    match frame.render_geometry(index) {
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
        _ => Err(PointerFillUnsupported::Content),
    }
}

#[cfg(test)]
mod tests;
