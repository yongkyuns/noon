use super::*;
use lyon_tessellation::{
    BuffersBuilder, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex,
    VertexBuffers,
};

const EPS: f32 = 1.0e-5;
const TESSELLATION_EPS: f32 = PATH_TESSELLATION_TOLERANCE * 1.5;

fn assert_close(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected}, got {actual} (tolerance {tolerance})"
    );
}

fn assert_vec_close(actual: Vec2, expected: Vec2, tolerance: f32) {
    assert_close(actual.x, expected.x, tolerance);
    assert_close(actual.y, expected.y, tolerance);
}

fn add(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x + b.x, a.y + b.y)
}

fn sub(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x - b.x, a.y - b.y)
}

fn scale(value: Vec2, scalar: f32) -> Vec2 {
    Vec2::new(value.x * scalar, value.y * scalar)
}

fn dot(a: Vec2, b: Vec2) -> f32 {
    a.x * b.x + a.y * b.y
}

fn magnitude(value: Vec2) -> f32 {
    value.x.hypot(value.y)
}

fn midpoint_of(a: Vec2, b: Vec2) -> Vec2 {
    scale(add(a, b), 0.5)
}

fn signed_triangle_area(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) * 0.5
}

fn stroke_triangle_area(mesh: &TessellatedPath) -> f32 {
    mesh.indices
        .chunks_exact(3)
        .filter_map(|triangle| {
            let a = &mesh.vertices[triangle[0] as usize];
            let b = &mesh.vertices[triangle[1] as usize];
            let c = &mesh.vertices[triangle[2] as usize];
            if [a, b, c]
                .iter()
                .all(|vertex| vertex.surface == PathSurface::Stroke)
            {
                Some(signed_triangle_area(a.position, b.position, c.position).abs())
            } else {
                None
            }
        })
        .sum()
}

fn stroke_vertices(mesh: &TessellatedPath) -> Vec<&MeshVertex> {
    mesh.vertices
        .iter()
        .filter(|vertex| vertex.surface == PathSurface::Stroke)
        .collect()
}

fn reference_lyon_stroke(path: &VectorPath, stroke_width: f32) -> Vec<(Vec2, f32)> {
    let lyon_path = build_lyon_path(path).expect("valid reference path");
    let mut buffers: VertexBuffers<(Vec2, f32), u32> = VertexBuffers::new();
    StrokeTessellator::new()
        .tessellate_path(
            &lyon_path,
            &StrokeOptions::default()
                .with_tolerance(PATH_TESSELLATION_TOLERANCE)
                .with_line_width(stroke_width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
            &mut BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex<'_, '_>| {
                (
                    Vec2::new(vertex.position().x, vertex.position().y),
                    vertex.advancement(),
                )
            }),
        )
        .expect("Lyon reference tessellation");
    buffers.vertices
}

fn star_vertices() -> [Vec2; 10] {
    [
        Vec2::new(0.0, 2.0),
        Vec2::new(0.47, 0.65),
        Vec2::new(1.9, 0.62),
        Vec2::new(0.76, -0.25),
        Vec2::new(1.18, -1.62),
        Vec2::new(0.0, -0.82),
        Vec2::new(-1.18, -1.62),
        Vec2::new(-0.76, -0.25),
        Vec2::new(-1.9, 0.62),
        Vec2::new(-0.47, 0.65),
    ]
}

fn polygon_path(points: &[Vec2]) -> VectorPath {
    let mut path = VectorPath::new().move_to(points[0]);
    for point in &points[1..] {
        path = path.line_to(*point);
    }
    path.close()
}

fn rounded_source() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 1.65))
        .cubic_to(
            Vec2::new(0.95, 1.65),
            Vec2::new(1.65, 0.95),
            Vec2::new(1.65, 0.0),
        )
        .cubic_to(
            Vec2::new(1.65, -0.95),
            Vec2::new(0.95, -1.65),
            Vec2::new(0.0, -1.65),
        )
        .cubic_to(
            Vec2::new(-0.95, -1.65),
            Vec2::new(-1.65, -0.95),
            Vec2::new(-1.65, 0.0),
        )
        .cubic_to(
            Vec2::new(-1.65, 0.95),
            Vec2::new(-0.95, 1.65),
            Vec2::new(0.0, 1.65),
        )
        .close()
}

