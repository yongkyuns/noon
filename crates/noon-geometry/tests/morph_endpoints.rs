use noon_core::Vec2;
use noon_geometry::{MorphContourPlan, MorphPlan};

fn cancellation_prone_plan() -> MorphPlan {
    MorphPlan {
        contours: vec![MorphContourPlan {
            source_points: vec![Vec2::new(7.579_909e25, -3.25)],
            target_points: vec![Vec2::new(-1.104_579_8, 9.75)],
            closed: false,
        }],
    }
}

#[test]
fn interpolation_returns_bit_exact_planned_endpoints_after_clamping() {
    let plan = cancellation_prone_plan();
    let contour = &plan.contours[0];

    for progress in [-10.0, 0.0] {
        assert_eq!(
            plan.interpolate(progress).contours[0].points,
            contour.source_points,
            "progress {progress} must return the stored source endpoint exactly"
        );
    }
    for progress in [1.0, 10.0] {
        assert_eq!(
            plan.interpolate(progress).contours[0].points,
            contour.target_points,
            "progress {progress} must return the stored target endpoint exactly"
        );
    }
}

#[test]
fn interior_interpolation_still_uses_finite_linear_arithmetic() {
    let plan = cancellation_prone_plan();
    let source = plan.contours[0].source_points[0];
    let target = plan.contours[0].target_points[0];
    let progress = 0.25;
    let expected = Vec2::new(
        source.x + (target.x - source.x) * progress,
        source.y + (target.y - source.y) * progress,
    );
    let actual = plan.interpolate(progress).contours[0].points[0];

    assert_eq!(actual, expected);
    assert!(actual.x.is_finite());
    assert!(actual.y.is_finite());
}
