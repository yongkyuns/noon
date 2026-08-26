use noon_core::{PathCommand, Vec2, VectorPath};
use noon_geometry::pointwise_partial_path;

fn assert_vec2(actual: Vec2, expected: Vec2) {
    assert!(
        (actual.x - expected.x).abs() < 1e-6 && (actual.y - expected.y).abs() < 1e-6,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn partial_reveal_preserves_subpath_break_when_crossing_into_next_contour() {
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
        .move_to(Vec2::new(10.0, 0.0))
        .line_to(Vec2::new(11.0, 0.0));

    // Manim divides global reveal progress uniformly by Bezier-curve count.
    // At 0.75, the first of two curves is complete and the second is halfway.
    // The second contour must remain a separate MoveTo rather than gaining an
    // artificial connecting segment from the first contour's endpoint.
    let partial = pointwise_partial_path(&path, 0.0, 0.75);
    assert_eq!(partial.commands().len(), 4);

    match partial.commands()[0] {
        PathCommand::MoveTo { to } => assert_vec2(to, Vec2::new(0.0, 0.0)),
        ref other => panic!("unexpected command: {other:?}"),
    }
    match partial.commands()[1] {
        PathCommand::LineTo { to } => assert_vec2(to, Vec2::new(1.0, 0.0)),
        ref other => panic!("unexpected command: {other:?}"),
    }
    match partial.commands()[2] {
        PathCommand::MoveTo { to } => assert_vec2(to, Vec2::new(10.0, 0.0)),
        ref other => panic!("unexpected command: {other:?}"),
    }
    match partial.commands()[3] {
        PathCommand::LineTo { to } => assert_vec2(to, Vec2::new(10.5, 0.0)),
        ref other => panic!("unexpected command: {other:?}"),
    }
}
