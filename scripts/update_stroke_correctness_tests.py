from pathlib import Path

path = Path("crates/noon-geometry/tests/tessellation_correctness.rs")
text = path.read_text()
text = text.replace(
    "use noon_core::{PathCommand, Vec2, VectorPath};\nuse noon_geometry::{tessellate, GeometryError, MeshVertex, PathSurface, TessellatedPath};",
    "use noon_core::{PathCommand, StrokeCap, StrokeJoin, Vec2, VectorPath};\nuse noon_geometry::{\n    tessellate, tessellate_styled, GeometryError, MeshVertex, PathSurface, TessellatedPath,\n};",
    1,
)

old_helpers_start = text.index("fn source_pairs(")
old_helpers_end = text.index("#[test]\nfn static_wrapper_matches_direct_lyon_reference", old_helpers_start)
new_helpers = r'''fn source_positions(mesh: &TessellatedPath) -> Vec<Vec2> {
    assert!(mesh.morphing);
    mesh.vertices.iter().map(|vertex| vertex.position).collect()
}

fn target_positions(mesh: &TessellatedPath) -> Vec<Vec2> {
    assert!(mesh.morphing);
    mesh.vertices
        .iter()
        .map(|vertex| vertex.target_position)
        .collect()
}

fn contains_point(points: &[Vec2], expected: Vec2, tolerance: f32) -> bool {
    points
        .iter()
        .any(|point| magnitude(sub(*point, expected)) <= tolerance)
}

fn canonical_positions(points: impl IntoIterator<Item = Vec2>, tolerance: f32) -> Vec<(i64, i64)> {
    let mut result: Vec<_> = points
        .into_iter()
        .map(|point| {
            (
                (point.x / tolerance).round() as i64,
                (point.y / tolerance).round() as i64,
            )
        })
        .collect();
    result.sort_unstable();
    result
}

fn self_morph(path: VectorPath, width: f32) -> TessellatedPath {
    self_morph_styled(path, width, StrokeJoin::Round, StrokeCap::Round)
}

fn self_morph_styled(
    path: VectorPath,
    width: f32,
    join: StrokeJoin,
    cap: StrokeCap,
) -> TessellatedPath {
    let target = path.clone();
    tessellate_styled(&path.with_morph_target(target), width, join, cap)
        .expect("valid styled self morph")
}

'''
text = text[:old_helpers_start] + new_helpers + text[old_helpers_end:]


def replace_test(name: str, body: str) -> None:
    global text
    start_marker = f"#[test]\nfn {name}()"
    start = text.index(start_marker)
    next_test = text.find("\n#[test]\n", start + len(start_marker))
    if next_test < 0:
        next_test = len(text)
    text = text[:start] + body.rstrip() + "\n" + text[next_test:]


replace_test(
    "morph_open_stroke_ends_are_centered_perpendicular_and_full_width",
    r'''#[test]
fn morph_open_stroke_ends_are_centered_perpendicular_and_full_width() {
    let start = Vec2::new(0.0, 0.0);
    let end = Vec2::new(3.0, 4.0);
    let width = 0.6;
    let tangent = normalized(sub(end, start));
    let normal = Vec2::new(-tangent.y, tangent.x);
    let length = magnitude(sub(end, start));
    let mesh = self_morph_styled(
        VectorPath::new().move_to(start).line_to(end),
        width,
        StrokeJoin::Round,
        StrokeCap::Butt,
    );
    let positions = source_positions(&mesh);

    for (center, along_expected) in [(start, 0.0), (end, length)] {
        let cross_section: Vec<_> = positions
            .iter()
            .copied()
            .filter(|point| {
                let relative = sub(*point, start);
                (dot(relative, tangent) - along_expected).abs() < EPS
                    && dot(sub(*point, center), normal).abs() > EPS
            })
            .collect();
        assert!(cross_section.len() >= 2);
        let min = cross_section
            .iter()
            .map(|point| dot(sub(*point, center), normal))
            .fold(f32::INFINITY, f32::min);
        let max = cross_section
            .iter()
            .map(|point| dot(sub(*point, center), normal))
            .fold(f32::NEG_INFINITY, f32::max);
        assert_close(min, -width * 0.5, EPS);
        assert_close(max, width * 0.5, EPS);
        assert_close(max - min, width, EPS);
    }
}''',
)

replace_test(
    "morph_straight_authored_join_does_not_bulge_or_shrink",
    r'''#[test]
fn morph_straight_authored_join_does_not_bulge_or_shrink() {
    let width = 0.4;
    let half_width = width * 0.5;
    let middle = Vec2::new(1.0, 0.0);
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(middle)
        .line_to(Vec2::new(2.0, 0.0));
    let mesh = self_morph_styled(path, width, StrokeJoin::Miter, StrokeCap::Butt);
    let positions = source_positions(&mesh);
    let at_middle: Vec<_> = positions
        .iter()
        .copied()
        .filter(|point| (point.x - middle.x).abs() < EPS)
        .collect();
    assert!(contains_point(
        &at_middle,
        Vec2::new(middle.x, half_width),
        EPS
    ));
    assert!(contains_point(
        &at_middle,
        Vec2::new(middle.x, -half_width),
        EPS
    ));
    assert!(at_middle
        .iter()
        .all(|point| point.y.abs() <= half_width + EPS));
}''',
)

