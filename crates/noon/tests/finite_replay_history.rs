use noon::integration::scene_execution;
use noon::integration::scene_execution::ExecutionSession;
use noon::{AnimationOptions, FadeEndpoint, FadeTranslation, RateFunction, Scene};
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
    let mut session = scene.into_execution().unwrap();
    session
        .session_mut()
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
        if session.session().frame().time < start {
            let midpoint = (session.session().frame().time + start) / 2.0;
            sample(session.session_mut(), &mut samples, midpoint);
        }
        session.session_mut().advance_to(start).unwrap();
        let segment = session
            .live()
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
        sample(session.session_mut(), &mut samples, start + duration * 0.2);
        sample(session.session_mut(), &mut samples, start + duration * 0.7);
        session
            .live()
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        session.live().complete_segment(segment).unwrap();
    }
    sample(session.session_mut(), &mut samples, 5.5);
    let final_store_revision = session
        .scene()
        .integration_store()
        .borrow()
        .scene_revision();
    assert!(session.scene_mut().contains_direct(&shape).unwrap());
    assert!(!session.scene_mut().contains_direct(&pulse).unwrap());
    verify(session.session_mut(), &samples);
    assert_eq!(
        session
            .scene()
            .integration_store()
            .borrow()
            .scene_revision(),
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
    let mut session = scene.into_execution().unwrap();
    session
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let mut samples = Vec::new();
    let segment = session
        .live()
        .declare_and_activate_transform_to(&shape, &circle, options(1.0))
        .unwrap();
    sample(session.session_mut(), &mut samples, 0.25);
    sample(session.session_mut(), &mut samples, 0.75);
    {
        session.live().advance_segment_to(segment, 1.0).unwrap();
        session.live().complete_segment(segment).unwrap();
    }
    sample(session.session_mut(), &mut samples, 1.25);
    session.session_mut().advance_to(1.5).unwrap();
    let target = session.scene_mut().target_editor(&shape).unwrap();
    {
        session
            .scene_mut()
            .shift_3d(&target, noon::SemanticVec3::new(3.0, 1.0, 0.0))
            .unwrap();
        session.scene_mut().rotate(&target, 0.4).unwrap();
    }
    let segment = session
        .live()
        .declare_and_activate_transform_to(&shape, &target, options(1.0))
        .unwrap();
    sample(session.session_mut(), &mut samples, 1.75);
    sample(session.session_mut(), &mut samples, 2.25);
    {
        session.live().advance_segment_to(segment, 2.5).unwrap();
        session.live().complete_segment(segment).unwrap();
    }
    sample(session.session_mut(), &mut samples, 2.75);
    session.session_mut().advance_to(3.0).unwrap();
    let segment = session
        .live()
        .declare_and_activate_transform_to(&shape, &rectangle, options(1.0))
        .unwrap();
    sample(session.session_mut(), &mut samples, 3.25);
    sample(session.session_mut(), &mut samples, 3.75);
    {
        session.live().advance_segment_to(segment, 4.0).unwrap();
        session.live().complete_segment(segment).unwrap();
    }
    sample(session.session_mut(), &mut samples, 4.25);
    verify(session.session_mut(), &samples);
}

#[test]
fn sealed_mutation_rejects_before_semantic_commit_and_discard_restores_frontier() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    scene.add(&shape).unwrap();
    let mut session = scene.into_execution().unwrap();
    session
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    session.session_mut().advance_to(1.0).unwrap();
    session
        .scene_mut()
        .shift_3d(&shape, noon::SemanticVec3::new(4.0, 0.0, 0.0))
        .unwrap();
    session.session_mut().advance_to(2.0).unwrap();
    let frontier = snapshot(session.session());
    session.session_mut().seal_replay().unwrap();
    session.seek(0.5).unwrap();
    let revision = session
        .scene()
        .integration_store()
        .borrow()
        .scene_revision();
    assert!(session
        .scene_mut()
        .shift_3d(&shape, noon::SemanticVec3::new(1.0, 0.0, 0.0))
        .is_err());
    assert_eq!(
        session
            .scene()
            .integration_store()
            .borrow()
            .scene_revision(),
        revision
    );
    session.session_mut().discard_replay_retention();
    assert_eq!(snapshot(session.session()), frontier);
    assert_eq!(session.session().replay_stats().revisions_retained, 0);
    session
        .scene_mut()
        .shift_3d(&shape, noon::SemanticVec3::new(1.0, 0.0, 0.0))
        .unwrap();
}

