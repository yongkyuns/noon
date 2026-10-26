//! Real compiler/runtime tests for a byte cap independent of payload counts.
use super::*;
use noon_compile::{CompiledScene, ExecutionPatch};
use noon_core::{Style, Transform2D};

fn object(id: u64) -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    )
}

fn source() -> SceneInstance {
    let compiled = CompiledScene::compile_objects(vec![object(0)], &[]).unwrap();
    let mut source = SceneInstance::new(compiled);
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 16,
            payloads: 512,
        })
        .unwrap();
    source.advance_to(1.0).unwrap();
    source.take_frame_changes();
    source
}

fn table_bytes() -> usize {
    16 * std::mem::size_of::<Revision>()
}

fn row_bytes(source: &SceneInstance) -> usize {
    source
        .compiled
        .prepare_replay_revision(&ExecutionPatch::CreateObject(object(1)))
        .unwrap()
        .prefix_record_payload_bytes()
        .unwrap()
}

fn create(source: &mut SceneInstance, id: u64) {
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(id)))
        .unwrap();
    source.advance_to(id as f64 + 1.0).unwrap();
}

fn adopt(receiver: &mut PrefixReceiver, runtime: &mut SceneInstance, batch: PrefixBatch) {
    match receiver.preflight_append(runtime, &batch) {
        Ok(_) => {}
        Err(PrefixError::CapacityRequired { object_slots }) => {
            receiver
                .reserve_object_capacity(runtime, object_slots)
                .unwrap();
        }
        Err(error) => panic!("unexpected preflight: {error}"),
    }
    receiver.append(runtime, batch).unwrap();
}

#[test]
fn bootstrap_charges_the_full_requested_table_before_reserving_it() {
    let source = source();
    let frame = source.frame().clone();
    let publication = source.publication_context();
    assert!(matches!(
        source.create_prefix_projection_with_budgets(0, table_bytes() - 1),
        Err(PrefixError::HistoryRecordBudgetExceeded {
            retained_bytes: 0,
            additional_bytes,
            ..
        }) if additional_bytes == table_bytes()
    ));
    let (publisher, runtime, receiver) = source
        .create_prefix_projection_with_budgets(0, table_bytes())
        .unwrap();
    assert_eq!(publisher.extracted_history_record_bytes(), table_bytes());
    assert_eq!(receiver.retained_history_record_bytes(), table_bytes());
    assert_eq!(receiver.history_record_budget_bytes(), table_bytes());
    assert_eq!(runtime.replay_stats().revisions_retained, 0);
    assert_eq!(source.frame(), &frame);
    assert_eq!(source.publication_context(), publication);
}

#[test]
fn bootstrap_includes_saved_payload_records_not_only_the_outer_table() {
    let mut source = source();
    let row = row_bytes(&source);
    create(&mut source, 1);
    assert!(matches!(
        source.create_prefix_projection_with_budgets(0, table_bytes() + row - 1),
        Err(PrefixError::HistoryRecordBudgetExceeded { .. })
    ));
    let (publisher, _, receiver) = source
        .create_prefix_projection_with_budgets(0, table_bytes() + row)
        .unwrap();
    assert_eq!(
        publisher.extracted_history_record_bytes(),
        table_bytes() + row
    );
    assert_eq!(
        receiver.retained_history_record_bytes(),
        table_bytes() + row
    );
    assert_eq!(publisher.extracted_resource_bytes(), 0);
}

#[test]
fn pending_records_count_cumulatively_and_failed_terminal_delivery_is_atomic() {
    for terminal in [false, true] {
        let mut source = source();
        let row = row_bytes(&source);
        let limit = table_bytes() + 2 * row - 1;
        let (mut publisher, mut runtime, mut receiver) = source
            .create_prefix_projection_with_budgets(0, limit)
            .unwrap();
        create(&mut source, 1);
        let pending = source.prepare_prefix_update(&mut publisher).unwrap();
        assert_eq!(pending.history_record_bytes(), row);
        assert_eq!(
            publisher.extracted_history_record_bytes(),
            table_bytes() + row
        );
        assert_eq!(receiver.retained_history_record_bytes(), table_bytes());
        create(&mut source, 2);
        if terminal {
            source.seal_replay().unwrap();
        }
        let source_frame = source.frame().clone();
        let reader_frame = runtime.frame().clone();
        let publication = runtime.publication_context();
        for _ in 0..2 {
            let rejected = if terminal {
                source.prepare_prefix_finish(&mut publisher)
            } else {
                source.prepare_prefix_update(&mut publisher)
            };
            assert!(matches!(
                rejected,
                Err(PrefixError::HistoryRecordBudgetExceeded { .. })
            ));
            assert_eq!(publisher.frontier(), 2.0);
            assert_eq!(publisher.next_revision, 1);
            assert_eq!(publisher.object_slots, 2);
            assert_eq!(
                publisher.extracted_history_record_bytes(),
                table_bytes() + row
            );
            assert!(!publisher.finished);
            assert_eq!(receiver.end_time(), None);
            assert_eq!(runtime.frame(), &reader_frame);
            assert_eq!(runtime.publication_context(), publication);
            assert_eq!(source.frame(), &source_frame);
        }
        adopt(&mut receiver, &mut runtime, pending);
        receiver.prepare_seek(&mut runtime, 1.5).unwrap();
        runtime.seek(1.5).unwrap();
        assert_eq!(
            runtime.frame().presences.iter().filter(|&&live| live).count(),
            2
        );
        receiver.prepare_seek(&mut runtime, 0.5).unwrap();
        runtime.seek(0.5).unwrap();
        assert_eq!(
            runtime.frame().presences.iter().filter(|&&live| live).count(),
            1
        );
    }
}

