//! Public activation/publication proofs for authored pulse attachment lifetime.

use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, ExecutionSegment, ExecutionSession,
    FadeEndpoint, Mobject, RateFunction, Scene, TransformToRequest,
};
use noon_core::{
    Color, EffectUpdate, Glow, GlowSource, Pixels, SemanticAnimationCompositionKind as Kind,
    SemanticFadeDirection,
};

fn fixture(attached: bool) -> (Scene, Mobject) {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.4).unwrap();
    object.disable_stroke().unwrap();
    scene.add(&object).unwrap();
    if attached {
        scene
            .set_glow(
                &object,
                EffectUpdate::default()
                    .intensity(0.4)
                    .color(Color::RED)
                    .radius(Pixels(3.25)),
            )
            .unwrap();
    }
    (scene, object)
}

fn options() -> AnimationOptions {
    AnimationOptions::new().run_time(1.0)
}

fn pulse(object: &Mobject) -> Request<'_> {
    Request::GlowPulse {
        target: object,
        intensity: 2.0,
        options: options(),
    }
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

fn effect(session: &ExecutionSession, object: &Mobject) -> Glow {
    (crate::effect_authoring::only_effect(row(session, object).effects.as_ref().unwrap())
        .definition)
        .as_glow()
        .unwrap()
}

fn authored(object: &Mobject) -> Glow {
    (object
        .get_effect("glow")
        .unwrap()
        .authored_definition()
        .unwrap())
    .as_glow()
    .unwrap()
}

fn advance(scene: &Scene, session: &mut ExecutionSession, segment: ExecutionSegment, time: f64) {
    scene
        .live(session)
        .advance_segment_to(segment, time)
        .unwrap();
}

fn finish(scene: &Scene, session: &mut ExecutionSession, segment: ExecutionSegment) {
    advance(scene, session, segment, segment.end_time());
    scene.live(session).complete_segment(segment).unwrap();
}

#[test]
fn glow_pulse_predeclared_activation_captures_later_live_baseline() {
    let (scene, object) = fixture(true);
    let animation = scene.declare_glow_pulse(&object, 2.0, options()).unwrap();
    let id = object.get_effect("glow").unwrap().node_id();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .set_glow(&object, EffectUpdate::default().intensity(0.7))
        .unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    assert_eq!(effect(&session, &object).intensity(), 0.7);
    advance(&scene, &mut session, segment, 0.5);
    assert_eq!(effect(&session, &object).intensity(), 2.0);
    assert_eq!(authored(&object).intensity(), 0.7);
    finish(&scene, &mut session, segment);
    assert_eq!(effect(&session, &object).intensity(), 0.7);
    assert_eq!(object.get_effect("glow").unwrap().node_id(), id);
}

#[test]
fn glow_pulse_predeclared_absence_enrolls_only_at_activation_and_replays_cleanup() {
    let (scene, object) = fixture(false);
    let animation = scene.declare_glow_pulse(&object, 2.0, options()).unwrap();
    assert!(object.get_effect("glow").is_err());
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    let temporary = object.get_effect("glow").unwrap();
    assert_eq!(authored(&object).intensity(), 0.0);
    advance(&scene, &mut session, segment, 0.5);
    let middle = row(&session, &object).clone();
    assert_eq!(
        crate::effect_authoring::only_effect(middle.effects.as_ref().unwrap()).attachment,
        temporary.node_id()
    );
    finish(&scene, &mut session, segment);
    assert!(object.get_effect("glow").is_err());
    assert!(temporary.authored_definition().is_err());
    assert!(row(&session, &object).effects.is_none());
    session.seal_replay().unwrap();
    for _ in 0..2 {
        session.seek(0.5).unwrap();
        assert_eq!(row(&session, &object), &middle);
        session.seek(1.0).unwrap();
        assert!(row(&session, &object).effects.is_none());
        assert!(object.get_effect("glow").is_err());
    }
}

#[test]
fn glow_pulse_repeated_predeclared_activation_never_rebinds_a_stale_attachment() {
    let (scene, object) = fixture(false);
    let animation = scene.declare_glow_pulse(&object, 2.0, options()).unwrap();
    let mut session = scene.execution_session().unwrap();
    let first = scene.live(&mut session).play_animation(&animation).unwrap();
    let old = object.get_effect("glow").unwrap();
    finish(&scene, &mut session, first);
    let second = scene.live(&mut session).play_animation(&animation).unwrap();
    let current = object.get_effect("glow").unwrap().node_id();
    assert_ne!(old.node_id(), current);
    let publication = session.publication_context();
    assert!(scene
        .live(&mut session)
        .set_effect(&object, &old, EffectUpdate::default().intensity(0.8))
        .is_err());
    assert_eq!(session.publication_context(), publication);
    assert_eq!(object.get_effect("glow").unwrap().node_id(), current);
    advance(&scene, &mut session, second, second.start_time() + 0.5);
    assert_eq!(effect(&session, &object).intensity(), 2.0);
    finish(&scene, &mut session, second);
    assert!(object.get_effect("glow").is_err());
}

