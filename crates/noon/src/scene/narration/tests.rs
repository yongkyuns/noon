use super::*;
use crate::{AudioResource, RateFunction, SemanticVec3};

fn prepared() -> PreparedNarration {
    PreparedNarration::new(
        AudioResource::from_interleaved(8_000, 1, vec![0.125; 16_000], 64_000).unwrap(),
        "Double the input.",
        "y = 2x",
        vec![NarrationCue::manual("move", 0.5, 1.5).unwrap()],
    )
    .unwrap()
}

#[test]
fn nonzero_placement_cue_waits_and_completed_animation_share_one_authored_cursor() {
    let mut scene = Scene::new();
    scene.wait(0.75).unwrap();
    let marker = scene.square(0.5).unwrap();
    scene.add(&marker).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    let position = scene
        .position_from_tracker(
            &tracker,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(0.0, 0.0, 0.0),
        )
        .unwrap();
    scene.bind_position(&marker, &position).unwrap();
    let narration = scene.narrate(&prepared()).unwrap();
    assert_eq!(scene.time(), 0.75, "admission inserts no implicit wait");
    assert_eq!(narration.start_time(), 0.75);
    assert_eq!(narration.end_time(), 2.75);
    assert_eq!(narration.duration_seconds(), 2.0);
    scene.wait_for_narration_cue(&narration, "move").unwrap();
    assert_eq!(scene.time(), 1.25);
    scene
        .play_value(&tracker, 2.0)
        .rate_func(RateFunction::Linear)
        .run_time(0.75)
        .unwrap();
    assert_eq!(scene.time(), 2.0);
    scene.wait_for_narration(&narration).unwrap();
    assert_eq!(scene.time(), 2.75, "wait only adds the remaining recording");
    scene.wait_for_narration_cue(&narration, "move").unwrap();
    assert_eq!(scene.time(), 2.75, "late cues never rewind");
    scene.wait(0.25).unwrap();
    scene.wait_for_narration(&narration).unwrap();
    assert_eq!(scene.time(), 3.0);

    let mut forward = scene.execution_session().unwrap();
    // The cold authoring cursor places later declarations; a trailing wait
    // alone does not add execution content. Explicit export holds remain the
    // existing ExportFramePolicy's responsibility.
    assert_eq!(forward.finite_timeline_end_time(), Some(2.75));
    for time in [0.0, 0.75, 1.25, 1.625, 2.0, 2.75, 3.0] {
        forward.advance_to(time).unwrap();
        let mut direct = scene.execution_session().unwrap();
        direct.seek(time).unwrap();
        assert_eq!(forward.frame().objects, direct.frame().objects);
        let clips = forward.audio_playback().unwrap().collect::<Vec<_>>();
        if time < 0.75 {
            // Device preparation includes future clips with a start delay; it
            // does not mean they are audible before their authored onset.
            assert_eq!(clips.len(), 1);
            assert_eq!(clips[0].start_delay, 0.75 - time);
            assert_eq!(clips[0].source_offset, 0.0);
            assert!(clips[0].definition.state_at(time).unwrap().is_none());
        } else if time < 2.75 {
            assert_eq!(clips.len(), 1);
            assert_eq!(clips[0].start_delay, 0.0);
            assert_eq!(clips[0].source_offset, time - 0.75);
        } else {
            assert!(clips.is_empty());
        }
    }
}

#[test]
fn repeated_placement_reuses_pcm_and_retires_the_arena_entry_with_its_last_clip() {
    let mut scene = Scene::new();
    let prepared = prepared();
    let pointer = prepared.audio().samples().as_ptr();
    let first = scene.narrate(&prepared).unwrap();
    scene.wait(3.0).unwrap();
    let second = scene.narrate(&prepared).unwrap();
    let store = scene.store.borrow();
    let resource = store
        .semantic_audio_clip_state(first.node_id())
        .unwrap()
        .definition()
        .resource();
    assert_eq!(
        resource,
        store
            .semantic_audio_clip_state(second.node_id())
            .unwrap()
            .definition()
            .resource()
    );
    assert_eq!(store.audio_resources().len(), 1);
    assert_eq!(
        store
            .audio_resources()
            .get(resource)
            .unwrap()
            .samples()
            .as_ptr(),
        pointer
    );
    drop(store);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(first.node_id());
    transaction.remove_node(second.node_id());
    scene.apply_semantic_transaction(transaction).unwrap();
    assert!(scene
        .store
        .borrow()
        .audio_resources()
        .get(resource)
        .is_none());
    assert!(scene.wait_for_narration(&first).is_err());
}

