//! Deterministic scalar history uses authored tracks, not sampled host results.
use noon::{ExecutionSession, RateFunction, RustHostCallbackTable, Scene};
use noon_core::{
    HostCallbackId, ReactiveValue, SemanticMutationTransaction, SemanticNodeId,
    SemanticObjectState, SemanticStore, StoredGeometry, TrackTiming,
};
use noon_runtime::{ReplayError, ReplayLimits};

fn fixture() -> (SemanticStore, SemanticNodeId, SemanticNodeId) {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 0.5,
    }));
    store.attach_to_scene(object).unwrap();
    let tracker = store.insert_semantic_input_signal(0.0_f64).unwrap();
    let external = store.insert_semantic_input_signal(4.0_f64).unwrap();
    let root = store.insert_family();
    store.attach_to_scene(root).unwrap();
    let mut scope = SemanticMutationTransaction::new();
    scope
        .scope_signal(root, tracker)
        .scope_signal(root, external);
    scope.apply(&mut store).unwrap();
    (store, tracker, external)
}
fn scalar(session: &ExecutionSession, signal: SemanticNodeId) -> f32 {
    match session.effective_signal_value(signal).unwrap() {
        ReactiveValue::Scalar(value) => *value,
        value => panic!("not scalar: {value:?}"),
    }
}

#[test]
fn predeclared_curve_playback_stays_sparse_and_replay_matches_authored_evaluation() {
    let (mut store, tracker, _) = fixture();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_scalar_signal_track(
        tracker,
        0.0,
        2.0,
        TrackTiming::new(0.0, 2.0, RateFunction::Linear),
    );
    transaction.add_scalar_signal_track(
        tracker,
        2.0,
        -2.0,
        TrackTiming::new(3.0, 2.0, RateFunction::Smooth),
    );
    transaction.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let mut oracle = ExecutionSession::from_semantic_store(&store).unwrap();
    let identity = session.runtime_identity();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    for time in [0.5, 1.0, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0] {
        session.take_frame_changes();
        session.advance_to(time).unwrap();
        assert!(session.take_frame_changes().is_empty());
    }
    session.seal_replay().unwrap();
    let authored = store.scene_revision();
    for time in [6.0, 4.25, 2.5, 0.0, 0.625, 3.75, 5.0] {
        session.take_frame_changes();
        oracle.seek(time).unwrap();
        session.seek(time).unwrap();
        assert_eq!(scalar(&session, tracker), scalar(&oracle, tracker));
        assert_eq!(session.frame(), oracle.frame());
        // Full seek deliberately republishes scene rows; locality is asserted
        // above for ordinary forward evaluation, not by weakening seek semantics.
        assert_eq!(session.runtime_identity(), identity);
        assert_eq!(store.scene_revision(), authored);
    }
    assert_eq!(session.replay_stats().payloads_retained, 0);
}

#[test]
fn appended_segments_and_holds_keep_their_historical_values() {
    let (mut store, tracker, _) = fixture();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let first = session
        .declare_and_activate_value_tracker(&mut store, tracker, 2.0, 2.0, RateFunction::Linear)
        .unwrap();
    assert_eq!(session.seal_replay(), Err(ReplayError::Incomplete));
    session.advance_segment_to(first, first.end_time()).unwrap();
    session.complete_segment(&mut store, first).unwrap();
    session.advance_to(3.0).unwrap();
    session
        .set_scalar_signal_value(&mut store, tracker, -1.0)
        .unwrap();
    let second = session
        .declare_and_activate_value_tracker(&mut store, tracker, 3.0, 2.0, RateFunction::Smooth)
        .unwrap();
    session
        .advance_segment_to(second, second.end_time())
        .unwrap();
    session.complete_segment(&mut store, second).unwrap();
    session.advance_to(6.0).unwrap();
    session.seal_replay().unwrap();
    assert_eq!(session.replay_stats().revisions_retained, 5);
    assert_eq!(session.replay_stats().payloads_retained, 5);
    let authored = store.scene_revision();
    for time in [0.0, 0.5, 1.5, 2.0, 2.99, 3.0, 3.75, 5.0, 6.0, 1.0] {
        session.seek(time).unwrap();
        assert_eq!(
            scalar(&session, tracker),
            store.semantic_input_scalar_value_at(tracker, time).unwrap() as f32
        );
        assert_eq!(session.publication_context().scene_revision(), authored);
        assert!(session
            .set_scalar_signal_value(&mut store, tracker, 7.0)
            .is_err());
    }
    session.discard_replay_retention();
    assert_eq!(session.frame().time, 6.0);
    assert_eq!(scalar(&session, tracker), 3.0);
    session
        .set_scalar_signal_value(&mut store, tracker, 7.0)
        .unwrap();
    assert_eq!(scalar(&session, tracker), 7.0);
}

