use super::*;
use crate::{effects::*, AnimationOptions, MobjectTarget, Vec2};

fn value(handle: &EffectHandle) -> Glow {
    let EffectDefinition::Glow(glow) = handle.authored_definition().unwrap();
    glow
}

#[test]
fn generic_and_canonical_methods_update_the_same_binding() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene
        .add_effect(
            &dot,
            Glow::new(GlowUpdate::default().radius(Pixels(12.0))).unwrap(),
            "glow",
        )
        .unwrap();
    let handle = scene.get_effect(&dot, "glow").unwrap();
    scene
        .set_glow(&dot, GlowUpdate::default().intensity(1.4))
        .unwrap();
    assert_eq!(
        scene.get_effect(&dot, "glow").unwrap().node_id(),
        handle.node_id()
    );
    assert_eq!(value(&handle).intensity(), 1.4);
    assert_eq!(value(&handle).radius(), GlowRadius::Pixels(12.0));
    scene
        .set_effect(&dot, &handle, GlowUpdate::default().intensity(0.0))
        .unwrap();
    assert!(value(&handle).is_neutral());
    assert_eq!(dot.get_effect("glow").unwrap().node_id(), handle.node_id());
}

#[test]
fn ordinary_target_copy_retains_independent_effects_and_motion_editing() {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.08).unwrap();
    dot.set_glow(GlowUpdate::default().intensity(0.25)).unwrap();
    scene.add(&dot).unwrap();
    let original = dot.get_effect("glow").unwrap();
    let mut target = dot.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_effect("glow", GlowUpdate::default().intensity(1.4))
        .unwrap();
    assert_eq!(value(&original).intensity(), 0.25);
    assert_eq!(value(&target.get_effect("glow").unwrap()).intensity(), 1.4);
    assert_ne!(
        original.node_id(),
        target.get_effect("glow").unwrap().node_id()
    );
    assert_eq!(target.state().unwrap().transform.translation.x, 2.0);
    assert_eq!(dot.state().unwrap().transform.translation.x, 0.0);
    // Ordinary declaration accepts the real target handle; playback is not
    // advertised before the corresponding execution profile exists.
    let declared = scene
        .declare_transform_to(&dot, &target, AnimationOptions::new().run_time(1.0))
        .unwrap();
    assert_eq!(declared.options().unwrap().run_time, Some(1.0));
}

#[test]
fn invalid_and_duplicate_calls_preserve_revision_and_prior_values() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    let before = scene.revision();
    assert!(scene
        .set_glow(
            &dot,
            GlowUpdate::default()
                .color(crate::Color::RED)
                .intensity(-1.0)
        )
        .is_err());
    assert!(scene.add_effect(&dot, Glow::default(), "glow").is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!(value(&dot.get_effect("glow").unwrap()), Glow::default());
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    assert_eq!(scene.revision(), before);
}

#[test]
fn stale_and_foreign_handles_do_not_select_a_replacement() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    let other = scene.circle(0.08).unwrap();
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    let old = dot.get_effect("glow").unwrap();
    assert!(scene
        .set_effect(&other, &old, GlowUpdate::default())
        .is_err());
    scene.remove_glow(&dot).unwrap();
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    assert!(scene.remove_effect(&dot, &old).is_err());
    assert!(old.authored_definition().is_err());
    let mut foreign = Scene::new();
    let foreign_dot = foreign.circle(0.08).unwrap();
    foreign
        .set_glow(&foreign_dot, GlowUpdate::default())
        .unwrap();
    let handle = foreign_dot.get_effect("glow").unwrap();
    assert!(matches!(
        scene.set_effect(&dot, &handle, GlowUpdate::default()),
        Err(AuthoringError::ForeignStore)
    ));
    assert!(matches!(
        scene.set_glow(&foreign_dot, GlowUpdate::default()),
        Err(AuthoringError::ForeignStore)
    ));
}

