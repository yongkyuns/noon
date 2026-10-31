//! Fixed root audio with the existing visual history, not host/device playback.
use noon::{
    AnimationOptions, AudioClipOptions, AudioResource, ExecutionSession, RateFunction, Scene,
};
use noon_core::{
    SemanticFadeDirection, SemanticMutationTransaction, SemanticObjectProperty, SemanticVec3,
};
use noon_runtime::{FrameState, ReplayError, ReplayLimits};

fn soundtrack(scene: &mut Scene) -> (*const f32, noon_core::SemanticNodeId) {
    let pcm = AudioResource::from_interleaved(8_000, 1, vec![0.125; 8_000], 32_000).unwrap();
    let address = pcm.samples().as_ptr();
    let clip = scene
        .audio(
            pcm,
            AudioClipOptions {
                start_time: 0.25,
                source_start_frame: 1_000,
                source_end_frame: Some(7_000),
                duration: Some(6.0),
                looped: true,
                gain: 0.5,
                fade_in: 0.5,
                fade_out: 1.0,
            },
        )
        .unwrap();
    (address, clip)
}

fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

fn check_audio(
    session: &ExecutionSession,
    address: *const f32,
    clip_id: noon_core::SemanticNodeId,
) {
    let time = session.frame().time;
    let publication = session.publication_context();
    let mut output = session.audio_playback().unwrap();
    if time >= 6.25 {
        assert!(
            output.next().is_none(),
            "half-open soundtrack end must be silent"
        );
    } else {
        let clip = output.next().unwrap();
        let elapsed = (time - 0.25).max(0.0);
        let gain = if elapsed < 0.5 {
            elapsed
        } else if elapsed > 5.0 {
            0.5 * (6.0 - elapsed)
        } else {
            0.5
        };
        assert_eq!(clip.clip, clip_id);
        assert_eq!(
            clip.pcm.samples().as_ptr(),
            address,
            "PCM must never be copied"
        );
        assert!((clip.start_delay - (0.25 - time).max(0.0)).abs() < 1e-12);
        assert!((clip.source_offset - (0.125 + elapsed.rem_euclid(0.75))).abs() < 1e-12);
        assert!((clip.duration - (6.0 - elapsed)).abs() < 1e-12);
        assert!((clip.gain_points.points()[0].gain - gain).abs() < 1e-12);
        assert!(output.next().is_none(), "one clip, no duplicate output");
    }
    assert_eq!(
        session.publication_context(),
        publication,
        "queries are read-only"
    );
}

// Retained physical slots are not historical membership. Compare every visible
// row/override in painter order, just as the existing visual replay fixtures do.
fn snapshot(session: &ExecutionSession) -> FrameState {
    let frame = session.frame();
    let mut result = frame.clone();
    let indices = session
        .painter_order()
        .iter()
        .map(|&i| i as usize)
        .collect::<Vec<_>>();
    result.objects = indices.iter().map(|&i| frame.objects[i].clone()).collect();
    result.presences = indices.iter().map(|&i| frame.presences[i]).collect();
    result.reveals = indices.iter().map(|&i| frame.reveals[i]).collect();
    result.morphs = indices.iter().map(|&i| frame.morphs[i]).collect();
    result.render_geometries = indices
        .iter()
        .map(|&i| frame.render_geometries[i].clone())
        .collect();
    result.render_transforms = indices
        .iter()
        .map(|&i| frame.render_transforms[i])
        .collect();
    result.family_animations = indices
        .iter()
        .map(|&i| frame.family_animations[i])
        .collect();
    result.family_animation_plan_indices = indices
        .iter()
        .map(|&i| frame.family_animation_plan_indices[i])
        .collect();
    result
}

