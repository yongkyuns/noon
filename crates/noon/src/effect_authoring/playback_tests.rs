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
fn initial() -> EffectUpdate {
    EffectUpdate::default()
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
    (object
        .get_effect("glow")
        .unwrap()
        .authored_definition()
        .unwrap())
    .as_glow()
    .unwrap()
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
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap()).attachment,
        id
    );
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap()).definition,
        (definition(&source)).into()
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
            EffectUpdate::default()
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
    let middle = only_effect(row(&session, &source).effects.as_ref().unwrap()).definition;
    assert_eq!(
        middle.as_glow().unwrap().intensity(),
        old.intensity() + (1.4 - old.intensity()) * 0.5
    );
    assert_eq!(middle.radius(), Some(Pixels(4.875).into()));
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
        only_effect(row(&session, &source).effects.as_ref().unwrap()).definition,
        (definition(&source)).into()
    );
    scene
        .live(&mut session)
        .set_glow(&source, EffectUpdate::default().intensity(0.2))
        .unwrap();
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
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
        .set_glow(&target, EffectUpdate::default().intensity(1.5))
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
        only_effect(row(&session, &source).effects.as_ref().unwrap()).attachment,
        original.node_id()
    );
    scene.live(&mut session).remove_glow(&source).unwrap();
    assert!(row(&session, &source).effects.is_none());
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
    for i in 1..noon_core::MAX_EFFECT_STACK_LEN {
        scene
            .live(&mut session)
            .add_effect(&source, noon_core::Glow::default(), &format!("extra{i}"))
            .unwrap();
    }
    let before = session.publication_context();
    let original = row(&session, &source).clone();
    assert!(scene
        .live(&mut session)
        .set_scale(&source, 0.0, 1.0)
        .is_err());
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
    for i in 1..=noon_core::MAX_EFFECT_STACK_LEN {
        target
            .add_effect(noon_core::Glow::default(), &format!("extra{i}"))
            .unwrap();
    }
    let mut session = scene.execution_session().unwrap();
    assert!(row(&session, &source).effects.is_none());
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
        .set_glow(EffectUpdate::default().intensity(1.4))
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
        let effect = only_effect(row(&session, &source).effects.as_ref().unwrap());
        assert_eq!(effect.attachment, original_id);
        assert_eq!(
            effect.definition.as_glow().unwrap().intensity(),
            original.intensity() + (1.4 - original.intensity()) * 0.5
        );
        assert_eq!(row(&session, &source).transform.translation.x, 1.0);
        session.seek(2.0).unwrap();
        assert!(row(&session, &source).effects.is_none());
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
            .set_glow(EffectUpdate::default().intensity(1.4))
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
            only_effect(row(&session, &source).effects.as_ref().unwrap()).definition,
            (original).into()
        );
    }
}

#[test]
fn pending_segment_keeps_the_existing_publication_barrier() {
    let (scene, source) = fixture(true);
    let mut target = source.target_editor().unwrap();
    target
        .set_glow(EffectUpdate::default().intensity(1.4))
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
    let captured = (effects[0].definition).as_glow().unwrap();
    assert_eq!(
        captured,
        (only_effect(row(&session, &source).effects.as_ref().unwrap()).definition)
            .as_glow()
            .unwrap()
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
    assert!(row(&session, &source).effects.is_none());
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
        only_effect(row(&session, &source).effects.as_ref().unwrap()).attachment,
        new_generation
    );
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
            .intensity(),
        0.0
    );
    assert!(!session.take_renderer_publication().changes().is_empty());

    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
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
    assert!(row(&session, &source).effects.is_none());
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
        let effect = only_effect(row(&session, &source).effects.as_ref().unwrap());
        assert_eq!(effect.attachment, new_generation);
        assert_eq!(
            effect.definition.as_glow().unwrap().intensity(),
            initial().intensity.unwrap() * 0.5
        );
        session.seek(2.0).unwrap();
        assert!(row(&session, &source).effects.is_none());
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
        only_effect(row(&session, &source).effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
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
        only_effect(row(&session, &source).effects.as_ref().unwrap()).attachment,
        fresh
    );
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
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
    assert!(row(&session, &source).effects.is_none());
}