#[test]
fn glow_pulse_same_valued_color_adopts_but_empty_patch_does_not() {
    for explicit in [false, true] {
        let (scene, object) = fixture(false);
        let mut session = scene.execution_session().unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_glow_pulse(&object, 2.0, options())
            .unwrap();
        let id = object.get_effect("glow").unwrap().node_id();
        advance(&scene, &mut session, segment, 0.5);
        let revision = scene.revision();
        let update = if explicit {
            EffectUpdate::default().color(authored(&object).color())
        } else {
            EffectUpdate::default()
        };
        scene.live(&mut session).set_glow(&object, update).unwrap();
        assert_eq!(
            scene.revision(),
            revision,
            "value-equal adoption is completion ownership, not a semantic rewrite"
        );
        assert_eq!(effect(&session, &object).intensity(), 2.0);
        finish(&scene, &mut session, segment);
        if explicit {
            assert_eq!(object.get_effect("glow").unwrap().node_id(), id);
            assert_eq!(effect(&session, &object).intensity(), 0.0);
        } else {
            assert!(object.get_effect("glow").is_err());
            assert!(row(&session, &object).effects.is_none());
        }
    }
}

#[test]
fn glow_pulse_same_valued_intensity_supersedes_effective_driver_and_cleanup() {
    let (scene, object) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_glow_pulse(&object, 2.0, options())
        .unwrap();
    let id = object.get_effect("glow").unwrap().node_id();
    advance(&scene, &mut session, segment, 0.5);
    let revision = scene.revision();
    scene
        .live(&mut session)
        .set_glow(&object, EffectUpdate::default().intensity(0.0))
        .unwrap();
    assert_eq!(scene.revision(), revision);
    assert_eq!(effect(&session, &object).intensity(), 0.0);
    advance(&scene, &mut session, segment, 0.75);
    assert_eq!(effect(&session, &object).intensity(), 0.0);
    finish(&scene, &mut session, segment);
    assert_eq!(object.get_effect("glow").unwrap().node_id(), id);
    assert_eq!(authored(&object).intensity(), 0.0);
}

#[test]
fn glow_pulse_radius_and_color_edits_survive_without_releasing_intensity() {
    let (scene, object) = fixture(true);
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_glow_pulse(&object, 2.0, options())
        .unwrap();
    advance(&scene, &mut session, segment, 0.5);
    scene
        .live(&mut session)
        .set_glow(
            &object,
            EffectUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(12.0)),
        )
        .unwrap();
    assert_eq!(effect(&session, &object).intensity(), 2.0);
    finish(&scene, &mut session, segment);
    let final_glow = effect(&session, &object);
    assert_eq!(final_glow.intensity(), 0.4);
    assert_eq!(final_glow.color(), Color::BLUE);
    assert_eq!(final_glow.radius(), Pixels(12.0).into());
    assert_eq!(final_glow, authored(&object));
}

