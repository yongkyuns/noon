use noon_core::{Color, GeometryRef, ObjectId, StrokeCap, StrokeJoin, Style, Transform2D, Vec2, VectorPath};
use noon_render_wgpu::{FramePreparer, PathVertex};
use noon_runtime::{FrameObjectState, FrameState};

fn path() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-2.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(0.0, 2.0))
}

fn style(join: StrokeJoin, cap: StrokeCap) -> Style {
    Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.4,
        stroke_join: join,
        stroke_cap: cap,
        opacity: 1.0,
    }
}

fn frame(geometry: GeometryRef, style: Style) -> FrameState {
    FrameState {
        time: 0.0,
        objects: vec![FrameObjectState {
            id: ObjectId::new(0),
            geometry,
            transform: Transform2D::IDENTITY,
            style,
        }],
        reveals: vec![1.0],
        morphs: vec![0.0],
        render_geometries: vec![None],
    }
}

fn stroke_bounds(vertices: &[PathVertex], target: bool) -> ([f32; 2], [f32; 2]) {
    let mut points = vertices
        .iter()
        .filter(|vertex| vertex.surface & 1 == 1)
        .map(|vertex| if target { vertex.target_position } else { vertex.position });
    let first = points.next().expect("prepared stroke must have vertices");
    let mut min = first;
    let mut max = first;
    for point in points {
        min[0] = min[0].min(point[0]);
        min[1] = min[1].min(point[1]);
        max[0] = max[0].max(point[0]);
        max[1] = max[1].max(point[1]);
    }
    (min, max)
}

fn assert_bounds_close(actual: ([f32; 2], [f32; 2]), expected: ([f32; 2], [f32; 2])) {
    for (actual, expected) in [
        (actual.0[0], expected.0[0]),
        (actual.0[1], expected.0[1]),
        (actual.1[0], expected.1[0]),
        (actual.1[1], expected.1[1]),
    ] {
        assert!(
            (actual - expected).abs() < 1.0e-4,
            "prepared endpoint bound {actual} != {expected}"
        );
    }
}

#[test]
fn renderer_packed_static_and_identity_morph_endpoints_match_for_every_style() {
    let source = path();
    for join in [StrokeJoin::Round, StrokeJoin::Miter, StrokeJoin::Bevel] {
        for cap in [StrokeCap::Round, StrokeCap::Butt, StrokeCap::Square] {
            let style = style(join, cap);

            let mut static_preparer = FramePreparer::new();
            let static_frame = frame(GeometryRef::path(source.clone()), style);
            let static_prepared = static_preparer.prepare(&static_frame);
            let expected = stroke_bounds(static_prepared.path_vertices, false);

            let morph = source.clone().with_morph_target(source.clone());
            let mut morph_preparer = FramePreparer::new();
            let morph_frame = frame(GeometryRef::path(morph), style);
            let morph_prepared = morph_preparer.prepare(&morph_frame);

            assert_bounds_close(stroke_bounds(morph_prepared.path_vertices, false), expected);
            assert_bounds_close(stroke_bounds(morph_prepared.path_vertices, true), expected);
        }
    }
}