#[test]
fn ordinary_composition_enrolls_absent_glow_at_activation_and_replays_exact_generation() {
    use crate::{AnimationCompositionRequest, TransformToRequest};

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
    assert!(source.get_effect("glow").is_err());
    let before = session.publication_context();
    let request = AnimationCompositionRequest::Composition {
        kind: noon_core::SemanticAnimationCompositionKind::Parallel,
        children: vec![AnimationCompositionRequest::TransformTo(
            TransformToRequest::new(&source, &target, options()),
        )],
        options: AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    assert_ne!(session.publication_context(), before);
    let effect = source.get_effect("glow").unwrap();
    let generation = effect.node_id();
    assert_ne!(generation, target.get_effect("glow").unwrap().node_id());
    assert_eq!(definition(&source).intensity(), 0.0);
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap()).attachment,
        generation
    );

    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    assert_eq!(
        only_effect(row(&session, &source).effects.as_ref().unwrap())
            .definition
            .as_glow()
            .unwrap()
            .intensity(),
        initial().intensity.unwrap() * 0.5
    );
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        definition(&source).intensity(),
        initial().intensity.unwrap()
    );
    scene.live(&mut session).remove_glow(&source).unwrap();
    assert!(effect.authored_definition().is_err());
    session.seal_replay().unwrap();
    for _ in 0..2 {
        session.seek(0.5).unwrap();
        assert_eq!(
            only_effect(row(&session, &source).effects.as_ref().unwrap()).attachment,
            generation
        );
        assert_eq!(
            only_effect(row(&session, &source).effects.as_ref().unwrap())
                .definition
                .as_glow()
                .unwrap()
                .intensity(),
            initial().intensity.unwrap() * 0.5
        );
        session.seek(1.0).unwrap();
        assert!(row(&session, &source).effects.is_none());
    }
}

#[test]
fn ordinary_composition_rejects_stacked_absent_glow_without_any_publication() {
    use crate::{AnimationCompositionRequest, TransformToRequest};

    let (scene, source) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let target = scene.live(&mut session).target_editor(&source).unwrap();
    scene
        .live(&mut session)
        .set_glow(&target, initial())
        .unwrap();
    scene
        .live(&mut session)
        .add_effect(&target, noon_core::Glow::default(), "second")
        .unwrap();
    let before = session.publication_context();
    let old_frame = session.frame().clone();
    let node_count = scene.integration_store().borrow().len();
    let request = AnimationCompositionRequest::TransformTo(TransformToRequest::new(
        &source,
        &target,
        options(),
    ));
    assert!(scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.frame(), &old_frame);
    assert_eq!(scene.integration_store().borrow().len(), node_count);
    assert!(source.get_effect("glow").is_err());
}