#[test]
fn glow_pulse_persistent_intensity_supersedes_future_pulses_and_replays_exactly() {
    let (scene, object) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(noon_runtime::ReplayLimits::default())
        .unwrap();
    let request = Request::Composition {
        kind: Kind::Sequence,
        children: vec![
            pulse(&object),
            Request::Wait { duration: 0.25 },
            pulse(&object),
        ],
        options: AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    advance(&scene, &mut session, segment, 0.25);
    let before_edit = row(&session, &object).clone();
    advance(&scene, &mut session, segment, 0.5);
    scene
        .live(&mut session)
        .set_glow(&object, EffectUpdate::default().intensity(0.3))
        .unwrap();
    for time in [0.75, 1.25, 1.75, 2.25] {
        advance(&scene, &mut session, segment, time);
        assert_eq!(effect(&session, &object).intensity(), 0.3);
    }
    finish(&scene, &mut session, segment);
    session.seal_replay().unwrap();
    session.seek(0.25).unwrap();
    assert_eq!(row(&session, &object), &before_edit);
    session.seek(1.75).unwrap();
    assert_eq!(effect(&session, &object).intensity(), 0.3);
    session.seek(2.25).unwrap();
    assert_eq!(effect(&session, &object).intensity(), 0.3);
}

#[test]
fn glow_pulse_invalid_updates_and_structural_edits_preserve_unadopted_cleanup() {
    let (scene, object) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_glow_pulse(&object, 2.0, options())
        .unwrap();
    advance(&scene, &mut session, segment, 0.5);
    let publication = session.publication_context();
    let frame = session.frame().clone();
    for update in [
        EffectUpdate::default().intensity(-1.0),
        EffectUpdate::default().radius(Pixels(12.0)),
        EffectUpdate::default().source(GlowSource::Silhouette),
    ] {
        assert!(scene.live(&mut session).set_glow(&object, update).is_err());
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
    }
    assert!(scene.live(&mut session).remove_glow(&object).is_err());
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    finish(&scene, &mut session, segment);
    assert!(object.get_effect("glow").is_err());
}

#[test]
fn glow_pulse_overlapping_activation_fails_without_neutral_identity_or_tracks() {
    let (scene, object) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let publication = session.publication_context();
    let frame = session.frame().clone();
    let nodes = scene.integration_store().borrow().len();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![pulse(&object), pulse(&object)],
        options: AnimationOptions::new(),
    };
    assert!(scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .is_err());
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert!(object.get_effect("glow").is_err());
    let segment = scene
        .live(&mut session)
        .declare_and_activate_glow_pulse(&object, 2.0, options())
        .unwrap();
    finish(&scene, &mut session, segment);
}

#[test]
fn glow_pulse_predeclared_lifecycle_requires_root_aware_activation_atomically() {
    let (scene, object) = fixture(false);
    let (animation, fade) = {
        let mut store = scene.integration_store().borrow_mut();
        let pulse = store
            .insert_semantic_glow_pulse_animation(object.node_id(), 2.0, options())
            .unwrap();
        let fade = store
            .insert_semantic_fade_animation(object.node_id(), SemanticFadeDirection::Out, options())
            .unwrap();
        let animation = store
            .insert_semantic_parallel_animation(&[pulse, fade], AnimationOptions::new())
            .unwrap();
        (animation, fade)
    };
    let mut session = scene.execution_session().unwrap();
    let publication = session.publication_context();
    let frame = session.frame().clone();
    let nodes = scene.integration_store().borrow().len();
    assert_eq!(
        session.activate_animation_segment(
            &mut scene.integration_store().borrow_mut(),
            animation,
            AnimationOptions::new(),
        ),
        Err(crate::ExecutionSessionAnimationError::Payload(
            noon_compile::SemanticAffineAnimationTrackError::UnsupportedLifecycle {
                animation: fade,
                remover: true,
                introducer: false,
            }
        ))
    );
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame(), &frame);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert!(object.get_effect("glow").is_err());
    assert!(session.pending_segment_token().is_none());
}

#[test]
fn glow_pulse_motion_does_not_adopt_a_temporary_attachment() {
    let (scene, object) = fixture(false);
    let mut target = object.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![
            pulse(&object),
            Request::TransformTo(TransformToRequest::new(
                &object,
                &target,
                options().rate_func(RateFunction::Linear),
            )),
        ],
        options: AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    advance(&scene, &mut session, segment, 0.5);
    assert_eq!(row(&session, &object).transform.translation.x, 1.0);
    assert_eq!(effect(&session, &object).intensity(), 2.0);
    finish(&scene, &mut session, segment);
    assert_eq!(row(&session, &object).transform.translation.x, 2.0);
    assert!(object.get_effect("glow").is_err());
    assert!(row(&session, &object).effects.is_none());
}

#[test]
fn glow_pulse_followed_by_complete_glow_target_persists_its_attachment() {
    let (scene, object) = fixture(false);
    let mut target = object.target_editor().unwrap();
    target
        .set_glow(
            EffectUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(12.0)),
        )
        .unwrap();
    let target_intensity = authored(&target).intensity();
    let mut session = scene.execution_session().unwrap();
    let children = vec![
        pulse(&object),
        Request::TransformTo(TransformToRequest::new(
            &object,
            &target,
            options().rate_func(RateFunction::Linear),
        )),
    ];
    let publication = session.publication_context();
    let nodes = scene.integration_store().borrow().len();
    let parallel = Request::Composition {
        kind: Kind::Parallel,
        children: children.clone(),
        options: AnimationOptions::new(),
    };
    assert!(scene
        .live(&mut session)
        .declare_and_activate_composition(&parallel, AnimationOptions::new())
        .is_err());
    assert_eq!(session.publication_context(), publication);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert!(object.get_effect("glow").is_err());
    let request = Request::Composition {
        kind: Kind::Sequence,
        children,
        options: AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    let attachment = object.get_effect("glow").unwrap().node_id();
    advance(&scene, &mut session, segment, 1.0);
    assert_eq!(effect(&session, &object).intensity(), 0.0);
    finish(&scene, &mut session, segment);
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
    assert_eq!(authored(&object).radius(), Pixels(12.0).into());
    assert_eq!(authored(&object).color(), Color::BLUE);
    assert_eq!(authored(&object).intensity(), target_intensity);
}