#[test]
fn retention_exhaustion_preserves_live_execution_and_rejects_replay() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    scene.add(&shape).unwrap();
    let mut session = scene.into_execution().unwrap();
    session
        .session_mut()
        .begin_replay_retention(ReplayLimits {
            revisions: 1,
            payloads: 1,
        })
        .unwrap();
    session
        .scene_mut()
        .shift_3d(&shape, noon::SemanticVec3::new(1.0, 0.0, 0.0))
        .unwrap();
    session.session_mut().advance_to(1.0).unwrap();
    session
        .scene_mut()
        .shift_3d(&shape, noon::SemanticVec3::new(1.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(
        session.session().frame().objects[0].transform.translation.x,
        2.0
    );
    assert_eq!(
        session.session_mut().seal_replay(),
        Err(ReplayError::RetentionLimit)
    );
    session.session_mut().discard_replay_retention();
    session
        .scene_mut()
        .shift_3d(&shape, noon::SemanticVec3::new(1.0, 0.0, 0.0))
        .unwrap();
    assert_eq!(
        session.session().frame().objects[0].transform.translation.x,
        3.0
    );
}

#[test]
fn inset_relations_replay_activation_retirement_retarget_and_clear_with_the_frame() {
    let mut scene = Scene::new();
    let view = scene.zoomed_view(Default::default()).unwrap();
    let other = scene.zoomed_view(Default::default()).unwrap();
    scene.add(other.camera_frame()).unwrap();
    let mut target = view.camera_frame().target_editor().unwrap();
    target.set_translation(2.0, -1.0).unwrap();
    let mut session = scene.into_execution().unwrap();
    session
        .session_mut()
        .begin_replay_retention(Default::default())
        .unwrap();
    let mut samples = Vec::new();
    let mut views = Vec::new();
    let mut record = |session: &mut ExecutionSession| {
        samples.push(snapshot(session));
        views.push((session.frame().time, session.inset_2d_views().unwrap()));
    };
    record(session.session_mut());
    session.session_mut().advance_to(1.0).unwrap();
    session.scene_mut().activate_zooming(&view).unwrap();
    record(session.session_mut());
    let segment = session
        .live()
        .declare_and_activate_transform_to(view.camera_frame(), &target, options(1.0))
        .unwrap();
    for time in [1.113, 1.471, 1.937] {
        session.live().advance_segment_to(segment, time).unwrap();
        record(session.session_mut());
    }
    session.live().advance_segment_to(segment, 2.0).unwrap();
    session.live().complete_segment(segment).unwrap();
    record(session.session_mut());
    session.session_mut().advance_to(3.0).unwrap();
    session.scene_mut().remove(view.display()).unwrap();
    record(session.session_mut());
    session.session_mut().advance_to(4.0).unwrap();
    session.scene_mut().add(view.display()).unwrap();
    record(session.session_mut());
    session.session_mut().advance_to(5.0).unwrap();
    let mut retarget = noon_core::SemanticMutationTransaction::new();
    retarget.set_inset_2d_view(
        view.display().node_id(),
        other.camera_frame().node_id(),
        true,
    );
    scene_execution::publish(session.scene_mut(), retarget).unwrap();
    record(session.session_mut());
    for (time, enabled) in [(5.25, false), (5.75, true)] {
        session.session_mut().advance_to(time).unwrap();
        let mut drawing = noon_core::SemanticMutationTransaction::new();
        drawing.set_draw_enabled(view.display().node_id(), enabled);
        scene_execution::publish(session.scene_mut(), drawing).unwrap();
        assert_eq!(
            session.session().inset_2d_views().unwrap().len(),
            usize::from(enabled)
        );
        record(session.session_mut());
    }
    session.session_mut().advance_to(6.0).unwrap();
    let mut clear = noon_core::SemanticMutationTransaction::new();
    clear.clear_inset_2d_view(view.display().node_id());
    scene_execution::publish(session.scene_mut(), clear).unwrap();
    record(session.session_mut());
    session.session_mut().advance_to(7.0).unwrap();
    let mut restore = noon_core::SemanticMutationTransaction::new();
    restore.set_inset_2d_view(
        view.display().node_id(),
        view.camera_frame().node_id(),
        false,
    );
    scene_execution::publish(session.scene_mut(), restore).unwrap();
    record(session.session_mut());
    session.session_mut().advance_to(8.0).unwrap();
    let mut retire = noon_core::SemanticMutationTransaction::new();
    retire.remove_node(view.camera_frame().node_id());
    scene_execution::publish(session.scene_mut(), retire).unwrap();
    record(session.session_mut());
    assert!(session.session().inset_2d_views().unwrap().is_empty());
    let frontier = session.scene().revision();
    verify(session.session_mut(), &samples);
    for (time, expected) in views.iter().rev().chain(views.iter()) {
        session.seek(*time).unwrap();
        assert_eq!(
            &session.session().inset_2d_views().unwrap(),
            expected,
            "inset at {time}"
        );
    }
    session.seek(6.5).unwrap();
    session.session_mut().take_frame_changes();
    session.seek(5.9).unwrap();
    assert!(
        !session.session_mut().take_frame_changes().is_empty(),
        "a relation-only seek must wake presentation"
    );
    session.session_mut().advance_to(6.5).unwrap(); // Cross only the relation clear at t=6.
    let changes = session.session_mut().take_frame_changes();
    assert!(
        !changes.is_empty(),
        "a relation-only revision must wake presentation"
    );
    assert_eq!(
        changes.object_indices().len(),
        1,
        "only the display is dirty"
    );
    assert_eq!(session.session().replay_stats().objects_restored, 1);
    assert_eq!(session.session().replay_stats().channels_restored, 0);
    session.session_mut().advance_to(6.5).unwrap();
    assert!(session.session_mut().take_frame_changes().is_empty());
    assert_eq!(
        session.scene().revision(),
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
    let mut session = scene_execution::lower(&scene).unwrap();
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
    scene_execution::publish_at_root(
        scene.integration_store(),
        scene.root(),
        &mut session,
        transaction,
    )
    .unwrap();
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
