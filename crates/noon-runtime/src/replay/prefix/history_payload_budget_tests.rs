//! Byte admission for saved revision vectors, not total history/heap accounting.
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
            payloads: 256,
        })
        .unwrap();
    source.advance_to(1.0).unwrap();
    source.take_frame_changes();
    source
}

fn create(source: &mut SceneInstance, id: u64, frontier: f64) {
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(id)))
        .unwrap();
    source.advance_to(frontier).unwrap();
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

fn retained_bytes(runtime: &SceneInstance) -> usize {
    history_payload_budget::retained_bytes(
        runtime
            .replay_history
            .as_ref()
            .unwrap()
            .revisions
            .iter()
            .filter_map(|revision| revision.inverse.as_ref()),
    )
    .unwrap()
}

#[test]
fn history_payload_budget_exact_limit_and_rejected_preflight_are_atomic() {
    let budget = HistoryPayloadBudget::new(7).with_additional(4).unwrap();
    for _ in 0..2 {
        assert_eq!(
            budget.with_additional(4),
            Err(PrefixError::HistoryPayloadBudgetExceeded {
                retained_bytes: 4,
                additional_bytes: 4,
                limit_bytes: 7,
            })
        );
    }
    assert_eq!(budget.bytes(), 4);
    let full = budget.with_additional(3).unwrap();
    assert_eq!(full.bytes(), 7);
    assert_eq!(full.with_additional(0).unwrap(), full);
    assert_eq!(
        HistoryPayloadBudget::new(0).with_additional(0).unwrap().bytes(),
        0
    );
}

#[test]
fn history_payload_budget_overflow_reports_representable_operands() {
    let budget = HistoryPayloadBudget::new(usize::MAX)
        .with_additional(usize::MAX)
        .unwrap();
    assert_eq!(
        budget.with_additional(1),
        Err(PrefixError::HistoryPayloadBudgetExceeded {
            retained_bytes: usize::MAX,
            additional_bytes: 1,
            limit_bytes: usize::MAX,
        })
    );
    assert_eq!(budget.bytes(), usize::MAX);
}

#[test]
fn history_payload_bootstrap_rejection_preserves_source_and_ordinary_replay() {
    let mut source = source();
    create(&mut source, 1, 2.0);
    let frame = source.frame().clone();
    let publication = source.publication_context();
    let identity = source.runtime_identity();
    let stats = source.replay_stats();
    let history = source.replay_history.as_ref().unwrap();
    let scope_pins = Arc::strong_count(&history.scope);
    let index_capacity = history.revisions.capacity();
    for _ in 0..2 {
        assert!(matches!(
            source.create_prefix_projection_with_budgets(0, 0),
            Err(PrefixError::HistoryPayloadBudgetExceeded {
                retained_bytes: 0,
                additional_bytes,
                limit_bytes: 0,
            }) if additional_bytes > 0
        ));
        assert_eq!(source.frame(), &frame);
        assert_eq!(source.publication_context(), publication);
        assert_eq!(source.runtime_identity(), identity);
        assert_eq!(source.replay_stats(), stats);
        let history = source.replay_history.as_ref().unwrap();
        assert_eq!(Arc::strong_count(&history.scope), scope_pins);
        assert_eq!(history.revisions.capacity(), index_capacity);
        assert_eq!(history.end, None);
    }
    source.seal_replay().unwrap();
    for (time, live) in [(0.5, 1), (1.5, 2), (0.5, 1)] {
        source.seek(time).unwrap();
        assert_eq!(source.frame().presences.iter().filter(|&&p| p).count(), live);
    }
}

#[test]
fn history_payload_bootstrap_charges_reported_capacity_and_keeps_readiness_precedence() {
    let mut source = source();
    create(&mut source, 1, 2.0);
    let (publisher, runtime, receiver) = source.create_prefix_projection().unwrap();
    let bytes = retained_bytes(&runtime);
    assert!(bytes > 0);
    assert_eq!(publisher.extracted_history_payload_bytes(), bytes);
    assert_eq!(receiver.retained_history_payload_bytes(), bytes);
    drop((publisher, runtime, receiver));
    let (publisher, runtime, receiver) = source
        .create_prefix_projection_with_budgets(0, bytes)
        .unwrap();
    assert_eq!(publisher.extracted_history_payload_bytes(), bytes);
    assert_eq!(
        receiver.retained_history_payload_bytes(),
        retained_bytes(&runtime)
    );
    source.seal_replay().unwrap();
    assert!(matches!(
        source.create_prefix_projection_with_budgets(0, 0),
        Err(PrefixError::NotReady)
    ));
}

