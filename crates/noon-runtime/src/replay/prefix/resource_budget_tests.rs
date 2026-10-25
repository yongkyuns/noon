//! Real runtime/publisher/receiver resource admission and ownership regressions.
use super::*;
use noon_compile::{CompiledScene, ExecutionPatch};
use noon_core::{
    GeometryResource, GeometryResourceArena, GeometryResourceHandle, Style, Transform2D, Vec2,
    VectorPath,
};
use std::sync::Weak;

fn source() -> SceneInstance {
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            ObjectId::new(0),
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        )],
        &[],
    )
    .unwrap();
    let mut source = SceneInstance::new(compiled);
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 16,
            payloads: 128,
        })
        .unwrap();
    source.advance_to(1.0).unwrap();
    source.take_frame_changes();
    source
}

type PathResource = (
    CompiledResources,
    GeometryResourceHandle,
    Weak<VectorPath>,
    usize,
);

fn path_resource(width: f32) -> PathResource {
    path_resource_in(&mut GeometryResourceArena::new(), width)
}

fn path_resource_in(arena: &mut GeometryResourceArena, width: f32) -> PathResource {
    let path = Arc::new(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(width, 0.0)),
    );
    let weak = Arc::downgrade(&path);
    let resource = GeometryResource::VectorPath(path);
    let bytes = resource.retained_bytes();
    let handle = arena.insert(resource);
    let mut resources = CompiledResources::default();
    resources
        .capture_geometry_from_arena(arena, handle)
        .unwrap();
    (resources, handle, weak, bytes)
}

fn set_content(source: &mut SceneInstance, geometry: GeometryRef) {
    source
        .apply_execution_patch(&ExecutionPatch::SetContent {
            object: ObjectId::new(0),
            content: geometry.into(),
            text_bounds: None,
        })
        .unwrap();
}

#[test]
fn resource_free_bootstrap_admits_zero_and_default_limit_is_explicit() {
    let source = source();
    let (publisher, mut runtime, receiver) = source
        .create_prefix_projection_with_resource_budget(0)
        .unwrap();
    assert_eq!(publisher.extracted_resource_bytes(), 0);
    assert_eq!(publisher.resource_budget_bytes(), 0);
    assert_eq!(receiver.retained_resource_bytes(), 0);
    assert_eq!(receiver.resource_budget_bytes(), 0);
    receiver.prepare_seek(&mut runtime, 0.5).unwrap();
    runtime.seek(0.5).unwrap();
    let (default_publisher, _, default_receiver) = source.create_prefix_projection().unwrap();
    assert_eq!(
        default_publisher.resource_budget_bytes(),
        SceneInstance::DEFAULT_PREFIX_RESOURCE_BUDGET_BYTES
    );
    assert_eq!(
        default_receiver.resource_budget_bytes(),
        default_publisher.resource_budget_bytes()
    );
}

#[test]
fn bootstrap_counts_all_pinned_resources_and_rejects_before_cloning() {
    let mut source = source();
    let (resources, _, weak, bytes) = path_resource(1.0);
    // Even unused resources in the bootstrap projection are physical pins.
    source.compiled.merge_prepared_resources(resources);
    let strong = weak.strong_count();
    let frame = source.frame().clone();
    let publication = source.publication_context();
    assert!(matches!(
        source.create_prefix_projection_with_resource_budget(bytes - 1),
        Err(PrefixError::ResourceBudgetExceeded {
            retained_bytes: 0,
            additional_bytes,
            limit_bytes,
        }) if additional_bytes == bytes && limit_bytes == bytes - 1
    ));
    assert_eq!(weak.strong_count(), strong);
    assert_eq!(source.frame(), &frame);
    assert_eq!(source.publication_context(), publication);
    let (publisher, runtime, receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    assert_eq!(publisher.extracted_resource_bytes(), bytes);
    assert_eq!(receiver.retained_resource_bytes(), bytes);
    drop((publisher, runtime, receiver));
    assert_eq!(weak.strong_count(), strong);
}

#[test]
fn suffix_rejection_preserves_cursor_reader_and_previous_seekable_range() {
    let mut source = source();
    // External rows use bare GeometryIds: allocate both IDs in one arena so
    // this exercises byte rejection, not unsupported geometry-ID retargeting.
    let mut arena = GeometryResourceArena::new();
    let (resources, handle, _, bytes) = path_resource_in(&mut arena, 1.0);
    let (next_resources, next_handle, weak, next_bytes) = path_resource_in(&mut arena, 2.0);
    drop(arena);
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    source.compiled.merge_prepared_resources(resources);
    set_content(&mut source, GeometryRef::External(handle.id));
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.resource_bytes(), bytes);
    let reader_frame = runtime.frame().clone();
    let reader_publication = runtime.publication_context();
    for _ in 0..2 {
        receiver.preflight_append(&runtime, &batch).unwrap();
        assert_eq!(receiver.retained_resource_bytes(), 0);
    }
    receiver.append(&mut runtime, batch).unwrap();
    assert_eq!(receiver.retained_resource_bytes(), bytes);
    assert_eq!(runtime.frame(), &reader_frame);
    assert_eq!(runtime.publication_context(), reader_publication);
    source.compiled.merge_prepared_resources(next_resources);
    set_content(&mut source, GeometryRef::External(next_handle.id));
    source.advance_to(3.0).unwrap();
    let source_frame = source.frame().clone();
    let source_publication = source.publication_context();
    let strong = weak.strong_count();
    let revision = publisher.next_revision;
    for _ in 0..2 {
        assert!(matches!(
            source.prepare_prefix_update(&mut publisher),
            Err(PrefixError::ResourceBudgetExceeded {
                retained_bytes,
                additional_bytes,
                limit_bytes,
            }) if retained_bytes == bytes && additional_bytes == next_bytes && limit_bytes == bytes
        ));
        assert_eq!(publisher.frontier(), 2.0);
        assert_eq!(publisher.next_revision, revision);
        assert_eq!(publisher.extracted_resource_bytes(), bytes);
        assert_eq!(weak.strong_count(), strong);
    }
    assert_eq!(source.frame(), &source_frame);
    assert_eq!(source.publication_context(), source_publication);
    assert_eq!(receiver.frontier(), 2.0);
    assert_eq!(receiver.end_time(), None);
    receiver.prepare_seek(&mut runtime, 0.5).unwrap();
    runtime.seek(0.5).unwrap();
    assert_eq!(
        runtime.frame().objects[0].content.geometry(),
        Some(&GeometryRef::circle(0.5))
    );
    receiver.prepare_seek(&mut runtime, 1.5).unwrap();
    runtime.seek(1.5).unwrap();
    assert_eq!(receiver.retained_resource_bytes(), bytes);
}

