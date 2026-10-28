//! Prepared history must not silently lose ordinary replay's inset relations.
use super::*;
use noon_compile::{CompiledInset2DView, CompiledScene, ExecutionPatch};
use noon_core::{Style, Transform2D, Vec2};

fn inset(view: bool) -> ExecutionPatch {
    ExecutionPatch::SetInset2DView {
        object: ObjectId::new(1),
        view: view.then_some(CompiledInset2DView {
            camera_frame: ObjectId::new(0),
            capture_own_display: false,
        }),
    }
}

fn source(initial_inset: bool) -> SceneInstance {
    let mut compiled = CompiledScene::compile_objects(
        (0..2)
            .map(|id| {
                CompiledObject::new(
                    ObjectId::new(id),
                    GeometryRef::Rectangle {
                        size: Vec2::new(2.0, 1.0),
                    },
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .collect(),
        &[],
    )
    .unwrap();
    if initial_inset {
        compiled.apply_execution_patch(&inset(true)).unwrap();
    }
    let mut source = SceneInstance::new(compiled);
    source
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    source.advance_to(1.0).unwrap();
    source.take_frame_changes();
    source
}

#[test]
fn initial_inset_is_rejected_before_prefix_bootstrap() {
    let source = source(true);
    assert!(matches!(
        source.create_prefix_projection(),
        Err(PrefixError::Replay(ReplayError::UnsupportedDomain))
    ));
    assert_eq!(source.inset_2d_views().count(), 1);
}

#[test]
fn removed_inset_history_is_rejected_without_changing_ordinary_replay() {
    let mut source = source(false);
    source.apply_execution_patch(&inset(true)).unwrap();
    source.advance_to(1.5).unwrap();
    source.apply_execution_patch(&inset(false)).unwrap();
    source.advance_to(2.0).unwrap();
    assert_eq!(source.inset_2d_views().count(), 0);
    let before = source.publication_context();
    let frame = source.frame().clone();
    assert!(matches!(
        source.create_prefix_projection(),
        Err(PrefixError::Replay(ReplayError::UnsupportedDomain))
    ));
    assert_eq!(source.publication_context(), before);
    assert_eq!(source.frame(), &frame);
    source.seal_replay().unwrap();
    source.seek(1.25).unwrap();
    assert_eq!(source.inset_2d_views().count(), 1);
    source.seek(1.75).unwrap();
    assert_eq!(source.inset_2d_views().count(), 0);
}

#[test]
fn removed_inset_in_suffix_keeps_the_previous_reader_and_publisher_cursor() {
    let mut source = source(false);
    let (mut publisher, mut reader, receiver) = source.create_prefix_projection().unwrap();
    source.apply_execution_patch(&inset(true)).unwrap();
    source.advance_to(1.5).unwrap();
    source.apply_execution_patch(&inset(false)).unwrap();
    source.advance_to(2.0).unwrap();
    for _ in 0..2 {
        assert!(matches!(
            source.prepare_prefix_update(&mut publisher),
            Err(PrefixError::Replay(ReplayError::UnsupportedDomain))
        ));
        assert_eq!(publisher.frontier(), 1.0);
        assert_eq!(publisher.next_revision, 0);
        assert_eq!(publisher.extracted_payloads, 0);
        assert!(!publisher.finished);
    }
    assert_eq!(receiver.frontier(), 1.0);
    assert_eq!(receiver.end_time(), None);
    receiver.prepare_seek(&mut reader, 0.5).unwrap();
    reader.seek(0.5).unwrap();
    assert_eq!(reader.inset_2d_views().count(), 0);
}

#[test]
fn historical_inset_rejection_precedes_zero_component_budgets() {
    let mut source = source(false);
    source.apply_execution_patch(&inset(true)).unwrap();
    source.advance_to(1.5).unwrap();
    source.apply_execution_patch(&inset(false)).unwrap();
    source.advance_to(2.0).unwrap();
    let before = source.frame().clone();
    let publication = source.publication_context();
    for _ in 0..2 {
        assert!(matches!(
            source.create_prefix_projection_with_object_budget(0, 0, 0),
            Err(PrefixError::Replay(ReplayError::UnsupportedDomain))
        ));
        assert_eq!(source.frame(), &before);
        assert_eq!(source.publication_context(), publication);
    }
    source.seal_replay().unwrap();
    source.seek(1.25).unwrap();
    assert_eq!(source.inset_2d_views().count(), 1);
    source.seek(1.75).unwrap();
    assert_eq!(source.inset_2d_views().count(), 0);
}