replace_test(
    "morph_right_angle_miter_matches_closed_form_offset_intersection",
    r'''#[test]
fn morph_right_angle_miter_matches_closed_form_offset_intersection() {
    let width = 0.5;
    let half_width = width * 0.5;
    let corner = Vec2::new(1.0, 0.0);
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(corner)
        .line_to(Vec2::new(1.0, 1.0));
    let mesh = self_morph_styled(path, width, StrokeJoin::Miter, StrokeCap::Butt);
    let positions = source_positions(&mesh);
    let inner = Vec2::new(corner.x - half_width, corner.y + half_width);
    let outer = Vec2::new(corner.x + half_width, corner.y - half_width);

    assert!(contains_point(&positions, inner, EPS));
    assert!(contains_point(&positions, outer, EPS));
    assert_close(magnitude(sub(outer, corner)), half_width * 2.0_f32.sqrt(), EPS);
}''',
)

replace_test(
    "morph_miter_limit_bounds_near_reversal_spikes",
    r'''#[test]
fn morph_miter_limit_bounds_near_reversal_spikes() {
    let width = 0.2;
    let half_width = width * 0.5;
    let corner = Vec2::new(1.0, 0.0);
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(corner)
        .line_to(Vec2::new(0.001, 0.01));
    let mesh = self_morph_styled(path, width, StrokeJoin::Miter, StrokeCap::Butt);
    let positions = source_positions(&mesh);
    let nearby: Vec<_> = positions
        .iter()
        .copied()
        .filter(|point| magnitude(sub(*point, corner)) <= half_width * MORPH_MITER_LIMIT + EPS)
        .collect();
    assert!(!nearby.is_empty());
    assert!(nearby.iter().all(|point| {
        let distance = magnitude(sub(*point, corner));
        distance.is_finite() && distance <= half_width * MORPH_MITER_LIMIT + EPS
    }));
    // Lyon-style miter-limit fallback is bevel, so at least one outer endpoint
    // remains exactly one half-width from the corner rather than forming a spike.
    assert!(nearby.iter().any(|point| {
        (magnitude(sub(*point, corner)) - half_width).abs() < EPS
    }));
}''',
)

replace_test(
    "star_morph_target_miters_match_offset_line_theory_or_miter_limit",
    r'''#[test]
fn star_morph_target_miters_match_offset_line_theory_or_miter_limit() {
    let points = star_vertices();
    let target = polygon_path(&points);
    let width = 0.16;
    let half_width = width * 0.5;
    let mesh = tessellate_styled(
        &rounded_source().with_morph_target(target),
        width,
        StrokeJoin::Miter,
        StrokeCap::Round,
    )
    .expect("valid star morph tessellation");
    let target_positions = target_positions(&mesh);

    for index in 0..points.len() {
        let point = points[index];
        let previous = points[(index + points.len() - 1) % points.len()];
        let next = points[(index + 1) % points.len()];
        let incoming = normalized(sub(point, previous));
        let outgoing = normalized(sub(next, point));
        let turn = incoming.x * outgoing.y - incoming.y * outgoing.x;
        let sign = if turn > 0.0 { -1.0 } else { 1.0 };
        let incoming_normal = scale(Vec2::new(-incoming.y, incoming.x), sign);
        let outgoing_normal = scale(Vec2::new(-outgoing.y, outgoing.x), sign);
        let summed = add(incoming_normal, outgoing_normal);
        let miter = normalized(summed);
        let alignment = dot(miter, outgoing_normal).abs();
        let theoretical_length = half_width / alignment;

        if theoretical_length <= half_width * MORPH_MITER_LIMIT {
            let expected = add(point, scale(miter, theoretical_length));
            assert!(
                contains_point(&target_positions, expected, 2.0 * EPS),
                "missing theoretical miter at star vertex {index}: {expected:?}"
            );
            let offset = sub(expected, point);
            assert_close(dot(offset, incoming_normal).abs(), half_width, 2.0 * EPS);
            assert_close(dot(offset, outgoing_normal).abs(), half_width, 2.0 * EPS);
        } else {
            let outer_in = add(point, scale(incoming_normal, half_width));
            let outer_out = add(point, scale(outgoing_normal, half_width));
            assert!(contains_point(&target_positions, outer_in, 2.0 * EPS));
            assert!(contains_point(&target_positions, outer_out, 2.0 * EPS));
        }
    }
}''',
)

