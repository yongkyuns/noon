#[test]
fn analytic_fill_and_stroke_use_cairo_source_over_order() {
    let shader = include_str!("../src/analytic.wgsl");
    assert!(shader.contains("fn source_over(source: vec4<f32>, destination: vec4<f32>)"));
    assert!(shader.contains("let stroke_band_coverage = outer_coverage * inner_stroke_coverage;"));
    assert!(shader.contains("return source_over(stroke_layer, fill_layer);"));
    assert!(!shader.contains("mix(premultiplied(fill), premultiplied(stroke), stroke_coverage)"));
}
