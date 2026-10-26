use super::*;
use noon_core::{SemanticClickIndicate, SemanticVec3, YELLOW};

fn action(target: &Mobject) -> Option<SemanticClickAction> {
    target.state().unwrap().click_action()
}

#[test]
fn click_declaration_is_inert_and_copies_authored_pulse_options() {
    let mut scene = Scene::new();
    let target = scene.circle(1.0).unwrap();
    scene.add(&target).unwrap();
    let declaration = scene
        .declare_glow_pulse(&target, 2.0, AnimationOptions::new())
        .unwrap();
    let nodes = scene.integration_store().borrow().len();
    scene.on_click(&target, Some(&declaration)).unwrap();
    assert_eq!(
        action(&target),
        Some(SemanticClickAction::GlowPulse(SemanticClickGlowPulse::new(
            2.0,
            1.0,
            RateFunction::ThereAndBack,
            false
        )))
    );
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert!(!scene.integration_store().borrow().has_effect_attachments());
    assert!(!scene
        .execution_session()
        .unwrap()
        .has_replay_timeline_work());

    let copied = target.copy_handle().unwrap();
    scene.on_click(&target, None).unwrap();
    assert_eq!(action(&target), None);
    assert!(matches!(
        action(&copied),
        Some(SemanticClickAction::GlowPulse(_))
    ));
    scene
        .on_click_glow_pulse(
            &target,
            3.0,
            AnimationOptions::new()
                .run_time(0.4)
                .rate_func(RateFunction::Linear)
                .reverse_rate_function(true),
        )
        .unwrap();
    assert_eq!(
        action(&target),
        Some(SemanticClickAction::GlowPulse(SemanticClickGlowPulse::new(
            3.0,
            0.4,
            RateFunction::Linear,
            true
        )))
    );
    assert!(!scene.integration_store().borrow().has_effect_attachments());
}

#[test]
fn click_declarations_reject_foreign_stale_different_target_and_other_intents_before_mutation() {
    let mut scene = Scene::new();
    let target = scene.circle(1.0).unwrap();
    let other = scene.square(1.0).unwrap();
    let mut foreign_scene = Scene::new();
    let foreign_target = foreign_scene.circle(1.0).unwrap();
    let foreign = foreign_scene
        .declare_glow_pulse(&foreign_target, 2.0, AnimationOptions::new())
        .unwrap();
    let different = scene
        .declare_glow_pulse(&other, 2.0, AnimationOptions::new())
        .unwrap();
    let unsupported = scene
        .declare_transform_to(&target, &other, AnimationOptions::new())
        .unwrap();
    let stale = scene
        .declare_glow_pulse(&target, 2.0, AnimationOptions::new())
        .unwrap();
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_animation(stale.node_id());
    remove
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    scene
        .on_click_glow_pulse(&target, 1.5, AnimationOptions::new())
        .unwrap();
    let previous = action(&target);
    for declaration in [&foreign, &different, &unsupported, &stale] {
        let revision = scene.integration_store().borrow().scene_revision();
        assert!(scene.on_click(&target, Some(declaration)).is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(action(&target), previous);
    }
    assert!(matches!(
        scene.on_click(&foreign_target, None),
        Err(AuthoringError::ForeignStore)
    ));
    assert!(!scene.integration_store().borrow().has_effect_attachments());
}

#[test]
fn click_glow_pulse_options_are_validated_before_replacing_the_action() {
    let mut scene = Scene::new();
    let target = scene.circle(1.0).unwrap();
    scene
        .on_click_indicate(
            &target,
            crate::IndicateOptions::new(1.2, YELLOW),
            AnimationOptions::new(),
        )
        .unwrap();
    let previous = action(&target);
    for options in [
        AnimationOptions::new().run_time(0.0),
        AnimationOptions::new().run_time(-1.0),
        AnimationOptions::new().run_time(f64::NAN),
        AnimationOptions::new().run_time(f64::INFINITY),
        AnimationOptions::new().lag_ratio(0.2),
        AnimationOptions::new().lag_ratio(f64::NAN),
        AnimationOptions::new().path_arc(0.3),
        AnimationOptions::new().path_arc(1e-14),
        AnimationOptions::new().remover(true),
        AnimationOptions::new().introducer(true),
        AnimationOptions::new().rate_func(RateFunction::EaseOutBack),
    ] {
        let revision = scene.integration_store().borrow().scene_revision();
        assert!(scene.on_click_glow_pulse(&target, 2.0, options).is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(action(&target), previous);
    }
    for intensity in [f64::NAN, f64::INFINITY, -0.1, 8.1] {
        assert!(scene
            .on_click_glow_pulse(&target, intensity, AnimationOptions::new())
            .is_err());
        assert_eq!(action(&target), previous);
    }
    assert!(!scene.integration_store().borrow().has_effect_attachments());
}

#[test]
fn generic_click_indicate_reuses_existing_binding_and_live_actions_clear_and_replace() {
    let mut scene = Scene::new();
    let target = scene.circle(1.0).unwrap();
    scene.add(&target).unwrap();
    let indication = scene
        .declare_animation(
            SemanticAnimationIntent::Indicate {
                target: target.node_id(),
                scale_factor: 1.2,
                color: YELLOW,
                scale_center: SemanticVec3::ZERO,
            },
            AnimationOptions::new().run_time(0.4),
        )
        .unwrap();
    let pulse = scene
        .declare_glow_pulse(&target, 2.0, AnimationOptions::new())
        .unwrap();
    scene.on_click(&target, Some(&indication)).unwrap();
    assert_eq!(
        action(&target),
        Some(SemanticClickAction::Indicate(SemanticClickIndicate::new(
            1.2, YELLOW, 0.4
        )))
    );
    let mut session = scene.execution_session().unwrap();
    {
        let mut live = scene.live(&mut session);
        live.on_click(&target, Some(&pulse)).unwrap();
        assert!(matches!(
            action(&target),
            Some(SemanticClickAction::GlowPulse(_))
        ));
        live.on_click(&target, None).unwrap();
        assert_eq!(action(&target), None);
        live.on_click_glow_pulse(
            &target,
            1.5,
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )
        .unwrap();
    }
    assert_eq!(
        action(&target),
        Some(SemanticClickAction::GlowPulse(SemanticClickGlowPulse::new(
            1.5,
            1.0,
            RateFunction::Linear,
            false
        )))
    );
    assert!(!scene.integration_store().borrow().has_effect_attachments());
    assert!(!session.has_replay_timeline_work());
}
