//! Deterministic scalar history uses authored tracks, not sampled host results.
use noon::integration::scene_execution::ExecutionSession;
use noon::{RateFunction, RustHostCallbackTable, Scene, SceneExecution, ValueTracker};
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
fn scene_fixture() -> (SceneExecution, ValueTracker, ValueTracker) {
    let mut scene = Scene::new();
    let object = scene.circle(0.5).unwrap();
    scene.add(&object).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    let external = scene.value_tracker(4.0).unwrap();
    (scene.into_execution().unwrap(), tracker, external)
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
    assert_eq!(session.replay_stats().revisions_retained, 0);
    // The two immutable authored tracks are charged once, without sampled
    // revisions or additional payloads from forward evaluation and seeking.
    assert_eq!(session.replay_stats().payloads_retained, 2);
}

#[test]
fn appended_segments_and_holds_keep_their_historical_values() {
    let (mut execution, tracker, _) = scene_fixture();
    execution
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    let first = execution
        .live()
        .declare_and_activate_value_tracker(&tracker, 2.0, 2.0, RateFunction::Linear)
        .unwrap();
    assert_eq!(
        execution.session_mut().seal_replay(),
        Err(ReplayError::Incomplete)
    );
    execution
        .live()
        .advance_segment_to(first, first.end_time())
        .unwrap();
    execution.live().complete_segment(first).unwrap();
    execution.session_mut().advance_to(3.0).unwrap();
    execution.scene_mut().set_value(&tracker, -1.0).unwrap();
    let second = execution
        .live()
        .declare_and_activate_value_tracker(&tracker, 3.0, 2.0, RateFunction::Smooth)
        .unwrap();
    execution
        .live()
        .advance_segment_to(second, second.end_time())
        .unwrap();
    execution.live().complete_segment(second).unwrap();
    execution.session_mut().advance_to(6.0).unwrap();
    execution.session_mut().seal_replay().unwrap();
    assert_eq!(execution.session().replay_stats().revisions_retained, 5);
    assert_eq!(execution.session().replay_stats().payloads_retained, 5);
    let store = std::rc::Rc::clone(execution.scene().integration_store());
    let authored = store.borrow().scene_revision();
    for time in [0.0, 0.5, 1.5, 2.0, 2.99, 3.0, 3.75, 5.0, 6.0, 1.0] {
        execution.seek(time).unwrap();
        let expected = store
            .borrow()
            .semantic_input_scalar_value_at(tracker.node_id(), time)
            .unwrap();
        assert_eq!(
            execution.scene().value_tracker_value(&tracker).unwrap(),
            f64::from(expected as f32)
        );
        let publication = execution.session().publication_context();
        assert_eq!(publication.scene_revision(), authored);
        assert!(execution.scene_mut().set_value(&tracker, 7.0).is_err());
        assert_eq!(execution.session().publication_context(), publication);
    }
    execution.session_mut().discard_replay_retention();
    assert_eq!(execution.session().frame().time, 6.0);
    assert_eq!(
        execution.scene().value_tracker_value(&tracker).unwrap(),
        3.0
    );
    execution.scene_mut().set_value(&tracker, 7.0).unwrap();
    assert_eq!(
        execution.scene().value_tracker_value(&tracker).unwrap(),
        7.0
    );
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
        let (mut execution, tracker, _) = scene_fixture();
        execution
            .session_mut()
            .begin_replay_retention(limits)
            .unwrap();
        let segment = execution
            .live()
            .declare_and_activate_value_tracker(&tracker, 2.0, 2.0, RateFunction::Linear)
            .unwrap();
        execution
            .live()
            .advance_segment_to(segment, segment.end_time())
            .unwrap();
        execution.live().complete_segment(segment).unwrap();
        assert_eq!(
            execution.scene().value_tracker_value(&tracker).unwrap(),
            2.0
        );
        let session = execution.session_mut();
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
    let (mut execution, tracker, external) = scene_fixture();
    execution
        .session_mut()
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    assert!(execution
        .live()
        .declare_and_activate_value_tracker(&tracker, f64::NAN, 1.0, RateFunction::Linear)
        .is_err());
    assert_eq!(execution.session().replay_stats().revisions_retained, 0);
    execution.scene_mut().set_value(&tracker, 2.0).unwrap();
    execution
        .session_mut()
        .set_reactive_input(external.node_id(), 4.0_f32)
        .unwrap();
    execution.session_mut().advance_to(1.0).unwrap();
    execution
        .session_mut()
        .set_reactive_input(external.node_id(), 5.0_f32)
        .unwrap();
    assert_eq!(
        execution.session_mut().seal_replay(),
        Err(ReplayError::UnrecordedInput)
    );
    assert_eq!(
        execution.scene().value_tracker_value(&tracker).unwrap(),
        2.0
    );
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
    let mut session = noon::integration::scene_execution::lower(&scene).unwrap();
    session
        .begin_replay_retention(ReplayLimits::default())
        .unwrap();
    callbacks.advance_to(&mut session, 1.0).unwrap();
    assert_eq!(session.seal_replay(), Err(ReplayError::UnsupportedDomain));
}

#[test]
fn polynomial_rate_scene_uses_shared_tracks_for_forward_and_seek() {
    let mut session = noon::example_scenes::polynomial_rates::session().unwrap();
    let identity = session.session().runtime_identity();
    let expected = [0.15625_f64, 0.103515625, 0.070556640625];
    session.session_mut().advance_to(0.25).unwrap();
    for (index, quarter) in expected.into_iter().enumerate() {
        let x = session.session().frame().objects[index]
            .transform
            .translation
            .x;
        assert!((f64::from(x) - (-2.0 + 4.0 * quarter)).abs() < 1e-6);
    }
    session.session_mut().advance_to(1.0).unwrap();
    assert!(session
        .session()
        .frame()
        .objects
        .iter()
        .all(|object| (object.transform.translation.x - 2.0).abs() < 1e-6));
    session.seek(0.25).unwrap();
    for (index, quarter) in expected.into_iter().enumerate() {
        let x = session.session().frame().objects[index]
            .transform
            .translation
            .x;
        assert!((f64::from(x) - (-2.0 + 4.0 * quarter)).abs() < 1e-6);
    }
    assert_eq!(session.session().runtime_identity(), identity);
}