#[test]
fn glow_pulse_persistent_edit_respects_ordinary_parameter_ownership() {
    let (scene, object) = fixture(true);
    let mut target = object.target_editor().unwrap();
    target
        .set_glow(EffectUpdate::default().radius(Pixels(12.0)))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![
            pulse(&object),
            Request::TransformTo(
                TransformToRequest::new(
                    &object,
                    &target,
                    options().rate_func(RateFunction::Linear),
                )
                .method_target(),
            ),
        ],
        options: AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    advance(&scene, &mut session, segment, 0.5);
    let publication = session.publication_context();
    let frame = session.frame().clone();
    for radius in [authored(&object).radius(), Pixels(5.0).into()] {
        assert!(scene
            .live(&mut session)
            .set_glow(&object, EffectUpdate::default().radius(radius))
            .is_err());
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
    }
    scene
        .live(&mut session)
        .set_glow(&object, EffectUpdate::default().color(Color::BLUE))
        .unwrap();
    assert_eq!(effect(&session, &object).intensity(), 2.0);
    finish(&scene, &mut session, segment);
    assert_eq!(authored(&object).radius(), Pixels(12.0).into());
    assert_eq!(authored(&object).color(), Color::BLUE);
    assert_eq!(authored(&object).intensity(), 0.4);
}

#[test]
fn glow_pulse_fade_out_retires_source_and_temporary_output_together() {
    let (scene, object) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let request = Request::Composition {
        kind: Kind::Parallel,
        children: vec![
            pulse(&object),
            Request::Fade {
                target: &object,
                direction: SemanticFadeDirection::Out,
                endpoint: FadeEndpoint::default(),
                options: options().rate_func(RateFunction::Linear),
            },
        ],
        options: AnimationOptions::new(),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    let temporary = object.get_effect("glow").unwrap();
    let execution_object = session.execution_object_id(object.node_id()).unwrap();
    let source_frame_index = session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == execution_object)
        .unwrap();
    advance(&scene, &mut session, segment, 0.5);
    assert!(effect(&session, &object).intensity() > 0.0);
    finish(&scene, &mut session, segment);
    assert!(object.get_effect("glow").is_err());
    assert!(temporary.authored_definition().is_err());
    assert!(
        !scene.live(&mut session).contains(&object).unwrap(),
        "FadeOut must remove source membership at completion"
    );
    // Stable identity and slots survive detachment for replay and re-entry;
    // reachability, presence and painter membership retire in one publication.
    assert_eq!(
        session.execution_object_id(object.node_id()),
        Some(execution_object)
    );
    assert!(!session.semantic_object_is_reachable(object.node_id()));
    assert!(!session.frame().is_present(source_frame_index));
    assert!(!session
        .painter_order()
        .contains(&(source_frame_index as u32)));
}

#[test]
fn glow_pulse_retains_temporary_generation_until_reversing_composition_finishes() {
    let (scene, object) = fixture(false);
    let mut session = scene.execution_session().unwrap();
    let request = Request::Composition {
        kind: Kind::Sequence,
        children: vec![pulse(&object), Request::Wait { duration: 1.0 }],
        options: AnimationOptions::new()
            .run_time(4.0)
            .rate_func(RateFunction::ThereAndBack),
    };
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    let attachment = object.get_effect("glow").unwrap().node_id();
    advance(&scene, &mut session, segment, 2.0);
    assert_eq!(effect(&session, &object).intensity(), 0.0);
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
    advance(&scene, &mut session, segment, 3.5);
    assert!(effect(&session, &object).intensity() > 0.0);
    assert_eq!(object.get_effect("glow").unwrap().node_id(), attachment);
    finish(&scene, &mut session, segment);
    assert!(object.get_effect("glow").is_err());
}
