use noon::{Color, Scene, StrokeCap, StrokeJoin, StrokeWidthMode, Style};

#[test]
fn shared_shapes_lower_manim_defaults_into_effective_styles() {
    let mut scene = Scene::new();
    for shape in [
        scene.square(2.0).unwrap(),
        scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap(),
        scene.circle(1.0).unwrap(),
    ] {
        scene.add(&shape).unwrap();
    }
    let session = scene.execution_session().unwrap();
    for (object, color) in
        session
            .frame()
            .objects
            .iter()
            .zip([Color::WHITE, Color::WHITE, Color::RED])
    {
        let fill = object
            .style
            .fill
            .expect("transparent fill layer is retained");
        assert_eq!(
            (fill.red, fill.green, fill.blue, fill.alpha),
            (color.red, color.green, color.blue, 0.0)
        );
        assert_eq!(object.style.stroke, Some(color));
        assert_eq!(object.style.stroke_width, 0.04);
        assert_eq!(object.style.stroke_width_mode, StrokeWidthMode::ScreenSpace);
        assert_eq!(object.style.stroke_join, StrokeJoin::Miter);
        assert_eq!(object.style.stroke_cap, StrokeCap::Butt);
    }
}

#[test]
fn core_style_default_remains_renderer_neutral() {
    let style = Style::default();
    assert_eq!(style.fill, Some(Color::WHITE));
    assert_eq!(style.stroke, None);
    assert_eq!(style.stroke_join, StrokeJoin::Round);
    assert_eq!(style.stroke_cap, StrokeCap::Round);
}
