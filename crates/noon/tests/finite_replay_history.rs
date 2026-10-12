use noon::{
    AnimationOptions, ExecutionSession, FadeEndpoint, FadeTranslation, RateFunction, Scene,
};
use noon_core::{SemanticFadeDirection, SemanticVec3};
use noon_runtime::{FrameState, ReplayError, ReplayLimits};

fn options(duration: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(duration)
        .rate_func(RateFunction::Linear)
}

// Compact only live execution rows, in painter order. Retired physical slots may
// remain pinned, but must never be mistaken for visible historical membership.
fn snapshot(session: &ExecutionSession) -> FrameState {
    let source = session.frame();
    let mut result = source.clone();
    let indices: Vec<_> = session
        .painter_order()
        .iter()
        .map(|&i| i as usize)
        .collect();
    result.objects = indices.iter().map(|&i| source.objects[i].clone()).collect();
    result.presences = indices.iter().map(|&i| source.presences[i]).collect();
    result.reveals = indices.iter().map(|&i| source.reveals[i]).collect();
    result.morphs = indices.iter().map(|&i| source.morphs[i]).collect();
    result.render_geometries = indices
        .iter()
        .map(|&i| source.render_geometries[i].clone())
        .collect();
    result.render_transforms = indices
        .iter()
        .map(|&i| source.render_transforms[i])
        .collect();
    result.family_animations = indices
        .iter()
        .map(|&i| source.family_animations[i])
        .collect();
    result.family_animation_plan_indices = indices
        .iter()
        .map(|&i| source.family_animation_plan_indices[i])
        .collect();
    result
}
fn sample(session: &mut ExecutionSession, samples: &mut Vec<FrameState>, time: f64) {
    session.advance_to(time).unwrap();
    samples.push(snapshot(session));
}
fn verify(session: &mut ExecutionSession, samples: &[FrameState]) {
    session.seal_replay().unwrap();
    for expected in samples.iter().rev() {
        session.seek(expected.time).unwrap();
        assert_eq!(
            &snapshot(session),
            expected,
            "reverse seek at {}",
            expected.time
        );
    }
    for expected in samples {
        session.advance_to(expected.time).unwrap();
        assert_eq!(
            &snapshot(session),
            expected,
            "forward replay at {}",
            expected.time
        );
    }
}

#[test]
fn detach_reentry_and_permanent_removal_preserve_history_not_just_final_state() {
    let mut scene = Scene::new();
    let anchor = scene.circle(0.5).unwrap();
    let shape = scene.square(1.0).unwrap();
    let pulse = scene.circle(0.1).unwrap();
    scene.add(&anchor).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let mut samples = Vec::new();
    for (object, direction, start, duration) in [
        (&shape, SemanticFadeDirection::In, 0.0, 1.0),
        (&shape, SemanticFadeDirection::Out, 1.5, 1.0),
        (&pulse, SemanticFadeDirection::In, 3.0, 0.5),
        (&shape, SemanticFadeDirection::In, 3.5, 1.0),
        (&pulse, SemanticFadeDirection::Out, 4.5, 0.5),
    ] {
        if session.frame().time < start {
            let midpoint = (session.frame().time + start) / 2.0;
            sample(&mut session, &mut samples, midpoint);
        }
        session.advance_to(start).unwrap();
        let segment = scene
            .live(&mut session)
            .declare_and_activate_fade_with_endpoint(
                object,
                direction,
                FadeEndpoint::new(
                    0.25,
                    FadeTranslation::Shift(SemanticVec3::new(2.0, 0.0, 0.0)),
                ),
                options(duration),
            )
            .unwrap();
        sample(&mut session, &mut samples, start + duration * 0.2);
        sample(&mut session, &mut samples, start + duration * 0.7);
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    sample(&mut session, &mut samples, 5.5);
    let final_store_revision = scene.integration_store().borrow().scene_revision();
    assert!(scene.live(&mut session).contains(&shape).unwrap());
    assert!(!scene.live(&mut session).contains(&pulse).unwrap());
    verify(&mut session, &samples);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        final_store_revision,
        "replay never mutates or rewinds SemanticStore"
    );
}

#[test]
fn morph_affine_morph_replay_selects_time_qualified_content_and_render_frame() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    let mut circle = scene.circle(0.6).unwrap();
    circle.shift(2.0, 0.0).unwrap();
    let rectangle = scene.rectangle(2.0, 0.4).unwrap();
    scene.add(&shape).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let mut samples = Vec::new();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(&shape, &circle, options(1.0))
        .unwrap();
    sample(&mut session, &mut samples, 0.25);
    sample(&mut session, &mut samples, 0.75);
    {
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, 1.0).unwrap();
        live.complete_segment(segment).unwrap();
    }
    sample(&mut session, &mut samples, 1.25);
    session.advance_to(1.5).unwrap();
    let target = scene.live(&mut session).target_editor(&shape).unwrap();
    {
        let mut live = scene.live(&mut session);
        live.shift(&target, 3.0, 1.0).unwrap();
        live.rotate(&target, 0.4).unwrap();
    }
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(&shape, &target, options(1.0))
        .unwrap();
    sample(&mut session, &mut samples, 1.75);
    sample(&mut session, &mut samples, 2.25);
    {
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, 2.5).unwrap();
        live.complete_segment(segment).unwrap();
    }
    sample(&mut session, &mut samples, 2.75);
    session.advance_to(3.0).unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(&shape, &rectangle, options(1.0))
        .unwrap();
    sample(&mut session, &mut samples, 3.25);
    sample(&mut session, &mut samples, 3.75);
    {
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, 4.0).unwrap();
        live.complete_segment(segment).unwrap();
    }
    sample(&mut session, &mut samples, 4.25);
    verify(&mut session, &samples);
}

#[test]
fn sealed_mutation_rejects_before_semantic_commit_and_discard_restores_frontier() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    scene.add(&shape).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    session.advance_to(1.0).unwrap();
    scene.live(&mut session).shift(&shape, 4.0, 0.0).unwrap();
    session.advance_to(2.0).unwrap();
    let frontier = snapshot(&session);
    session.seal_replay().unwrap();
    session.seek(0.5).unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(scene.live(&mut session).shift(&shape, 1.0, 0.0).is_err());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    session.discard_replay_retention();
    assert_eq!(snapshot(&session), frontier);
    assert_eq!(session.replay_stats().revisions_retained, 0);
    scene.live(&mut session).shift(&shape, 1.0, 0.0).unwrap();
}

#[test]
fn retention_exhaustion_preserves_live_execution_and_rejects_replay() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    scene.add(&shape).unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits {
            revisions: 1,
            payloads: 1,
        })
        .unwrap();
    scene.live(&mut session).shift(&shape, 1.0, 0.0).unwrap();
    session.advance_to(1.0).unwrap();
    scene.live(&mut session).shift(&shape, 1.0, 0.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
    assert_eq!(session.seal_replay(), Err(ReplayError::RetentionLimit));
    session.discard_replay_retention();
    scene.live(&mut session).shift(&shape, 1.0, 0.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 3.0);
}
