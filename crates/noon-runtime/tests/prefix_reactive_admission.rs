#![cfg(feature = "prepared-prefix")]

mod prefix_support;

use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
use noon_core::{
    ReactiveValue, SemanticMutationTransaction, SemanticObjectProperty, SemanticObjectState,
    SemanticStore, SignalId, StoredGeometry,
};
use noon_runtime::{PrefixError, ReplayError, ReplayLimits, SceneInstance};

// Exercise the canonical semantic lowering path. SceneInstance::new(compiled)
// leaves reactive=None and cannot reproduce empty semantic-runtime admission.
fn semantic_runtime(with_signal: bool, bound: bool) -> SceneInstance {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 0.5,
    }));
    store.attach_to_scene(object).unwrap();
    if with_signal {
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        if bound {
            store
                .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
                .unwrap();
        } else {
            // Scope an unbound tracker so it actually participates in lowering.
            let root = store.insert_family();
            store.attach_to_scene(root).unwrap();
            let mut scope = SemanticMutationTransaction::new();
            scope.scope_signal(root, signal);
            scope.apply(&mut store).unwrap();
        }
    }
    let lowered = lower_semantic_execution(&store, &mut SemanticExecutionIndex::new()).unwrap();
    let mut runtime = SceneInstance::from_semantic_execution(lowered);
    runtime
        .begin_replay_retention(ReplayLimits {
            revisions: 64,
            payloads: 256,
        })
        .unwrap();
    runtime.advance_to(1.0).unwrap();
    runtime
}

fn assert_unrecorded_input<T: std::fmt::Debug>(result: Result<T, PrefixError>) {
    assert!(
        matches!(
            result,
            Err(PrefixError::Replay(ReplayError::UnrecordedInput))
        ),
        "{result:?}"
    );
}

#[test]
fn empty_semantic_reactive_runtime_allows_bootstrap_and_quiet_extension() {
    let mut source = semantic_runtime(false, false);
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    reader.seek(0.25).unwrap();
    let displayed = reader.frame().clone();
    source.advance_to(2.0).unwrap();
    let batch = source.prepare_prefix_update(&mut publisher).unwrap();
    assert_eq!(batch.stats().revisions, 0);
    assert_eq!(batch.stats().payloads, 0);
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &displayed);
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(reader.seek(1.5).unwrap().time, 1.5);
}

#[test]
fn scoped_unbound_signal_preserves_the_scene_without_a_source_callback() {
    let source = semantic_runtime(true, false);
    let (_, mut reader) = prefix_support::bootstrap(&source).unwrap();
    assert_eq!(reader.seek(0.5).unwrap().objects, source.frame().objects);
}

#[test]
fn immutable_property_binding_is_preserved_by_the_reader() {
    let source = semantic_runtime(true, true);
    let (_, mut reader) = prefix_support::bootstrap(&source).unwrap();
    assert_eq!(reader.seek(0.5).unwrap().objects[0].style.opacity, 0.4);
}

#[test]
fn later_unbound_enrollment_allows_extension_but_raw_input_preserves_old_history() {
    let mut source = semantic_runtime(false, false);
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let previous = reader.seek(0.5).unwrap().clone();
    let signal = SignalId::new(100);
    let prepared = source
        .prepare_reactive_signal_enrollment(Some(signal), ReactiveValue::Scalar(2.0))
        .unwrap();
    source.commit_reactive_signal_enrollment(prepared, signal);
    assert_eq!(
        source.reactive_value(signal),
        Some(&ReactiveValue::Scalar(2.0))
    );
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    source
        .set_reactive_input(signal, ReactiveValue::Scalar(3.0))
        .unwrap();
    let source_frame = source.frame().clone();
    let source_publication = source.publication_context();
    assert_unrecorded_input(source.prepare_prefix_update(&mut publisher));
    assert_unrecorded_input(source.prepare_prefix_update(&mut publisher));
    assert_eq!(source.frame(), &source_frame);
    assert_eq!(source.publication_context(), source_publication);
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(reader.seek(0.5).unwrap(), &previous);
}

#[test]
fn enrollment_batches_preserve_admission_until_an_unrecorded_value_change() {
    let mut source = semantic_runtime(false, false);
    let (mut publisher, mut reader) = prefix_support::bootstrap(&source).unwrap();
    let signal = SignalId::new(200);
    assert!(source
        .prepare_reactive_signal_enrollment_batch(&[(signal, ReactiveValue::Scalar(f32::NAN))])
        .is_err());
    let empty = source
        .prepare_reactive_signal_enrollment_batch(&[])
        .unwrap();
    source.commit_reactive_signal_enrollment_batch(empty);
    source.advance_to(2.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    let preserved = reader.seek(1.5).unwrap().clone();
    let pending = source
        .prepare_reactive_signal_enrollment_batch(&[(signal, ReactiveValue::Scalar(3.0))])
        .unwrap();
    // Preparation alone has not installed any signal in the compute VM.
    source.advance_to(3.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    source.commit_reactive_signal_enrollment_batch(pending);
    assert_eq!(
        source.reactive_value(signal),
        Some(&ReactiveValue::Scalar(3.0))
    );
    source.advance_to(4.0).unwrap();
    reader
        .append(source.prepare_prefix_update(&mut publisher).unwrap())
        .unwrap();
    source
        .seek_with_reactive_inputs(4.0, &[(signal, ReactiveValue::Scalar(4.0))])
        .unwrap();
    assert_unrecorded_input(source.prepare_prefix_update(&mut publisher));
    assert_eq!(reader.frontier(), 4.0);
    assert_eq!(reader.seek(1.5).unwrap(), &preserved);
}
