//! Object-vector admission, deliberately distinct from total heap accounting.
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
    let mut source = SceneInstance::new(
        CompiledScene::compile_objects(vec![object(0)], &[]).unwrap(),
    );
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 16,
            payloads: usize::MAX,
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

fn bootstrap(
    source: &SceneInstance,
    object_bytes: usize,
) -> Result<(PrefixPublisher, SceneInstance, PrefixReceiver), PrefixError> {
    source.create_prefix_projection_with_object_budget(
        SceneInstance::DEFAULT_PREFIX_RESOURCE_BUDGET_BYTES,
        SceneInstance::MAX_PREFIX_HISTORY_PAYLOAD_BYTES,
        object_bytes,
    )
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
fn object_vector_measurement_counts_all_runtime_arrays_and_spare_capacity() {
    let mut source = source();
    fn bytes<T>(values: &Vec<T>) -> usize {
        values.capacity() * std::mem::size_of::<T>()
    }
    let expected = source.compiled.prefix_object_reserved_bytes(0).unwrap()
        + bytes(&source.frame.objects)
        + bytes(&source.frame.presences)
        + bytes(&source.frame.reveals)
        + bytes(&source.frame.morphs)
        + bytes(&source.frame.render_geometries)
        + bytes(&source.frame.render_transforms)
        + bytes(&source.frame.family_animations)
        + bytes(&source.frame.family_animation_plan_indices)
        + bytes(&source.painter_ranks)
        + bytes(&source.painter_order);
    assert_eq!(object_budget::reserved_bytes(&source, 0), Some(expected));
    let cloned = object_budget::clone_bytes(&source, 0);
    let old = bytes(&source.frame.family_animation_plan_indices);
    let extra_slots = source.frame.family_animation_plan_indices.capacity() + 32;
    source
        .frame
        .family_animation_plan_indices
        .reserve_exact(extra_slots);
    let extra = bytes(&source.frame.family_animation_plan_indices) - old;
    assert!(extra > 0);
    assert_eq!(
        object_budget::reserved_bytes(&source, 0),
        Some(expected + extra)
    );
    assert_eq!(object_budget::clone_bytes(&source, 0), cloned);
}

#[test]
fn object_vector_exact_limit_and_arithmetic_overflow_are_checked_without_allocation() {
    let source = source();
    let expected = object_budget::reserved_bytes(&source, 3).unwrap();
    assert_eq!(
        object_budget::preflight_reserved(&source, 3, expected),
        Ok(expected)
    );
    assert_eq!(
        object_budget::preflight_reserved(&source, 3, expected - 1),
        Err(PrefixError::ObjectStorageBudgetExceeded {
            object_slots: 3,
            required_bytes: Some(expected),
            limit_bytes: expected - 1,
        })
    );
    assert_eq!(
        object_budget::preflight_clone(&source, usize::MAX, usize::MAX),
        Err(PrefixError::ObjectStorageBudgetExceeded {
            object_slots: usize::MAX,
            required_bytes: None,
            limit_bytes: usize::MAX,
        })
    );
    assert_eq!(
        object_budget::preflight_reserved(&source, usize::MAX, usize::MAX),
        Err(PrefixError::ObjectStorageBudgetExceeded {
            object_slots: usize::MAX,
            required_bytes: None,
            limit_bytes: usize::MAX,
        })
    );
}

#[test]
fn object_vector_bootstrap_rejection_preserves_source_history_and_readiness() {
    let mut source = source();
    create(&mut source, 1, 2.0);
    let frame = source.frame().clone();
    let publication = source.publication_context();
    let stats = source.replay_stats();
    let identity = source.runtime_identity();
    let bytes = object_budget::reserved_bytes(&source, 0);
    let pins = Arc::strong_count(&source.replay_history.as_ref().unwrap().scope);
    for _ in 0..2 {
        assert!(matches!(
            bootstrap(&source, 0),
            Err(PrefixError::ObjectStorageBudgetExceeded {
                object_slots: 2,
                required_bytes: Some(required),
                limit_bytes: 0,
            }) if required > 0
        ));
        assert_eq!(source.frame(), &frame);
        assert_eq!(source.publication_context(), publication);
        assert_eq!(source.replay_stats(), stats);
        assert_eq!(source.runtime_identity(), identity);
        assert_eq!(object_budget::reserved_bytes(&source, 0), bytes);
        assert_eq!(
            Arc::strong_count(&source.replay_history.as_ref().unwrap().scope),
            pins
        );
        assert!(!source.replay_is_sealed());
    }
    source.seal_replay().unwrap();
    assert!(matches!(bootstrap(&source, 0), Err(PrefixError::NotReady)));
    for (time, count) in [(0.5, 1), (1.5, 2)] {
        source.seek(time).unwrap();
        assert_eq!(source.frame().presences.iter().filter(|&&p| p).count(), count);
    }
}

#[test]
fn object_vector_bootstrap_reserves_inactive_rows_for_historical_order() {
    let mut source = source();
    create(&mut source, 1, 2.0);
    source
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1)))
        .unwrap();
    source.advance_to(3.0).unwrap();
    let requested = object_budget::clone_bytes(&source, 2).unwrap();
    let limit = SceneInstance::MAX_PREFIX_OBJECT_VECTOR_BYTES;
    let (_, mut runtime, receiver) = bootstrap(&source, limit).unwrap();
    drop(source);
    let bytes = object_budget::reserved_bytes(&runtime, 0).unwrap();
    assert!(bytes >= requested && bytes <= limit);
    assert!(runtime.painter_order.capacity() >= 2);
    assert!(runtime.frame.objects.capacity() >= 2);
    let pointer = runtime.frame.objects.as_ptr();
    for (time, count) in [(1.5, 2), (2.5, 1), (0.5, 1), (1.5, 2)] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(runtime.frame().presences.iter().filter(|&&p| p).count(), count);
        assert_eq!(object_budget::reserved_bytes(&runtime, 0), Some(bytes));
        assert_eq!(runtime.frame.objects.as_ptr(), pointer);
    }
}

