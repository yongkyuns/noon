from pathlib import Path

# Geometry-level review: fill-only support and endpoint fidelity against the
# normal static Lyon fill path. These tests deliberately exercise the render
# mesh, not only the correspondence planner.
path = Path("crates/noon-geometry/tests/filled_morph.rs")
text = path.read_text()
marker = "\n#[test]\nfn self_intersecting_target_is_rejected()"
if marker not in text:
    raise SystemExit("filled morph review insertion marker missing")
addition = r'''

fn fill_mesh_area(mesh: &noon_geometry::TessellatedPath, target: bool) -> f32 {
    mesh.indices
        .chunks_exact(3)
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
            let a = if target { a.target_position } else { a.position };
            let b = if target { b.target_position } else { b.position };
            let c = if target { c.target_position } else { c.position };
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
    let mesh = tessellate_styled_with_fill(
        &source,
        0.0,
        StrokeJoin::Round,
        StrokeCap::Round,
        true,
    )
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
    let static_source = tessellate_styled_with_fill(
        &source,
        0.0,
        StrokeJoin::Round,
        StrokeCap::Round,
        true,
    )
    .expect("static source fill");
    let static_target = tessellate_styled_with_fill(
        &target,
        0.0,
        StrokeJoin::Round,
        StrokeCap::Round,
        true,
    )
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
'''
text = text.replace(marker, addition + marker, 1)
path.write_text(text)

# Renderer-boundary review: fill participation must be part of mesh identity,
# otherwise a style-only fill edit can incorrectly reuse a stroke-only mesh.
path = Path("crates/noon-render-wgpu/src/lib.rs")
text = path.read_text()
marker = "\n    #[test]\n    fn packed_instance_layout_is_stable()"
if marker not in text:
    raise SystemExit("renderer review insertion marker missing")
addition = r'''

    #[test]
    fn fill_presence_is_part_of_path_mesh_cache_identity() {
        let geometry = GeometryRef::path(curved_path());
        let mut path = object(17, geometry);
        path.style.fill = None;
        path.style.stroke = Some(Color::WHITE);
        path.style.stroke_width = 0.08;
        let initial = frame(vec![path]);
        let mut preparer = FramePreparer::new();

        let cold = preparer.prepare(&initial);
        assert_eq!(cold.stats.geometry_cache_misses, 1);
        assert_eq!(preparer.cached_path_mesh_count(), 1);

        let mut filled = initial.clone();
        filled.objects[0].style.fill = Some(Color::WHITE);
        let changes = FrameChanges::objects(vec![0]);
        let rebuilt = preparer.prepare_incremental(&filled, &changes);
        assert_eq!(rebuilt.stats.geometry_cache_misses, 1);
        assert!(rebuilt.path_geometry_dirty);
        assert!(rebuilt.path_vertices.iter().any(|vertex| vertex.surface & 1 == 0));
        assert_eq!(preparer.cached_path_mesh_count(), 2);
    }
'''
text = text.replace(marker, addition + marker, 1)
path.write_text(text)
