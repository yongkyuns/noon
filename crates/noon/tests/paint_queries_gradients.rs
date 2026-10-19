use noon::{color_gradient, Color, Scene, SemanticPaint, SemanticSheenDirection, StyleUpdate};
use noon_core::SemanticVec3;

#[test]
fn matching_color_preserves_opacity_geometry_and_aliased_family_locality() {
    let mut scene = Scene::new();
    let mut donor = scene.circle(0.2).unwrap();
    donor.set_fill(1.0, 0.0, 0.0, 0.0).unwrap();
    donor.set_stroke_color(0.0, 0.0, 1.0, 1.0).unwrap();
    let mut recipient = scene.square(0.5).unwrap();
    recipient.set_fill(1.0, 0.0, 0.0, 0.25).unwrap();
    recipient.set_stroke_opacity(0.6).unwrap();
    let content = recipient.state().unwrap().content;
    recipient.match_color(&donor).unwrap();
    assert_eq!(recipient.manim_color().unwrap(), Color::rgb(0.0, 0.0, 1.0));
    assert_eq!(recipient.fill_opacity().unwrap(), 0.25);
    assert_eq!(recipient.stroke_opacity().unwrap(), 0.6);
    assert_eq!(recipient.state().unwrap().content, content);
    let other = scene.square(0.3).unwrap();
    let untouched = other.state().unwrap();
    let family = scene
        .family(&[(&recipient).into(), (&recipient).into()])
        .unwrap();
    donor.set_stroke_color(0.0, 1.0, 0.0, 1.0).unwrap();
    let before = scene.revision();
    family.match_color(&donor).unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(recipient.manim_color().unwrap(), Color::rgb(0.0, 1.0, 0.0));
    assert_eq!(other.state().unwrap(), untouched);
}

#[test]
fn live_matching_captures_current_donor_color_and_rejects_foreign_donor_atomically() {
    let mut scene = Scene::new();
    let donor = scene.circle(0.2).unwrap();
    let recipient = scene.square(0.3).unwrap();
    let family = scene
        .family(&[(&recipient).into(), (&recipient).into()])
        .unwrap();
    scene
        .add_many(&[(&donor).into(), (&family).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.circle(0.1).unwrap();
    let mut live = scene.live(&mut session);
    live.set_color(&donor, 0.0, 0.0, 1.0, 1.0).unwrap();
    let revision = family.integration_store().borrow().scene_revision();
    let result = live.match_family_color(&family, &donor).unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert!(result.impacts().iter().any(|impact| matches!(impact,
        noon_core::SemanticMutationImpact::FamilyColor { node } if *node == family.node_id())));
    assert_eq!(family.own_color().unwrap(), Color::rgb(0.0, 0.0, 1.0));
    assert_eq!(
        family.integration_store().borrow().scene_revision(),
        revision.checked_next().unwrap()
    );
    assert_eq!(
        live.effective_manim_color(&recipient).unwrap(),
        Color::rgb(0.0, 0.0, 1.0)
    );
    let before = recipient.state().unwrap();
    assert!(live.match_color(&recipient, &foreign).is_err());
    assert_eq!(recipient.state().unwrap(), before);
}

#[test]
fn live_manim_interpolate_color_publishes_effective_style_once() {
    let mut scene = Scene::new();
    let receiver = scene.square(0.5).unwrap();
    let start = scene.square(0.25).unwrap();
    let end = scene.square(0.75).unwrap();
    for object in [&receiver, &start, &end] {
        scene.add(object).unwrap();
    }

    let mut start_state = start.state().unwrap();
    start_state.style.fill = Some(SemanticPaint::Solid(Color::RED));
    start_state.style.fill_opacity = 0.2;
    start_state.style.stroke = Some(SemanticPaint::Solid(Color::BLUE));
    start_state.style.stroke_opacity = 0.25;
    start_state.style.stroke_width = 0.02;
    start_state.sheen_direction = SemanticSheenDirection::new(SemanticVec3::new(-1.0, 0.0, 0.0), 3);
    let mut start = start;
    start.commit_state(start_state).unwrap();

    let mut end_state = end.state().unwrap();
    end_state.style.fill = Some(SemanticPaint::Solid(Color::BLUE));
    end_state.style.fill_opacity = 0.8;
    end_state.style.stroke = Some(SemanticPaint::Solid(Color::RED));
    end_state.style.stroke_opacity = 0.75;
    end_state.style.stroke_width = 0.06;
    end_state.sheen_direction = SemanticSheenDirection::new(SemanticVec3::new(1.0, 2.0, 0.0), 3);
    let mut end = end;
    end.commit_state(end_state).unwrap();

    let mut session = scene.execution_session().unwrap();
    let before = scene.revision();
    let result = scene
        .live(&mut session)
        .interpolate_manim_style(&receiver, &start, &end, 0.5)
        .unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(result.impacts().len(), 1);
    let state = receiver.state().unwrap();
    let midpoint = Color {
        red: (Color::RED.red + Color::BLUE.red) * 0.5,
        green: (Color::RED.green + Color::BLUE.green) * 0.5,
        blue: (Color::RED.blue + Color::BLUE.blue) * 0.5,
        alpha: (Color::RED.alpha + Color::BLUE.alpha) * 0.5,
    };
    assert_eq!(state.style.fill, Some(SemanticPaint::Solid(midpoint)));
    assert!((state.style.fill_opacity - 0.5).abs() < 1.0e-12);
    assert_eq!(state.style.stroke, Some(SemanticPaint::Solid(midpoint)));
    assert!((state.style.stroke_opacity - 0.5).abs() < 1.0e-12);
    assert_eq!(state.style.stroke_width, 0.04);
    assert_eq!(
        state.sheen_direction.value,
        SemanticVec3::new(0.0, 1.0, 0.0)
    );
}

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
fn late_alias_gradient_uses_last_occurrence_order_authored_and_live_atomically() {
    let mut scene = Scene::new();
    let a = scene.square(0.2).unwrap();
    let b = scene.circle(0.2).unwrap();
    let unrelated = scene.square(0.1).unwrap();
    let unchanged = unrelated.state().unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    family.set_fill(None, Some(0.4)).unwrap();
    let before = scene.revision();
    family
        .set_color_by_gradient(&[Color::RED, Color::BLUE])
        .unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(a.manim_color().unwrap(), Color::BLUE);
    assert_eq!(b.manim_color().unwrap(), Color::RED);
    scene
        .add_many(&[(&family).into(), (&unrelated).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let result = live
        .set_family_color_by_gradient(&family, &[Color::BLUE, Color::RED])
        .unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(live.effective_manim_color(&a).unwrap(), Color::RED);
    assert_eq!(live.effective_manim_color(&b).unwrap(), Color::BLUE);
    assert_eq!(a.fill_opacity().unwrap(), 0.4);
    assert_eq!(unrelated.state().unwrap(), unchanged);
    let before = (a.state().unwrap(), b.state().unwrap());
    assert!(live
        .set_family_color_by_gradient(&family, &[Color::rgb(f32::NAN, 0.0, 0.0)])
        .is_err());
    assert_eq!((a.state().unwrap(), b.state().unwrap()), before);
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
    assert_eq!(colors[0].blue, 1.0);
    assert_eq!(colors[1].red, 1.0);
    assert_eq!(colors[2].red, 0.5);
    assert_eq!(colors[2].green, 0.5);
    assert_eq!(colors[3].green, 1.0);
    assert_eq!(colors[4].blue, 0.5);
    assert!(colors.iter().all(|color| (color.alpha - 0.7).abs() < 1e-6));
}
