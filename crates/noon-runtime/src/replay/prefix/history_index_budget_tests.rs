//! The index ceiling counts reserved Revision slots, not only occupied history.
use super::*;
use noon_compile::{CompiledScene, ExecutionPatch};
use noon_core::{Style, Transform2D};
use storage::preflight_history_index_capacity;

fn object(id: u64) -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    )
}

fn source(revisions: usize) -> SceneInstance {
    let compiled = CompiledScene::compile_objects(vec![object(0)], &[]).unwrap();
    let mut source = SceneInstance::new(compiled);
    source
        .begin_replay_retention(ReplayLimits {
            revisions,
            payloads: 32,
        })
        .unwrap();
    source.advance_to(1.0).unwrap();
    source.take_frame_changes();
    source
}

fn maximum_capacity() -> usize {
    SceneInstance::MAX_PREFIX_HISTORY_INDEX_BYTES / std::mem::size_of::<Revision>()
}

fn rejection(capacity: usize) -> PrefixError {
    PrefixError::HistoryIndexBudgetExceeded {
        revision_capacity: capacity,
        bytes_per_revision: std::mem::size_of::<Revision>(),
        limit_bytes: SceneInstance::MAX_PREFIX_HISTORY_INDEX_BYTES,
    }
}

#[test]
fn history_index_budget_counts_the_largest_whole_entry_without_allocating() {
    let capacity = maximum_capacity();
    let bytes = capacity * std::mem::size_of::<Revision>();
    assert_eq!(preflight_history_index_capacity(0), Ok(0));
    assert_eq!(preflight_history_index_capacity(capacity), Ok(bytes));
    assert_eq!(
        preflight_history_index_capacity(capacity + 1),
        Err(rejection(capacity + 1))
    );
    // This is an arithmetic test: it must not allocate the maximum-sized index.
}

#[test]
fn history_index_budget_rejects_multiplication_overflow_with_exact_operands() {
    assert!(usize::MAX
        .checked_mul(std::mem::size_of::<Revision>())
        .is_none());
    assert_eq!(
        preflight_history_index_capacity(usize::MAX),
        Err(rejection(usize::MAX))
    );
}

#[test]
fn history_index_budget_admits_the_existing_default_revision_count() {
    let bytes = preflight_history_index_capacity(ReplayLimits::default().revisions).unwrap();
    assert!(bytes > 0);
    assert!(bytes <= SceneInstance::MAX_PREFIX_HISTORY_INDEX_BYTES);
}

#[test]
fn history_index_budget_rejects_empty_but_oversized_reservations_atomically() {
    for capacity in [maximum_capacity() + 1, usize::MAX] {
        let mut source = source(capacity);
        let identity = source.runtime_identity();
        let frame = source.frame().clone();
        let publication = source.publication_context();
        let stats = source.replay_stats();
        let history = source.replay_history.as_ref().unwrap();
        let allocated = history.revisions.capacity();
        let scope_pins = Arc::strong_count(&history.scope);
        assert_eq!(history.revisions.len(), 0);
        for _ in 0..2 {
            assert_eq!(
                source.create_prefix_projection().unwrap_err(),
                rejection(capacity)
            );
            let history = source.replay_history.as_ref().unwrap();
            assert_eq!(history.revisions.capacity(), allocated);
            assert_eq!(Arc::strong_count(&history.scope), scope_pins);
            assert_eq!(history.limits.revisions, capacity);
            assert_eq!(source.runtime_identity(), identity);
            assert_eq!(source.frame(), &frame);
            assert_eq!(source.publication_context(), publication);
            assert_eq!(source.replay_stats(), stats);
            assert!(source.replay_retention_valid());
            assert!(!source.replay_is_sealed());
        }
        // Refusing a derived reader does not invalidate or stop its producer.
        source.advance_to(2.0).unwrap();
        assert_eq!(source.frame().time, 2.0);
    }
}

