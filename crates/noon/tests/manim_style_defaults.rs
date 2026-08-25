use noon::prelude::*;

#[test]
fn rust_authoring_shapes_use_manim_vmobject_defaults() {
    for snapshot in [Square::default().snapshot(), Line::default().snapshot()] {
        let fill = snapshot
            .style
            .fill
            .expect("Manim VMobject keeps a fill paint layer");
        assert_eq!(fill.red, 1.0);
        assert_eq!(fill.green, 1.0);
        assert_eq!(fill.blue, 1.0);
        assert_eq!(fill.alpha, 0.0);
        assert_eq!(snapshot.style.stroke, Some(WHITE));
        assert!((snapshot.style.stroke_width - 0.04).abs() < f32::EPSILON);
        assert_eq!(
            snapshot.style.stroke_width_mode,
            noon_core::StrokeWidthMode::ScreenSpace
        );
        assert_eq!(snapshot.style.stroke_join, noon_core::StrokeJoin::Miter);
        assert_eq!(snapshot.style.stroke_cap, noon_core::StrokeCap::Butt);
    }
}

#[test]
fn rust_circle_uses_manim_specific_red_default() {
    let snapshot = Circle::default();
    let fill = snapshot
        .snapshot()
        .style
        .fill
        .expect("Manim Circle keeps a transparent fill paint layer");
    assert_eq!(fill.red, RED.red);
    assert_eq!(fill.green, RED.green);
    assert_eq!(fill.blue, RED.blue);
    assert_eq!(fill.alpha, 0.0);
    assert_eq!(snapshot.snapshot().style.stroke, Some(RED));
    assert!((snapshot.snapshot().style.stroke_width - 0.04).abs() < f32::EPSILON);
    assert_eq!(
        snapshot.snapshot().style.stroke_width_mode,
        noon_core::StrokeWidthMode::ScreenSpace
    );
}

#[test]
fn core_style_default_remains_renderer_neutral() {
    let style = noon_core::Style::default();
    assert_eq!(style.fill, Some(WHITE));
    assert_eq!(style.stroke, None);
    assert_eq!(style.stroke_join, noon_core::StrokeJoin::Round);
    assert_eq!(style.stroke_cap, noon_core::StrokeCap::Round);
}
