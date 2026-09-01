use noon_core::Vec2;
use noon_geometry::smooth_cubic_bezier_handles;

fn assert_point(actual: Vec2, expected: Vec2) {
    assert!(
        (actual - expected).length() <= 1.0e-5,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn negative_start_closure_matches_manim_v021_tolerance_asymmetry() {
    // Manim v0.21's bezier.is_closed intentionally computes
    // tolerance = atol + rtol * start, without abs(start). With a negative
    // starting x coordinate this makes even an exactly repeated endpoint fail
    // the closed-spline test, so the open-spline solver must be selected.
    let anchors = [
        Vec2::new(-1.0, 0.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(-1.0, 0.0),
    ];

    let (first, second) = smooth_cubic_bezier_handles(&anchors).unwrap();

    let expected_first = [
        Vec2::new(-0.60714287, -0.10714286),
        Vec2::new(0.21428572, 0.21428572),
        Vec2::new(-0.25, 1.25),
        Vec2::new(-1.2142857, 0.78571427),
    ];
    let expected_second = [
        Vec2::new(-0.21428572, -0.21428572),
        Vec2::new(0.25, 0.75),
        Vec2::new(-0.78571427, 1.2142857),
        Vec2::new(-1.1071428, 0.39285713),
    ];

    for index in 0..4 {
        assert_point(first[index], expected_first[index]);
        assert_point(second[index], expected_second[index]);
    }
}
