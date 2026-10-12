use super::*;
use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
use noon_core::{
    ObjectId, ReactiveValue, SemanticObjectState, SemanticStore, SignalId, StoredGeometry,
    Transform2D, Vec2,
};

fn fixture() -> (SceneInstance, SignalId, ObjectId) {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 0.5,
    }));
    store.attach_to_scene(object).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&store, &mut index).unwrap();
    let object = index.execution_object_id(object).unwrap();
    let mut runtime = SceneInstance::from_semantic_execution(lowered);
    // A signal-only runtime is initially a supported replay domain. Its value
    // can later be read by authoring, so external changes still need a transcript.
    let signal = SignalId::new(0);
    let enrollment = runtime
        .prepare_reactive_signal_enrollment(Some(signal), ReactiveValue::Scalar(1.0))
        .unwrap();
    runtime.commit_reactive_signal_enrollment(enrollment, signal);
    runtime.take_frame_changes();
    (runtime, signal, object)
}

fn translated(object: ObjectId, x: f32) -> ExecutionPatch {
    ExecutionPatch::SetTransform {
        object,
        transform: Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
    }
}

fn expect_unrecorded(runtime: &mut SceneInstance) {
    assert_eq!(
        runtime.seal_replay().map_err(|error| error.to_string()),
        Err("replay unavailable: UnrecordedInput".to_owned())
    );
    assert!(!runtime.replay_is_sealed());
    assert_eq!(runtime.replay_stats().revisions_retained, 0);
}

#[test]
fn direct_input_rejects_a_sealed_replay_before_any_mutation() {
    let (mut runtime, signal, _) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime.advance_to(2.0).unwrap();
    runtime.seal_replay().unwrap();
    runtime.seek(0.5).unwrap();
    runtime.take_frame_changes();
    let publication = runtime.publication_context();
    let frame = runtime.frame().clone();
    assert!(runtime.set_reactive_input(signal, 2.0_f32).is_err());
    assert_eq!(runtime.publication_context(), publication);
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(
        runtime.reactive_value(signal),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert!(runtime.take_frame_changes().is_empty());
    assert!(runtime.replay_is_sealed());
    runtime.discard_replay_retention();
    assert_eq!(runtime.frame().time, 2.0);
    runtime.set_reactive_input(signal, 2.0_f32).unwrap();
    assert_eq!(
        runtime.reactive_value(signal),
        Some(&ReactiveValue::Scalar(2.0))
    );
}

#[test]
fn input_seek_cannot_write_through_a_sealed_replay() {
    let (mut runtime, signal, _) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime.advance_to(2.0).unwrap();
    runtime.seal_replay().unwrap();
    let publication = runtime.publication_context();
    let frame = runtime.frame().clone();
    assert!(runtime
        .seek_with_reactive_inputs(0.5, &[(signal, ReactiveValue::Scalar(2.0))])
        .is_err());
    assert_eq!(runtime.publication_context(), publication);
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(
        runtime.reactive_value(signal),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert!(runtime.replay_is_sealed());
}

#[test]
fn empty_input_seek_uses_time_qualified_historical_revisions() {
    let (mut runtime, _, object) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime.advance_to(1.0).unwrap();
    runtime
        .apply_execution_patch(&translated(object, 4.0))
        .unwrap();
    runtime.advance_to(2.0).unwrap();
    runtime.seal_replay().unwrap();
    runtime.seek_with_reactive_inputs(0.5, &[]).unwrap();
    assert_eq!(runtime.frame().objects[0].transform.translation.x, 0.0);
    runtime.seek_with_reactive_inputs(1.5, &[]).unwrap();
    assert_eq!(runtime.frame().objects[0].transform.translation.x, 4.0);
    let publication = runtime.publication_context();
    assert!(runtime.seek_with_reactive_inputs(3.0, &[]).is_err());
    assert_eq!(runtime.publication_context(), publication);
}

#[test]
fn empty_input_seek_does_not_require_a_reactive_runtime() {
    let compiled = noon_compile::CompiledScene::compile_objects(Vec::new(), &[]).unwrap();
    let mut runtime = SceneInstance::new(compiled);
    runtime.seek_with_reactive_inputs(1.0, &[]).unwrap();
    assert_eq!(runtime.frame().time, 1.0);
}

#[test]
fn changed_direct_input_invalidates_retention_without_changing_authored_time() {
    let (mut runtime, signal, object) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime
        .apply_execution_patch(&translated(object, 4.0))
        .unwrap();
    assert_eq!(runtime.replay_stats().revisions_retained, 1);
    runtime.take_frame_changes();
    runtime.set_reactive_input(signal, 2.0_f32).unwrap();
    assert!(
        runtime.take_frame_changes().is_empty(),
        "signal-only input does not dirty unrelated frame rows"
    );
    assert_eq!(runtime.frame().time, 0.0);
    assert_eq!(runtime.frame().objects[0].transform.translation.x, 4.0);
    expect_unrecorded(&mut runtime);
    runtime.advance_to(3.0).unwrap();
    assert_eq!(
        runtime.frame().time,
        3.0,
        "first execution remains available"
    );
}

#[test]
fn committed_input_batch_has_explicit_unrecorded_classification() {
    let (mut runtime, signal, _) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime
        .advance_to_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(3.0))])
        .unwrap();
    assert_eq!(
        runtime.reactive_value(signal),
        Some(&ReactiveValue::Scalar(3.0))
    );
    expect_unrecorded(&mut runtime);
}