#[test]
fn invalid_seeks_and_cloned_runtime_preserve_scalar_coherence() {
    let (mut store, tracker, _) = fixture();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_scalar_signal_track(
        tracker,
        0.0,
        4.0,
        TrackTiming::new(0.0, 2.0, RateFunction::Linear),
    );
    transaction.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    session.advance_to(2.0).unwrap();
    session.seal_replay().unwrap();
    session.seek(0.5).unwrap();
    let before = session.publication_context();
    let frame = session.frame().clone();
    for time in [-0.1, 2.1, f64::NAN, f64::INFINITY] {
        assert!(session.seek(time).is_err());
        assert_eq!(session.publication_context(), before);
        assert_eq!(session.frame(), &frame);
        assert_eq!(scalar(&session, tracker), 1.0);
    }
    let mut cloned = session.clone();
    assert_ne!(cloned.runtime_identity(), session.runtime_identity());
    cloned.seek(1.5).unwrap();
    assert_eq!(scalar(&cloned, tracker), 3.0);
    assert_eq!(scalar(&session, tracker), 1.0);
}

#[test]
fn scalar_extensions_obey_shared_budgets_without_breaking_first_execution() {
    for limits in [
        ReplayLimits {
            revisions: 0,
            payloads: 10,
        },
        ReplayLimits {
            revisions: 10,
            payloads: 0,
        },
    ] {
        let (mut store, tracker, _) = fixture();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        session.begin_replay_retention(limits).unwrap();
        let segment = session
            .declare_and_activate_value_tracker(&mut store, tracker, 2.0, 2.0, RateFunction::Linear)
            .unwrap();
        session
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        session.complete_segment(&mut store, segment).unwrap();
        assert_eq!(scalar(&session, tracker), 2.0);
        assert_eq!(session.seal_replay(), Err(ReplayError::RetentionLimit));
        assert_eq!(session.replay_stats().revisions_retained, 0);
        session.discard_replay_retention();
        session
            .begin_replay_retention(ReplayLimits::default())
            .unwrap();
        session.advance_to(3.0).unwrap();
        session.seal_replay().unwrap();
    }
}

#[test]
fn external_inputs_remain_unrecorded_and_failed_scalar_edits_do_not_poison_history() {
    let (mut store, tracker, external) = fixture();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    assert!(session
        .declare_and_activate_value_tracker(
            &mut store,
            tracker,
            f64::NAN,
            1.0,
            RateFunction::Linear
        )
        .is_err());
    assert_eq!(session.replay_stats().revisions_retained, 0);
    session
        .set_scalar_signal_value(&mut store, tracker, 2.0)
        .unwrap();
    session.set_reactive_input(external, 4.0_f32).unwrap();
    session.advance_to(1.0).unwrap();
    session.set_reactive_input(external, 5.0_f32).unwrap();
    assert_eq!(session.seal_replay(), Err(ReplayError::UnrecordedInput));
    assert_eq!(scalar(&session, tracker), 2.0);
}

#[test]
fn opaque_callbacks_are_not_misclassified_as_replayable_scalar_curves() {
    let mut scene = Scene::new();
    let object = scene.circle(0.5).unwrap();
    scene.add(&object).unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    let id = HostCallbackId::new(91);
    callbacks
        .insert(id, |_context| Ok::<(), std::io::Error>(()))
        .unwrap();
    callbacks
        .add_updater(
            &mut scene.integration_store().borrow_mut(),
            object.node_id(),
            id,
            0.0,
            None,
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    callbacks.advance_to(&mut session, 1.0).unwrap();
    assert_eq!(session.seal_replay(), Err(ReplayError::UnsupportedDomain));
}
