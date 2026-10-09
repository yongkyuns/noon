//! Public Scene/LiveSession integration: no compiler, runtime, or host bypass.
use super::*;
use crate::{AnimationOptions, Color, ExecutionSession, RateFunction};
use noon_core::{Pixels, SemanticObjectProperty, SemanticVec3};

fn fixture(attached: bool) -> (Scene, Mobject) {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.4).unwrap();
    object.disable_stroke().unwrap();
    object.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    scene.add(&object).unwrap();
    if attached {
        scene.set_glow(&object, initial()).unwrap();
    }
    (scene, object)
}
fn initial() -> GlowUpdate {
    GlowUpdate::default()
        .radius(Pixels(3.25))
        .color(Color::RED)
        .intensity(0.4000000000000123)
}
fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}
fn definition(object: &Mobject) -> noon_core::Glow {
    let EffectDefinition::Glow(value) = object
        .get_effect("glow")
        .unwrap()
        .authored_definition()
        .unwrap();
    value
}
fn row<'a>(session: &'a ExecutionSession, object: &Mobject) -> &'a noon_runtime::FrameObjectState {
    let id = session.execution_object_id(object.node_id()).unwrap();
    session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap()
}

#[test]
fn supported_bootstrap_keeps_identity_and_clean_publications() {
    let (scene, source) = fixture(true);
    let revision = scene.revision();
    let id = source.get_effect("glow").unwrap().node_id();
    let mut session = scene.execution_session().unwrap();
    assert_eq!(row(&session, &source).glow.as_ref().unwrap().attachment, id);
    assert_eq!(
        row(&session, &source).glow.as_ref().unwrap().definition,
        definition(&source)
    );
    assert!(session.take_renderer_publication().changes().is_all());
    assert!(session.take_renderer_publication().changes().is_empty());
    assert_eq!(scene.revision(), revision);
}

#[test]
fn predeclared_motion_and_glow_complete_through_public_scene() {
    let (scene, source) = fixture(true);
    let old = definition(&source);
    let mut target = source.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_glow(
            GlowUpdate::default()
                .radius(Pixels(6.5))
                .color(Color::BLUE)
                .intensity(1.4),
        )
        .unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let middle = row(&session, &source).glow.as_ref().unwrap().definition;
    assert_eq!(
        middle.intensity(),
        old.intensity() + (1.4 - old.intensity()) * 0.5
    );
    assert_eq!(middle.radius(), Pixels(4.875).into());
    assert_eq!(row(&session, &source).transform.translation.x, 1.0);
    assert_eq!(
        definition(&source),
        old,
        "active values do not rewrite authored state"
    );
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(definition(&source).intensity(), 1.4);
    assert_eq!(definition(&source).color(), Color::BLUE);
    assert_eq!(definition(&source).radius(), Pixels(6.5).into());
    assert_eq!(
        row(&session, &source).glow.as_ref().unwrap().definition,
        definition(&source)
    );
    scene
        .live(&mut session)
        .set_glow(&source, GlowUpdate::default().intensity(0.2))
        .unwrap();
    assert_eq!(
        row(&session, &source)
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity(),
        0.2,
        "completion released the old driver"
    );
}

#[test]
fn live_target_preserves_an_independent_attachment_then_activates() {
    let (scene, source) = fixture(true);
    let mut session = scene.execution_session().unwrap();
    session.take_renderer_publication();
    let target = scene.live(&mut session).target_editor(&source).unwrap();
    assert_eq!(definition(&target), definition(&source));
    assert_ne!(
        target.get_effect("glow").unwrap().node_id(),
        source.get_effect("glow").unwrap().node_id()
    );
    assert!(session.take_renderer_publication().changes().is_empty());
    scene
        .live(&mut session)
        .set_glow(&target, GlowUpdate::default().intensity(1.5))
        .unwrap();
    scene.live(&mut session).shift(&target, 2.0, 0.0).unwrap();
    assert!(session.take_renderer_publication().changes().is_empty());
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(&source, &target, options())
        .unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(definition(&source).intensity(), 1.5);
    assert_eq!(row(&session, &source).transform.translation.x, 2.0);
}

