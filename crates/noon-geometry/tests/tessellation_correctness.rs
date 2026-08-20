use lyon_path::{math::point, Path};
use lyon_tessellation::{
    BuffersBuilder, LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex,
    VertexBuffers,
};
use noon_core::{PathCommand, Vec2, VectorPath};
use noon_geometry::{tessellate, GeometryError, MeshVertex, PathSurface, TessellatedPath};

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

fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    scale(add(a, b), 0.5)
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

fn source_pairs(mesh: &TessellatedPath) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    assert!(mesh.morphing);
    mesh.vertices
        .chunks_exact(2)
        .map(|pair| (pair[0].position, pair[1].position))
}

fn target_pairs(mesh: &TessellatedPath) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    assert!(mesh.morphing);
    mesh.vertices
        .chunks_exact(2)
        .map(|pair| (pair[0].target_position, pair[1].target_position))
}

fn find_pair_at_center<I>(pairs: I, center: Vec2) -> (Vec2, Vec2)
where
    I: IntoIterator<Item = (Vec2, Vec2)>,
{
    pairs
        .into_iter()
        .find(|(left, right)| magnitude(sub(midpoint(*left, *right), center)) < EPS)
        .unwrap_or_else(|| panic!("no stroke pair centered at {center:?}"))
}

fn unordered_pair_matches(actual: (Vec2, Vec2), expected: (Vec2, Vec2), tolerance: f32) -> bool {
    let direct = magnitude(sub(actual.0, expected.0)) <= tolerance
        && magnitude(sub(actual.1, expected.1)) <= tolerance;
    let swapped = magnitude(sub(actual.0, expected.1)) <= tolerance
        && magnitude(sub(actual.1, expected.0)) <= tolerance;
    direct || swapped
}

fn self_morph(path: VectorPath, width: f32) -> TessellatedPath {
    let target = path.clone();
    tessellate(&path.with_morph_target(target), width).expect("valid self morph")
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
    let mesh = self_morph(VectorPath::new().move_to(start).line_to(end), width);
    let pairs: Vec<_> = source_pairs(&mesh).collect();

    for (center, pair) in [(start, pairs[0]), (end, *pairs.last().unwrap())] {
        assert_vec_close(midpoint(pair.0, pair.1), center, EPS);
        assert_close(magnitude(sub(pair.0, pair.1)), width, EPS);
        assert_close(dot(sub(pair.0, center), tangent), 0.0, EPS);
        assert_close(dot(sub(pair.1, center), tangent), 0.0, EPS);
    }
}

#[test]
fn morph_straight_authored_join_does_not_bulge_or_shrink() {
    let width = 0.4;
    let middle = Vec2::new(1.0, 0.0);
    let path = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(middle)
        .line_to(Vec2::new(2.0, 0.0));
    let mesh = self_morph(path, width);
    let pair = find_pair_at_center(source_pairs(&mesh), middle);

    assert_close(magnitude(sub(pair.0, pair.1)), width, EPS);
    assert_close(pair.0.x, middle.x, EPS);
    assert_close(pair.1.x, middle.x, EPS);
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
    let mesh = self_morph(path, width);
    let actual = find_pair_at_center(source_pairs(&mesh), corner);
    let offset = Vec2::new(-half_width, half_width);
    let expected = (add(corner, offset), sub(corner, offset));

    assert!(unordered_pair_matches(actual, expected, EPS));
    assert_close(magnitude(offset), half_width * 2.0_f32.sqrt(), EPS);
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
    let mesh = self_morph(path, width);
    let pair = find_pair_at_center(source_pairs(&mesh), corner);

    for edge in [pair.0, pair.1] {
        let offset = sub(edge, corner);
        assert!(offset.x.is_finite() && offset.y.is_finite());
        assert_close(magnitude(offset), half_width * MORPH_MITER_LIMIT, EPS);
    }
}

