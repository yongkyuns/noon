use noon::{
    AnimationCompositionRequest, AnimationOptions, RateFunction, Scene, TransformToRequest,
};

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(2.)
        .rate_func(RateFunction::Linear)
}

#[test]
fn style_target_width_uses_existing_channel_and_retains_geometry() {
    let mut scene = Scene::new();
    let mut source = scene.circle(1.).unwrap();
    source.set_stroke_width(0.).unwrap();
    source.set_stroke_color(0., 0., 1., 1.).unwrap();
    let mut target = source.target_editor().unwrap();
    target.set_stroke_width(0.2).unwrap();
    target.set_stroke_color(1., 0., 0., 1.).unwrap();
    scene.add(&source).unwrap();
    let content = source.state().unwrap().content;
    let mut session = scene.execution_session().unwrap();
    let geometry = session.frame().render_geometry(0).unwrap().clone();
    let segment =
        scene
            .live(&mut session)
            .declare_and_activate_composition(
                &AnimationCompositionRequest::TransformTo(
                    TransformToRequest::point_correspondence(&source, &target, options()),
                ),
                AnimationOptions::new(),
            )
            .unwrap();
    let revision = scene.revision();
    session.advance_segment_to(segment, 1.).unwrap();
    let halfway = session.frame().objects[0].clone();
    assert!((halfway.style.stroke_width - 0.1).abs() < 1e-6);
    assert_eq!(
        halfway.style.stroke.unwrap(),
        noon::Color::rgb(0.5, 0., 0.5)
    );
    assert_eq!(session.frame().render_geometry(0), Some(&geometry));
    assert_eq!(source.state().unwrap().content, content);
    assert_eq!(scene.revision(), revision);
    session.seek(0.).unwrap();
    session.seek(1.).unwrap();
    assert_eq!(session.frame().objects[0].style, halfway.style);
    session.advance_segment_to(segment, 2.).unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(source.state().unwrap().style.stroke_width, 0.2);
    // Completion releases the width driver: a later persistent edit stays visible.
    scene
        .live(&mut session)
        .set_style(
            &source,
            noon::StyleUpdate {
                stroke_width: Some(0.03),
                ..Default::default()
            },
        )
        .unwrap();
    assert!((session.frame().objects[0].style.stroke_width - 0.03).abs() < 1e-6);
}

#[test]
fn paired_example_replays_both_width_segments() {
    let mut session = noon::example_scenes::animated_stroke_width::session().unwrap();
    for (time, expected) in [(0., 0.), (0.5, 0.1), (1., 0.2), (1.5, 0.11), (2., 0.02)] {
        session.seek(time).unwrap();
        assert!((session.frame().objects[0].style.stroke_width - expected).abs() < 1e-6);
    }
}
