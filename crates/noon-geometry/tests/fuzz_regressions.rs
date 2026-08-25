use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{plan_morph, tessellate_styled_with_fill, MorphOptions};

fn assert_mesh_is_well_formed(path: &VectorPath, fill: bool) {
    let mesh = tessellate_styled_with_fill(path, 0.04, StrokeJoin::Round, StrokeCap::Round, fill)
        .expect("fixed regression geometry should tessellate");
    assert!(mesh
        .indices
        .iter()
        .all(|&index| (index as usize) < mesh.vertices.len()));
    assert!(mesh.vertices.iter().all(|vertex| {
        vertex.position.x.is_finite()
            && vertex.position.y.is_finite()
            && vertex.target_position.x.is_finite()
            && vertex.target_position.y.is_finite()
            && vertex.path_distance.is_finite()
            && vertex.path_progress.is_finite()
    }));
}

#[test]
fn fixed_degenerate_and_self_intersecting_path_corpus_stays_safe() {
    let repeated = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 0.0));
    let collinear = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 0.0));
    let bow_tie = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .close();

    for path in [&repeated, &collinear, &bow_tie] {
        let result =
            tessellate_styled_with_fill(path, 0.04, StrokeJoin::Round, StrokeCap::Round, true);
        if result.is_ok() {
            assert_mesh_is_well_formed(path, true);
        }
    }
}

#[test]
fn fixed_morph_corpus_is_finite_or_rejected_cleanly() {
    let source = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0))
        .close();
    let target = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
        .line_to(Vec2::new(0.0, 0.0))
        .close();

    if let Ok(plan) = plan_morph(&source, &target, MorphOptions::default()) {
        for progress in [0.0, 0.5, 1.0] {
            let frame = plan.interpolate(progress);
            assert!(frame.contours.iter().all(|contour| contour
                .points
                .iter()
                .all(|point| { point.x.is_finite() && point.y.is_finite() })));
        }
    }
}