#[test]
fn soundtrack_survives_create_morph_detach_reentry_and_reverse_seek() {
    let mut scene = Scene::new();
    let shape = scene.square(1.0).unwrap();
    let mut target = scene.circle(0.6).unwrap();
    target.shift(2.0, 0.0).unwrap();
    let (address, clip) = soundtrack(&mut scene);
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let mut frames = Vec::new();
    for stage in 0..4 {
        let start = stage as f64 * 1.25;
        session.advance_to(start).unwrap();
        let segment = if stage == 1 {
            scene
                .live(&mut session)
                .declare_and_activate_transform_to(&shape, &target, options())
                .unwrap()
        } else {
            scene
                .live(&mut session)
                .declare_and_activate_fade(
                    &shape,
                    if stage == 2 {
                        SemanticFadeDirection::Out
                    } else {
                        SemanticFadeDirection::In
                    },
                    options(),
                )
                .unwrap()
        };
        for fraction in [0.125, 0.375, 0.875] {
            scene
                .live(&mut session)
                .advance_segment_to(segment, start + fraction)
                .unwrap();
            check_audio(&session, address, clip);
            frames.push(snapshot(&session));
        }
        scene
            .live(&mut session)
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        session.advance_to(start + 1.125).unwrap();
        check_audio(&session, address, clip);
        frames.push(snapshot(&session));
    }
    for time in [5.5, 6.0, 6.25] {
        session.advance_to(time).unwrap();
        check_audio(&session, address, clip);
        frames.push(snapshot(&session));
    }
    let authored = scene.integration_store().borrow().scene_revision();
    session.seal_replay().unwrap();
    for frame in frames.iter().rev() {
        session.seek(frame.time).unwrap();
        assert_eq!(&snapshot(&session), frame, "rewind at {}", frame.time);
        check_audio(&session, address, clip);
    }
    for frame in &frames {
        session.advance_to(frame.time).unwrap();
        assert_eq!(&snapshot(&session), frame, "forward at {}", frame.time);
        check_audio(&session, address, clip);
    }
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        authored
    );
}

#[test]
fn root_removal_and_clip_replacement_reject_an_entire_mixed_batch() {
    let mut scene = Scene::new();
    let shape = scene.circle(0.5).unwrap();
    scene.add(&shape).unwrap();
    let (address, clip) = soundtrack(&mut scene);
    let mut session = scene.execution_session().unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    session.advance_to(0.5).unwrap();
    session.take_frame_changes();
    let before = snapshot(&session);
    let publication = session.publication_context();
    let revision = scene.integration_store().borrow().scene_revision();
    let resources = scene.integration_store().borrow().audio_resources().stats();
    let definition = *scene
        .integration_store()
        .borrow()
        .semantic_audio_clip_state(clip)
        .unwrap()
        .definition();
    for target in [scene.root(), clip] {
        let mut batch = SemanticMutationTransaction::new();
        batch.set_property(
            shape.node_id(),
            SemanticObjectProperty::Translation,
            SemanticVec3::new(99.0, 0.0, 0.0),
        );
        batch.remove_node(target);
        assert!(scene.live(&mut session).apply(batch).is_err());
        assert_eq!(snapshot(&session), before);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(
            scene.integration_store().borrow().audio_resources().stats(),
            resources
        );
        assert!(session.take_frame_changes().is_empty());
        check_audio(&session, address, clip);
    }
    let mut batch = SemanticMutationTransaction::new();
    batch.set_property(
        shape.node_id(),
        SemanticObjectProperty::Translation,
        SemanticVec3::new(99.0, 0.0, 0.0),
    );
    batch.replace_audio_clip(clip, definition);
    assert!(scene.live(&mut session).apply(batch).is_err());
    assert_eq!(snapshot(&session), before);
    assert_eq!(session.publication_context(), publication);
    assert!(session.take_frame_changes().is_empty());
    scene.live(&mut session).shift(&shape, 1.0, 0.0).unwrap();
    check_audio(&session, address, clip);
}

#[test]
fn retention_limit_and_discard_do_not_interrupt_valid_forward_visual_edits() {
    let mut scene = Scene::new();
    let shape = scene.circle(0.5).unwrap();
    scene.add(&shape).unwrap();
    let (address, clip) = soundtrack(&mut scene);
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits {
            revisions: 1,
            payloads: 1,
        })
        .unwrap();
    for time in [0.5, 1.0, 1.5] {
        session.advance_to(time).unwrap();
        scene.live(&mut session).shift(&shape, 1.0, 0.0).unwrap();
        check_audio(&session, address, clip);
    }
    assert_eq!(session.seal_replay(), Err(ReplayError::RetentionLimit));
    session.discard_replay_retention();
    scene.live(&mut session).shift(&shape, 1.0, 0.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 4.0);
    check_audio(&session, address, clip);
}

#[cfg(feature = "prepared-prefix")]
#[test]
fn root_audio_history_admits_bounded_reader_but_not_immutable_output() {
    let mut scene = Scene::new();
    soundtrack(&mut scene);
    let mut session = scene.execution_session().unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    session.validate_prepared_source_domain().unwrap();
    assert!(!session.supports_immutable_playback());
    let segment = session.wait_segment(1.0).unwrap();
    session.advance_to(segment.end_time()).unwrap();
    let (_, mut reader) = session.create_prepared_prefix(segment).unwrap();
    reader.seek(0.5).unwrap();
    assert!(reader.has_audio());
    let sound = reader.audio_playback().unwrap().next().unwrap();
    assert_eq!(
        sound.duration, 0.5,
        "only delivered visuals authorize the plan"
    );
    assert!(!session.supports_immutable_playback());
}
