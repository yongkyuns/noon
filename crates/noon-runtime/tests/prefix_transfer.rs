#![cfg(feature = "prepared-prefix")]
mod prefix_support;

use noon_compile::{CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::{PrefixError, ReplayError, ReplayLimits, SceneInstance};

fn source(count: usize, limits: ReplayLimits) -> SceneInstance {
    let objects = (0..count)
        .map(|i| {
            CompiledObject::new(
                ObjectId::new(i as u64),
                GeometryRef::circle(0.2),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let mut source = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    source.begin_replay_retention(limits).unwrap();
    source.advance_to(1.0).unwrap();
    source
}
fn limits() -> ReplayLimits {
    ReplayLimits {
        revisions: 64,
        payloads: 256,
    }
}
fn position(x: f32) -> ExecutionPatch {
    ExecutionPatch::SetTransform {
        object: ObjectId::new(0),
        transform: Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
    }
}

#[test]
fn multiple_writes_in_one_batch_keep_intermediate_history_and_append_does_not_seek() {
    let mut source = source(4, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let original = reader.seek(0.5).unwrap().clone();
    let _ = reader.take_renderer_publication();
    source.apply_execution_patch(&position(10.0)).unwrap();
    source.advance_to(1.5).unwrap();
    let first = source.frame().clone();
    source.advance_to(2.0).unwrap();
    source.apply_execution_patch(&position(20.0)).unwrap();
    source.advance_to(2.5).unwrap();
    let second = source.frame().clone();
    source.advance_to(3.0).unwrap();
    let source_context = source.publication_context();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().row_payloads, 2);
    assert_eq!(source.publication_context(), source_context);
    reader.append(batch).unwrap();
    assert_eq!(
        reader.frame(),
        &original,
        "append must not move/rebuild the reader frame"
    );
    assert_eq!(reader.replay_stats().revisions_crossed, 0);
    for _ in 0..8 {
        assert_eq!(reader.seek(2.5).unwrap(), &second);
        assert_eq!(reader.seek(1.5).unwrap(), &first);
        assert_eq!(reader.seek(0.5).unwrap(), &original);
    }
    source.apply_execution_patch(&position(30.0)).unwrap();
    source.advance_to(3.5).unwrap();
    let third = source.frame().clone();
    source.advance_to(4.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    assert_eq!(reader.seek(3.5).unwrap(), &third);
    assert_eq!(reader.seek(1.5).unwrap(), &first);
}

#[test]
fn published_frontier_is_half_open_and_same_time_source_edits_become_visible_later() {
    let mut source = source(1, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    assert!(matches!(
        reader.seek(1.0),
        Err(PrefixError::OutsideRange { .. })
    ));
    source.apply_execution_patch(&position(5.0)).unwrap();
    source.apply_execution_patch(&position(7.0)).unwrap();
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::NotReady)
    ));
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    assert_eq!(
        reader.seek(1.0).unwrap().objects[0].transform.translation.x,
        7.0
    );
    assert_eq!(
        reader.seek(0.999).unwrap().objects[0]
            .transform
            .translation
            .x,
        0.0
    );
    assert!(matches!(
        reader.seek(2.0),
        Err(PrefixError::OutsideRange { .. })
    ));
}

#[test]
fn ordinary_added_and_reconciled_position_track_uses_existing_replay_evaluator() {
    let mut source = source(1, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(1),
            object: ObjectId::new(0),
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(4.0, 0.0),
            },
            timing: TrackTiming::new(1.0, 1.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }))
        .unwrap();
    source.advance_to(1.5).unwrap();
    let middle = source.frame().clone();
    assert_eq!(middle.objects[0].transform.translation.x, 2.0);
    source.advance_to(2.0).unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::ReconcileTrack {
            track: TrackId::new(1),
            object: ObjectId::new(0),
            property: Property::Position,
            end_time: 2.0,
        })
        .unwrap();
    source.advance_to(2.5).unwrap();
    let end = source.frame().clone();
    source.advance_to(3.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    assert_eq!(reader.seek(2.5).unwrap(), &end);
    assert_eq!(reader.seek(1.5).unwrap(), &middle);
    assert_eq!(
        reader.seek(0.5).unwrap().objects[0].transform.translation.x,
        0.0
    );
}

#[test]
fn quiet_wait_extension_has_no_row_payload_and_preserves_the_playhead() {
    let mut source = source(100, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let frame = reader.frame().clone();
    source.advance_to(100.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().payloads, 0);
    assert_eq!(batch.stats().revisions, 0);
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.frontier(), 100.0);
    assert_eq!(reader.seek(99.0).unwrap().time, 99.0);
}

#[test]
fn foreign_and_out_of_order_batches_reject_without_mutating_reader() {
    let mut source = source(1, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let (mut other, _) = prefix_support::bootstrap(&source).unwrap();
    source.apply_execution_patch(&position(2.0)).unwrap();
    source.advance_to(2.0).unwrap();
    let first = source.prepare_prefix_update(&mut publisher).unwrap();
    let foreign = source.prepare_prefix_update(&mut other).unwrap();
    source.advance_to(3.0).unwrap();
    let second = source.prepare_prefix_update(&mut publisher).unwrap();
    let frame = reader.frame().clone();
    assert_eq!(reader.append(foreign), Err(PrefixError::UnexpectedBatch));
    assert_eq!(reader.append(second), Err(PrefixError::UnexpectedBatch));
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.frontier(), 1.0);
    reader.append(first).unwrap();
    assert_eq!(
        reader.seek(1.5).unwrap().objects[0].transform.translation.x,
        2.0
    );
}

#[test]
fn cloned_source_and_replaced_replay_scope_cannot_continue_the_old_stream() {
    let mut source = source(1, limits());
    let (mut publisher, _) = prefix_support::bootstrap(&source).unwrap();
    let clone = source.clone();
    assert!(matches!(
        clone.prepare_prefix_update(&mut publisher),
        Err(PrefixError::SourceChanged)
    ));
    source.discard_replay_retention();
    source.begin_replay_retention(limits()).unwrap();
    source.advance_to(2.0).unwrap();
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::SourceChanged)
    ));
}

