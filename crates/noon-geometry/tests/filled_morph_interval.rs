use noon_core::{Vec2, VectorPath};
use noon_geometry::{plan_filled_morph, FilledMorphError, MorphOptions};

fn polygon(points: &[(f32, f32)]) -> VectorPath {
    let mut path = VectorPath::new().move_to(Vec2::new(points[0].0, points[0].1));
    for &(x, y) in &points[1..] {
        path = path.line_to(Vec2::new(x, y));
    }
    path.close()
}

#[test]
fn safe_endpoints_with_interior_fan_inversion_are_rejected() {
    // Both endpoint polygons are simple and individually star-shaped around
    // their area centroids. With this deterministic correspondence, however,
    // one center-fan triangle crosses through zero signed area near the middle
    // of the animation. Endpoint-only validation would therefore accept an
    // unsafe fixed topology; the continuous quadratic-area certificate must
    // reject it.
    let source = polygon(&[
        (-1.26448, 1.30265),
        (-1.16587, 0.68960),
        (-0.27352, -0.34754),
        (-0.78441, -1.27173),
        (2.10721, -0.52243),
    ]);
    let target = polygon(&[
        (1.80403, 1.08663),
        (-0.93748, -0.27848),
        (0.44549, -1.80117),
        (0.63486, -1.29563),
        (1.31780, -0.78932),
    ]);

    assert!(matches!(
        plan_filled_morph(
            &source,
            &target,
            MorphOptions {
                samples_per_contour: 5,
                ..MorphOptions::DEFAULT
            },
        ),
        Err(FilledMorphError::NoStableFanTriangulation)
    ));
}