#[test]
fn history_payload_outstanding_batch_blocks_further_extraction_without_false_completion() {
    for terminal in [false, true] {
        let mut source = source();
        let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
        create(&mut source, 1, 2.0);
        let pending = source.prepare_prefix_update(&mut publisher).unwrap();
        let bytes = pending.history_payload_bytes();
        assert!(bytes > 0);
        assert_eq!(publisher.extracted_history_payload_bytes(), bytes);
        assert_eq!(receiver.retained_history_payload_bytes(), 0);
        // Test-only reduced ceiling avoids allocating a maximum-sized history.
        publisher.history_payload_budget = HistoryPayloadBudget::new(bytes)
            .with_additional(bytes)
            .unwrap();
        create(&mut source, 2, 3.0);
        if terminal {
            source.seal_replay().unwrap();
        }
        let frame = source.frame().clone();
        let publication = source.publication_context();
        for _ in 0..2 {
            let result = if terminal {
                source.prepare_prefix_finish(&mut publisher)
            } else {
                source.prepare_prefix_update(&mut publisher)
            };
            assert!(matches!(
                result,
                Err(PrefixError::HistoryPayloadBudgetExceeded {
                    retained_bytes,
                    additional_bytes,
                    limit_bytes,
                }) if retained_bytes == bytes && additional_bytes > 0 && limit_bytes == bytes
            ));
            assert_eq!(publisher.frontier(), 2.0);
            assert_eq!(publisher.next_revision, 1);
            assert_eq!(publisher.object_slots, 2);
            assert_eq!(publisher.extracted_history_payload_bytes(), bytes);
            assert!(!publisher.finished);
            assert_eq!(source.frame(), &frame);
            assert_eq!(source.publication_context(), publication);
            assert_eq!(receiver.frontier(), 1.0);
            assert_eq!(receiver.end_time(), None);
        }
        drop(source);
        adopt(&mut receiver, &mut runtime, pending);
        assert_eq!(receiver.frontier(), 2.0);
        assert_eq!(receiver.retained_history_payload_bytes(), bytes);
        assert_eq!(retained_bytes(&runtime), bytes);
    }
}

#[test]
fn history_payload_receiver_rejects_before_capacity_and_can_adopt_the_same_batch() {
    for terminal in [false, true] {
        let mut source = source();
        let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
        create(&mut source, 1, 2.0);
        let batch = if terminal {
            source.seal_replay().unwrap();
            source.prepare_prefix_finish(&mut publisher).unwrap()
        } else {
            source.prepare_prefix_update(&mut publisher).unwrap()
        };
        let bytes = batch.history_payload_bytes();
        let budget = receiver.history_payload_budget;
        let frame = runtime.frame().clone();
        let publication = runtime.publication_context();
        let stats = runtime.replay_stats();
        let capacity = runtime.frame.objects.capacity();
        drop((source, publisher));
        receiver.history_payload_budget = HistoryPayloadBudget::new(bytes - 1);
        for _ in 0..2 {
            assert_eq!(
                receiver.preflight_append(&runtime, &batch),
                Err(PrefixError::HistoryPayloadBudgetExceeded {
                    retained_bytes: 0,
                    additional_bytes: bytes,
                    limit_bytes: bytes - 1,
                })
            );
            assert_eq!(runtime.frame(), &frame);
            assert_eq!(runtime.publication_context(), publication);
            assert_eq!(runtime.replay_stats(), stats);
            assert_eq!(runtime.frame.objects.capacity(), capacity);
            assert_eq!(receiver.object_capacity, 1);
            assert_eq!(receiver.frontier(), 1.0);
            assert_eq!(receiver.end_time(), None);
            assert_eq!(receiver.retained_history_payload_bytes(), 0);
        }
        receiver.history_payload_budget = budget;
        assert_eq!(
            receiver.preflight_append(&runtime, &batch),
            Err(PrefixError::CapacityRequired { object_slots: 2 })
        );
        receiver.reserve_object_capacity(&mut runtime, 2).unwrap();
        for _ in 0..2 {
            receiver.preflight_append(&runtime, &batch).unwrap();
            assert_eq!(receiver.retained_history_payload_bytes(), 0);
        }
        receiver.append(&mut runtime, batch).unwrap();
        assert_eq!(runtime.frame(), &frame);
        assert_eq!(runtime.publication_context(), publication);
        assert_eq!(receiver.retained_history_payload_bytes(), bytes);
        assert_eq!(receiver.end_time(), terminal.then_some(2.0));
        for (time, live) in [(1.5, 2), (0.5, 1), (1.5, 2)] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert_eq!(runtime.frame().presences.iter().filter(|&&p| p).count(), live);
            assert_eq!(receiver.retained_history_payload_bytes(), bytes);
            assert_eq!(retained_bytes(&runtime), bytes);
        }
    }
}