#[test]
fn structural_removal_preserves_delivered_history_and_later_membership() {
    let mut source = source(2, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    source.apply_execution_patch(&position(3.0)).unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1)))
        .unwrap();
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    drop(source);
    assert_eq!(
        reader.seek(1.5).unwrap().objects[0].transform.translation.x,
        3.0
    );
    assert_eq!(reader.take_renderer_publication().painter_order(), &[0]);
    reader.seek(0.5).unwrap();
    assert_eq!(reader.take_renderer_publication().painter_order(), &[0, 1]);
}

#[test]
fn retention_failure_keeps_delivered_history_valid() {
    let mut source = source(
        1,
        ReplayLimits {
            revisions: 2,
            payloads: 2,
        },
    );
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    for x in [1.0, 2.0, 3.0] {
        source.apply_execution_patch(&position(x)).unwrap();
    }
    source.advance_to(2.0).unwrap();
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::Replay(ReplayError::RetentionLimit))
    ));
    assert_eq!(
        reader.seek(0.5).unwrap().objects[0].transform.translation.x,
        0.0
    );
}

#[test]
fn failed_source_transaction_never_enters_transferred_history() {
    let mut source = source(1, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let transaction =
        ExecutionMutationTransaction::from_mutations([position(4.0), position(f32::NAN)]);
    assert!(source.apply_execution_transaction(&transaction).is_err());
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().revisions, 0);
    reader.append(batch).unwrap();
    assert_eq!(
        reader.seek(1.5).unwrap().objects[0].transform.translation.x,
        0.0
    );
}

#[test]
fn transfer_payload_depends_on_new_changes_not_scene_or_old_history_size() {
    let mut observed = Vec::new();
    for count in [10, 10_000] {
        let mut source = source(count, limits());
        let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
        for step in 1..=16 {
            source
                .apply_execution_patch(&position(step as f32))
                .unwrap();
            source.advance_to(step as f64 + 1.0).unwrap();
            let batch = source.prepare_prefix_update(&mut publisher).unwrap();
            assert_eq!(batch.stats().revisions, 1);
            assert_eq!(batch.stats().row_payloads, 1);
            observed.push(batch.stats());
            reader.append(batch).unwrap();
        }
        assert_eq!(
            reader.seek(15.5).unwrap().objects[0]
                .transform
                .translation
                .x,
            15.0
        );
        assert_eq!(
            reader.seek(1.5).unwrap().objects[0].transform.translation.x,
            1.0
        );
    }
    assert!(observed.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn bounded_cross_thread_delivery_keeps_old_samples_stable_during_further_preparation() {
    use std::{sync::mpsc, time::Duration};
    let mut source = source(20, limits());
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let (ack, acknowledged) = mpsc::sync_channel(1);
    let handle = std::thread::spawn(move || {
        for step in 1..=12 {
            reader
                .append(receive.recv_timeout(Duration::from_secs(10)).unwrap())
                .unwrap();
            assert_eq!(
                reader.seek(step as f64 + 0.5).unwrap().objects[0]
                    .transform
                    .translation
                    .x,
                step as f32
            );
            assert_eq!(
                reader.seek(0.5).unwrap().objects[0].transform.translation.x,
                0.0
            );
            ack.send(()).unwrap();
        }
        reader
    });
    for step in 1..=12 {
        source
            .apply_execution_patch(&position(step as f32))
            .unwrap();
        source.advance_to(step as f64 + 1.0).unwrap();
        send.send(source.prepare_prefix_update(&mut publisher).unwrap())
            .unwrap();
        acknowledged.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    drop(source);
    let mut reader = handle.join().unwrap();
    assert_eq!(
        reader.seek(12.5).unwrap().objects[0]
            .transform
            .translation
            .x,
        12.0
    );
}

#[test]
fn unavailable_and_invalid_time_do_not_change_the_reader() {
    let source = source(1, limits());
    let (_, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let before = reader.frame().clone();
    for time in [-1.0, 1.0, 2.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            reader.seek(time),
            Err(PrefixError::OutsideRange { .. })
        ));
        assert_eq!(reader.frame(), &before);
    }
}