fn target_pairs(mesh: &TessellatedPath) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    mesh.vertices.chunks_exact(2).map(|pair| {
        debug_assert_eq!(pair[0].surface, PathSurface::Stroke);
        debug_assert_eq!(pair[1].surface, PathSurface::Stroke);
        (pair[0].target_position, pair[1].target_position)
    })
}

#[test]
fn static_wrapper_matches_direct_lyon_reference() {
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.5))
        .line_to(Vec2::new(0.5, 1.25))
        .quadratic_to(Vec2::new(1.5, 2.0), Vec2::new(2.5, 0.25));
    let width = 0.35;
    let reference = reference_lyon_stroke(&path, width);
    let actual = tessellate(&path, width).expect("Noon tessellation");
    let actual = stroke_vertices(&actual);

    assert_eq!(actual.len(), reference.len());
    for (actual, (reference_position, reference_advancement)) in actual.iter().zip(reference) {
        assert_vec_close(actual.position, reference_position, EPS);
        assert_close(actual.path_distance, reference_advancement, EPS);
    }
}

#[test]
fn lyon_round_cap_line_matches_capsule_extents_and_area() {
    let length = 4.0;
    let width = 0.4;
    let radius = width * 0.5;
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(length, 0.0));
    let mesh = tessellate(&path, width).expect("valid line");
    let stroke = stroke_vertices(&mesh);

    let min_x = stroke
        .iter()
        .map(|vertex| vertex.position.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = stroke
        .iter()
        .map(|vertex| vertex.position.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = stroke
        .iter()
        .map(|vertex| vertex.position.y)
        .fold(f32::INFINITY, f32::min);
    let max_y = stroke
        .iter()
        .map(|vertex| vertex.position.y)
        .fold(f32::NEG_INFINITY, f32::max);

    assert_close(min_x, -radius, TESSELLATION_EPS);
    assert_close(max_x, length + radius, TESSELLATION_EPS);
    assert_close(min_y, -radius, TESSELLATION_EPS);
    assert_close(max_y, radius, TESSELLATION_EPS);

    let theoretical_area = length * width + std::f32::consts::PI * radius * radius;
    let area = stroke_triangle_area(&mesh);
    assert_close(area, theoretical_area, theoretical_area * 0.02);
}

#[test]
fn lyon_round_caps_are_rotation_invariant_in_tangent_space() {
    let start = Vec2::new(1.0, -2.0);
    let end = Vec2::new(4.0, 2.0);
    let delta = sub(end, start);
    let length = magnitude(delta);
    let tangent = scale(delta, 1.0 / length);
    let normal = Vec2::new(-tangent.y, tangent.x);
    let width = 0.6;
    let radius = width * 0.5;
    let path = VectorPath::new().move_to(start).line_to(end);
    let mesh = tessellate(&path, width).expect("valid diagonal line");

    let mut min_tangent = f32::INFINITY;
    let mut max_tangent = f32::NEG_INFINITY;
    let mut min_normal = f32::INFINITY;
    let mut max_normal = f32::NEG_INFINITY;
    for vertex in stroke_vertices(&mesh) {
        let relative = sub(vertex.position, start);
        let along = dot(relative, tangent);
        let across = dot(relative, normal);
        min_tangent = min_tangent.min(along);
        max_tangent = max_tangent.max(along);
        min_normal = min_normal.min(across);
        max_normal = max_normal.max(across);
    }

    assert_close(min_tangent, -radius, TESSELLATION_EPS);
    assert_close(max_tangent, length + radius, TESSELLATION_EPS);
    assert_close(min_normal, -radius, TESSELLATION_EPS);
    assert_close(max_normal, radius, TESSELLATION_EPS);
}

#[test]
fn quadratic_advancement_matches_independent_high_resolution_length() {
    let from = Vec2::new(0.0, 0.0);
    let control = Vec2::new(1.0, 2.0);
    let to = Vec2::new(2.0, 0.0);
    let path = VectorPath::new().move_to(from).quadratic_to(control, to);
    let mesh = tessellate(&path, 0.1).expect("valid quadratic");

    let point = |t: f32| {
        let one_minus_t = 1.0 - t;
        Vec2::new(
            one_minus_t * one_minus_t * from.x
                + 2.0 * one_minus_t * t * control.x
                + t * t * to.x,
            one_minus_t * one_minus_t * from.y
                + 2.0 * one_minus_t * t * control.y
                + t * t * to.y,
        )
    };
    let steps = 8192;
    let mut reference_length = 0.0;
    let mut previous = point(0.0);
    for step in 1..=steps {
        let current = point(step as f32 / steps as f32);
        reference_length += magnitude(sub(current, previous));
        previous = current;
    }

    assert_close(mesh.stroke_length, reference_length, 0.02);
}

#[test]
fn morph_open_end_edges_are_exactly_perpendicular_and_full_width() {
    let points = [Vec2::new(0.0, 0.0), Vec2::new(3.0, 4.0)];
    let width = 0.6;
    let half_width = width * 0.5;
    let tangent = normalized(sub(points[1], points[0]));
    let edges = stroke_edges(&points, false, half_width);

    for (point, (left, right)) in points.into_iter().zip(edges) {
        assert_vec_close(midpoint_of(left, right), point, EPS);
        assert_close(magnitude(sub(left, right)), width, EPS);
        assert_close(dot(sub(left, point), tangent), 0.0, EPS);
        assert_close(dot(sub(right, point), tangent), 0.0, EPS);
    }
}

#[test]
fn morph_straight_join_does_not_bulge_or_shrink() {
    let half_width = 0.2;
    let offset = miter_offset(
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(2.0, 0.0),
        half_width,
    );
    assert_vec_close(offset, Vec2::new(0.0, half_width), EPS);
}

#[test]
fn morph_right_angle_miter_matches_closed_form_intersection() {
    let half_width = 0.25;
    let offset = miter_offset(
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(1.0, 1.0),
        half_width,
    );
    assert_vec_close(offset, Vec2::new(-half_width, half_width), EPS);
    assert_close(magnitude(offset), half_width * 2.0_f32.sqrt(), EPS);
}

#[test]
fn morph_miter_limit_bounds_near_reversal_spikes() {
    let half_width = 0.1;
    let offset = miter_offset(
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.001, 0.01),
        half_width,
    );
    assert!(offset.x.is_finite() && offset.y.is_finite());
    assert_close(
        magnitude(offset),
        half_width * MORPH_MITER_LIMIT,
        EPS,
    );
}

