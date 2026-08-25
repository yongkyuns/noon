use noon_core::{GeometryRef, StrokeCap, StrokeJoin};
use noon_geometry::{canonical_outline_path, tessellate_styled_with_fill};

const EPS: f32 = 0.015;

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= EPS,
        "expected {expected}, got {actual} (tolerance {EPS})"
    );
}

#[test]
fn canonical_create_strokes_match_centered_analytic_outer_bounds() {
    let stroke_width = 0.12;

    let circle = GeometryRef::circle(0.9);
    let circle_path = canonical_outline_path(&circle).expect("circle outline");
    let circle_mesh = tessellate_styled_with_fill(
        &circle_path,
        stroke_width,
        StrokeJoin::Round,
        StrokeCap::Round,
        false,
    )
    .expect("circle tessellation");
    let circle_bounds = circle_mesh.bounds.expect("circle bounds");
    let circle_extent = 0.9 + stroke_width * 0.5;
    assert_close(circle_bounds.min.x, -circle_extent);
    assert_close(circle_bounds.min.y, -circle_extent);
    assert_close(circle_bounds.max.x, circle_extent);
    assert_close(circle_bounds.max.y, circle_extent);

    let rectangle = GeometryRef::rectangle(1.7, 1.7);
    let rectangle_path = canonical_outline_path(&rectangle).expect("rectangle outline");
    let rectangle_mesh = tessellate_styled_with_fill(
        &rectangle_path,
        stroke_width,
        StrokeJoin::Round,
        StrokeCap::Round,
        false,
    )
    .expect("rectangle tessellation");
    let rectangle_bounds = rectangle_mesh.bounds.expect("rectangle bounds");
    let rectangle_extent = 1.7 * 0.5 + stroke_width * 0.5;
    assert_close(rectangle_bounds.min.x, -rectangle_extent);
    assert_close(rectangle_bounds.min.y, -rectangle_extent);
    assert_close(rectangle_bounds.max.x, rectangle_extent);
    assert_close(rectangle_bounds.max.y, rectangle_extent);
}

#[test]
fn analytic_shader_uses_centered_vector_path_stroke_contract() {
    let shader = include_str!("../src/analytic.wgsl");

    assert!(shader.contains("let stroke_padding = select("));
    assert!(shader.contains("stroke_is_screen_space(input.flags)"));
    assert!(shader
        .contains("let invariant_padding = vec2<f32>(half_width) / safe_abs_scale(input.scale);"));
    assert!(shader.contains("inside_coverage(signed_distance - half_stroke_width)"));
    assert!(shader.contains("outside_coverage(signed_distance + half_stroke_width)"));
    assert!(!shader.contains("clamp(input.metrics.x"));
}
