use noon_compile::{CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch};
use noon_core::{GeometryRef, ObjectId, Style, Transform2D, Vec2};
use noon_runtime::{EvaluationStats, PreparedFrameCommitError, ReplayLimits, SceneInstance};

fn runtime(count: usize) -> SceneInstance {
    let objects = (0..count)
        .map(|i| {
            CompiledObject::new(
                ObjectId::new(i as u64),
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap())
}
fn move_to(object: u64, x: f32) -> ExecutionPatch {
    ExecutionPatch::SetTransform {
        object: ObjectId::new(object),
        transform: Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
    }
}

#[test]
fn finite_history_restores_one_affected_object_and_static_ticks_remain_idle() {
    let mut rt = runtime(100_000);
    rt.begin_replay_retention(ReplayLimits::default()).unwrap();
    for i in 1..=50 {
        rt.advance_to(f64::from(i)).unwrap();
        rt.apply_execution_patch(&move_to(42, i as f32)).unwrap();
    }
    assert_eq!(rt.replay_stats().revisions_retained, 50);
    assert_eq!(
        rt.replay_stats().payloads_retained,
        50,
        "one saved row per value change, not a scene snapshot"
    );
    rt.advance_to(60.0).unwrap();
    rt.seal_replay().unwrap();
    rt.seek(24.5).unwrap();
    assert_eq!(rt.frame().objects[42].transform.translation.x, 24.0);
    assert_eq!(rt.replay_stats().objects_restored, 1);
    rt.take_frame_changes();
    rt.advance_to(25.5).unwrap();
    assert_eq!(rt.frame().objects[42].transform.translation.x, 25.0);
    assert_eq!(rt.replay_stats().revisions_crossed, 1);
    assert_eq!(rt.replay_stats().objects_restored, 1);
    rt.take_frame_changes();
    rt.advance_to(25.75).unwrap();
    assert_eq!(rt.replay_stats().revisions_crossed, 0);
    assert_eq!(rt.last_stats(), EvaluationStats::default());
    assert!(rt.take_frame_changes().is_empty());
}

#[test]
fn failed_atomic_publication_cannot_append_partial_history() {
    let mut rt = runtime(2);
    rt.begin_replay_retention(ReplayLimits::default()).unwrap();
    let frame = rt.frame().clone();
    let publication = rt.publication_context();
    let transaction =
        ExecutionMutationTransaction::from_mutations([move_to(0, 4.0), move_to(1, f32::NAN)]);
    assert!(rt.apply_execution_transaction(&transaction).is_err());
    assert_eq!(rt.frame(), &frame);
    assert_eq!(rt.publication_context(), publication);
    assert_eq!(rt.replay_stats().revisions_retained, 0);
}

#[test]
fn prepared_live_frame_cannot_commit_after_replay_is_sealed() {
    let mut rt = runtime(2);
    rt.begin_replay_retention(ReplayLimits::default()).unwrap();
    let prepared = rt.prepare_advance_to(1.0).unwrap();
    let effective = rt.prepare_effective_property_batch(&[]).unwrap();
    rt.seal_replay().unwrap();
    let frame = rt.frame().clone();
    assert_eq!(
        rt.commit_prepared_frame(prepared, effective),
        Err(PreparedFrameCommitError::ReplaySealed)
    );
    assert_eq!(rt.frame(), &frame);
    assert!(rt.prepare_advance_to(1.0).is_err());
}

#[test]
fn historical_membership_restores_equal_priority_painter_order() {
    let mut rt = runtime(3);
    rt.begin_replay_retention(ReplayLimits::default()).unwrap();
    rt.advance_to(1.0).unwrap();
    rt.apply_execution_patch(&ExecutionPatch::ReorderObject {
        object: ObjectId::new(2),
        before: Some(ObjectId::new(0)),
    })
    .unwrap();
    let first = rt.painter_order().to_vec();
    rt.advance_to(2.0).unwrap();
    rt.apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(0)))
        .unwrap();
    let second = rt.painter_order().to_vec();
    rt.advance_to(3.0).unwrap();
    rt.seal_replay().unwrap();
    for _ in 0..3 {
        rt.seek(0.5).unwrap();
        assert_eq!(rt.painter_order(), &[0, 1, 2]);
        rt.advance_to(1.5).unwrap();
        assert_eq!(rt.painter_order(), &first);
        rt.advance_to(2.5).unwrap();
        assert_eq!(rt.painter_order(), &second);
    }
}
