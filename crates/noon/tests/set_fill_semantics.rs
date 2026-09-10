use noon::{AnimationOptions, Color, RateFunction, Scene, Style};

fn assert_paint(style: Style, color: Color, alpha: f32) {
    let fill = style.fill.expect("fill remains enabled");
    assert_eq!(
        (fill.red, fill.green, fill.blue, fill.alpha),
        (color.red, color.green, color.blue, alpha)
    );
    assert_eq!(style.stroke, Some(Color::GREEN));
    assert_eq!(style.stroke_width, 0.04);
    assert_eq!(style.opacity, 1.0);
}

#[test]
fn authored_and_animated_fill_keep_stroke_and_object_opacity_independent() {
    let mut scene = Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    let red = Color::RED;
    let green = Color::GREEN;
    let pink = Color::PINK;
    circle
        .set_stroke_color(green.red.into(), green.green.into(), green.blue.into(), 1.0)
        .unwrap();
    circle
        .set_fill(red.red.into(), red.green.into(), red.blue.into(), 0.25)
        .unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    assert_paint(session.frame().objects[0].style, red, 0.25);
    let id = session.frame().objects[0].id;
    let mut live = scene.live(&mut session);
    let target = live.target_editor(&circle).unwrap();
    live.set_fill(
        &target,
        pink.red.into(),
        pink.green.into(),
        pink.blue.into(),
        0.5,
    )
    .unwrap();
    let segment = live
        .declare_and_activate_transform_to(
            &circle,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    live.complete_segment(segment).unwrap();
    assert_paint(live.effective(&circle).unwrap().style, pink, 0.5);
    assert_eq!(session.frame().objects[0].id, id);
    assert_paint(session.frame().objects[0].style, pink, 0.5);
}

#[test]
fn partial_fill_edits_preserve_other_authored_paint_layers() {
    for mode in 0..3 {
        let scene = Scene::new();
        let mut object = scene.square(1.0).unwrap();
        object.set_fill(1.0, 0.0, 0.0, 0.25).unwrap();
        object.set_stroke_color(0.0, 1.0, 0.0, 1.0).unwrap();
        object.set_stroke_opacity(0.75).unwrap();
        object.set_object_opacity(0.8).unwrap();
        let before = object.state().unwrap().style;
        match mode {
            0 => object.set_fill_color(0.0, 0.0, 1.0, 1.0).unwrap(),
            1 => object.set_fill_opacity(0.5).unwrap(),
            _ => object.set_fill(0.0, 0.0, 1.0, 0.4).unwrap(),
        }
        let after = object.state().unwrap().style;
        assert_eq!(after.stroke, before.stroke);
        assert_eq!(after.stroke_opacity, before.stroke_opacity);
        assert_eq!(after.object_opacity, before.object_opacity);
        let expected_opacity = [0.25, 0.5, 0.4][mode];
        assert_eq!(object.fill_opacity().unwrap(), expected_opacity);
        if mode == 1 {
            assert_eq!(after.fill, before.fill);
        } else {
            assert_ne!(after.fill, before.fill);
            let mut expected = before.clone();
            expected.fill = after.fill.clone();
            expected.fill_opacity = expected_opacity;
            assert_eq!(after, expected);
        }
    }
}
