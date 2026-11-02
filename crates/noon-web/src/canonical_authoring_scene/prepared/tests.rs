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
        .live_set_translation(&target, 3.0, 0.0)
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
    assert!(context.begin_observed_prepared_source().unwrap());
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
    let mut reader = context.create_finished_static_reader().unwrap();
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
fn completed_zero_barriers_keep_final_same_time_edits_and_reject_pending_work() {
    let (mut context, target) = fixture();
    context.begin_ordinary_wait(0.0).unwrap();
    assert!(context.create_finished_static_reader().is_err());
    assert!(!context.prepare_initial_source_segment().unwrap());
    context
        .active_live_player()
        .unwrap()
        .live_set_translation(&target, 3.0, 0.0)
        .unwrap();
    context.begin_ordinary_wait(0.0).unwrap();
    assert!(!context.prepare_initial_source_segment().unwrap());
    let mut reader = context.create_finished_static_reader().unwrap();
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
