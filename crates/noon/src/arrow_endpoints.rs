//! Manim-compatible constructor-time Arrow endpoint resolution.
//!
//! Manim resolves Mobject endpoints once when a Line/Arrow is constructed: it
//! computes the rough center-to-center direction, then asks each VMobject for the
//! boundary anchor furthest along that direction. The source Mobjects are not a
//! retained dependency afterwards. Keep that rule in shared Rust so frontends do
//! not reconstruct geometry or silently substitute analytic/AABB intersections.

use crate::{AuthoringError, ManimLineEndpoints, Mobject};
use noon_geometry::PathProportionError;

/// Resolve two Mobject endpoints exactly once using ManimCE v0.21 VMobject
/// boundary-anchor semantics.
pub fn manim_arrow_endpoints_from_mobjects(
    start: &Mobject,
    end: &Mobject,
) -> Result<ManimLineEndpoints, AuthoringError> {
    start.require_same_store(end)?;
    resolve_endpoints(Endpoint::Mobject(start), Endpoint::Mobject(end))
}

/// Resolve a Mobject start and numeric end exactly once at Arrow construction.
pub fn manim_arrow_endpoints_from_mobject(
    start: &Mobject,
    end_x: f64,
    end_y: f64,
) -> Result<ManimLineEndpoints, AuthoringError> {
    resolve_endpoints(
        Endpoint::Mobject(start),
        Endpoint::Point(checked_point("arrow rough end", end_x, end_y)?),
    )
}

/// Resolve a numeric start and Mobject end exactly once at Arrow construction.
pub fn manim_arrow_endpoints_to_mobject(
    start_x: f64,
    start_y: f64,
    end: &Mobject,
) -> Result<ManimLineEndpoints, AuthoringError> {
    resolve_endpoints(
        Endpoint::Point(checked_point("arrow rough start", start_x, start_y)?),
        Endpoint::Mobject(end),
    )
}

#[derive(Clone, Copy)]
enum Endpoint<'a> {
    Point((f64, f64)),
    Mobject(&'a Mobject),
}

impl Endpoint<'_> {
    fn rough_center(self) -> Result<(f64, f64), AuthoringError> {
        match self {
            Self::Point(point) => Ok(point),
            Self::Mobject(mobject) => mobject.center(),
        }
    }

    fn resolve(self, direction: (f64, f64)) -> Result<(f64, f64), AuthoringError> {
        match self {
            Self::Point(point) => Ok(point),
            Self::Mobject(mobject) => mobject.manim_boundary_point(direction),
        }
    }
}

fn resolve_endpoints(
    start: Endpoint<'_>,
    end: Endpoint<'_>,
) -> Result<ManimLineEndpoints, AuthoringError> {
    let rough_start = start.rough_center()?;
    let rough_end = end.rough_center()?;
    let dx = rough_end.0 - rough_start.0;
    let dy = rough_end.1 - rough_start.1;
    let length = dx.hypot(dy);
    let direction = if length == 0.0 {
        // Manim's normalize() zero fallback is the zero vector. Boundary argmax
        // then chooses the first defining anchor for both endpoints.
        (0.0, 0.0)
    } else {
        (dx / length, dy / length)
    };
    Ok(ManimLineEndpoints {
        start: start.resolve(direction)?,
        end: end.resolve((-direction.0, -direction.1))?,
    })
}

impl Mobject {
    /// ManimCE v0.21 `VMobject.get_boundary_point()` over authored world-space
    /// anchors. Ties preserve the first anchor, matching NumPy `argmax`.
    pub fn manim_boundary_point(
        &self,
        direction: (f64, f64),
    ) -> Result<(f64, f64), AuthoringError> {
        let direction = checked_point("boundary direction", direction.0, direction.1)?;
        let anchors = self.path_query()?.anchors();
        let Some(mut best) = anchors.first().copied() else {
            return Err(AuthoringError::PathQuery(PathProportionError::EmptyPath));
        };
        let mut best_projection = dot(best, direction);
        for point in anchors.into_iter().skip(1) {
            let projection = dot(point, direction);
            if projection > best_projection {
                best = point;
                best_projection = projection;
            }
        }
        Ok(best)
    }
}

fn dot(point: (f64, f64), direction: (f64, f64)) -> f64 {
    point.0 * direction.0 + point.1 * direction.1
}

fn checked_point(name: &str, x: f64, y: f64) -> Result<(f64, f64), AuthoringError> {
    crate::integration::authoring_render_f64(&format!("{name}.x"), x)?;
    crate::integration::authoring_render_f64(&format!("{name}.y"), y)?;
    Ok((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;

    fn assert_point(actual: (f64, f64), expected: (f64, f64)) {
        assert!(
            (actual.0 - expected.0).abs() < 1.0e-6 && (actual.1 - expected.1).abs() < 1.0e-6,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn mobject_pair_resolves_center_direction_then_vmobject_boundaries() {
        let scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut square = scene.square(2.0).unwrap();
        square.shift(4.0, 0.0).unwrap();

        let endpoints = manim_arrow_endpoints_from_mobjects(&circle, &square).unwrap();
        assert_point(endpoints.start, (1.0, 0.0));
        // For -x the square's upper-left/lower-left anchors tie. Manim/NumPy
        // argmax preserves the first defining anchor, which is upper-left.
        assert_point(endpoints.end, (3.0, 1.0));
    }

    #[test]
    fn off_axis_circle_uses_discrete_manim_anchor_not_analytic_intersection() {
        let scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let angle = 20.0_f64.to_radians();
        let endpoints =
            manim_arrow_endpoints_from_mobject(&circle, 10.0 * angle.cos(), 10.0 * angle.sin())
                .unwrap();

        // Manim Circle has eight cubic segments. At 20 degrees the +x anchor
        // wins the directional argmax; an analytic circle intersection would not.
        assert_point(endpoints.start, (1.0, 0.0));
    }

    #[test]
    fn coincident_centers_preserve_first_anchor_argmax_fallback() {
        let scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let square = scene.square(2.0).unwrap();
        let endpoints = manim_arrow_endpoints_from_mobjects(&circle, &square).unwrap();

        assert_point(endpoints.start, (1.0, 0.0));
        // Rectangle canonical path begins at upper-right, matching first-anchor
        // selection when the direction is exactly zero.
        assert_point(endpoints.end, (1.0, 1.0));
    }

    #[test]
    fn object_pair_rejects_foreign_store_before_resolution() {
        let first = Scene::new();
        let second = Scene::new();
        let start = first.circle(1.0).unwrap();
        let end = second.square(1.0).unwrap();

        assert_eq!(
            manim_arrow_endpoints_from_mobjects(&start, &end),
            Err(AuthoringError::ForeignStore)
        );
    }
}