#[test]
fn history_payload_exact_limit_keeps_rewind_and_empty_terminal_delivery_available() {
    let mut source = source();
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    create(&mut source, 1, 2.0);
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    let bytes = batch.history_payload_bytes();
    adopt(&mut receiver, &mut runtime, batch);
    let full = HistoryPayloadBudget::new(bytes)
        .with_additional(bytes)
        .unwrap();
    publisher.history_payload_budget = full;
    receiver.history_payload_budget = full;
    for time in [1.5, 0.5] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(retained_bytes(&runtime), bytes);
        assert_eq!(receiver.retained_history_payload_bytes(), bytes);
    }
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    assert_eq!(terminal.history_payload_bytes(), 0);
    assert_eq!(receiver.end_time(), None);
    receiver.append(&mut runtime, terminal).unwrap();
    assert_eq!(receiver.end_time(), Some(2.0));
    assert_eq!(runtime.frame().time, 0.5);
    assert_eq!(receiver.history_payload_budget, full);
    assert_eq!(publisher.history_payload_budget, full);
}

#[test]
fn history_payload_zero_budget_admits_quiet_history_and_terminal_endpoint() {
    let mut source = source();
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_budgets(0, 0)
        .unwrap();
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    assert_eq!(terminal.history_payload_bytes(), 0);
    receiver.append(&mut runtime, terminal).unwrap();
    receiver.prepare_seek(&mut runtime, 1.0).unwrap();
    runtime.seek(1.0).unwrap();
    assert_eq!(receiver.end_time(), Some(1.0));
    assert_eq!(receiver.retained_history_payload_bytes(), 0);
}

#[test]
fn history_payload_receiver_accumulates_bootstrap_and_suffix_bytes() {
    let mut source = source();
    create(&mut source, 1, 2.0);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    let initial = receiver.retained_history_payload_bytes();
    create(&mut source, 2, 3.0);
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    let additional = batch.history_payload_bytes();
    assert!(initial > 0 && additional > 0);
    assert_eq!(
        publisher.extracted_history_payload_bytes(),
        initial + additional
    );
    receiver.history_payload_budget = HistoryPayloadBudget::new(initial + additional - 1)
        .with_additional(initial)
        .unwrap();
    assert_eq!(
        receiver.preflight_append(&runtime, &batch),
        Err(PrefixError::HistoryPayloadBudgetExceeded {
            retained_bytes: initial,
            additional_bytes: additional,
            limit_bytes: initial + additional - 1,
        })
    );
    assert_eq!(receiver.retained_history_payload_bytes(), initial);
    assert_eq!(receiver.frontier(), 2.0);
    receiver.history_payload_budget = HistoryPayloadBudget::new(initial + additional)
        .with_additional(initial)
        .unwrap();
    drop((source, publisher));
    adopt(&mut receiver, &mut runtime, batch);
    for (time, live) in [(2.5, 3), (0.5, 1), (1.5, 2)] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(runtime.frame().presences.iter().filter(|&&p| p).count(), live);
        assert_eq!(
            receiver.retained_history_payload_bytes(),
            initial + additional
        );
        assert_eq!(retained_bytes(&runtime), initial + additional);
    }
}
