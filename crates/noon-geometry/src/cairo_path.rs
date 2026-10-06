//! Cold Cairo path metadata derived from the shared canonical curve controls.

use noon_core::{CairoSurfaceAppearance, SemanticVec3, Vec2, VectorPath};

/// Manim's four control points per curve, including repeated curve endpoints.
/// These controls also define the bounds used by Cairo family gradients.
pub fn cairo_path_control_points(path: &VectorPath) -> Vec<Vec2> {
    crate::partial::cubic_contours(path)
        .into_iter()
        .flat_map(|contour| contour.curves)
        .flatten()
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CairoPathGeometry {
    /// Reuse the endpoint/span representation of Cairo Surface cells. Field
    /// names retain that public format; path corner indices depend on length.
    pub corners: CairoSurfaceAppearance,
    /// Cairo uses world UP for a single-curve path, even after world rotation.
    pub world_up_normal: bool,
}

/// Preserve pinned ManimCE 0.21 `three_d_utils` corner/normal inputs. Lighting
/// and projection remain renderer work; this runs only during path preparation.
pub fn cairo_path_geometry(path: &VectorPath) -> Option<CairoPathGeometry> {
    let points = cairo_path_control_points(path);
    if points.is_empty() || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return None;
    }
    let point = |i: usize| SemanticVec3::from_vec2(points[i]);
    let relative = |a: usize, b: usize| {
        let a = point(a);
        let b = point(b);
        SemanticVec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
    };
    let end = ((points.len() - 1) / 6) * 3;
    let spans = |i: usize| {
        let previous = if i > 2 { i - 3 } else { points.len() - 4 };
        let next = if i < points.len() - 3 { i + 3 } else { 3 };
        (relative(next, i), relative(previous, i))
    };
    let (start_next, start_previous) = spans(0);
    let (end_next, end_previous) = spans(end);
    Some(CairoPathGeometry {
        corners: CairoSurfaceAppearance {
            p0: point(0),
            p6: point(end),
            span_p3_p0: start_next,
            span_p12_p0: start_previous,
            span_p9_p6: end_next,
            span_p3_p6: end_previous,
        },
        world_up_normal: points.len() == 4,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::GeometryRef;

    #[test]
    fn shaft_lighting_uses_one_corner_and_world_up() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-2., 0.))
            .line_to(Vec2::new(2., 0.));
        let geometry = cairo_path_geometry(&path).unwrap();
        assert!(geometry.world_up_normal);
        assert_eq!(geometry.corners.p0, SemanticVec3::new(-2., 0., 0.));
        assert_eq!(geometry.corners.p6, geometry.corners.p0);
    }

    #[test]
    fn closed_triangle_retains_cairo_corner_neighbors() {
        let path = VectorPath::new()
            .move_to(Vec2::new(1., 0.))
            .line_to(Vec2::new(-1., 1.))
            .line_to(Vec2::new(-1., -1.))
            .close();
        let geometry = cairo_path_geometry(&path).unwrap();
        assert!(!geometry.world_up_normal);
        assert_eq!(cairo_path_control_points(&path).len(), 12);
        assert_eq!(geometry.corners.p6, SemanticVec3::new(-1., 1., 0.));
        assert_eq!(geometry.corners.span_p3_p0, SemanticVec3::new(-2., 1., 0.));
        assert_eq!(
            geometry.corners.span_p12_p0,
            SemanticVec3::new(-2., -1., 0.)
        );
        assert_eq!(geometry.corners.span_p9_p6.x, 0.);
        assert!((geometry.corners.span_p9_p6.y + 4. / 3.).abs() < 1e-6);
        assert_eq!(geometry.corners.span_p3_p6, SemanticVec3::new(2., -1., 0.));
    }

    #[test]
    fn circle_controls_keep_all_eight_curves_and_opposite_corner() {
        let path = crate::canonical_outline_path(&GeometryRef::circle(1.)).unwrap();
        assert_eq!(cairo_path_control_points(&path).len(), 32);
        let geometry = cairo_path_geometry(&path).unwrap();
        assert!(!geometry.world_up_normal);
        assert!((geometry.corners.p6.x + 1.).abs() < 1e-6);
        assert!(geometry.corners.p6.y.abs() < 1e-6);
        assert!(cairo_path_geometry(&VectorPath::new()).is_none());
    }
}