#[test]
fn morph_polygon_miters_match_offset_line_theory_or_miter_limit() {
    let points = star_vertices();
    let half_width = 0.08;
    let edges = stroke_edges(&points, true, half_width);

    for index in 0..points.len() {
        let point = points[index];
        let previous = points[(index + points.len() - 1) % points.len()];
        let next = points[(index + 1) % points.len()];
        let incoming = normalized(sub(point, previous));
        let outgoing = normalized(sub(next, point));
        let incoming_normal = Vec2::new(-incoming.y, incoming.x);
        let outgoing_normal = Vec2::new(-outgoing.y, outgoing.x);
        let summed = add(incoming_normal, outgoing_normal);
        let miter = scale(summed, 1.0 / magnitude(summed));
        let alignment = dot(miter, outgoing_normal).abs();
        let theoretical_length = half_width / alignment;
        let offset = sub(edges[index].0, point);

        if theoretical_length <= half_width * MORPH_MITER_LIMIT {
            assert_close(dot(offset, incoming_normal).abs(), half_width, EPS);
            assert_close(dot(offset, outgoing_normal).abs(), half_width, EPS);
        } else {
            assert_close(
                magnitude(offset),
                half_width * MORPH_MITER_LIMIT,
                EPS,
            );
        }
    }
}

#[test]
fn closed_stroke_edges_are_invariant_to_contour_start_index() {
    let points = vec![
        Vec2::new(-1.3, -0.5),
        Vec2::new(0.2, -1.1),
        Vec2::new(1.4, 0.1),
        Vec2::new(0.4, 1.6),
        Vec2::new(-1.1, 1.0),
    ];
    let half_width = 0.12;
    let baseline = stroke_edges(&points, true, half_width);
    let shift = 2;
    let rotated: Vec<Vec2> = points[shift..]
        .iter()
        .chain(&points[..shift])
        .copied()
        .collect();
    let rotated_edges = stroke_edges(&rotated, true, half_width);

    for (rotated_index, edge) in rotated_edges.iter().enumerate() {
        let baseline_index = (rotated_index + shift) % points.len();
        assert_vec_close(edge.0, baseline[baseline_index].0, EPS);
        assert_vec_close(edge.1, baseline[baseline_index].1, EPS);
    }
}

