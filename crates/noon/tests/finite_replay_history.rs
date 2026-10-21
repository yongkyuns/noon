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

#[test]
fn inset_relations_replay_activation_retirement_retarget_and_clear_with_the_frame() {
    let mut scene = Scene::new();
    let view = scene.zoomed_view(Default::default()).unwrap();
    let other = scene.zoomed_view(Default::default()).unwrap();
    scene.add(other.camera_frame()).unwrap();
    let mut target = view.camera_frame().target_editor().unwrap();
    target.set_translation(2.0, -1.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    let mut samples = Vec::new();
    let mut views = Vec::new();
    let mut record = |session: &mut ExecutionSession| {
        samples.push(snapshot(session));
        views.push((session.frame().time, session.inset_2d_views().unwrap()));
    };
    record(&mut session);
    session.advance_to(1.0).unwrap();
    scene.live(&mut session).activate_zooming(&view).unwrap();
    record(&mut session);
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(view.camera_frame(), &target, options(1.0))
        .unwrap();
    for time in [1.113, 1.471, 1.937] {
        scene
            .live(&mut session)
            .advance_segment_to(segment, time)
            .unwrap();
        record(&mut session);
    }
    scene
        .live(&mut session)
        .advance_segment_to(segment, 2.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    record(&mut session);
    session.advance_to(3.0).unwrap();
    scene.live(&mut session).remove(view.display()).unwrap();
    record(&mut session);
    session.advance_to(4.0).unwrap();
    scene.live(&mut session).add(view.display()).unwrap();
    record(&mut session);
    session.advance_to(5.0).unwrap();
    let mut retarget = noon_core::SemanticMutationTransaction::new();
    retarget.set_inset_2d_view(
        view.display().node_id(),
        other.camera_frame().node_id(),
        true,
    );
    scene.live(&mut session).apply(retarget).unwrap();
    record(&mut session);
    for (time, enabled) in [(5.25, false), (5.75, true)] {
        session.advance_to(time).unwrap();
        let mut drawing = noon_core::SemanticMutationTransaction::new();
        drawing.set_draw_enabled(view.display().node_id(), enabled);
        scene.live(&mut session).apply(drawing).unwrap();
        assert_eq!(
            session.inset_2d_views().unwrap().len(),
            usize::from(enabled)
        );
        record(&mut session);
    }
    session.advance_to(6.0).unwrap();
    let mut clear = noon_core::SemanticMutationTransaction::new();
    clear.clear_inset_2d_view(view.display().node_id());
    scene.live(&mut session).apply(clear).unwrap();
    record(&mut session);
    session.advance_to(7.0).unwrap();
    let mut restore = noon_core::SemanticMutationTransaction::new();
    restore.set_inset_2d_view(
        view.display().node_id(),
        view.camera_frame().node_id(),
        false,
    );
    scene.live(&mut session).apply(restore).unwrap();
    record(&mut session);
    session.advance_to(8.0).unwrap();
    let mut retire = noon_core::SemanticMutationTransaction::new();
    retire.remove_node(view.camera_frame().node_id());
    scene.live(&mut session).apply(retire).unwrap();
    record(&mut session);
    assert!(session.inset_2d_views().unwrap().is_empty());
    let frontier = scene.revision();
    verify(&mut session, &samples);
    for (time, expected) in views.iter().rev().chain(views.iter()) {
        session.seek(*time).unwrap();
        assert_eq!(
            &session.inset_2d_views().unwrap(),
            expected,
            "inset at {time}"
        );
    }
    session.seek(6.5).unwrap();
    session.take_frame_changes();
    session.seek(5.9).unwrap();
    assert!(
        !session.take_frame_changes().is_empty(),
        "a relation-only seek must wake presentation"
    );
    session.advance_to(6.5).unwrap(); // Cross only the relation clear at t=6.
    let changes = session.take_frame_changes();
    assert!(
        !changes.is_empty(),
        "a relation-only revision must wake presentation"
    );
    assert_eq!(
        changes.object_indices().len(),
        1,
        "only the display is dirty"
    );
    assert_eq!(session.replay_stats().objects_restored, 1);
    assert_eq!(session.replay_stats().channels_restored, 0);
    session.advance_to(6.5).unwrap();
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(
        scene.revision(),
        frontier,
        "replay must not rewind authored roles"
    );
}

#[test]
fn pending_inset_pair_and_existing_display_bind_planned_camera_identity_atomically() {
    use noon_core::{
        SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState, StoredGeometry,
        Vec2,
    };
    let mut scene = Scene::new();
    let display = scene.rectangle(3.0, 2.0).unwrap();
    scene.add(&display).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    let mut samples = Vec::new();
    sample(&mut session, &mut samples, 0.0);
    session.advance_to(1.0).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let rectangle = || {
        SemanticNodeCreation::object(SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(2.0, 1.0),
        }))
    };
    let camera = transaction.create_node(rectangle());
    let pending_display = transaction.create_node(rectangle());
    transaction
        .add_member(scene.root(), camera)
        .add_member(scene.root(), pending_display)
        .set_inset_2d_view(display.node_id(), camera, true)
        .set_inset_2d_view(pending_display, camera, false);
    scene.live(&mut session).apply(transaction).unwrap();
    sample(&mut session, &mut samples, 1.5);
    let expected = session.inset_2d_views().unwrap();
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[0].camera_frame, expected[1].camera_frame);
    assert_ne!(
        expected[0].capture_own_display,
        expected[1].capture_own_display
    );
    verify(&mut session, &samples);
    session.seek(0.0).unwrap();
    assert!(session.inset_2d_views().unwrap().is_empty());
    session.seek(1.5).unwrap();
    assert_eq!(session.inset_2d_views().unwrap(), expected);
}
