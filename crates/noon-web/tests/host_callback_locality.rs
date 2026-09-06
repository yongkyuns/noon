use noon::{
    CallbackAdvance, CallbackTerminationKind, EffectivePropertyBatch,
    EffectiveSemanticPropertyWrite, ExecutionSession, ExecutionSessionCallbackError,
    HostCallbackId, SemanticMutationTransaction, SemanticNodeId, SemanticObjectState,
    SemanticStore, StoredGeometry, Style, Transform2D, Vec2,
};

const SCENE_OBJECTS: usize = 10_000;
const HOST_OBJECT_INDEX: usize = SCENE_OBJECTS / 2;

fn large_callback_scene() -> (SemanticStore, SemanticNodeId) {
    let mut store = SemanticStore::new();
    let mut host_object = None;
    for index in 0..SCENE_OBJECTS {
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.1,
            }));
        store.attach_to_scene(object).unwrap();
        if index == HOST_OBJECT_INDEX {
            host_object = Some(object);
        }
    }
    let host_object = host_object.expect("host object must be present in the scene");

    let removed = HostCallbackId::new(90);
    let first = HostCallbackId::new(7);
    let second = HostCallbackId::new(3);
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(host_object, removed, 0.0, None);
    callbacks.remove_updater(host_object, removed, 0.0);
    callbacks.add_updater(host_object, first, 0.0, None);
    callbacks.add_updater(host_object, second, 0.0, None);
    callbacks.apply(&mut store).unwrap();

    (store, host_object)
}

#[test]
fn callback_barrier_stays_target_local_in_a_large_canonical_session() {
    let (store, host_object) = large_callback_scene();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();

    let initial = match session.advance_to_callback_barrier(0.0).unwrap() {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => {
            assert_eq!(invocations.len(), 2);
            assert_eq!(overlay.objects().count(), 1);
            overlay
        }
        CallbackAdvance::Ready(_) => panic!("time-zero updaters must run before later phases"),
    };
    session
        .commit_required_callback_phase(initial.finish())
        .unwrap();
    session.take_frame_changes();

    let (invocations, mut overlay) = match session.advance_to_callback_barrier(0.25).unwrap() {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => (invocations, overlay),
        CallbackAdvance::Ready(_) => panic!("the active updater phase must require its host"),
    };
    assert_eq!(
        invocations
            .iter()
            .map(|invocation| (invocation.occurrence_index(), invocation.callback_id()))
            .collect::<Vec<_>>(),
        vec![(1, HostCallbackId::new(7)), (2, HostCallbackId::new(3))],
        "the closed occurrence is skipped while authored order remains stable"
    );
    assert!(invocations
        .iter()
        .all(|invocation| invocation.target() == host_object));
    assert_eq!(overlay.time(), 0.25);
    assert_eq!(overlay.delta_time(), 0.25);
    assert_eq!(overlay.objects().count(), 1);
    assert_eq!(overlay.staged_row_count(), 1);
    assert_eq!(overlay.prior_driver_row_count(), 0);

    let transform = Transform2D {
        translation: Vec2::new(2.0, overlay.delta_time() as f32),
        ..Transform2D::IDENTITY
    };
    overlay.set_transform(host_object, transform).unwrap();
    assert_eq!(
        overlay.object(host_object).unwrap().transform,
        transform,
        "later callbacks read earlier writes through the ordered overlay"
    );
    overlay
        .set_style(
            host_object,
            Style {
                opacity: 0.75,
                ..Style::default()
            },
        )
        .unwrap();
    assert_eq!(
        session.frame().time,
        0.0,
        "preflight does not publish early"
    );
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert_eq!(session.frame().time, 0.25);
    assert_eq!(
        session.take_frame_changes().object_indices(),
        &[HOST_OBJECT_INDEX],
        "callback publication dirties only the one execution target"
    );

    let mut overlay = match session.advance_to_callback_barrier(0.75).unwrap() {
        CallbackAdvance::HostRequired { overlay, .. } => overlay,
        CallbackAdvance::Ready(_) => panic!("the continuing updater phase must require its host"),
    };
    assert_eq!(overlay.time(), 0.75);
    assert_eq!(overlay.delta_time(), 0.5);
    assert_eq!(overlay.objects().count(), 1);
    assert_eq!(overlay.staged_row_count(), 1);
    assert_eq!(overlay.prior_driver_row_count(), 1);
    assert_eq!(
        overlay.object(host_object).unwrap().transform.translation,
        Vec2::new(2.0, 0.25),
        "the next phase starts from the last coherent effective publication"
    );
    let next_transform = Transform2D {
        translation: Vec2::new(2.0, overlay.delta_time() as f32),
        ..transform
    };
    overlay.set_transform(host_object, next_transform).unwrap();
    overlay
        .set_style(
            host_object,
            Style {
                opacity: 1.0,
                ..Style::default()
            },
        )
        .unwrap();
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert_eq!(session.frame().time, 0.75);
    assert_eq!(
        session.take_frame_changes().object_indices(),
        &[HOST_OBJECT_INDEX]
    );
}

#[test]
fn canonical_callback_failures_preserve_the_last_coherent_publication() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(object, HostCallbackId::new(1), 0.0, None);
    callbacks.apply(&mut store).unwrap();

    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let overlay = match session.advance_to_callback_barrier(0.25).unwrap() {
        CallbackAdvance::HostRequired { overlay, .. } => overlay,
        CallbackAdvance::Ready(_) => panic!("the updater phase must require its host"),
    };
    let token = overlay.token();
    let before = session.frame().clone();
    let publication = session.publication_context();

    let mut foreign = ExecutionSession::from_semantic_store(&store).unwrap();
    let foreign_overlay = match foreign.advance_to_callback_barrier(0.25).unwrap() {
        CallbackAdvance::HostRequired { overlay, .. } => overlay,
        CallbackAdvance::Ready(_) => panic!("the foreign updater phase must require its host"),
    };
    assert!(matches!(
        session.commit_required_callback_phase(foreign_overlay.finish()),
        Err(ExecutionSessionCallbackError::StaleToken { .. })
    ));

    let invalid = EffectivePropertyBatch::new(
        token,
        [EffectiveSemanticPropertyWrite::Style {
            object,
            style: Style {
                opacity: f32::NAN,
                ..Style::default()
            },
        }],
    );
    assert!(matches!(
        session.commit_required_callback_phase(invalid),
        Err(ExecutionSessionCallbackError::InvalidEffectiveWrite(_))
    ));
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.pending_callback_token(), Some(token));
    assert!(session.take_frame_changes().is_empty());

    let mut interrupted = session.clone();
    assert_eq!(
        interrupted.callback_termination().unwrap().kind(),
        CallbackTerminationKind::Interrupted
    );
    assert!(matches!(
        interrupted.advance_to_callback_barrier(0.25),
        Err(ExecutionSessionCallbackError::Terminated(_))
    ));
    assert_eq!(session.pending_callback_token(), Some(token));

    session.fail_required_callback_phase(token).unwrap();
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        session.callback_termination().unwrap().kind(),
        CallbackTerminationKind::Failed
    );
}