#[test]
fn known_handle_and_empty_terminal_marker_cost_zero_at_the_limit() {
    let mut source = source();
    let (resources, handle, _, bytes) = path_resource(1.0);
    source.compiled.merge_prepared_resources(resources);
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    set_content(&mut source, GeometryRef::External(handle.id));
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.resource_bytes(), 0);
    assert_eq!(batch.stats().resource_payloads, 0);
    receiver.append(&mut runtime, batch).unwrap();
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    assert!(terminal.is_terminal());
    assert_eq!(terminal.resource_bytes(), 0);
    assert_eq!(receiver.end_time(), None);
    receiver.append(&mut runtime, terminal).unwrap();
    assert_eq!(receiver.end_time(), Some(2.0));
    assert_eq!(receiver.retained_resource_bytes(), bytes);
    receiver.prepare_seek(&mut runtime, 2.0).unwrap();
    runtime.seek(2.0).unwrap();
}

#[test]
fn receiver_rechecks_budget_without_consuming_a_retryable_batch() {
    let mut source = source();
    let (resources, handle, _, bytes) = path_resource(1.0);
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    source.compiled.merge_prepared_resources(resources);
    set_content(&mut source, GeometryRef::External(handle.id));
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    // Private test injection models stricter receiver admission; public streams
    // cannot forge either this accounting or the batch's private byte charge.
    receiver.resource_budget = ResourceBudget::new(bytes - 1);
    let frame = runtime.frame().clone();
    let publication = runtime.publication_context();
    for _ in 0..2 {
        assert!(matches!(
            receiver.preflight_append(&runtime, &batch),
            Err(PrefixError::ResourceBudgetExceeded { .. })
        ));
    }
    assert_eq!(receiver.retained_resource_bytes(), 0);
    assert_eq!(receiver.frontier(), 1.0);
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(runtime.publication_context(), publication);
    receiver.resource_budget = ResourceBudget::new(bytes);
    receiver.append(&mut runtime, batch).unwrap();
    assert_eq!(receiver.retained_resource_bytes(), bytes);
}

#[test]
fn history_pins_survive_removal_rewind_and_producer_drop_then_release_with_reader() {
    let mut source = source();
    let (resources, handle, weak, bytes) = path_resource(1.0);
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    source.compiled.merge_prepared_resources(resources);
    set_content(&mut source, GeometryRef::External(handle.id));
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    receiver.append(&mut runtime, batch).unwrap();
    source
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(0)))
        .unwrap();
    source.advance_to(3.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    receiver.append(&mut runtime, batch).unwrap();
    drop(source);
    drop(publisher);
    assert!(weak.upgrade().is_some());
    for time in [2.5, 0.5, 1.5] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(receiver.retained_resource_bytes(), bytes);
        assert!(weak.upgrade().is_some());
    }
    assert_eq!(
        runtime.frame().objects[0].content.geometry(),
        Some(&GeometryRef::External(handle.id))
    );
    drop(runtime);
    assert!(
        weak.upgrade().is_none(),
        "the cursor must not own resource pins"
    );
    drop(receiver);
}

#[test]
fn foreign_stream_rejection_does_not_charge_resource_bytes() {
    let mut source = source();
    let (resources, handle, _, bytes) = path_resource(1.0);
    let (mut publisher, _, _) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    let (_, runtime, receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    source.compiled.merge_prepared_resources(resources);
    set_content(&mut source, GeometryRef::External(handle.id));
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(
        receiver.preflight_append(&runtime, &batch),
        Err(PrefixError::UnexpectedBatch)
    );
    assert_eq!(receiver.retained_resource_bytes(), 0);
    assert_eq!(receiver.frontier(), 1.0);
}

#[test]
fn repeated_new_handle_within_one_suffix_is_charged_once() {
    let mut source = source();
    let (resources, handle, _, bytes) = path_resource(1.0);
    let (mut publisher, mut runtime, mut receiver) = source
        .create_prefix_projection_with_resource_budget(bytes)
        .unwrap();
    source.compiled.merge_prepared_resources(resources);
    set_content(&mut source, GeometryRef::External(handle.id));
    set_content(&mut source, GeometryRef::circle(0.5));
    set_content(&mut source, GeometryRef::External(handle.id));
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert!(batch.stats().revisions > 0);
    assert_eq!(batch.stats().resource_payloads, 1);
    assert_eq!(batch.resource_bytes(), bytes);
    receiver.append(&mut runtime, batch).unwrap();
    assert_eq!(receiver.retained_resource_bytes(), bytes);
}