#[test]
fn exact_limit_survives_exchange_rewind_and_an_empty_terminal_marker() {
    let mut source = source();
    let row = row_bytes(&source);
    let limit = table_bytes() + 2 * row;
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_budgets(0, limit)
        .unwrap();
    for id in 1..=2 {
        create(&mut source, id);
        let batch = source.prepare_prefix_update(&mut publisher).unwrap();
        // Forward conversion preserves the charge of an originally absent row.
        assert_eq!(batch.history_record_bytes(), row);
        assert_eq!(
            batch.revisions[0]
                .inverse
                .as_ref()
                .unwrap()
                .prefix_record_payload_bytes(),
            Some(row)
        );
        adopt(&mut receiver, &mut runtime, batch);
    }
    for time in [2.5, 0.5, 1.5, 0.5] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(receiver.retained_history_record_bytes(), limit);
        let saved: usize = runtime
            .replay_history
            .as_ref()
            .unwrap()
            .revisions
            .iter()
            .map(|revision| {
                revision
                    .inverse
                    .as_ref()
                    .unwrap()
                    .prefix_record_payload_bytes()
                    .unwrap()
            })
            .sum();
        assert_eq!(table_bytes() + saved, limit);
    }
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    assert_eq!(terminal.history_record_bytes(), 0);
    for _ in 0..2 {
        receiver.preflight_append(&runtime, &terminal).unwrap();
        assert_eq!(receiver.retained_history_record_bytes(), limit);
        assert_eq!(receiver.end_time(), None);
    }
    receiver.append(&mut runtime, terminal).unwrap();
    assert_eq!(receiver.end_time(), Some(3.0));
    assert_eq!(runtime.frame().time, 0.5);
    assert_eq!(receiver.retained_history_record_bytes(), limit);
}

#[test]
fn permanent_record_rejection_precedes_capacity_and_does_not_consume_the_batch() {
    let mut source = source();
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    create(&mut source, 1);
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    let budget = receiver.history_record_budget;
    let frame = runtime.frame().clone();
    let publication = runtime.publication_context();
    let capacity = runtime.frame.objects.capacity();
    // Private stricter-reader injection; no mismatched policy in the public API.
    receiver.history_record_budget = HistoryRecordBudget::new(table_bytes())
        .with_additional(table_bytes())
        .unwrap();
    for _ in 0..2 {
        assert!(matches!(
            receiver.preflight_append(&runtime, &batch),
            Err(PrefixError::HistoryRecordBudgetExceeded { .. })
        ));
    }
    assert_eq!(runtime.frame.objects.capacity(), capacity);
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(runtime.publication_context(), publication);
    assert_eq!(receiver.retained_history_record_bytes(), table_bytes());
    receiver.history_record_budget = budget;
    assert_eq!(
        receiver.preflight_append(&runtime, &batch),
        Err(PrefixError::CapacityRequired { object_slots: 2 })
    );
    let additional = batch.history_record_bytes();
    adopt(&mut receiver, &mut runtime, batch);
    assert_eq!(
        receiver.retained_history_record_bytes(),
        table_bytes() + additional
    );
}

#[test]
fn overflowing_table_request_is_rejected_without_attempting_the_allocation() {
    let mut source = source();
    source.replay_history.as_mut().unwrap().limits.revisions = usize::MAX;
    let frame = source.frame().clone();
    assert!(matches!(
        source.create_prefix_projection_with_budgets(0, usize::MAX),
        Err(PrefixError::Replay(ReplayError::RetentionLimit))
    ));
    assert_eq!(source.frame(), &frame);
    assert!(source.replay_retention_valid());
    source.replay_history.as_mut().unwrap().limits.revisions = 16;
    assert!(source.create_prefix_projection().is_ok());
}
