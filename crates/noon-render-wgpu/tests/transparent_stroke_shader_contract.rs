#[test]
fn transparent_stroke_does_not_enter_visible_stroke_compositing() {
    let shader = include_str!("../src/analytic.wgsl");
    assert!(
        shader.contains("stroke_enabled && stroke_width > 0.0 && stroke.a > 0.0"),
        "analytic fill/stroke compositing must ignore zero-alpha strokes so an invisible Manim stroke cannot erode the fill edge"
    );
}
