use super::*;
use noon::{AnimationOptions, IndicateOptions, Mobject, RateFunction};
use noon_core::{HostCallbackId, ObjectId, Vec2};

mod worker;

fn fixture() -> (CanonicalAuthoringScene, Mobject) {
    let mut context = CanonicalAuthoringScene::default();
    let mut target = context.scene.circle(0.5).unwrap();
    target.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
    context.bind_mobject(ObjectId::new(0), &target).unwrap();
    context
        .scene
        .on_click_indicate(
            &target,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(0.5),
        )
        .unwrap();
    (context, target)
}

fn deliver(reader: &mut PrefixReader, batch: PrefixBatch) {
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.preflight_append(&batch).unwrap();
    reader.append(batch).unwrap();
}

fn prepared_source_delivery() -> (CanonicalAuthoringScene, crate::PreparedExecutionPlayer) {
    let mut context = CanonicalAuthoringScene::default();
    let target = context.scene.circle(0.5).unwrap();
    context.bind_mobject(ObjectId::new(0), &target).unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let player = crate::PreparedExecutionPlayer::from_completed_source(&mut context, 711).unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    (context, player)
}

#[test]
fn prepared_source_segment_yields_before_completion_and_reader_adoption() {
    let (mut context, mut player) = prepared_source_delivery();
    let frontier = player.prepared_frontier();
    assert_eq!(
        player
            .prepare_interpreter_segment_step(&mut context)
            .unwrap(),
        None
    );
    assert_eq!(player.prepared_frontier(), frontier);
    assert_eq!(player.prepared_end_time(), None);

    assert_eq!(
        player
            .prepare_interpreter_segment_step(&mut context)
            .unwrap(),
        Some(true)
    );
    assert_eq!(player.prepared_frontier(), frontier);
    assert!(player.reserve_source_prefix_capacity().unwrap());
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(player.prepared_frontier(), 2.0);
}

#[test]
fn prepared_source_segment_rejects_a_changed_phase_and_cancellation_keeps_reader() {
    let (mut context, mut player) = prepared_source_delivery();
    let frontier = player.prepared_frontier();
    assert_eq!(
        player
            .prepare_interpreter_segment_step(&mut context)
            .unwrap(),
        None
    );
    context
        .active_live_player()
        .unwrap()
        .live_complete_segment()
        .unwrap();
    assert!(player
        .prepare_interpreter_segment_step(&mut context)
        .is_err());
    assert!(
        player
            .prepare_interpreter_segment_step(&mut context)
            .is_err(),
        "failed completion is not retried from the mutated source"
    );
    assert_eq!(player.prepared_frontier(), frontier);
    assert!(!player.reserve_source_prefix_capacity().unwrap());

    let (mut context, mut player) = prepared_source_delivery();
    let frontier = player.prepared_frontier();
    assert_eq!(
        player
            .prepare_interpreter_segment_step(&mut context)
            .unwrap(),
        None
    );
    player.stop_source_preparation();
    assert_eq!(player.prepared_frontier(), frontier);
    assert_eq!(player.prepared_time(), 0.0);
}

#[test]
fn foreign_source_step_rejection_preserves_the_rightful_owner() {
    let (mut context, mut player) = prepared_source_delivery();
    let mut foreign = CanonicalAuthoringScene::default();
    foreign.begin_ordinary_wait(1.0).unwrap();

    assert!(player
        .prepare_interpreter_segment_step(&mut foreign)
        .is_err());
    assert_eq!(
        player
            .prepare_interpreter_segment_step(&mut context)
            .unwrap(),
        None
    );
    assert_eq!(
        player
            .prepare_interpreter_segment_step(&mut context)
            .unwrap(),
        Some(true)
    );
}

#[test]
fn synchronous_source_segment_api_still_completes_without_exposing_a_stage() {
    let (mut context, mut player) = prepared_source_delivery();
    assert!(player.prepare_interpreter_segment(&mut context).unwrap());
    assert_eq!(player.prepared_frontier(), 1.0);
    assert!(player.reserve_source_prefix_capacity().unwrap());
    assert!(player.deliver_source_prefix().unwrap());
    assert_eq!(player.prepared_frontier(), 2.0);
}

