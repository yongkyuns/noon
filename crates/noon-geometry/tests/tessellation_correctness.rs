use lyon_path::{math::point, Path};
use lyon_tessellation::{
    BuffersBuilder, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex,
    VertexBuffers,
};
use noon_core::{PathCommand, StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::{
    tessellate, tessellate_styled, GeometryError, MeshVertex, PathSurface, TessellatedPath,
};

const EPS: f32 = 1.0e-5;
const REFERENCE_TOLERANCE: f32 = 0.01;
const TESSELLATION_EPS: f32 = REFERENCE_TOLERANCE * 1.5;
const MORPH_MITER_LIMIT: f32 = 4.0;

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

fn normalized(value: Vec2) -> Vec2 {
    let length = magnitude(value);
    assert!(length > EPS, "test vector must be non-degenerate");
    scale(value, 1.0 / length)
}

fn reflect_x(value: Vec2) -> Vec2 {
    Vec2::new(-value.x, value.y)
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

fn build_reference_path(path: &VectorPath) -> Path {
    let mut builder = Path::builder();
    let mut active = false;
    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                if active {
                    builder.end(false);
                }
                builder.begin(point(to.x, to.y));
                active = true;
            }
            PathCommand::LineTo { to } => {
                assert!(active);
                builder.line_to(point(to.x, to.y));
            }
            PathCommand::QuadraticTo { control, to } => {
                assert!(active);
                builder.quadratic_bezier_to(point(control.x, control.y), point(to.x, to.y));
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                assert!(active);
                builder.cubic_bezier_to(
                    point(control1.x, control1.y),
                    point(control2.x, control2.y),
                    point(to.x, to.y),
                );
            }
            PathCommand::Close => {
                assert!(active);
                builder.end(true);
                active = false;
            }
        }
    }
    if active {
        builder.end(false);
    }
    builder.build()
}

fn reference_lyon_stroke(path: &VectorPath, stroke_width: f32) -> Vec<(Vec2, f32)> {
    let lyon_path = build_reference_path(path);
    let mut buffers: VertexBuffers<(Vec2, f32), u32> = VertexBuffers::new();
    StrokeTessellator::new()
        .tessellate_path(
            &lyon_path,
            &StrokeOptions::default()
                .with_tolerance(REFERENCE_TOLERANCE)
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

fn source_positions(mesh: &TessellatedPath) -> Vec<Vec2> {
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
    assert_close(
        stroke_triangle_area(&mesh),
        theoretical_area,
        theoretical_area * 0.02,
    );
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
    let mesh = tessellate(&VectorPath::new().move_to(start).line_to(end), width)
        .expect("valid diagonal line");

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
            one_minus_t * one_minus_t * from.x + 2.0 * one_minus_t * t * control.x + t * t * to.x,
            one_minus_t * one_minus_t * from.y + 2.0 * one_minus_t * t * control.y + t * t * to.y,
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
}

#[test]
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
}

#[test]
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
    assert_close(
        magnitude(sub(outer, corner)),
        half_width * 2.0_f32.sqrt(),
        EPS,
    );
}

#[test]
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
    assert!(nearby
        .iter()
        .any(|point| { (magnitude(sub(*point, corner)) - half_width).abs() < EPS }));
}

#[test]
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
}

#[test]
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
}

#[test]
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
}

#[test]
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
}

#[test]
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
}

#[test]
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
}

#[test]
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
            assert!(
                area > EPS,
                "active triangle inverted at alpha={alpha}: {area}"
            );
        }
        assert!(active > 0, "morph must retain active stroke triangles");
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
