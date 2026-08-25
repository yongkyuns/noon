use noon_core::{GeometryRef, Vec2};
use noon_geometry::{
    canonical_outline_path, plan_morph, plan_morph_preserving_order, MorphOptions,
};

fn assert_vec2_close(actual: Vec2, expected: Vec2) {
    const EPSILON: f32 = 1.0e-4;
    assert!(
        (actual.x - expected.x).abs() <= EPSILON && (actual.y - expected.y).abs() <= EPSILON,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn ordered_morph_preserves_manim_circle_start_anchor() {
    // Manim Square starts at UR while Circle starts at RIGHT. Transform aligns
    // cubic point arrays by index; it does not rotate the closed circle contour
    // to put its first point nearest the square's first point.
    let square = canonical_outline_path(&GeometryRef::square(2.0)).expect("square outline");
    let circle = canonical_outline_path(&GeometryRef::circle(1.0)).expect("circle outline");

    let native = plan_morph(&square, &circle, MorphOptions::DEFAULT).expect("native morph");
    let ordered = plan_morph_preserving_order(&square, &circle, MorphOptions::DEFAULT)
        .expect("ordered morph");

    assert_eq!(native.contours.len(), 1);
    assert_eq!(ordered.contours.len(), 1);
    assert_vec2_close(ordered.contours[0].target_points[0], Vec2::new(1.0, 0.0));

    // Native Noon's minimum-distance alignment intentionally chooses a different
    // cyclic correspondence for this pair. Keep that behavior unchanged.
    assert!(
        (native.contours[0].target_points[0] - ordered.contours[0].target_points[0]).length()
            > 0.25
    );
}
