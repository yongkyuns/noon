use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{
    plan_filled_morph, tessellate_styled_with_fill, FilledMorphError, MorphOptions, PathSurface,
};

fn rounded_loop() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 1.6))
        .cubic_to(
            Vec2::new(0.95, 1.6),
            Vec2::new(1.6, 0.95),
            Vec2::new(1.6, 0.0),
        )
        .cubic_to(
            Vec2::new(1.6, -0.95),
            Vec2::new(0.95, -1.6),
            Vec2::new(0.0, -1.6),
        )
        .cubic_to(
            Vec2::new(-0.95, -1.6),
            Vec2::new(-1.6, -0.95),
            Vec2::new(-1.6, 0.0),
        )
        .cubic_to(
            Vec2::new(-1.6, 0.95),
            Vec2::new(-0.95, 1.6),
            Vec2::new(0.0, 1.6),
        )
        .close()
}

fn star() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 2.0))
        .line_to(Vec2::new(0.47, 0.65))
        .line_to(Vec2::new(1.9, 0.62))
        .line_to(Vec2::new(0.76, -0.25))
        .line_to(Vec2::new(1.18, -1.62))
        .line_to(Vec2::new(0.0, -0.82))
        .line_to(Vec2::new(-1.18, -1.62))
        .line_to(Vec2::new(-0.76, -0.25))
        .line_to(Vec2::new(-1.9, 0.62))
        .line_to(Vec2::new(-0.47, 0.65))
        .close()
}

fn triangle_area(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[test]
fn rounded_loop_to_concave_star_has_stable_fill_topology() {
    let plan = plan_filled_morph(&rounded_loop(), &star(), MorphOptions::DEFAULT)
        .expect("regular star is star-shaped around its centroid");
    assert_eq!(plan.indices.len(), plan.contour.source_points.len() * 3);
    assert_eq!(plan.vertex_count(), plan.contour.source_points.len() + 1);

    for progress in [0.0, 0.125, 0.25, 0.5, 0.75, 0.875, 1.0] {
        let vertices = plan.interpolate_vertices(progress);
        for triangle in plan.indices.as_chunks::<3>().0 {
            let a = vertices[triangle[0] as usize];
            let b = vertices[triangle[1] as usize];
            let c = vertices[triangle[2] as usize];
            assert!(
                triangle_area(a, b, c) > 1.0e-5,
                "triangle inverted at {progress}"
            );
        }
    }
}

#[test]
fn filled_morph_tessellation_contains_fill_and_stroke_with_one_topology() {
    let source = rounded_loop().with_morph_target(star());
    let mesh =
        tessellate_styled_with_fill(&source, 0.12, StrokeJoin::Round, StrokeCap::Round, true)
            .expect("safe filled morph must tessellate");

    assert!(mesh.morphing);
    assert!(mesh
        .vertices
        .iter()
        .any(|vertex| vertex.surface == PathSurface::Fill));
    assert!(mesh
        .vertices
        .iter()
        .any(|vertex| vertex.surface == PathSurface::Stroke));
    assert!(mesh
        .vertices
        .iter()
        .any(|vertex| vertex.position != vertex.target_position));
    assert!(mesh
        .indices
        .iter()
        .all(|index| (*index as usize) < mesh.vertices.len()));
}

fn fill_mesh_area(mesh: &noon_geometry::TessellatedPath, target: bool) -> f32 {
    mesh.indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|triangle| {
            let a = &mesh.vertices[triangle[0] as usize];
            let b = &mesh.vertices[triangle[1] as usize];
            let c = &mesh.vertices[triangle[2] as usize];
            if a.surface != PathSurface::Fill
                || b.surface != PathSurface::Fill
                || c.surface != PathSurface::Fill
            {
                return None;
            }
            let a = if target {
                a.target_position
            } else {
                a.position
            };
            let b = if target {
                b.target_position
            } else {
                b.position
            };
            let c = if target {
                c.target_position
            } else {
                c.position
            };
            Some(triangle_area(a, b, c).abs() * 0.5)
        })
        .sum()
}

fn assert_relative_close(actual: f32, expected: f32, tolerance: f32) {
    let scale = expected.abs().max(1.0e-5);
    let relative = (actual - expected).abs() / scale;
    assert!(
        relative <= tolerance,
        "actual={actual}, expected={expected}, relative error={relative}, tolerance={tolerance}"
    );
}

#[test]
fn fill_only_morph_emits_fixed_fill_mesh_without_stroke_vertices() {
    let source = rounded_loop().with_morph_target(star());
    let mesh = tessellate_styled_with_fill(&source, 0.0, StrokeJoin::Round, StrokeCap::Round, true)
        .expect("safe fill-only morph must tessellate");

    assert!(mesh.morphing);
    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.indices.is_empty());
    assert!(mesh
        .vertices
        .iter()
        .all(|vertex| vertex.surface == PathSurface::Fill));
    assert!(mesh
        .vertices
        .iter()
        .any(|vertex| vertex.position != vertex.target_position));
}

#[test]
fn fixed_fill_endpoints_match_static_lyon_fill_area_within_tolerance() {
    let source = rounded_loop();
    let target = star();
    let static_source =
        tessellate_styled_with_fill(&source, 0.0, StrokeJoin::Round, StrokeCap::Round, true)
            .expect("static source fill");
    let static_target =
        tessellate_styled_with_fill(&target, 0.0, StrokeJoin::Round, StrokeCap::Round, true)
            .expect("static target fill");
    let morph = tessellate_styled_with_fill(
        &source.with_morph_target(target),
        0.0,
        StrokeJoin::Round,
        StrokeCap::Round,
        true,
    )
    .expect("safe filled morph");

    assert_relative_close(
        fill_mesh_area(&morph, false),
        fill_mesh_area(&static_source, false),
        0.02,
    );
    assert_relative_close(
        fill_mesh_area(&morph, true),
        fill_mesh_area(&static_target, false),
        0.02,
    );
}

#[test]
fn self_intersecting_target_is_rejected() {
    let bow_tie = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, 1.0))
        .line_to(Vec2::new(-1.0, 1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .close();
    assert!(matches!(
        plan_filled_morph(&rounded_loop(), &bow_tie, MorphOptions::DEFAULT),
        Err(FilledMorphError::SelfIntersecting { .. })
            | Err(FilledMorphError::DegenerateArea { .. })
            | Err(FilledMorphError::NoStableFanTriangulation)
    ));
}

#[test]
fn open_or_multi_contour_fill_is_rejected() {
    let open = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
        .line_to(Vec2::new(0.0, 1.0));
    assert!(matches!(
        plan_filled_morph(&open, &open, MorphOptions::DEFAULT),
        Err(FilledMorphError::RequiresSingleClosedContour)
    ));
}