#[test]
fn reversing_closed_contour_swaps_left_and_right_edges_only() {
    let points = vec![
        Vec2::new(-1.3, -0.5),
        Vec2::new(0.2, -1.1),
        Vec2::new(1.4, 0.1),
        Vec2::new(0.4, 1.6),
        Vec2::new(-1.1, 1.0),
    ];
    let half_width = 0.12;
    let baseline = stroke_edges(&points, true, half_width);
    let reversed: Vec<Vec2> = points.iter().copied().rev().collect();
    let reversed_edges = stroke_edges(&reversed, true, half_width);

    for (index, point) in points.iter().enumerate() {
        let reversed_index = reversed
            .iter()
            .position(|candidate| candidate == point)
            .expect("same vertex exists after reversal");
        assert_vec_close(baseline[index].0, reversed_edges[reversed_index].1, EPS);
        assert_vec_close(baseline[index].1, reversed_edges[reversed_index].0, EPS);
    }
}

#[test]
fn morph_target_endpoint_mesh_contains_every_authored_star_vertex() {
    let target_vertices = star_vertices();
    let target = polygon_path(&target_vertices);
    let mesh = tessellate(&rounded_source().with_morph_target(target), 0.16)
        .expect("valid star morph tessellation");

    let centers: Vec<Vec2> = target_pairs(&mesh)
        .map(|(left, right)| midpoint_of(left, right))
        .collect();
    for vertex in target_vertices {
        assert!(
            centers.iter().any(|center| magnitude(sub(*center, vertex)) < EPS),
            "authored target vertex {vertex:?} was lost during morph tessellation"
        );
    }
}

#[test]
fn symmetric_star_tip_has_symmetric_left_and_right_taper() {
    let target_vertices = star_vertices();
    let target = polygon_path(&target_vertices);
    let mesh = tessellate(&rounded_source().with_morph_target(target), 0.16)
        .expect("valid star morph tessellation");
    let tip = target_vertices[0];
    let (left, right) = target_pairs(&mesh)
        .find(|(left, right)| magnitude(sub(midpoint_of(*left, *right), tip)) < EPS)
        .expect("top star tip is an exact morph sample");

    assert_close(left.x + right.x, tip.x * 2.0, EPS);
    assert_close(left.y, right.y, EPS);
    assert_close(magnitude(sub(left, tip)), magnitude(sub(right, tip)), EPS);
}

#[test]
fn morph_topology_matches_open_and_closed_strip_theory() {
    let open_source = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(4.0, 0.0));
    let open_target = VectorPath::new()
        .move_to(Vec2::new(0.0, 1.0))
        .line_to(Vec2::new(4.0, 1.0));
    let open = tessellate(&open_source.with_morph_target(open_target), 0.2)
        .expect("valid open morph");
    let samples = crate::MorphOptions::DEFAULT.samples_per_contour;
    assert_eq!(open.vertices.len(), samples * 2);
    assert_eq!(open.indices.len(), (samples - 1) * 6);

    let closed_source = polygon_path(&[
        Vec2::new(-1.0, -1.0),
        Vec2::new(1.0, -1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(-1.0, 1.0),
    ]);
    let closed_target = polygon_path(&[
        Vec2::new(0.0, -1.4),
        Vec2::new(1.4, 0.0),
        Vec2::new(0.0, 1.4),
        Vec2::new(-1.4, 0.0),
    ]);
    let closed = tessellate(&closed_source.with_morph_target(closed_target), 0.2)
        .expect("valid closed morph");
    assert_eq!(closed.vertices.len(), samples * 2);
    assert_eq!(closed.indices.len(), samples * 6);
}

#[test]
fn morph_strip_triangles_keep_winding_through_interpolation() {
    let source = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(4.0, 0.0));
    let target = VectorPath::new()
        .move_to(Vec2::new(0.0, 2.0))
        .line_to(Vec2::new(4.0, 2.0));
    let mesh = tessellate(&source.with_morph_target(target), 0.2).expect("valid morph");

    for alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
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
            assert!(area > EPS, "triangle inverted or collapsed at alpha={alpha}: {area}");
        }
    }
}

#[test]
fn invalid_stroke_widths_are_rejected_consistently() {
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0));
    for width in [-0.01, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            tessellate(&path, width),
            Err(GeometryError::InvalidStrokeWidth(_))
        ));
    }
}
