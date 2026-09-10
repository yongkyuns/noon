use noon::{color_gradient, Color, Scene, SemanticPaint, StyleUpdate};

#[test]
fn color_observation_prefers_visible_fill_and_does_not_reapply_opacity() {
    let mut scene = Scene::new();
    let mut object = scene.square(1.0).unwrap();
    object
        .set_style(StyleUpdate {
            fill_color: Some(Color::RED),
            fill_opacity: Some(0.25),
            stroke_color: Some(Color::BLUE),
            stroke_opacity: Some(0.7),
            stroke_width: Some(0.09),
        })
        .unwrap();
    object.set_object_opacity(0.0).unwrap();
    assert_eq!(object.manim_color().unwrap(), Color::RED);
    assert_eq!(object.fill_color().unwrap(), Some(Color::RED));
    assert_eq!(object.stroke_color().unwrap(), Some(Color::BLUE));
    let color = object.manim_color().unwrap();
    object
        .set_color(
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )
        .unwrap();
    assert_eq!(object.fill_opacity().unwrap(), 0.25);
    assert_eq!(object.stroke_width().unwrap(), 0.09);
    object.set_fill_opacity(0.0).unwrap();
    object
        .set_stroke_color(
            Color::BLUE.red.into(),
            Color::BLUE.green.into(),
            Color::BLUE.blue.into(),
            1.0,
        )
        .unwrap();
    assert_eq!(object.manim_color().unwrap(), Color::BLUE);
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();
    let live = scene.live(&mut session);
    assert_eq!(live.effective_manim_color(&object).unwrap(), Color::BLUE);
    assert_eq!(
        live.effective_fill_color(&object).unwrap(),
        Some(Color::RED)
    );
    assert_eq!(
        live.effective_stroke_color(&object).unwrap(),
        Some(Color::BLUE)
    );
    assert!((live.effective_stroke_width(&object).unwrap() - 0.09).abs() < 1e-7);
}

#[test]
fn gradient_interpolation_handles_empty_single_and_multiple_stops() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    assert!(color_gradient(&[], 0).is_err());
    assert!(color_gradient(&[Color::rgb(f32::NAN, 0.0, 0.0)], 0).is_err());
    assert!(color_gradient(&[red], 0).unwrap().is_empty());
    assert_eq!(color_gradient(&[red, blue], 1).unwrap(), [red]);
    assert_eq!(
        color_gradient(&[red, blue], 3).unwrap(),
        [red, Color::rgb(0.5, 0.0, 0.5), blue]
    );
    assert_eq!(
        color_gradient(&[red, blue, Color::GREEN], 3).unwrap(),
        [red, blue, Color::GREEN]
    );
}

#[test]
fn aliased_family_gradient_is_one_local_publication_and_preserves_content_and_opacity() {
    let mut scene = Scene::new();
    let a = scene.square(0.2).unwrap();
    let b = scene.circle(0.2).unwrap();
    let c = scene.square(0.3).unwrap();
    let unrelated = scene.square(0.1).unwrap();
    let untouched = unrelated.state().unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene
        .family(&[(&a).into(), (&nested).into(), (&c).into()])
        .unwrap();
    family.set_fill(None, Some(0.4)).unwrap();
    let content = a.state().unwrap().content;
    scene
        .add_many(&[(&family).into(), (&unrelated).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let result = live
        .set_family_color_by_gradient(
            &family,
            &[Color::rgb(1.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0)],
        )
        .unwrap();
    assert_eq!(result.impacts().len(), 3);
    assert_eq!(
        live.effective_manim_color(&b).unwrap(),
        Color::rgb(0.5, 0.0, 0.5)
    );
    assert_eq!(a.fill_opacity().unwrap(), 0.4);
    assert_eq!(a.state().unwrap().content, content);
    assert_eq!(unrelated.state().unwrap(), untouched);
    let before = a.state().unwrap();
    assert!(live.set_family_color_by_gradient(&family, &[]).is_err());
    assert_eq!(a.state().unwrap(), before);
    assert_eq!(
        before.style.fill,
        Some(SemanticPaint::Solid(Color::rgb(1.0, 0.0, 0.0)))
    );
}

#[test]
fn paired_gradient_example_reaches_the_ordinary_retained_runtime() {
    let session = noon::example_scenes::paint_queries_gradients::session().unwrap();
    let colors: Vec<_> = session
        .frame()
        .objects
        .iter()
        .map(|object| object.style.fill.unwrap())
        .collect();
    assert_eq!(colors.len(), 5);
    assert_eq!(colors[0].red, 1.0);
    assert_eq!(colors[1].red, 0.5);
    assert_eq!(colors[1].green, 0.5);
    assert_eq!(colors[2].green, 1.0);
    assert_eq!(colors[4].blue, 1.0);
    assert!(colors.iter().all(|color| (color.alpha - 0.7).abs() < 1e-6));
}