#[test]
fn interpreter_host_effect_admission_retains_a_quiet_original_source_at_its_actual_time() {
    let (mut context, _) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    let player = context.active_live_player().unwrap();
    let before = player.debug_frame_json();
    let identity = player.ownership_identity();
    assert_eq!(
        context
            .begin_observed_prepared_source(noon::ContinuationEffects::Opaque)
            .unwrap(),
        Some(noon::SourceExecutionRequirement::HostEffects)
    );
    let player = context.take_execution_player(1.0, 0).unwrap();
    assert_eq!(player.ownership_identity(), identity);
    assert_eq!(player.debug_frame_json(), before);
    assert_eq!(player.time(), 0.0);
    assert!(player.has_pending_live_segment());
}

#[test]
fn opaque_source_reason_refines_only_after_a_callback_is_registered() {
    let (mut context, target) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    assert_eq!(
        context
            .begin_observed_prepared_source(noon::ContinuationEffects::Opaque)
            .unwrap(),
        Some(noon::SourceExecutionRequirement::HostEffects)
    );
    assert_eq!(
        context.observed_source_execution_requirement().unwrap(),
        Some(noon::SourceExecutionRequirement::HostEffects)
    );

    let player = context.active_live_player().unwrap();
    player.live_advance_segment_to(1.0).unwrap();
    player.live_complete_segment().unwrap();
    context
        .add_updater(&target, HostCallbackId::new(92), 1.0, None)
        .unwrap();
    assert_eq!(
        context.observed_source_execution_requirement().unwrap(),
        Some(noon::SourceExecutionRequirement::HostCallbacks)
    );
    context.begin_ordinary_wait(1.0).unwrap();
    assert_eq!(context.active_live_player().unwrap().time(), 1.0);
    assert!(context
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
}

#[test]
fn completed_observed_source_keeps_original_history_for_replay() {
    let (mut context, target) = fixture();
    context
        .scene
        .next_section_at("Start", noon::SectionType::Normal, 0.0)
        .unwrap();
    context.begin_ordinary_wait(0.5).unwrap();
    let initial = context.active_live_player().unwrap().debug_frame_json();
    let identity = context.active_live_player().unwrap().ownership_identity();
    assert_eq!(
        context
            .begin_observed_prepared_source(noon::ContinuationEffects::Opaque)
            .unwrap(),
        Some(noon::SourceExecutionRequirement::HostEffects)
    );
    let mut player = context.take_execution_player(1.0, 0).unwrap();
    player.live_advance_segment_to(0.5).unwrap();
    player.live_complete_segment().unwrap();
    context.return_execution_player(player).unwrap();
    context
        .scene
        .next_section_at("Middle", noon::SectionType::Normal, 0.5)
        .unwrap();
    context
        .active_live_player()
        .unwrap()
        .scene_mut()
        .set_translation(&target, 2.0, 0.0)
        .unwrap();
    context.begin_ordinary_wait(0.5).unwrap();
    let mut player = context.resume_execution_player().unwrap();
    player.live_advance_segment_to(1.0).unwrap();
    player.live_complete_segment().unwrap();
    context.return_execution_player(player).unwrap();
    let mut player = context.complete_source_playback().unwrap();
    player.seal_replay().unwrap();
    let sections: serde_json::Value = serde_json::from_str(&player.sections_json()).unwrap();
    assert_eq!(sections.as_array().unwrap().len(), 2);
    assert_eq!(sections[1]["name"], "Middle");
    assert_eq!(sections[1]["time"], 0.5);
    assert_eq!(player.ownership_identity(), identity);
    assert_eq!(target.center().unwrap(), (2.0, 0.0));
    player.seek_delta_json(0.0).unwrap();
    let initial: serde_json::Value = serde_json::from_str(&initial).unwrap();
    let rewound: serde_json::Value = serde_json::from_str(&player.debug_frame_json()).unwrap();
    assert_eq!(rewound["objects"], initial["objects"]);
    player.resume();
    player.tick_delta_json(1_000.0).unwrap();
    player.tick_delta_json(1_100.0).unwrap();
    assert!(player.time() > 0.0 && player.time() < 0.5);
    player.seek_delta_json(1.0).unwrap();
    assert_eq!(player.time(), 1.0);
}

#[test]
fn completed_observed_zero_wait_keeps_exact_zero_during_replay() {
    let (mut context, _) = fixture();
    context.begin_ordinary_wait(0.0).unwrap();
    assert_eq!(
        context
            .begin_observed_prepared_source(noon::ContinuationEffects::Opaque)
            .unwrap(),
        Some(noon::SourceExecutionRequirement::HostEffects)
    );
    let mut player = context.take_execution_player(4.0, 0).unwrap();
    player.live_advance_segment_to(0.0).unwrap();
    player.live_complete_segment().unwrap();
    context.return_execution_player(player).unwrap();
    let mut player = context.complete_source_playback().unwrap();
    player.seal_replay().unwrap();
    assert!(!player.is_playing());
    player.resume();
    assert!(!player.is_playing());
    player.tick_delta_json(1_000.0).unwrap();
    player.tick_delta_json(1_100.0).unwrap();
    assert_eq!(player.time(), 0.0);
    player.seek_delta_json(0.0).unwrap();
    player.resume();
    assert!(!player.is_playing());
    player.tick_callback_phase_json(1_200.0).unwrap();
    assert_eq!(player.time(), 0.0);
}

#[test]
fn interpreter_prefix_uses_actual_completed_wait_without_transferring_source() {
    let (mut context, _) = fixture();
    assert!(context.prepare_live_segment().is_err());
    assert!(context.create_prepared_prefix().is_err());
    assert_eq!(context.begin_ordinary_wait(1.0).unwrap(), 1.0);
    let identity = context.active_live_player().unwrap().ownership_identity();
    assert!(context.create_prepared_prefix().is_err());
    assert_eq!(context.prepare_live_segment().unwrap(), 1.0);
    let (_, mut reader) = context.create_prepared_prefix().unwrap();
    assert_eq!(reader.frontier(), 1.0);
    assert_eq!(reader.end_time(), None);
    reader.seek(0.5).unwrap();
    let before = context.active_live_player().unwrap().debug_frame_json();
    assert!(
        context.prepare_live_segment().is_err(),
        "completed receipt cannot drive twice"
    );
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert_eq!(context.live_execution_ownership(), "active");
    assert_eq!(context.active_live_player().unwrap().time(), 1.0);
    assert_eq!(reader.frame().time, 0.5);
}

#[test]
fn interpreter_prefix_reconciles_animation_before_subsequent_source_observation() {
    let (mut context, target) = fixture();
    let mut endpoint = target.target_editor().unwrap();
    endpoint.set_translation(2.0, 0.0).unwrap();
    let animation = context
        .declare_live_transform_to(
            &target,
            &endpoint,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let (mut publisher, mut reader) = context.create_prepared_prefix().unwrap();
    reader.seek(0.5).unwrap();
    let before = reader.frame().clone();
    context
        .active_live_player()
        .unwrap()
        .live_play_animation(&animation)
        .unwrap();
    assert!(context.publish_prepared_prefix(&mut publisher).is_err());
    assert_eq!(context.prepare_live_segment().unwrap(), 2.0);
    assert_eq!(target.center().unwrap(), (2.0, 0.0));
    deliver(
        &mut reader,
        context.publish_prepared_prefix(&mut publisher).unwrap(),
    );
    assert_eq!(reader.frame(), &before);
    reader.seek(1.5).unwrap();
    let id = noon_compile::semantic_execution_object_id(target.node_id());
    let row = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == id)
        .unwrap();
    assert_eq!(row.transform.translation, Vec2::new(1.0, 0.0));
    assert_eq!(context.active_live_player().unwrap().time(), 2.0);
}

#[test]
fn interpreter_prefix_final_edits_and_effect_survive_source_destruction() {
    let (mut context, target) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let (mut publisher, mut reader) = context.create_prepared_prefix().unwrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let id = noon_compile::semantic_execution_object_id(target.node_id());
    let effect = reader.prepare_bound_click_indicate(id).unwrap().unwrap();
    reader.start_transient_animation(effect).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.25).unwrap();
    let midpoint = reader.frame().clone();
    let receipt = reader.publication_context();
    assert_ne!(midpoint, baseline);
    context
        .active_live_player()
        .unwrap()
        .scene_mut()
        .set_translation(&target, 3.0, 0.0)
        .unwrap();
    assert!(context.finish_prepared_prefix(&mut publisher).is_err());
    assert_eq!(reader.end_time(), None);
    context.seal_prepared_source().unwrap();
    let terminal = context.finish_prepared_prefix(&mut publisher).unwrap();
    drop(context);
    drop(target);
    deliver(&mut reader, terminal);
    assert_eq!(reader.frame(), &midpoint);
    assert_eq!(reader.publication_context(), receipt);
    assert_eq!(reader.end_time(), Some(1.0));
    reader.advance_interactions(10.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
    reader.seek(1.0).unwrap();
    assert_eq!(
        reader
            .frame()
            .objects
            .iter()
            .find(|row| row.id == id)
            .unwrap()
            .transform
            .translation,
        Vec2::new(3.0, 0.0)
    );
    reader.seek(0.5).unwrap();
    assert_eq!(reader.frame(), &baseline);
}

#[test]
fn interpreter_prefix_callback_rejection_does_not_advance_or_complete_source() {
    let (mut context, target) = fixture();
    context
        .add_updater(&target, HostCallbackId::new(91), 0.0, None)
        .unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    let before = context.active_live_player().unwrap().debug_frame_json();
    assert!(context.prepare_live_segment().is_err());
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    assert!(context
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
    assert!(context.create_prepared_prefix().is_err());

    let identity = context.active_live_player().unwrap().ownership_identity();
    assert_eq!(
        context
            .begin_observed_prepared_source(noon::ContinuationEffects::SceneOnly)
            .unwrap(),
        Some(noon::SourceExecutionRequirement::HostCallbacks)
    );
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    let player = context.take_execution_player(1.0, 0).unwrap();
    assert_eq!(player.ownership_identity(), identity);
    assert!(player.has_pending_live_segment());
    assert_eq!(player.time(), 0.0);
}

#[test]
fn interpreter_prefix_causal_drag_rejection_precedes_time_advance() {
    let (mut context, target) = fixture();
    context.set_translation_drag_targets(vec![target]).unwrap();
    context.begin_ordinary_wait(1.0).unwrap();
    let before = context.active_live_player().unwrap().debug_frame_json();
    assert!(context.prepare_live_segment().is_err());
    assert_eq!(
        context.active_live_player().unwrap().debug_frame_json(),
        before
    );
    assert!(context
        .active_live_player()
        .unwrap()
        .has_pending_live_segment());
}

#[test]
fn interpreter_prefix_respects_existing_exclusive_player_lease() {
    let (mut context, _) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    let player = context.take_execution_player(1.0, 42).unwrap();
    let identity = player.ownership_identity();
    assert!(context.prepare_live_segment().is_err());
    assert!(context.create_prepared_prefix().is_err());
    context.return_execution_player(player).unwrap();
    context.prepare_live_segment().unwrap();
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        identity
    );
    assert!(context.create_prepared_prefix().is_ok());
}

#[test]
fn interpreter_prefix_foreign_publisher_rejection_preserves_valid_delivery() {
    let (mut context, _) = fixture();
    let (mut other, _) = fixture();
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    let (mut publisher, mut reader) = context.create_prepared_prefix().unwrap();
    other.begin_ordinary_wait(2.0).unwrap();
    other.prepare_live_segment().unwrap();
    assert!(other.publish_prepared_prefix(&mut publisher).is_err());
    context.begin_ordinary_wait(1.0).unwrap();
    context.prepare_live_segment().unwrap();
    deliver(
        &mut reader,
        context.publish_prepared_prefix(&mut publisher).unwrap(),
    );
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(reader.end_time(), None);
}

#[test]
fn interpreter_prefix_zero_wait_does_not_invent_a_readable_range() {
    let mut context = CanonicalAuthoringScene::default();
    context.begin_ordinary_wait(0.0).unwrap();
    assert!(!context.prepare_initial_source_segment().unwrap());
    assert!(context.create_prepared_prefix().is_err());
    context.begin_ordinary_wait(0.25).unwrap();
    assert!(context.prepare_initial_source_segment().unwrap());
    assert_eq!(context.create_prepared_prefix().unwrap().1.frontier(), 0.25);
}

#[test]
fn completed_static_scene_reader_has_no_fabricated_duration_or_source_lease() {
    let (mut future, _) = fixture();
    future.scene.wait(1.0).unwrap();
    assert!(
        future.create_finished_static_reader().is_err(),
        "future work cannot be discarded as a static frame"
    );
    let (mut context, target) = fixture();
    let mut reader = context.create_finished_static_reader().unwrap().unwrap();
    assert_eq!(context.live_execution_ownership(), "none");
    assert_eq!(reader.end_time(), Some(0.0));
    assert_eq!(reader.frame().time, 0.0);
    assert_eq!(reader.frame().objects.len(), 1);
    let expected = reader.frame().clone();
    context.begin_ordinary_wait(1.0).unwrap();
    assert!(context.create_finished_static_reader().is_err());
    drop(target);
    drop(context);
    reader.seek(0.0).unwrap();
    assert_eq!(reader.frame(), &expected);
    assert!(reader.seek(f64::EPSILON).is_err());
}

#[test]
fn finished_source_admission_preserves_declared_work_and_static_history() {
    for future in [false, true] {
        let (mut context, _) = fixture();
        if future {
            let tracker = context.scene.value_tracker(0.0).unwrap();
            context
                .scene
                .play_value(&tracker, 2.0)
                .run_time(1.0)
                .unwrap();
        }
        assert_eq!(context.prepare_finished_source().unwrap(), !future);
        if future {
            let mut player = context.take_execution_player(1.0, 7).unwrap();
            player.pause();
            assert_eq!(player.fresh_finite_source_duration(), Some(1.0));
        } else {
            let reader = context.create_finished_static_reader().unwrap().unwrap();
            assert_eq!(reader.end_time(), Some(0.0));
            assert_eq!(reader.frame().objects.len(), 1);
        }
    }
    let (mut context, _) = fixture();
    let source = crate::authoring_audio::pcm_resource(vec![0.25; 8_000], 8_000, 1).unwrap();
    context
        .audio(&source, noon::AudioClipOptions::default())
        .unwrap();
    assert_eq!(context.scene.time(), 0.0);
    assert!(!context.prepare_finished_source().unwrap());
    let player = context.take_execution_player(1.0, 7).unwrap();
    assert!(
        player.has_audio_wasm(),
        "zero authored time cannot discard audio"
    );
}

#[test]
fn completed_static_drag_cannot_be_advertised_as_input_free_history() {
    for zero_wait in [false, true] {
        let (mut context, target) = fixture();
        if zero_wait {
            context.begin_ordinary_wait(0.0).unwrap();
            assert!(!context.prepare_initial_source_segment().unwrap());
        }
        context.set_translation_drag_targets(vec![target]).unwrap();
        let original = context
            .player_ownership
            .local()
            .map(|player| player.ownership_identity());
        assert!(context.create_finished_static_reader().unwrap().is_none());
        let retained = context.active_live_player().unwrap().ownership_identity();
        if let Some(original) = original {
            assert_eq!(retained, original);
        }
        let player = context.take_execution_player(1.0, 0).unwrap();
        assert_eq!(player.ownership_identity(), retained);
        assert_eq!(player.time(), 0.0);
        assert!(!player.is_playing());
        assert!(!player.has_pending_live_segment());
    }
}

#[test]
fn completed_zero_barriers_keep_final_same_time_edits_and_reject_pending_work() {
    let (mut context, target) = fixture();
    context.begin_ordinary_wait(0.0).unwrap();
    assert!(context.create_finished_static_reader().is_err());
    assert!(!context.prepare_initial_source_segment().unwrap());
    context
        .active_live_player()
        .unwrap()
        .scene_mut()
        .set_translation(&target, 3.0, 0.0)
        .unwrap();
    context.begin_ordinary_wait(0.0).unwrap();
    assert!(!context.prepare_initial_source_segment().unwrap());
    let mut reader = context.create_finished_static_reader().unwrap().unwrap();
    assert_eq!(reader.end_time(), Some(0.0));
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(3.0, 0.0)
    );
    drop(context);
    drop(target);
    reader.seek(0.0).unwrap();
    assert_eq!(
        reader.frame().objects[0].transform.translation,
        Vec2::new(3.0, 0.0)
    );
    assert!(reader.seek(f64::EPSILON).is_err());
}