#[test]
fn detach_readd_and_wrapper_drop_preserve_authored_attachment() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene.add(&dot).unwrap();
    scene.set_glow(&dot, GlowUpdate::default()).unwrap();
    let id = dot.get_effect("glow").unwrap().node_id();
    scene.remove(&dot).unwrap();
    scene.add(&dot).unwrap();
    assert_eq!(dot.get_effect("glow").unwrap().node_id(), id);
    let revision = scene.revision();
    drop(dot.get_effect("glow").unwrap());
    assert_eq!(scene.revision(), revision);
    assert_eq!(dot.get_effect("glow").unwrap().node_id(), id);
}

#[test]
fn effect_declarations_fail_closed_at_execution_bootstrap() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene.add(&dot).unwrap();
    let plain = scene.execution_session().unwrap();
    drop(plain);
    // Even neutral attachments are semantic declarations; no runtime support
    // can be inferred by silently discarding them. This conservative M0 gate
    // includes detached target copies and is replaced by M1 lowering.
    scene
        .set_glow(&dot, GlowUpdate::default().intensity(0.0))
        .unwrap();
    assert!(scene.execution_session().is_err());
    let mut target = dot.target_editor().unwrap();
    scene.remove_glow(&dot).unwrap();
    assert!(scene.execution_session().is_err());
    target.remove_glow().unwrap();
    assert!(scene.execution_session().is_ok());
}

#[test]
fn rejected_live_attachment_preserves_both_scene_and_runtime() {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08).unwrap();
    scene.add(&dot).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = scene.revision();
    let nodes_before = scene.integration_store().borrow().len();
    let publication = session.publication_context();
    {
        let mut live = scene.live(&mut session);
        assert!(live.set_glow(&dot, GlowUpdate::default()).is_err());
        assert_eq!(
            live.effective(&dot).unwrap().transform.translation,
            Vec2::ZERO
        );
    }
    assert_eq!(scene.revision(), before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(scene.integration_store().borrow().len(), nodes_before);
    {
        let mut live = scene.live(&mut session);
        live.set_translation(&dot, 1.0, 0.0).unwrap();
        assert_eq!(
            live.effective(&dot).unwrap().transform.translation,
            Vec2::new(1.0, 0.0)
        );
    }
    assert!(dot.get_effect("glow").is_err());
    assert_eq!(scene.integration_store().borrow().len(), nodes_before);
    assert_ne!(scene.revision(), before); // only the successful ordinary edit
}

#[test]
fn aliased_family_copy_keeps_independent_leaf_attachments() {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.08).unwrap();
    dot.set_glow(GlowUpdate::default()).unwrap();
    let family = scene
        .family(&[MobjectTarget::Object(&dot), MobjectTarget::Object(&dot)])
        .unwrap();
    let copied = family.copy_family().unwrap();
    let copy_dot = copied.mobject(&dot).unwrap();
    assert_ne!(
        dot.get_effect("glow").unwrap().node_id(),
        copy_dot.get_effect("glow").unwrap().node_id()
    );
    assert_eq!(
        value(&dot.get_effect("glow").unwrap()),
        value(&copy_dot.get_effect("glow").unwrap())
    );
}

#[test]
fn remove_absent_glow_is_an_idempotent_authoring_operation() {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.08).unwrap();
    let before = scene.revision();
    dot.remove_glow().unwrap();
    scene.remove_glow(&dot).unwrap();
    assert_eq!(scene.revision(), before);
}

#[test]
fn state_only_replacement_fails_before_losing_attachment_correspondence() {
    let mut scene = Scene::new();
    let mut source = scene.circle(0.08).unwrap();
    let target = scene.circle(1.0).unwrap();
    source.set_glow(GlowUpdate::default()).unwrap();
    let before = source.state().unwrap();
    let revision = scene.revision();
    let result = source.become_handle(&target, crate::ManimBecomeOptions::default());
    assert!(matches!(
        result,
        Err(AuthoringError::EffectStateReplacementUnavailable)
    ));
    assert_eq!(source.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert!(source.get_effect("glow").is_ok());
}
