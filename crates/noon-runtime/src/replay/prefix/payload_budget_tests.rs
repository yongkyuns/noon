//! Cumulative reader-side payload admission, distinct from resource-byte limits.
use super::*;
use noon_compile::{CompiledScene, ExecutionPatch};
use noon_core::{GeometryResource, GeometryResourceArena, Style, Transform2D, Vec2, VectorPath};

fn object(id: u64) -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    )
}

fn source(payloads: usize) -> SceneInstance {
    let compiled = CompiledScene::compile_objects(vec![object(0)], &[]).unwrap();
    let mut source = SceneInstance::new(compiled);
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 16,
            payloads,
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
    // Explicit capacity maintenance remains separate from ordinary adoption.
    match receiver.preflight_append(runtime, &batch) {
        Ok(_) => {}
        Err(PrefixError::CapacityRequired { object_slots }) => {
            receiver
                .reserve_object_capacity(runtime, object_slots)
                .unwrap();
        }
        Err(error) => panic!("unexpected preflight: {error}"),
    }
    receiver.preflight_append(runtime, &batch).unwrap();
    receiver.append(runtime, batch).unwrap();
}

fn add_path(source: &mut SceneInstance, arena: &mut GeometryResourceArena, width: f32) {
    let handle = arena.insert(GeometryResource::VectorPath(Arc::new(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(width, 0.0)),
    )));
    let mut resources = CompiledResources::default();
    resources
        .capture_geometry_from_arena(arena, handle)
        .unwrap();
    source.compiled.merge_prepared_resources(resources);
    source
        .apply_execution_patch(&ExecutionPatch::SetContent {
            object: ObjectId::new(0),
            content: GeometryRef::External(handle.id).into(),
            text_bounds: None,
        })
        .unwrap();
}

#[test]
fn cumulative_new_slots_reject_before_advancing_the_publisher() {
    let mut source = source(3);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    create(&mut source, 1, 2.0);
    let first = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(first.stats().payloads, 2); // One revision row plus one new slot.
    adopt(&mut receiver, &mut runtime, first);
    create(&mut source, 2, 3.0);
    assert_eq!(source.replay_stats().payloads_retained, 2);
    assert!(source.replay_retention_valid());
    let source_frame = source.frame().clone();
    let source_publication = source.publication_context();
    let reader_frame = runtime.frame().clone();
    let reader_publication = runtime.publication_context();
    let reader_stats = runtime.replay_stats();
    for _ in 0..2 {
        assert!(matches!(
            source.prepare_prefix_update(&mut publisher),
            Err(PrefixError::Replay(ReplayError::RetentionLimit))
        ));
        assert_eq!(publisher.frontier(), 2.0);
        assert_eq!(publisher.next_revision, 1);
        assert_eq!(publisher.object_slots, 2);
        assert_eq!(publisher.extracted_payloads, 2);
        assert!(!publisher.finished);
    }
    assert_eq!(source.frame(), &source_frame);
    assert_eq!(source.publication_context(), source_publication);
    assert_eq!(runtime.frame(), &reader_frame);
    assert_eq!(runtime.publication_context(), reader_publication);
    assert_eq!(runtime.replay_stats(), reader_stats);
    assert_eq!(receiver.frontier(), 2.0);
    assert_eq!(receiver.end_time(), None);
    for (time, count) in [(0.5, 1), (1.5, 2)] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(
            runtime.frame().presences.iter().filter(|&&live| live).count(),
            count
        );
    }
}