fn immutable_glow_execution(rate: RateFunction) -> (crate::SceneExecution, Mobject) {
    use noon_core::SemanticAnimationIntent;
    let (scene, source) = fixture(true);
    let mut target = source.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_glow(EffectUpdate::default().radius(Pixels(6.5)).intensity(1.4))
        .unwrap();
    let animation = scene
        .declare_transform_to(
            &source,
            &target,
            AnimationOptions::new().run_time(1.0).rate_func(rate),
        )
        .unwrap();
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: crate::SemanticAnimationCompositionKind::Parallel,
                children: vec![animation.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    (
        scene.into_execution_with_animation_root(&root).unwrap(),
        source,
    )
}

#[test]
fn initial_glow_targets_admit_immutable_seek_with_exact_endpoints_and_authored_state() {
    for rate in [RateFunction::Linear, RateFunction::ThereAndBack] {
        let (mut execution, source) = immutable_glow_execution(rate);
        let authored = definition(&source);
        let attachment = source.get_effect("glow").unwrap().node_id();
        let revision = execution.session().publication_context().scene_revision();
        assert!(execution.session().supports_immutable_playback());
        assert_eq!(execution.session().finite_playback_end_time(), Some(1.0));
        for (time, returning) in [
            (0.0, 0.0),
            (0.5, 1.0),
            (0.25, 0.5),
            (0.75, 0.5),
            (1.0, 0.0),
            (0.25, 0.5),
        ] {
            execution.seek(time).unwrap();
            let progress = if rate == RateFunction::Linear {
                time
            } else {
                returning
            };
            let actual = row(execution.session(), &source);
            let effect = only_effect(actual.effects.as_ref().unwrap());
            let glow = effect.definition.as_glow().unwrap();
            assert_eq!(effect.attachment, attachment);
            assert!(
                (glow.intensity()
                    - (authored.intensity() + (1.4 - authored.intensity()) * progress))
                    .abs()
                    < 1e-12
            );
            assert!((glow.radius().value() - (3.25 + 3.25 * progress)).abs() < 1e-12);
            assert!((f64::from(actual.transform.translation.x) - 2.0 * progress).abs() < 1e-6);
            assert_eq!(definition(&source), authored);
            assert_eq!(
                execution.session().publication_context().scene_revision(),
                revision
            );
        }
    }
}

#[test]
fn initial_effect_targets_allow_disjoint_channels_and_reject_conflicts_without_mutation() {
    use noon_core::SemanticAnimationIntent;
    for conflicting in [false, true] {
        let (scene, source) = fixture(true);
        let mut first = source.target_editor().unwrap();
        first
            .set_glow(EffectUpdate::default().intensity(1.4))
            .unwrap();
        let mut second = source.target_editor().unwrap();
        second
            .set_glow(if conflicting {
                EffectUpdate::default().intensity(2.0)
            } else {
                EffectUpdate::default().radius(Pixels(6.5))
            })
            .unwrap();
        let a = scene
            .declare_transform_to(&source, &first, options())
            .unwrap();
        let b = scene
            .declare_transform_to(&source, &second, options())
            .unwrap();
        let root = scene
            .declare_animation(
                SemanticAnimationIntent::Composition {
                    kind: crate::SemanticAnimationCompositionKind::Parallel,
                    children: vec![a.node_id(), b.node_id()],
                },
                AnimationOptions::new(),
            )
            .unwrap();
        let revision = scene.revision();
        let authored = definition(&source);
        let result = scene.execution_session_with_animation_root(&root);
        if conflicting {
            assert!(result.is_err(), "parallel writes must conflict");
            let fallback = scene.execution_session().unwrap();
            assert_eq!(
                only_effect(row(&fallback, &source).effects.as_ref().unwrap())
                    .definition
                    .as_glow()
                    .unwrap(),
                authored
            );
        } else {
            let mut execution = result.unwrap();
            execution.seek(0.5).unwrap();
            let glow = only_effect(row(&execution, &source).effects.as_ref().unwrap())
                .definition
                .as_glow()
                .unwrap();
            assert!((glow.intensity() - 0.9).abs() < 1e-12);
            assert_eq!(glow.radius().value(), 4.875);
        }
        assert_eq!(scene.revision(), revision);
        assert_eq!(definition(&source), authored);
    }
}

#[test]
fn initial_affine_target_rejects_overlapping_exact_tracks_in_either_child_order() {
    use noon_core::{
        CompositionTimeMap, SemanticAnimationIntent, SemanticObjectTrackProperty,
        SemanticObjectTrackValues, TrackTiming,
    };
    for (reversed, start) in [(false, 0.0), (true, 0.0), (false, 2.0), (true, 2.0)] {
        let (scene, source) = fixture(true);
        let mut target = source.target_editor().unwrap();
        target.shift(2.0, 0.0).unwrap();
        let affine = scene
            .declare_transform_to(&source, &target, options())
            .unwrap();
        let exact = scene
            .declare_animation(
                SemanticAnimationIntent::ObjectPropertyTrack {
                    target: source.node_id(),
                    property: SemanticObjectTrackProperty::Position,
                    values: SemanticObjectTrackValues::Vec3 {
                        from: crate::SemanticVec3::new(0.0, 0.0, 0.0),
                        to: crate::SemanticVec3::new(3.0, 0.0, 0.0),
                    },
                    timing: TrackTiming::new(start, 1.0, RateFunction::Linear),
                    time_map: CompositionTimeMap::identity(),
                },
                AnimationOptions::new(),
            )
            .unwrap();
        let mut children = vec![affine.node_id(), exact.node_id()];
        if reversed {
            children.reverse();
        }
        let root = scene
            .declare_animation(
                SemanticAnimationIntent::Composition {
                    kind: crate::SemanticAnimationCompositionKind::Parallel,
                    children,
                },
                AnimationOptions::new(),
            )
            .unwrap();
        let revision = scene.revision();
        let result = scene.execution_session_with_animation_root(&root);
        if start == 0.0 {
            assert!(result.is_err());
        } else {
            let mut execution = result.unwrap();
            execution.seek(2.5).unwrap();
            assert_eq!(row(&execution, &source).transform.translation.x, 1.5);
        }
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            row(&scene.execution_session().unwrap(), &source)
                .transform
                .translation
                .x,
            0.0
        );
    }
}
