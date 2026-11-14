use crate::{
    effects::{BackdropBlur, EffectUpdate, Pixels},
    AnimationCompositionRequest, AnimationOptions, MobjectFamily, RateFunction, Scene, Text,
};
use noon_core::EffectRadius;
use noon_runtime::{FrameState, ReplayLimits};

fn editorial_card() -> (Scene, MobjectFamily) {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).unwrap();
    background.set_fill(0.08, 0.12, 0.22, 1.0).unwrap();
    background.disable_stroke().unwrap();
    scene.add(&background).unwrap();
    let mut pane = scene.rectangle(3.8, 1.0).unwrap();
    pane.set_fill(1.0, 1.0, 1.0, 0.16).unwrap();
    pane.disable_stroke().unwrap();
    let mut heading = scene.text(Text::new("COLOR / GRAIN / GLASS")).unwrap();
    heading.set_translation(0.0, 0.16).unwrap();
    let mut detail = scene.text(Text::new("A cover in motion")).unwrap();
    detail.set_translation(0.0, -0.18).unwrap();
    let mut card = scene
        .family(&[(&pane).into(), (&heading).into(), (&detail).into()])
        .unwrap();
    card.add_effect(
        BackdropBlur::new(EffectUpdate::default().radius(Pixels(4.0))).unwrap(),
        "glass",
    )
    .unwrap();
    scene.add_many(&[(&card).into()]).unwrap();
    (scene, card)
}

fn glass_radius(frame: &FrameState) -> Option<EffectRadius> {
    let radii: Vec<_> = frame
        .objects
        .iter()
        .filter_map(|row| row.effects.as_ref())
        .flat_map(|stack| stack.iter())
        .filter_map(|effect| effect.definition.as_backdrop_blur())
        .map(|blur| blur.radius())
        .collect();
    assert_eq!(radii.len(), 1);
    radii.first().copied()
}

#[test]
fn editorial_backdrop_prefix_preserves_group_resources_and_radius_history() {
    let (scene, card) = editorial_card();
    let mut source = scene.into_execution().unwrap();
    source
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let wait = source.session().wait_segment(0.7).unwrap();
    source.session_mut().advance_to(wait.end_time()).unwrap();
    source.live().complete_segment(wait).unwrap();
    let (mut publisher, mut reader) = source
        .session_mut()
        .create_prepared_prefix(wait)
        .expect("a retained standalone backdrop should be publishable");
    let paused = reader.frame().clone();
    let painter_order = reader.painter_order().to_vec();
    assert_eq!(glass_radius(&paused), Some(EffectRadius::Pixels(4.0)));

    let movement = {
        let target = source.scene_mut().copy_family(&card).unwrap();
        source
            .scene_mut()
            .set_effect(
                target.root(),
                "glass",
                EffectUpdate::default().radius(Pixels(12.0)),
            )
            .unwrap();
        source
            .live()
            .declare_and_activate_composition(
                &AnimationCompositionRequest::FamilyMethodTargetTo {
                    source: &card,
                    target_state: target.root(),
                    options: AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Smooth),
                },
                AnimationOptions::new(),
            )
            .unwrap()
    };
    source
        .session_mut()
        .advance_to(movement.end_time())
        .unwrap();
    source.live().complete_segment(movement).unwrap();
    let suffix = source
        .session_mut()
        .publish_prepared_prefix(movement, &mut publisher)
        .expect("typed backdrop radius and group transform should be retained");
    reader.preflight_append(&suffix).unwrap();
    reader.append(suffix).unwrap();
    assert_eq!(
        reader.frame(),
        &paused,
        "suffix adoption does not advance the reader"
    );
    assert_eq!(reader.painter_order(), painter_order);

    source.session_mut().seal_replay().unwrap();
    reader
        .append(
            source
                .session_mut()
                .finish_prepared_prefix(movement, &mut publisher)
                .unwrap(),
        )
        .unwrap();
    for (time, radius) in [
        (0.0, 4.0),
        (0.35, 4.0),
        (0.7, 4.0),
        (1.2, 8.0),
        (1.7, 12.0),
        (0.5, 4.0),
    ] {
        source.seek(time).unwrap();
        reader.seek(time).unwrap();
        assert_eq!(
            reader.frame(),
            source.session().frame(),
            "at authored time {time}"
        );
        assert_eq!(reader.painter_order(), source.session().painter_order());
        assert_eq!(
            glass_radius(reader.frame()),
            Some(EffectRadius::Pixels(radius))
        );
    }
    assert!(source
        .session()
        .frame()
        .objects
        .iter()
        .any(|row| row.effects.as_ref().is_some_and(|stack| {
            stack
                .iter()
                .any(|effect| effect.definition.as_backdrop_blur().is_some())
        })));
}

#[test]
fn zero_resource_budget_does_not_change_backdrop_source_at_handoff() {
    let (scene, _card) = editorial_card();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let wait = source.wait_segment(0.7).unwrap();
    source.advance_to(wait.end_time()).unwrap();
    scene.live(&mut source).complete_segment(wait).unwrap();
    let before = source.frame().clone();

    assert!(matches!(
        source.create_prepared_prefix_with_resource_budget(wait, 0),
        Err(noon_runtime::PrefixError::ResourceBudgetExceeded {
            limit_bytes: 0,
            additional_bytes,
            ..
        }) if additional_bytes > 0
    ));
    assert_eq!(source.frame(), &before);
}