#[test]
fn newly_delivered_resource_entries_count_toward_the_same_cumulative_limit() {
    let mut source = source(3);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    let mut arena = GeometryResourceArena::new();
    add_path(&mut source, &mut arena, 1.0);
    source.advance_to(2.0).unwrap();
    let first = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(first.stats().resource_payloads, 1);
    assert_eq!(first.stats().payloads, 2);
    adopt(&mut receiver, &mut runtime, first);
    let bytes = publisher.extracted_resource_bytes();
    add_path(&mut source, &mut arena, 2.0);
    source.advance_to(3.0).unwrap();
    let resources_before = source.compiled.resources().prefix_payload_count();
    assert_eq!(source.replay_stats().payloads_retained, 2);
    for _ in 0..2 {
        // The row alone fits (2 + 1); its new resource entry must also be charged.
        assert!(matches!(
            source.prepare_prefix_update(&mut publisher),
            Err(PrefixError::Replay(ReplayError::RetentionLimit))
        ));
        assert_eq!(publisher.extracted_payloads, 2);
        assert_eq!(publisher.extracted_resource_bytes(), bytes);
        assert_eq!(publisher.resources.prefix_payload_count(), 1);
        assert_eq!(publisher.next_revision, 1);
        assert_eq!(publisher.frontier(), 2.0);
        assert_eq!(
            source.compiled.resources().prefix_payload_count(),
            resources_before
        );
    }
    assert_eq!(receiver.retained_resource_bytes(), bytes);
    assert_eq!(receiver.frontier(), 2.0);
    assert_eq!(receiver.end_time(), None);
}

#[test]
fn bootstrap_history_is_charged_before_the_first_extension() {
    let mut source = source(2);
    create(&mut source, 1, 2.0);
    let (mut publisher, runtime, receiver) = source.create_prefix_projection().unwrap();
    assert_eq!(publisher.extracted_payloads, 1);
    assert_eq!(runtime.replay_stats().payloads_retained, 1);
    create(&mut source, 2, 3.0);
    assert_eq!(source.replay_stats().payloads_retained, 2);
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::Replay(ReplayError::RetentionLimit))
    ));
    assert_eq!(publisher.extracted_payloads, 1);
    assert_eq!(publisher.next_revision, 1);
    assert_eq!(publisher.frontier(), 2.0);
    assert_eq!(receiver.frontier(), 2.0);
}

#[test]
fn outstanding_batch_is_charged_without_claiming_it_was_adopted() {
    let mut source = source(3);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    create(&mut source, 1, 2.0);
    let pending = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(publisher.extracted_payloads, 2);
    assert_eq!(runtime.replay_stats().payloads_retained, 0);
    create(&mut source, 2, 3.0);
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::Replay(ReplayError::RetentionLimit))
    ));
    assert_eq!(receiver.frontier(), 1.0);
    assert_eq!(publisher.frontier(), 2.0);
    adopt(&mut receiver, &mut runtime, pending);
    assert_eq!(receiver.frontier(), 2.0);
    assert_eq!(runtime.replay_stats().payloads_retained, 2);
}

#[test]
fn exact_cumulative_limit_admits_an_empty_terminal_marker_after_rewind() {
    let mut source = source(4);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    for id in 1..=2 {
        create(&mut source, id, id as f64 + 1.0);
        let batch = source.prepare_prefix_update(&mut publisher).unwrap();
        adopt(&mut receiver, &mut runtime, batch);
        assert_eq!(
            publisher.extracted_payloads,
            runtime.replay_stats().payloads_retained
        );
    }
    assert_eq!(publisher.extracted_payloads, 4);
    for time in [2.5, 0.5] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(runtime.replay_stats().payloads_retained, 4);
    }
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    assert!(terminal.is_terminal());
    assert_eq!(terminal.stats().payloads, 0);
    assert_eq!(receiver.end_time(), None);
    for _ in 0..2 {
        receiver.preflight_append(&runtime, &terminal).unwrap();
        assert_eq!(runtime.replay_stats().payloads_retained, 4);
    }
    receiver.append(&mut runtime, terminal).unwrap();
    assert_eq!(receiver.end_time(), Some(3.0));
    assert_eq!(runtime.frame().time, 0.5);
    assert_eq!(publisher.extracted_payloads, 4);
}