#[test]
fn object_vector_oversized_reservation_rejects_before_any_capacity_changes() {
    let source = source();
    let (_, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    let per_slot = object_budget::clone_bytes(&source, 1).unwrap();
    let oversized = SceneInstance::MAX_PREFIX_OBJECT_VECTOR_BYTES / per_slot + 1;
    assert!(oversized < u32::MAX as usize);
    let frame = runtime.frame().clone();
    let publication = runtime.publication_context();
    let stats = runtime.replay_stats();
    let bytes = object_budget::reserved_bytes(&runtime, 0);
    let capacity = receiver.object_capacity;
    for _ in 0..2 {
        assert!(matches!(
            receiver.reserve_object_capacity(&mut runtime, oversized),
            Err(PrefixError::ObjectStorageBudgetExceeded { .. })
        ));
        assert_eq!(object_budget::reserved_bytes(&runtime, 0), bytes);
        assert_eq!(receiver.object_capacity, capacity);
        assert_eq!(runtime.frame(), &frame);
        assert_eq!(runtime.publication_context(), publication);
        assert_eq!(runtime.replay_stats(), stats);
        assert_eq!(receiver.frontier(), 1.0);
        assert_eq!(receiver.end_time(), None);
    }
    receiver.reserve_object_capacity(&mut runtime, 2).unwrap();
    assert_eq!(receiver.object_capacity, 2);
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(runtime.publication_context(), publication);
}

#[test]
fn object_vector_receiver_rejects_before_requesting_growth_and_preserves_pending_batch() {
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
        let required = object_budget::reserved_bytes(&runtime, 2).unwrap();
        let limit = receiver.object_vector_budget_bytes;
        receiver.object_vector_budget_bytes = required - 1;
        let frame = runtime.frame().clone();
        let publication = runtime.publication_context();
        let bytes = object_budget::reserved_bytes(&runtime, 0);
        drop((source, publisher));
        for _ in 0..2 {
            let error = PrefixError::ObjectStorageBudgetExceeded {
                object_slots: 2,
                required_bytes: Some(required),
                limit_bytes: required - 1,
            };
            assert_eq!(receiver.preflight_append(&runtime, &batch), Err(error));
            assert!(matches!(
                receiver.reserve_object_capacity(&mut runtime, 2),
                Err(PrefixError::ObjectStorageBudgetExceeded { .. })
            ));
            assert_eq!(object_budget::reserved_bytes(&runtime, 0), bytes);
            assert_eq!(runtime.frame(), &frame);
            assert_eq!(runtime.publication_context(), publication);
            assert_eq!(receiver.object_capacity, 1);
            assert_eq!(receiver.frontier(), 1.0);
            assert_eq!(receiver.end_time(), None);
        }
        receiver.object_vector_budget_bytes = limit;
        assert_eq!(
            receiver.preflight_append(&runtime, &batch),
            Err(PrefixError::CapacityRequired { object_slots: 2 })
        );
        adopt(&mut receiver, &mut runtime, batch);
        assert_eq!(runtime.frame(), &frame);
        assert_eq!(runtime.publication_context(), publication);
        assert_eq!(receiver.end_time(), terminal.then_some(2.0));
        let retained = object_budget::reserved_bytes(&runtime, 0);
        for (time, count) in [(1.5, 2), (0.5, 1), (1.5, 2)] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert_eq!(runtime.frame().presences.iter().filter(|&&p| p).count(), count);
            assert_eq!(object_budget::reserved_bytes(&runtime, 0), retained);
        }
    }
}