replace_test(
    "closed_morph_stroke_is_invariant_to_contour_start_index",
    r'''#[test]
fn closed_morph_stroke_is_invariant_to_contour_start_index() {
    let points = [
        Vec2::new(-1.0, -1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(-1.0, 1.0),
    ];
    let baseline = self_morph(polygon_path(&points), 0.2);
    let shifted = [points[2], points[3], points[0], points[1]];
    let shifted = self_morph(polygon_path(&shifted), 0.2);

    assert_eq!(
        canonical_positions(source_positions(&baseline), 5.0 * EPS),
        canonical_positions(source_positions(&shifted), 5.0 * EPS)
    );
}''',
)

replace_test(
    "reversing_closed_morph_contour_preserves_geometry_and_swaps_sides_only",
    r'''#[test]
fn reversing_closed_morph_contour_preserves_geometry_and_swaps_sides_only() {
    let points = [
        Vec2::new(-1.0, -1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(-1.0, 1.0),
    ];
    let baseline = self_morph(polygon_path(&points), 0.2);
    let reversed = [points[3], points[2], points[1], points[0]];
    let reversed = self_morph(polygon_path(&reversed), 0.2);

    assert_eq!(
        canonical_positions(source_positions(&baseline), 5.0 * EPS),
        canonical_positions(source_positions(&reversed), 5.0 * EPS)
    );
}''',
)

replace_test(
    "morph_target_endpoint_mesh_contains_every_authored_star_vertex",
    r'''#[test]
fn morph_target_endpoint_mesh_contains_every_authored_star_vertex() {
    let target_vertices = star_vertices();
    let target = polygon_path(&target_vertices);
    let mesh = tessellate(&rounded_source().with_morph_target(target), 0.16)
        .expect("valid star morph tessellation");
    let positions = target_positions(&mesh);

    for vertex in target_vertices {
        assert!(
            contains_point(&positions, vertex, EPS),
            "authored target vertex {vertex:?} was lost during morph tessellation"
        );
    }
}''',
)

replace_test(
    "symmetric_star_tip_taper_is_mirror_symmetric",
    r'''#[test]
fn symmetric_star_tip_taper_is_mirror_symmetric() {
    let target = polygon_path(&star_vertices());
    let mesh = tessellate_styled(
        &rounded_source().with_morph_target(target),
        0.16,
        StrokeJoin::Miter,
        StrokeCap::Round,
    )
    .expect("valid star morph tessellation");
    let target = target_positions(&mesh);
    let reflected = target.iter().copied().map(reflect_x).collect::<Vec<_>>();

    assert_eq!(
        canonical_positions(target, 5.0 * EPS),
        canonical_positions(reflected, 5.0 * EPS),
        "target stroke mesh must preserve the star's mirror symmetry"
    );
}''',
)

replace_test(
    "morph_topology_matches_open_and_closed_strip_theory",
    r'''#[test]
fn morph_topology_matches_fixed_segment_join_cap_theory() {
    let samples = 64;
    let round_vertices = 8 + 2; // center + 9 arc points
    let round_indices = 8 * 3;
    let open = self_morph(
        VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(4.0, 0.0)),
        0.2,
    );
    let open_segments = samples - 1;
    let open_joins = samples - 2;
    assert_eq!(
        open.vertices.len(),
        open_segments * 4 + open_joins * 2 * round_vertices + 2 * round_vertices
    );
    assert_eq!(
        open.indices.len(),
        open_segments * 6 + open_joins * 2 * round_indices + 2 * round_indices
    );

    let closed = self_morph(
        polygon_path(&[
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ]),
        0.2,
    );
    assert_eq!(
        closed.vertices.len(),
        samples * 4 + samples * 2 * round_vertices
    );
    assert_eq!(
        closed.indices.len(),
        samples * 6 + samples * 2 * round_indices
    );
}''',
)

replace_test(
    "morph_strip_triangles_keep_winding_through_interpolation",
    r'''#[test]
fn morph_active_triangles_keep_winding_through_interpolation() {
    let source = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(4.0, 0.0));
    let target = VectorPath::new()
        .move_to(Vec2::new(0.0, 2.0))
        .line_to(Vec2::new(4.0, 2.0));
    let mesh = tessellate_styled(
        &source.with_morph_target(target),
        0.2,
        StrokeJoin::Miter,
        StrokeCap::Butt,
    )
    .expect("valid morph");

    for alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let mut active = 0;
        for triangle in mesh.indices.chunks_exact(3) {
            let position = |index: u32| {
                let vertex = mesh.vertices[index as usize];
                add(
                    scale(vertex.position, 1.0 - alpha),
                    scale(vertex.target_position, alpha),
                )
            };
            let area = signed_triangle_area(
                position(triangle[0]),
                position(triangle[1]),
                position(triangle[2]),
            );
            // Inactive join slots intentionally collapse to zero area so turn
            // direction can change without changing topology.
            if area.abs() <= EPS {
                continue;
            }
            active += 1;
            assert!(area > EPS, "active triangle inverted at alpha={alpha}: {area}");
        }
        assert!(active > 0, "morph must retain active stroke triangles");
    }
}''',
)

path.write_text(text)