#[test]
fn live_add_remove_and_stale_handle_preserve_source_and_spatial_state() {
    let (scene, source) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let object_id = session.execution_object_id(source.node_id()).unwrap();
    let layout = scene.live(&mut session).effective_layout(&source).unwrap();
    session.take_renderer_publication();
    scene
        .live(&mut session)
        .set_glow(&source, initial())
        .unwrap();
    let original = source.get_effect("glow").unwrap();
    assert_eq!(
        row(&session, &source).glow.as_ref().unwrap().attachment,
        original.node_id()
    );
    scene.live(&mut session).remove_glow(&source).unwrap();
    assert!(row(&session, &source).glow.is_none());
    scene
        .live(&mut session)
        .set_glow(&source, initial())
        .unwrap();
    assert_ne!(
        source.get_effect("glow").unwrap().node_id(),
        original.node_id()
    );
    let before = session.publication_context();
    assert!(scene
        .live(&mut session)
        .remove_effect(&source, &original)
        .is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(
        session.execution_object_id(source.node_id()),
        Some(object_id)
    );
    let after_layout = scene.live(&mut session).effective_layout(&source).unwrap();
    assert_eq!(
        (after_layout.center, after_layout.width, after_layout.height),
        (layout.center, layout.width, layout.height)
    );
    scene.live(&mut session).remove_glow(&source).unwrap();
    let before = session.publication_context();
    scene.live(&mut session).remove_glow(&source).unwrap();
    assert_eq!(
        session.publication_context(),
        before,
        "absent removal is idle"
    );
}

#[test]
fn public_glow_profile_rejects_invalid_live_edits_without_partial_commit() {
    let (scene, source) = fixture(true);
    let mut session = scene.execution_session().unwrap();
    let before = session.publication_context();
    let original = row(&session, &source).clone();
    assert!(scene.live(&mut session).disable_fill(&source).is_err());
    assert!(scene
        .live(&mut session)
        .add_effect(&source, noon_core::Glow::default(), "other")
        .is_err());
    let mut batch = SemanticMutationTransaction::new();
    batch.set_property(
        source.node_id(),
        SemanticObjectProperty::Translation,
        SemanticVec3::new(9.0, 0.0, 0.0),
    );
    batch.create_effect(source.node_id(), "second", noon_core::Glow::default());
    assert!(scene.live(&mut session).apply(batch).is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(row(&session, &source), &original);
    assert!(source.get_effect("second").is_err());
}

#[test]
fn detached_unsupported_target_is_inert_until_membership_admission() {
    let (scene, source) = fixture(false);
    let mut target = source.target_editor().unwrap();
    target.set_glow(initial()).unwrap();
    target
        .add_effect(noon_core::Glow::default(), "second")
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    assert!(row(&session, &source).glow.is_none());
    let before = session.publication_context();
    assert!(scene.live(&mut session).add(&target).is_err());
    assert_eq!(session.publication_context(), before);
    assert!(session.execution_object_id(target.node_id()).is_none());
}

#[test]
fn public_replay_restores_original_attachment_after_completed_motion_and_removal() {
    let (scene, source) = fixture(true);
    let original = definition(&source);
    let original_id = source.get_effect("glow").unwrap().node_id();
    let mut target = source.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_glow(GlowUpdate::default().intensity(1.4))
        .unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    scene.live(&mut session).remove_glow(&source).unwrap();
    let wait = scene.live(&mut session).wait_segment(1.0).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(wait, 2.0)
        .unwrap();
    scene.live(&mut session).complete_segment(wait).unwrap();
    session.seal_replay().unwrap();
    for _ in 0..3 {
        session.seek(0.5).unwrap();
        let glow = row(&session, &source).glow.as_ref().unwrap();
        assert_eq!(glow.attachment, original_id);
        assert_eq!(
            glow.definition.intensity(),
            original.intensity() + (1.4 - original.intensity()) * 0.5
        );
        assert_eq!(row(&session, &source).transform.translation.x, 1.0);
        session.seek(2.0).unwrap();
        assert!(row(&session, &source).glow.is_none());
        assert!(
            source.get_effect("glow").is_err(),
            "replay does not rewind authored identity"
        );
    }
}

#[test]
fn returning_and_reversed_rates_reconcile_exact_public_endpoints() {
    for (rate, reverse) in [
        (RateFunction::ThereAndBack, false),
        (RateFunction::Linear, true),
    ] {
        let (scene, source) = fixture(true);
        let original = definition(&source);
        let mut target = source.target_editor().unwrap();
        target
            .set_glow(GlowUpdate::default().intensity(1.4))
            .unwrap();
        let animation = scene
            .declare_transform_to(
                &source,
                &target,
                options().rate_func(rate).reverse_rate_function(reverse),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        scene
            .live(&mut session)
            .advance_segment_to(segment, 1.0)
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        assert_eq!(definition(&source), original);
        assert_eq!(
            row(&session, &source).glow.as_ref().unwrap().definition,
            original
        );
    }
}

#[test]
fn pending_segment_keeps_the_existing_publication_barrier() {
    let (scene, source) = fixture(true);
    let mut target = source.target_editor().unwrap();
    target
        .set_glow(GlowUpdate::default().intensity(1.4))
        .unwrap();
    let animation = scene
        .declare_transform_to(&source, &target, options())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    let before = session.publication_context();
    let count = scene.integration_store().borrow().len();
    assert!(scene.live(&mut session).remove_glow(&source).is_err());
    assert!(scene.live(&mut session).target_editor(&source).is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(scene.integration_store().borrow().len(), count);

    // The target-copy preparation helper must still capture the current value,
    // not the authored base. Abort the prepared copy: the ordinary live barrier
    // above remains authoritative about when it may be published.
    let store = scene.integration_store().borrow();
    let mut copy = SemanticMutationTransaction::new();
    let target = copy.create_node(noon_core::SemanticNodeCreation::object(
        source.state().unwrap(),
    ));
    crate::effective_capture::stage_effect_copy(
        &store,
        &session,
        source.node_id(),
        target,
        &mut copy,
    )
    .unwrap();
    drop(store);
    let mut store = scene.integration_store().borrow_mut();
    let prepared = copy.prepare(&mut store).unwrap();
    let effects = prepared.animation_effect_snapshot(target).unwrap();
    let EffectDefinition::Glow(captured) = effects[0].definition;
    assert_eq!(
        captured,
        row(&session, &source).glow.as_ref().unwrap().definition
    );
    assert_ne!(captured, definition_from_initial());
    drop(prepared);
    assert_eq!(store.len(), count);
}

fn definition_from_initial() -> noon_core::Glow {
    noon_core::Glow::new(initial()).unwrap()
}

#[test]
fn absent_to_glow_enrolls_neutral_at_activation_and_completes_normally() {
    let (scene, source) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let source_execution_id = session.execution_object_id(source.node_id());
    let target = scene.live(&mut session).target_editor(&source).unwrap();
    scene
        .live(&mut session)
        .set_glow(&target, initial())
        .unwrap();
    assert!(
        source.get_effect("glow").is_err(),
        "target building is inert"
    );
    assert!(row(&session, &source).glow.is_none());
    session.take_renderer_publication();

    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(&source, &target, options())
        .unwrap();
    let attached = source.get_effect("glow").unwrap();
    let new_generation = attached.node_id();
    assert_ne!(target.get_effect("glow").unwrap().node_id(), new_generation);
    assert_eq!(
        session.execution_object_id(source.node_id()),
        source_execution_id
    );
    assert_eq!(definition(&source).intensity(), 0.0);
    assert_eq!(
        row(&session, &source).glow.as_ref().unwrap().attachment,
        new_generation
    );
    assert_eq!(
        row(&session, &source)
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity(),
        0.0
    );
    assert!(!session.take_renderer_publication().changes().is_empty());

    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    assert_eq!(
        row(&session, &source)
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity(),
        initial().intensity.unwrap() * 0.5,
    );
    assert_eq!(definition(&source).intensity(), 0.0);
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        definition(&source).intensity(),
        initial().intensity.unwrap()
    );
    assert_eq!(source.get_effect("glow").unwrap().node_id(), new_generation);
    scene.live(&mut session).remove_glow(&source).unwrap();
    assert!(row(&session, &source).glow.is_none());
    assert!(
        attached.authored_definition().is_err(),
        "old handle is stale"
    );
}

#[test]
fn absent_to_glow_replay_restores_neutral_generation_and_never_restores_authored_identity() {
    let (scene, source) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    let target = scene.live(&mut session).target_editor(&source).unwrap();
    scene
        .live(&mut session)
        .set_glow(&target, initial())
        .unwrap();
    let animation = scene
        .live(&mut session)
        .declare_and_activate_transform_to(&source, &target, options())
        .unwrap();
    let new_generation = source.get_effect("glow").unwrap().node_id();
    scene
        .live(&mut session)
        .advance_segment_to(animation, 1.0)
        .unwrap();
    scene
        .live(&mut session)
        .complete_segment(animation)
        .unwrap();
    scene.live(&mut session).remove_glow(&source).unwrap();
    let wait = scene.live(&mut session).wait_segment(1.0).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(wait, 2.0)
        .unwrap();
    scene.live(&mut session).complete_segment(wait).unwrap();
    session.seal_replay().unwrap();
    for _ in 0..3 {
        session.seek(0.5).unwrap();
        let glow = row(&session, &source).glow.as_ref().unwrap();
        assert_eq!(glow.attachment, new_generation);
        assert_eq!(
            glow.definition.intensity(),
            initial().intensity.unwrap() * 0.5
        );
        session.seek(2.0).unwrap();
        assert!(row(&session, &source).glow.is_none());
        assert!(source.get_effect("glow").is_err());
    }
}

#[test]
fn returning_absent_to_glow_reconciles_neutral_without_losing_attachment_identity() {
    let (scene, source) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let target = scene.live(&mut session).target_editor(&source).unwrap();
    scene
        .live(&mut session)
        .set_glow(&target, initial())
        .unwrap();
    let animation = scene
        .live(&mut session)
        .declare_and_activate_transform_to(
            &source,
            &target,
            options().rate_func(RateFunction::ThereAndBack),
        )
        .unwrap();
    let fresh = source.get_effect("glow").unwrap().node_id();
    scene
        .live(&mut session)
        .advance_segment_to(animation, 0.5)
        .unwrap();
    assert!(
        row(&session, &source)
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity()
            > 0.0
    );
    scene
        .live(&mut session)
        .advance_segment_to(animation, 1.0)
        .unwrap();
    scene
        .live(&mut session)
        .complete_segment(animation)
        .unwrap();
    assert_eq!(definition(&source).intensity(), 0.0);
    assert_eq!(
        row(&session, &source).glow.as_ref().unwrap().attachment,
        fresh
    );
    assert_eq!(
        row(&session, &source)
            .glow
            .as_ref()
            .unwrap()
            .definition
            .intensity(),
        0.0
    );
}

#[test]
fn unsupported_absent_target_glow_does_not_allocate_or_publish() {
    let (scene, source) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let target = scene.live(&mut session).target_editor(&source).unwrap();
    scene
        .live(&mut session)
        .set_glow(&target, initial())
        .unwrap();
    scene
        .live(&mut session)
        .add_effect(&target, noon_core::Glow::default(), "other")
        .unwrap();
    let before = session.publication_context();
    let count = scene.integration_store().borrow().len();
    assert!(scene
        .live(&mut session)
        .declare_and_activate_transform_to(&source, &target, options())
        .is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert!(source.get_effect("glow").is_err());
    assert!(row(&session, &source).glow.is_none());
}