#[test]
fn object_vector_publisher_rejection_preserves_prior_undelivered_output() {
    for terminal in [false, true] {
        let mut source = source();
        let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
        create(&mut source, 1, 2.0);
        let pending = source.prepare_prefix_update(&mut publisher).unwrap();
        publisher.object_vector_budget_bytes = object_budget::clone_bytes(&source, 2).unwrap();
        let payloads = publisher.extracted_payloads;
        let history_bytes = publisher.extracted_history_payload_bytes();
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
                Err(PrefixError::ObjectStorageBudgetExceeded { .. })
            ));
            assert_eq!(publisher.frontier(), 2.0);
            assert_eq!(publisher.next_revision, 1);
            assert_eq!(publisher.object_slots, 2);
            assert_eq!(publisher.extracted_payloads, payloads);
            assert_eq!(publisher.extracted_history_payload_bytes(), history_bytes);
            assert!(!publisher.finished);
            assert_eq!(source.frame(), &frame);
            assert_eq!(source.publication_context(), publication);
            assert_eq!(receiver.frontier(), 1.0);
        }
        drop((source, publisher));
        adopt(&mut receiver, &mut runtime, pending);
        assert_eq!(receiver.frontier(), 2.0);
        assert_eq!(receiver.end_time(), None);
    }
}

#[test]
fn object_vector_exact_limit_still_admits_quiet_terminal_after_rewind() {
    let mut source = source();
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    create(&mut source, 1, 2.0);
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    adopt(&mut receiver, &mut runtime, batch);
    publisher.object_vector_budget_bytes = object_budget::clone_bytes(&source, 2).unwrap();
    receiver.object_vector_budget_bytes = object_budget::reserved_bytes(&runtime, 0).unwrap();
    receiver.prepare_seek(&mut runtime, 0.5).unwrap();
    runtime.seek(0.5).unwrap();
    let frame = runtime.frame().clone();
    let bytes = object_budget::reserved_bytes(&runtime, 0);
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    receiver.append(&mut runtime, terminal).unwrap();
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(receiver.end_time(), Some(2.0));
    assert_eq!(object_budget::reserved_bytes(&runtime, 0), bytes);
}

#[test]
fn object_vector_foreign_reader_rejects_before_reservation() {
    let source = source();
    let (_, runtime, mut receiver) = source.create_prefix_projection().unwrap();
    let mut foreign = runtime.clone();
    let bytes = object_budget::reserved_bytes(&foreign, 0);
    assert_eq!(
        receiver.reserve_object_capacity(&mut foreign, 2),
        Err(PrefixError::ReaderChanged)
    );
    assert_eq!(object_budget::reserved_bytes(&foreign, 0), bytes);
}