#[test]
fn unknown_foreign_stale_and_changed_receipts_cannot_advance_scene_time() {
    let mut scene = Scene::new();
    let narration = scene.narrate(&prepared()).unwrap();
    let before = scene.time();
    assert!(scene.wait_for_narration_cue(&narration, "missing").is_err());
    assert_eq!(scene.time(), before);
    let mut foreign = Scene::new();
    let equal_raw_id = foreign.narrate(&prepared()).unwrap();
    assert_eq!(equal_raw_id.node_id(), narration.node_id());
    assert!(matches!(
        foreign.wait_for_narration(&narration),
        Err(AuthoringError::Narration(NarrationError::ForeignScene))
    ));
    assert_eq!(foreign.time(), 0.0);
    let mut other_root = Scene::with_integration_store(Rc::clone(&scene.store));
    assert!(other_root.wait_for_narration(&narration).is_err());
    assert_eq!(other_root.time(), 0.0);

    let changed = {
        let store = scene.store.borrow();
        let handle = narration.definition.resource();
        AudioClipDefinition::new(
            handle,
            store.audio_resources().get(handle).unwrap(),
            AudioClipOptions {
                start_time: 1.0,
                ..AudioClipOptions::default()
            },
        )
        .unwrap()
    };
    let mut transaction = SemanticMutationTransaction::new();
    transaction.replace_audio_clip(narration.node_id(), changed);
    scene.apply_semantic_transaction(transaction).unwrap();
    assert!(matches!(
        scene.wait_for_narration(&narration),
        Err(AuthoringError::Narration(NarrationError::ChangedClip))
    ));
    assert_eq!(scene.time(), before);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.remove_node(narration.node_id());
    scene.apply_semantic_transaction(transaction).unwrap();
    assert!(scene.wait_for_narration(&narration).is_err());
    assert_eq!(scene.time(), before);
}

#[test]
fn unrepresentable_placement_and_running_admission_leave_scene_coherent() {
    let mut scene = Scene::new();
    scene.wait(1e20).unwrap();
    let before = (
        scene.time(),
        scene.revision(),
        scene.store.borrow().audio_resources().stats(),
    );
    assert!(scene.narrate(&prepared()).is_err());
    assert_eq!(
        before,
        (
            scene.time(),
            scene.revision(),
            scene.store.borrow().audio_resources().stats()
        )
    );

    let mut scene = Scene::new();
    let narration = scene.narrate(&prepared()).unwrap();
    let session = scene.execution_session().unwrap();
    scene.install_execution(session);
    let before = (
        scene.time(),
        scene.revision(),
        scene.store.borrow().audio_resources().stats(),
        scene.owned_execution().publication_context(),
    );
    assert!(scene.narrate(&prepared()).is_err());
    assert!(matches!(
        scene.wait_for_narration(&narration),
        Err(AuthoringError::Narration(NarrationError::RunningWait))
    ));
    assert_eq!(
        before,
        (
            scene.time(),
            scene.revision(),
            scene.store.borrow().audio_resources().stats(),
            scene.owned_execution().publication_context()
        )
    );
}

#[test]
fn absolute_cue_and_audio_endpoints_survive_subtraction_rounding_exactly() {
    let duration = 2802.0 / 8000.0;
    assert!(
        0.1 + (duration - 0.1) < duration,
        "fixture exercises lost precision"
    );
    let short = PreparedNarration::new(
        AudioResource::from_interleaved(8_000, 1, vec![0.125; 2802], 2802 * 4).unwrap(),
        "Short phrase.",
        "Short phrase.",
        vec![],
    )
    .unwrap();
    let mut scene = Scene::new();
    let first = scene.narrate(&short).unwrap();
    scene.wait(0.1).unwrap();
    scene.wait_for_narration(&first).unwrap();
    assert_eq!(scene.time(), first.end_time());
    assert_eq!(scene.time(), duration);
    let session = scene.execution_session().unwrap();
    assert!(session
        .audio_playback_at(scene.time())
        .unwrap()
        .next()
        .is_none());
    let second = scene.narrate(&short).unwrap();
    assert_eq!(second.start_time(), first.end_time());
    let session = scene.execution_session().unwrap();
    let clips = session
        .audio_playback_at(scene.time())
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].clip, second.node_id());
    assert_eq!(clips[0].source_offset, 0.0);

    let longer = PreparedNarration::new(
        AudioResource::from_interleaved(8_000, 1, vec![0.125; 8000], 32_000).unwrap(),
        "A phrase cue.",
        "A phrase cue.",
        vec![NarrationCue::manual("move", duration, 0.5).unwrap()],
    )
    .unwrap();
    let mut scene = Scene::new();
    let handle = scene.narrate(&longer).unwrap();
    scene.wait(0.1).unwrap();
    scene.wait_for_narration_cue(&handle, "move").unwrap();
    assert_eq!(scene.time(), duration);
}
