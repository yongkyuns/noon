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