#[test]
fn history_index_budget_rejects_nonempty_history_without_sealing_or_losing_it() {
    let capacity = maximum_capacity() + 1;
    let mut source = source(capacity);
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(1)))
        .unwrap();
    source.advance_to(2.0).unwrap();
    let frame = source.frame().clone();
    let publication = source.publication_context();
    let stats = source.replay_stats();
    let history = source.replay_history.as_ref().unwrap();
    let pointer = history.revisions.as_ptr();
    let allocated = history.revisions.capacity();
    assert_eq!(stats.revisions_retained, 1);
    assert_eq!(
        source
            .create_prefix_projection_with_resource_budget(0)
            .unwrap_err(),
        rejection(capacity)
    );
    let history = source.replay_history.as_ref().unwrap();
    assert_eq!(history.revisions.as_ptr(), pointer);
    assert_eq!(history.revisions.capacity(), allocated);
    assert_eq!(source.frame(), &frame);
    assert_eq!(source.publication_context(), publication);
    assert_eq!(source.replay_stats(), stats);
    assert!(!source.replay_is_sealed());
    // Ordinary finite replay is not subject to the prefix reader's reservation.
    source.seal_replay().unwrap();
    source.seek(0.5).unwrap();
    assert_eq!(
        source
            .frame()
            .presences
            .iter()
            .filter(|&&live| live)
            .count(),
        1
    );
}

#[test]
fn history_index_budget_does_not_replace_existing_readiness_errors() {
    let mut source = source(usize::MAX);
    source.seal_replay().unwrap();
    assert_eq!(
        source.create_prefix_projection().unwrap_err(),
        PrefixError::NotReady
    );
}

#[test]
fn history_index_budget_stays_reserved_through_delivery_rewind_and_completion() {
    let mut source = source(2);
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(1)))
        .unwrap();
    source.advance_to(2.0).unwrap();
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    let history = runtime.replay_history.as_ref().unwrap();
    let pointer = history.revisions.as_ptr();
    let capacity = history.revisions.capacity();
    assert!(capacity >= 2);
    assert!(preflight_history_index_capacity(capacity).is_ok());
    source
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(2)))
        .unwrap();
    source.advance_to(3.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    receiver
        .reserve_object_capacity(&mut runtime, batch.required_object_capacity())
        .unwrap();
    receiver.append(&mut runtime, batch).unwrap();
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    receiver.append(&mut runtime, terminal).unwrap();
    assert_eq!(receiver.end_time(), Some(3.0));
    for (time, count) in [(2.5, 3), (0.5, 1), (1.5, 2), (3.0, 3)] {
        receiver.prepare_seek(&mut runtime, time).unwrap();
        runtime.seek(time).unwrap();
        assert_eq!(
            runtime
                .frame()
                .presences
                .iter()
                .filter(|&&live| live)
                .count(),
            count
        );
        let history = runtime.replay_history.as_ref().unwrap();
        assert_eq!(history.revisions.capacity(), capacity);
        assert_eq!(history.revisions.as_ptr(), pointer);
    }
}

#[test]
fn history_index_budget_allows_zero_capacity_quiet_history_and_terminal_marker() {
    let mut source = source(0);
    let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
    assert_eq!(
        runtime
            .replay_history
            .as_ref()
            .unwrap()
            .revisions
            .capacity(),
        0
    );
    source.seal_replay().unwrap();
    let terminal = source.prepare_prefix_finish(&mut publisher).unwrap();
    assert_eq!(terminal.stats().revisions, 0);
    receiver.append(&mut runtime, terminal).unwrap();
    receiver.prepare_seek(&mut runtime, 1.0).unwrap();
    runtime.seek(1.0).unwrap();
    assert_eq!(receiver.end_time(), Some(1.0));
    assert_eq!(runtime.frame().time, 1.0);
    assert_eq!(
        runtime
            .replay_history
            .as_ref()
            .unwrap()
            .revisions
            .capacity(),
        0
    );
}