#[test]
fn input_seek_classifies_changed_signals_and_publishes_signal_only_updates() {
    let (mut runtime, signal, _) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let before = runtime.publication_context();
    runtime
        .seek_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(3.0))])
        .unwrap();
    assert_ne!(runtime.publication_context(), before);
    assert_eq!(
        runtime.frame().objects[0].transform.translation,
        Vec2::ZERO,
        "seek preserves existing geometry"
    );
    expect_unrecorded(&mut runtime);
}

#[test]
fn rejected_and_unchanged_inputs_preserve_replay_capability() {
    let (mut runtime, signal, _) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let before = runtime.publication_context();
    assert!(runtime.set_reactive_input(signal, f32::NAN).is_err());
    assert!(runtime
        .advance_to_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(f32::NAN))])
        .is_err());
    assert!(runtime
        .seek_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(f32::NAN))])
        .is_err());
    runtime.set_reactive_input(signal, 1.0_f32).unwrap();
    runtime
        .advance_to_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(1.0))])
        .unwrap();
    runtime
        .seek_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(1.0))])
        .unwrap();
    assert_eq!(runtime.publication_context(), before);
    runtime.seal_replay().unwrap();
}

#[test]
fn speculative_input_does_not_poison_history_or_commit_after_sealing() {
    let (mut runtime, signal, _) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let prepared = runtime
        .prepare_advance_to_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(2.0))])
        .unwrap();
    let effective = runtime.prepare_effective_property_batch(&[]).unwrap();
    runtime.seal_replay().unwrap();
    let before = runtime.publication_context();
    assert!(runtime.commit_prepared_frame(prepared, effective).is_err());
    assert_eq!(
        runtime.reactive_value(signal),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert_eq!(runtime.publication_context(), before);
    assert!(runtime.replay_is_sealed());
}

#[test]
fn later_input_preserves_the_first_retention_failure() {
    let (mut runtime, signal, object) = fixture();
    runtime
        .begin_replay_retention(ReplayLimits {
            revisions: 0,
            payloads: 0,
        })
        .unwrap();
    runtime
        .apply_execution_patch(&translated(object, 4.0))
        .unwrap();
    assert_eq!(runtime.seal_replay(), Err(ReplayError::RetentionLimit));
    runtime
        .advance_to_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(2.0))])
        .unwrap();
    assert_eq!(runtime.seal_replay(), Err(ReplayError::RetentionLimit));
    runtime.discard_replay_retention();
    runtime
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    runtime.seal_replay().unwrap();
}

#[test]
fn signal_only_input_rejects_a_previously_prepared_phase() {
    let (mut runtime, signal, _) = fixture();
    let stale = runtime
        .prepare_advance_to_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(3.0))])
        .unwrap();
    let effective = runtime.prepare_effective_property_batch(&[]).unwrap();
    let before = runtime.publication_context();
    runtime
        .advance_to_with_reactive_inputs(0.0, &[(signal, ReactiveValue::Scalar(2.0))])
        .unwrap();
    let current = runtime.publication_context();
    assert_ne!(
        current, before,
        "signal-only changes are coherent publications too"
    );
    assert!(matches!(
        runtime.commit_prepared_frame(stale, effective),
        Err(crate::PreparedFrameCommitError::StalePublication { .. })
    ));
    assert_eq!(runtime.publication_context(), current);
    assert_eq!(
        runtime.reactive_value(signal),
        Some(&ReactiveValue::Scalar(2.0))
    );
    assert!(
        runtime.take_frame_changes().is_empty(),
        "no geometry changed"
    );
}