#[test]
fn rejected_terminal_suffix_does_not_close_the_publisher_or_reader() {
    let mut source = source(3);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    create(&mut source, 1, 2.0);
    let first = source.prepare_prefix_update(&mut publisher).unwrap();
    adopt(&mut receiver, &mut runtime, first);
    create(&mut source, 2, 3.0);
    source.seal_replay().unwrap();
    assert!(matches!(
        source.prepare_prefix_finish(&mut publisher),
        Err(PrefixError::Replay(ReplayError::RetentionLimit))
    ));
    assert!(!publisher.finished);
    assert_eq!(publisher.frontier(), 2.0);
    assert_eq!(publisher.extracted_payloads, 2);
    assert_eq!(receiver.end_time(), None);
    assert_eq!(receiver.frontier(), 2.0);
}

#[test]
fn receiver_checks_permanent_limits_before_requesting_object_capacity() {
    let mut source = source(4);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    create(&mut source, 1, 2.0);
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    let limits = receiver.limits;
    let capacity = receiver.object_capacity;
    let frame_capacity = runtime.frame.objects.capacity();
    let frame = runtime.frame().clone();
    let publication = runtime.publication_context();
    // Test-only stricter receiver caps, as in the resource-byte admission tests.
    for strict in [
        ReplayLimits {
            revisions: 0,
            ..limits
        },
        ReplayLimits {
            payloads: 1,
            ..limits
        },
    ] {
        receiver.limits = strict;
        for _ in 0..2 {
            assert_eq!(
                receiver.preflight_append(&runtime, &batch),
                Err(PrefixError::Replay(ReplayError::RetentionLimit))
            );
        }
        assert_eq!(receiver.object_capacity, capacity);
        assert_eq!(runtime.frame.objects.capacity(), frame_capacity);
        assert_eq!(runtime.frame(), &frame);
        assert_eq!(runtime.publication_context(), publication);
        assert_eq!(runtime.replay_stats().payloads_retained, 0);
    }
    receiver.limits = limits;
    assert_eq!(
        receiver.preflight_append(&runtime, &batch),
        Err(PrefixError::CapacityRequired { object_slots: 2 })
    );
    adopt(&mut receiver, &mut runtime, batch);
    assert_eq!(receiver.frontier(), 2.0);
    assert_eq!(runtime.replay_stats().payloads_retained, 2);
}

#[test]
fn cumulative_payload_arithmetic_overflow_rejects_without_moving_the_cursor() {
    let mut source = source(usize::MAX);
    let (mut publisher, _, _) = source.create_prefix_projection().unwrap();
    create(&mut source, 1, 2.0);
    // Private counter injection exercises overflow without enormous allocations.
    publisher.extracted_payloads = usize::MAX;
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::Replay(ReplayError::RetentionLimit))
    ));
    assert_eq!(publisher.extracted_payloads, usize::MAX);
    assert_eq!(publisher.next_revision, 0);
    assert_eq!(publisher.object_slots, 1);
    assert_eq!(publisher.frontier(), 1.0);
}

#[test]
fn resource_entry_addition_cannot_overflow_the_cumulative_payload_charge() {
    let mut source = source(usize::MAX);
    let (mut publisher, _, _) = source.create_prefix_projection().unwrap();
    let mut arena = GeometryResourceArena::new();
    add_path(&mut source, &mut arena, 1.0);
    source.advance_to(2.0).unwrap();
    publisher.extracted_payloads = usize::MAX - 1;
    assert!(matches!(
        source.prepare_prefix_update(&mut publisher),
        Err(PrefixError::Replay(ReplayError::RetentionLimit))
    ));
    assert_eq!(publisher.extracted_payloads, usize::MAX - 1);
    assert_eq!(publisher.extracted_resource_bytes(), 0);
    assert_eq!(publisher.resources.prefix_payload_count(), 0);
    assert_eq!(publisher.next_revision, 0);
    assert_eq!(publisher.frontier(), 1.0);
}