#[test]
fn star_morph_target_miters_match_offset_line_theory_or_miter_limit() {
    let points = star_vertices();
    let target = polygon_path(&points);
    let width = 0.16;
    let half_width = width * 0.5;
    let mesh = tessellate(&rounded_source().with_morph_target(target), width)
        .expect("valid star morph tessellation");

    for index in 0..points.len() {
        let point = points[index];
        let previous = points[(index + points.len() - 1) % points.len()];
        let next = points[(index + 1) % points.len()];
        let incoming = normalized(sub(point, previous));
        let outgoing = normalized(sub(next, point));
        let incoming_normal = Vec2::new(-incoming.y, incoming.x);
        let outgoing_normal = Vec2::new(-outgoing.y, outgoing.x);
        let summed = add(incoming_normal, outgoing_normal);
        let miter = normalized(summed);
        let alignment = dot(miter, outgoing_normal).abs();
        let theoretical_length = half_width / alignment;
        let pair = find_pair_at_center(target_pairs(&mesh), point);
        let offset = sub(pair.0, point);

        if theoretical_length <= half_width * MORPH_MITER_LIMIT {
            assert_close(dot(offset, incoming_normal).abs(), half_width, EPS);
            assert_close(dot(offset, outgoing_normal).abs(), half_width, EPS);
        } else {
            assert_close(magnitude(offset), half_width * MORPH_MITER_LIMIT, EPS);
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
    let shifted_pairs: Vec<_> = source_pairs(&shifted).collect();

    for baseline_pair in source_pairs(&baseline) {
        let center = midpoint(baseline_pair.0, baseline_pair.1);
        let shifted_pair = find_pair_at_center(shifted_pairs.iter().copied(), center);
        assert!(unordered_pair_matches(baseline_pair, shifted_pair, EPS));
    }
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
    let reversed_pairs: Vec<_> = source_pairs(&reversed).collect();

    for baseline_pair in source_pairs(&baseline) {
        let center = midpoint(baseline_pair.0, baseline_pair.1);
        let reversed_pair = find_pair_at_center(reversed_pairs.iter().copied(), center);
        assert!(unordered_pair_matches(baseline_pair, reversed_pair, EPS));
    }
}

#[test]
fn morph_target_endpoint_mesh_contains_every_authored_star_vertex() {
    let target_vertices = star_vertices();
    let target = polygon_path(&target_vertices);
    let mesh = tessellate(&rounded_source().with_morph_target(target), 0.16)
        .expect("valid star morph tessellation");

    let centers: Vec<Vec2> = target_pairs(&mesh)
        .map(|(left, right)| midpoint(left, right))
        .collect();
    for vertex in target_vertices {
        assert!(
            centers
                .iter()
                .any(|center| magnitude(sub(*center, vertex)) < EPS),
            "authored target vertex {vertex:?} was lost during morph tessellation"
        );
    }
}

#[test]
fn symmetric_star_tip_taper_is_mirror_symmetric() {
    let target_vertices = star_vertices();
    let target = polygon_path(&target_vertices);
    let mesh = tessellate(&rounded_source().with_morph_target(target), 0.16)
        .expect("valid star morph tessellation");

    let tip_pair = find_pair_at_center(target_pairs(&mesh), target_vertices[0]);
    assert_close(tip_pair.0.x, 0.0, EPS);
    assert_close(tip_pair.1.x, 0.0, EPS);

    for (right_index, left_index) in [(1, 9), (2, 8), (3, 7), (4, 6)] {
        let right_pair = find_pair_at_center(target_pairs(&mesh), target_vertices[right_index]);
        let left_pair = find_pair_at_center(target_pairs(&mesh), target_vertices[left_index]);
        let reflected_right = (reflect_x(right_pair.0), reflect_x(right_pair.1));
        assert!(
            unordered_pair_matches(left_pair, reflected_right, EPS),
            "stroke corners at mirrored star vertices {right_index}/{left_index} are asymmetric"
        );
    }
}

#[test]
fn morph_topology_matches_open_and_closed_strip_theory() {
    let samples = 64;
    let open = self_morph(
        VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .line_to(Vec2::new(4.0, 0.0)),
        0.2,
    );
    assert_eq!(open.vertices.len(), samples * 2);
    assert_eq!(open.indices.len(), (samples - 1) * 6);

    let closed = self_morph(
        polygon_path(&[
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ]),
        0.2,
    );
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
            assert!(
                area > EPS,
                "triangle inverted or collapsed at alpha={alpha}: {area}"
            );
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
