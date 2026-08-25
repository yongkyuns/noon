use noon_core::{GeometryRef, Vec2, VectorPath};

/// Convert renderer-supported geometry to a deterministic vector outline.
///
/// This is intended for transient path-level effects such as `Create`: analytic
/// primitives stay analytic in semantic/runtime state and are converted only
/// while a renderer needs ordered path progress.
pub fn canonical_outline_path(geometry: &GeometryRef) -> Option<VectorPath> {
    match geometry {
        GeometryRef::Circle { radius } => Some(circle_path(*radius)),
        GeometryRef::Rectangle { size } => Some(rectangle_path(*size)),
        GeometryRef::Line { start, end } => Some(VectorPath::new().move_to(*start).line_to(*end)),
        GeometryRef::VectorPath(path) => Some(path.clone()),
        GeometryRef::External(_) => None,
    }
}

fn circle_path(radius: f32) -> VectorPath {
    // Standard four-cubic approximation, matching the temporary path used by
    // cross-kind Transform. The semantic Circle itself remains analytic.
    let handle = radius * 0.552_284_8;
    VectorPath::new()
        .move_to(Vec2::new(radius, 0.0))
        .cubic_to(
            Vec2::new(radius, handle),
            Vec2::new(handle, radius),
            Vec2::new(0.0, radius),
        )
        .cubic_to(
            Vec2::new(-handle, radius),
            Vec2::new(-radius, handle),
            Vec2::new(-radius, 0.0),
        )
        .cubic_to(
            Vec2::new(-radius, -handle),
            Vec2::new(-handle, -radius),
            Vec2::new(0.0, -radius),
        )
        .cubic_to(
            Vec2::new(handle, -radius),
            Vec2::new(radius, -handle),
            Vec2::new(radius, 0.0),
        )
        .close()
}

fn rectangle_path(size: Vec2) -> VectorPath {
    let half = size * 0.5;
    // ManimCE Rectangle sets the vertices UR -> UL -> DL -> DR and closes back
    // to UR. Preserve that exact four-curve ordering because Create distributes
    // normalized progress by Bezier-curve count rather than by arc length.
    VectorPath::new()
        .move_to(Vec2::new(half.x, half.y))
        .line_to(Vec2::new(-half.x, half.y))
        .line_to(Vec2::new(-half.x, -half.y))
        .line_to(Vec2::new(half.x, -half.y))
        .close()
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryId, PathCommand};

    use super::*;

    #[test]
    fn analytic_shapes_have_stable_outline_paths() {
        for geometry in [
            GeometryRef::circle(1.25),
            GeometryRef::rectangle(2.0, 3.0),
            GeometryRef::line(Vec2::new(-1.0, 2.0), Vec2::new(3.0, -4.0)),
        ] {
            let first = canonical_outline_path(&geometry).expect("supported geometry");
            let second = canonical_outline_path(&geometry).expect("supported geometry");
            assert_eq!(first, second);
            assert!(!first.commands().is_empty());
        }
    }

    #[test]
    fn rectangle_matches_manim_curve_order() {
        let path = canonical_outline_path(&GeometryRef::rectangle(4.0, 2.0)).unwrap();
        assert_eq!(path.commands().len(), 5);
        let expected = [
            Vec2::new(2.0, 1.0),
            Vec2::new(-2.0, 1.0),
            Vec2::new(-2.0, -1.0),
            Vec2::new(2.0, -1.0),
        ];
        match path.commands()[0] {
            PathCommand::MoveTo { to } => assert_eq!(to, expected[0]),
            other => panic!("unexpected first command: {other:?}"),
        }
        for (command, expected) in path.commands()[1..4].iter().zip(expected[1..].iter()) {
            match command {
                PathCommand::LineTo { to } => assert_eq!(to, expected),
                other => panic!("unexpected edge command: {other:?}"),
            }
        }
        assert_eq!(path.commands()[4], PathCommand::Close);
    }

    #[test]
    fn vector_path_is_preserved_exactly() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .quadratic_to(Vec2::new(0.5, 1.0), Vec2::ONE);
        assert_eq!(
            canonical_outline_path(&GeometryRef::path(path.clone())),
            Some(path)
        );
    }

    #[test]
    fn external_geometry_has_no_implicit_outline() {
        assert!(canonical_outline_path(&GeometryRef::External(GeometryId::new(7))).is_none());
    }
}
