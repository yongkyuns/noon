use crate::{ExecutionSessionInputError, Scene};
use noon_core::{
    HostCallbackId, NativeEventOccurrence, NativeEventSource, NativeInputValue, NativeStateSource,
    RateFunction, SemanticMutationTransaction, SemanticObjectProperty, SemanticObjectState,
    SemanticStore, SemanticVec3, StoredGeometry, TrackTiming, Vec2,
};
use noon_runtime::TimelineWakeState;

use super::*;

#[test]
fn contiguous_host_declarations_share_one_region_without_native_between() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut first = SemanticMutationTransaction::new();
    first.add_updater(object, HostCallbackId::new(1), 1.0, None);
    first.apply(&mut store).unwrap();
    let mut second = SemanticMutationTransaction::new();
    second.add_updater(object, HostCallbackId::new(2), 1.0, None);
    second.apply(&mut store).unwrap();

    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let CallbackAdvance::HostRequired {
        invocations,
        overlay,
    } = session.advance_to_callback_barrier(1.0).unwrap()
    else {
        panic!("contiguous host callbacks require one region")
    };
    assert_eq!(overlay.region(), 0);
    assert_eq!(
        invocations
            .iter()
            .map(|invocation| invocation.callback_id())
            .collect::<Vec<_>>(),
        vec![HostCallbackId::new(1), HostCallbackId::new(2)]
    );
}

#[test]
fn native_host_native_host_regions_share_one_unpublished_overlay_and_reject_stale_replies() {
    let mut store = SemanticStore::new();
    let translation = store
        .insert_semantic_input_signal(SemanticVec3::new(4.0, 1.0, 0.0))
        .unwrap();
    let width = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let mut track = SemanticMutationTransaction::new();
    track.add_scalar_signal_track(
        width,
        1.0,
        2.0,
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
    );
    track.apply(&mut store).unwrap();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    store
        .bind_semantic_signal(translation, object, SemanticObjectProperty::Translation)
        .unwrap();
    let mut first = SemanticMutationTransaction::new();
    first.add_updater(object, HostCallbackId::new(1), 1.0, None);
    first.apply(&mut store).unwrap();
    store
        .bind_semantic_signal(width, object, SemanticObjectProperty::StrokeWidth)
        .unwrap();
    let mut second = SemanticMutationTransaction::new();
    second.add_updater(object, HostCallbackId::new(2), 1.0, None);
    second.apply(&mut store).unwrap();

    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let publication = session.publication_context();
    let (first_invocations, mut overlay) = match session.advance_to_callback_barrier(1.0).unwrap() {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => (invocations, overlay),
        CallbackAdvance::Ready(_) => panic!("first host region required"),
    };
    assert_eq!(
        first_invocations
            .iter()
            .map(|entry| entry.callback_id())
            .collect::<Vec<_>>(),
        vec![HostCallbackId::new(1)]
    );
    assert_eq!(
        overlay.object(object).unwrap().transform.translation,
        Vec2::new(4.0, 1.0)
    );
    assert_eq!(overlay.object(object).unwrap().style.stroke_width, 1.0);
    let token = overlay.token();
    assert!(matches!(
        session
            .submit_required_callback_region(EffectivePropertyBatch::new(token, []).with_region(1)),
        Err(ExecutionSessionCallbackError::StaleRegion {
            expected: 0,
            actual: 1
        })
    ));
    overlay
        .write(EffectiveSemanticPropertyWrite::Translation {
            object,
            translation: Vec2::new(4.0, 9.0),
        })
        .unwrap();
    overlay
        .write(EffectiveSemanticPropertyWrite::StrokeWidth {
            object,
            stroke_width: 3.0,
        })
        .unwrap();
    let stale_reply = overlay.clone().finish();
    let (second_invocations, mut overlay) = match session
        .submit_required_callback_region(overlay.finish())
        .unwrap()
    {
        CallbackRegionAdvance::HostRequired {
            invocations,
            overlay,
        } => (invocations, overlay),
        CallbackRegionAdvance::Complete(_) => panic!("second host region required"),
    };
    assert_eq!(
        second_invocations
            .iter()
            .map(|entry| entry.callback_id())
            .collect::<Vec<_>>(),
        vec![HostCallbackId::new(2)]
    );
    assert_eq!(overlay.region(), 1);
    assert_eq!(
        overlay.object(object).unwrap().transform.translation,
        Vec2::new(4.0, 9.0)
    );
    assert_eq!(overlay.object(object).unwrap().style.stroke_width, 2.0);
    assert_eq!(session.publication_context(), publication);
    assert!(matches!(
        session.submit_required_callback_region(stale_reply),
        Err(ExecutionSessionCallbackError::StaleRegion {
            expected: 1,
            actual: 0
        })
    ));
    assert!(matches!(
        session.submit_required_callback_region(
            EffectivePropertyBatch::new(
                token,
                [EffectiveSemanticPropertyWrite::Opacity {
                    object,
                    opacity: f32::NAN
                }]
            )
            .with_region(1)
        ),
        Err(ExecutionSessionCallbackError::InvalidEffectiveWrite(_))
    ));
    assert_eq!(session.publication_context(), publication);
    overlay
        .write(EffectiveSemanticPropertyWrite::Opacity {
            object,
            opacity: 0.7,
        })
        .unwrap();
    let final_reply = overlay.finish();
    let final_batch = match session
        .submit_required_callback_region(final_reply.clone())
        .unwrap()
    {
        CallbackRegionAdvance::Complete(batch) => batch,
        CallbackRegionAdvance::HostRequired { .. } => panic!("final region must complete"),
    };
    assert!(matches!(
        session
            .submit_required_callback_region(final_reply)
            .unwrap(),
        CallbackRegionAdvance::Complete(_)
    ));
    session.commit_required_callback_phase(final_batch).unwrap();
    let row = &session.frame().objects[0];
    assert_eq!(row.transform.translation, Vec2::new(4.0, 9.0));
    assert_eq!(row.style.stroke_width, 2.0);
    assert_eq!(row.style.opacity, 0.7);
    assert_eq!(
        session.publication_context().frame_epoch(),
        publication.frame_epoch().checked_next().unwrap()
    );
}

#[test]
fn native_only_binding_keeps_the_ready_fast_path() {
    let mut store = SemanticStore::new();
    let width = store.insert_semantic_input_signal(2.0_f64).unwrap();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    store
        .bind_semantic_signal(width, object, SemanticObjectProperty::StrokeWidth)
        .unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    assert!(matches!(
        session.advance_to_callback_barrier(1.0).unwrap(),
        CallbackAdvance::Ready(_)
    ));
    assert_eq!(session.frame().objects[0].style.stroke_width, 2.0);
    assert!(session.pending_callback_token().is_none());
}

#[test]
fn token_pinned_callback_reads_prepared_scalar_and_arbitrary_object_without_commit() {
    let mut scene = Scene::new();
    let active = scene.circle(0.5).unwrap();
    let anchor = scene.circle(0.25).unwrap();
    scene.add(&active).unwrap();
    scene.add(&anchor).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    scene
        .play_value(&tracker, 2.0)
        .rate_func(noon_core::RateFunction::Linear)
        .run_time(1.0)
        .unwrap();
    let mut hold = SemanticMutationTransaction::new();
    hold.set_scalar_signal_at(tracker.node_id(), 3.0, 1.0);
    hold.apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut callbacks = SemanticMutationTransaction::new();
    callbacks.add_updater(active.node_id(), HostCallbackId::new(7), 0.0, None);
    callbacks
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut session = scene.execution_session().unwrap();

    let CallbackAdvance::HostRequired { overlay, .. } =
        session.advance_to_callback_barrier(0.0).unwrap()
    else {
        panic!("callback must run at authored time zero")
    };
    let token = overlay.token();
    assert_eq!(
        session
            .required_callback_read(token, CallbackReadRequest::ScalarSignal(tracker.node_id()))
            .unwrap(),
        CallbackReadValue::Scalar(0.0)
    );
    assert!(matches!(
        session
            .required_callback_read(token, CallbackReadRequest::Object(anchor.node_id()))
            .unwrap(),
        CallbackReadValue::Object(_)
    ));
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();

    let CallbackAdvance::HostRequired { overlay, .. } =
        session.advance_to_callback_barrier(0.5).unwrap()
    else {
        panic!("active callback must run at the requested midpoint")
    };
    assert_eq!(
        session
            .required_callback_read(
                overlay.token(),
                CallbackReadRequest::ScalarSignal(tracker.node_id()),
            )
            .unwrap(),
        CallbackReadValue::Scalar(1.0)
    );
    let stale = CallbackPhaseToken::new(
        overlay.token().runtime(),
        overlay.token().publication(),
        CallbackSequence::new(999),
    );
    assert!(matches!(
        session.required_callback_read(stale, CallbackReadRequest::Object(anchor.node_id())),
        Err(ExecutionSessionCallbackReadError::StaleToken { .. })
    ));
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    let CallbackAdvance::HostRequired { overlay, .. } =
        session.advance_to_callback_barrier(1.25).unwrap()
    else {
        panic!("active callback must observe the post-track Hold during wait")
    };
    assert_eq!(
        session
            .required_callback_read(
                overlay.token(),
                CallbackReadRequest::ScalarSignal(tracker.node_id()),
            )
            .unwrap(),
        CallbackReadValue::Scalar(3.0)
    );
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert!(matches!(
        session.required_callback_read(token, CallbackReadRequest::Object(anchor.node_id())),
        Err(ExecutionSessionCallbackReadError::NoPendingPhase)
    ));
}

#[test]
fn callback_entrypoint_preserves_deterministic_seek_without_callbacks() {
    let store = SemanticStore::new();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.advance_to(2.25).unwrap();
    assert!(matches!(
        session.advance_to_callback_barrier(0.0).unwrap(),
        CallbackAdvance::Ready(frame) if frame.time == 0.0
    ));
}

#[test]
fn structural_callback_commits_provisional_object_and_effective_frame_once() {
    use noon_core::{SemanticNodeCreation, SemanticVec3};
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.add_member(root, object).unwrap();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let mut overlay = session
        .begin_required_callback_phase(0.5, [object])
        .unwrap();
    overlay
        .set_transform(
            object,
            Transform2D {
                translation: Vec2::new(3.0, 0.0),
                ..Transform2D::IDENTITY
            },
        )
        .unwrap();
    let batch = overlay.finish();
    let mut tx = SemanticMutationTransaction::new();
    let provisional = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.25 },
    )));
    tx.set_property(
        provisional,
        SemanticObjectProperty::Translation,
        SemanticVec3::new(2.0, 4.0, 0.0),
    );
    tx.add_member(root, provisional);
    let prepared = tx.prepare(&mut store).unwrap();
    assert_eq!(
        prepared
            .object_state(provisional)
            .unwrap()
            .transform
            .translation
            .x,
        2.0
    );
    let planned = prepared.planned_node_id(provisional).unwrap();
    let result = session
        .commit_prepared_required_callback_transaction(batch, prepared, None)
        .unwrap();
    assert_eq!(result.resolve(provisional), Some(planned));
    assert_eq!(session.pending_callback_token(), None);
    assert_eq!(session.frame().time, 0.5);
    assert_eq!(
        session
            .effective_semantic_object(&store, object)
            .unwrap()
            .object
            .transform
            .translation
            .x,
        3.0
    );
    assert_eq!(
        store
            .semantic_object_state_checked(object)
            .unwrap()
            .transform
            .translation
            .x,
        0.0,
        "effective driver writes do not become authored values"
    );
    assert_eq!(
        session
            .effective_semantic_object(&store, planned)
            .unwrap()
            .object
            .transform
            .translation,
        Vec2::new(2.0, 4.0)
    );
    assert_eq!(
        session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(
        session.publication_context().scene_revision(),
        before.scene_revision().checked_next().unwrap()
    );
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        1
    );
}

#[test]
fn structural_callback_can_detach_its_own_target_without_leaking_driver_or_wake() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.add_member(root, object).unwrap();
    store.attach_to_scene(root).unwrap();
    let mut register = SemanticMutationTransaction::new();
    register.add_updater(object, HostCallbackId::new(1), 0.0, None);
    register.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let CallbackAdvance::HostRequired { mut overlay, .. } =
        session.advance_to_callback_barrier(0.0).unwrap()
    else {
        panic!("required updater");
    };
    overlay
        .set_transform(
            object,
            Transform2D {
                translation: Vec2::new(3.0, 0.0),
                ..Transform2D::IDENTITY
            },
        )
        .unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_member(root, object);
    session
        .commit_required_callback_transaction(&mut store, overlay.finish(), tx)
        .unwrap();
    assert!(session.effective_semantic_object(&store, object).is_err());
    assert_eq!(
        session.callback_schedule.wake_timeline(0.0),
        TimelineWakeState::Quiescent
    );
    assert!(matches!(
        session.advance_to_callback_barrier(0.5).unwrap(),
        CallbackAdvance::Ready(_)
    ));
    let mut tx = SemanticMutationTransaction::new();
    tx.add_member(root, object);
    session.apply_semantic_transaction(&mut store, tx).unwrap();
    assert!(matches!(
        session.advance_to_callback_barrier(0.5).unwrap(),
        CallbackAdvance::HostRequired { .. }
    ));
}

#[test]
fn callback_reentry_restores_target_after_committing_the_prior_schedule_preview() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let objects = [0, 1].map(|_| {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }))
    });
    for object in objects {
        store.add_member(root, object).unwrap();
    }
    store.attach_to_scene(root).unwrap();
    let mut register = SemanticMutationTransaction::new();
    for (index, object) in objects.into_iter().enumerate() {
        register.add_updater(object, HostCallbackId::new(index as u64), 0.0, None);
    }
    register.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let CallbackAdvance::HostRequired { overlay, .. } =
        session.advance_to_callback_barrier(0.0).unwrap()
    else {
        panic!("callbacks");
    };
    let mut detach = SemanticMutationTransaction::new();
    detach.remove_member(root, objects[1]);
    session
        .commit_required_callback_transaction(&mut store, overlay.finish(), detach)
        .unwrap();
    let CallbackAdvance::HostRequired {
        overlay,
        invocations,
    } = session.advance_to_callback_barrier(0.5).unwrap()
    else {
        panic!("remaining callback");
    };
    assert_eq!(invocations.len(), 1);
    let mut reentry = SemanticMutationTransaction::new();
    reentry.add_member(root, objects[1]);
    session
        .commit_required_callback_transaction(&mut store, overlay.finish(), reentry)
        .unwrap();
    let CallbackAdvance::HostRequired { invocations, .. } =
        session.advance_to_callback_barrier(0.6).unwrap()
    else {
        panic!("restored callbacks");
    };
    assert_eq!(
        invocations
            .iter()
            .map(|invocation| invocation.target())
            .collect::<Vec<_>>(),
        objects
    );
}

#[test]
fn failed_structural_callback_keeps_same_token_frame_and_semantic_allocator_retryable() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let before = session.publication_context();
    let frame = session.frame().clone();
    let overlay = session
        .begin_required_callback_phase(1.0, [object])
        .unwrap();
    let batch = overlay.finish();
    let token = batch.token;
    let mut invalid = SemanticMutationTransaction::new();
    invalid.set_property(object, SemanticObjectProperty::RotationZ, f64::MAX);
    assert!(matches!(
        session.commit_required_callback_transaction(&mut store, batch.clone(), invalid),
        Err(ExecutionSessionCallbackError::Publication(
            super::super::ExecutionSessionPublicationError::Lowering(_)
        ))
    ));
    assert_eq!(session.pending_callback_token(), Some(token));
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), before);
    assert_eq!(store.scene_revision(), before.scene_revision());
    assert!(session.take_frame_changes().is_empty());
    let mut valid = SemanticMutationTransaction::new();
    valid.set_property(object, SemanticObjectProperty::RotationZ, 0.5);
    session
        .commit_required_callback_transaction(&mut store, batch.clone(), valid)
        .unwrap();
    assert_eq!(session.frame().time, 1.0);
    assert_eq!(
        session
            .effective_semantic_object(&store, object)
            .unwrap()
            .object
            .transform
            .rotation,
        0.5
    );
    assert!(matches!(
        session.commit_required_callback_transaction(
            &mut store,
            batch,
            SemanticMutationTransaction::new()
        ),
        Err(ExecutionSessionCallbackError::NoPendingPhase)
    ));
}

#[test]
fn required_callback_phase_preserves_coherent_frame_and_orders_overlay_writes() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let before = session.frame().clone();
    let publication = session.publication_context();

    let mut overlay = session
        .begin_required_callback_phase(1.0, [object])
        .unwrap();
    assert_eq!(overlay.token().publication(), publication);
    assert_eq!(overlay.delta_time(), 1.0);
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(
        session.advance_to(2.0),
        Err(EvaluationError::RequiredCallbackPending)
    );
    assert_eq!(
        session.set_reactive_input(object, 1.0_f32),
        Err(ExecutionSessionInputError::RequiredCallbackPending)
    );

    let first = Transform2D {
        translation: Vec2::new(2.0, 0.0),
        ..Transform2D::IDENTITY
    };
    let second = Transform2D {
        translation: Vec2::new(3.0, 0.0),
        ..Transform2D::IDENTITY
    };
    overlay.set_transform(object, first).unwrap();
    assert_eq!(overlay.object(object).unwrap().transform, first);
    assert_eq!(
        overlay.object(object).unwrap().bounds.unwrap().center(),
        Vec2::new(2.0, 0.0)
    );
    overlay.set_transform(object, second).unwrap();
    assert_eq!(overlay.object(object).unwrap().transform, second);
    assert_eq!(
        overlay.object(object).unwrap().bounds.unwrap().center(),
        Vec2::new(3.0, 0.0)
    );

    let token = overlay.token();
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert_eq!(session.frame().time, 1.0);
    assert_eq!(session.frame().objects[0].transform, second);
    assert_eq!(session.pending_callback_token(), None);
    assert_eq!(
        session.publication_context().frame_epoch(),
        publication.frame_epoch().checked_next().unwrap()
    );
    let CallbackRendererObservationOutcome::Committed(observation) =
        session.committed_callback_renderer_observation(token, object)
    else {
        panic!("the exact committed callback target must remain observable");
    };
    assert_eq!(observation.token(), token);
    assert_eq!(observation.target(), object);
    assert_eq!(observation.execution_slot(), ExecutionSlotId::new(0, 0));
    assert_eq!(observation.frame_index(), 0);
    assert_eq!(observation.transform(), second);
    assert_eq!(
        observation.dirty(),
        CallbackRendererDirtyClassification::Updated
    );
    assert_eq!(session.take_frame_changes().object_indices(), &[0]);

    let mut next = session
        .begin_required_callback_phase(2.0, [object])
        .unwrap();
    assert_eq!(next.prior_driver_row_count(), 1);
    assert_eq!(next.staged_row_count(), 1);
    assert_eq!(
        next.object(object).unwrap().transform.translation,
        Vec2::new(3.0, 0.0)
    );
    let accumulated = Transform2D {
        translation: next.object(object).unwrap().transform.translation
            + Vec2::new(next.delta_time() as f32, 0.0),
        ..next.object(object).unwrap().transform
    };
    next.set_transform(object, accumulated).unwrap();
    assert_eq!(session.frame().objects[0].transform, second);
    session
        .commit_required_callback_phase(next.finish())
        .unwrap();
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(4.0, 0.0)
    );
    assert!(matches!(
        session.committed_callback_renderer_observation(token, object),
        CallbackRendererObservationOutcome::StaleCallback { .. }
    ));
}

#[test]
fn scoped_callback_writes_preserve_other_channels_and_whole_write_order() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let publication = session.publication_context();
    let mut overlay = session
        .begin_required_callback_phase(0.5, [object])
        .unwrap();
    let transform = Transform2D {
        translation: Vec2::new(1.0, 2.0),
        rotation: 0.75,
        scale: Vec2::new(2.0, 3.0),
    };
    overlay.set_transform(object, transform).unwrap();
    overlay
        .write(EffectiveSemanticPropertyWrite::Translation {
            object,
            translation: Vec2::new(4.0, 5.0),
        })
        .unwrap();
    overlay
        .write(EffectiveSemanticPropertyWrite::Opacity {
            object,
            opacity: 0.25,
        })
        .unwrap();
    let read = overlay.object(object).unwrap();
    assert_eq!(read.transform.rotation, transform.rotation);
    assert_eq!(read.transform.scale, transform.scale);
    assert_eq!(read.transform.translation, Vec2::new(4.0, 5.0));
    assert_eq!(read.style.opacity, 0.25);
    let expected = *read;
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert_eq!(session.frame().objects[0].transform, expected.transform);
    assert_eq!(session.frame().objects[0].style, expected.style);
    assert_eq!(
        session.publication_context().scene_revision(),
        publication.scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        publication.execution_revision()
    );
    assert_eq!(
        session.publication_context().frame_epoch(),
        publication.frame_epoch().checked_next().unwrap()
    );
    let receipt = session.last_callback_receipt.as_ref().unwrap();
    assert_eq!(
        receipt.domains[&object],
        CALLBACK_TRANSLATION | CALLBACK_ROTATION | CALLBACK_SCALE | CALLBACK_OPACITY
    );
    let mut next = session
        .begin_required_callback_phase(1.0, [object])
        .unwrap();
    next.write(EffectiveSemanticPropertyWrite::Translation {
        object,
        translation: Vec2::new(9.0, 9.0),
    })
    .unwrap();
    next.set_transform(object, transform).unwrap();
    session
        .commit_required_callback_phase(next.finish())
        .unwrap();
    assert_eq!(session.frame().objects[0].transform, transform);
}

#[test]
fn invalid_or_stale_callback_batch_is_atomic_and_leaves_barrier_retryable() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let overlay = session
        .begin_required_callback_phase(1.0, [object])
        .unwrap();
    let token = overlay.token();
    let before = session.frame().clone();
    let publication = session.publication_context();

    let stale = CallbackPhaseToken::new(
        token.runtime(),
        token.publication(),
        CallbackSequence::new(token.sequence().get() + 1),
    );
    assert!(matches!(
        session.commit_required_callback_phase(EffectivePropertyBatch::new(stale, [])),
        Err(ExecutionSessionCallbackError::StaleToken { .. })
    ));

    let valid_transform = Transform2D {
        translation: Vec2::new(5.0, 0.0),
        ..Transform2D::IDENTITY
    };
    let invalid = EffectivePropertyBatch::new(
        token,
        [
            EffectiveSemanticPropertyWrite::Transform {
                object,
                transform: valid_transform,
            },
            EffectiveSemanticPropertyWrite::Style {
                object,
                style: Style {
                    opacity: f32::NAN,
                    ..Style::default()
                },
            },
        ],
    );
    assert!(matches!(
        session.commit_required_callback_phase(invalid),
        Err(ExecutionSessionCallbackError::InvalidEffectiveWrite(_))
    ));
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.pending_callback_token(), Some(token));
    assert!(session.take_frame_changes().is_empty());

    let retry = EffectivePropertyBatch::new(
        token,
        [EffectiveSemanticPropertyWrite::Transform {
            object,
            transform: valid_transform,
        }],
    );
    session.commit_required_callback_phase(retry).unwrap();
    assert_eq!(session.frame().objects[0].transform, valid_transform);
}

#[test]
fn failed_required_callback_discards_sparse_evaluation_without_advancing_time() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    session.take_frame_changes();
    let before = session.frame().clone();
    let publication = session.publication_context();
    let token = session
        .begin_required_callback_phase(4.0, [object])
        .unwrap()
        .token();

    session.fail_required_callback_phase(token).unwrap();
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), publication);
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(session.pending_callback_token(), None);
    assert_eq!(
        session.callback_termination().unwrap().kind(),
        CallbackTerminationKind::Failed
    );
    assert!(matches!(
        session.advance_to_callback_barrier(4.0),
        Err(ExecutionSessionCallbackError::Terminated(_))
    ));
    assert_eq!(
        session.wake_state().timeline(),
        TimelineWakeState::Quiescent
    );
}

#[test]
fn equal_revision_sessions_reject_each_others_callback_batches() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut first = ExecutionSession::from_semantic_store(&store).unwrap();
    let mut second = ExecutionSession::from_semantic_store(&store).unwrap();

    let first_overlay = first.begin_required_callback_phase(1.0, [object]).unwrap();
    let second_overlay = second.begin_required_callback_phase(1.0, [object]).unwrap();
    assert_eq!(
        first_overlay.token().publication(),
        second_overlay.token().publication()
    );
    assert_eq!(
        first_overlay.token().sequence(),
        second_overlay.token().sequence()
    );
    assert_ne!(
        first_overlay.token().runtime(),
        second_overlay.token().runtime()
    );

    assert!(matches!(
        second.commit_required_callback_phase(first_overlay.finish()),
        Err(ExecutionSessionCallbackError::StaleToken { .. })
    ));
    assert_eq!(
        second.pending_callback_token(),
        Some(second_overlay.token())
    );
    assert_eq!(second.frame().time, 0.0);
}

#[test]
fn cloning_a_pending_session_preserves_progress_as_an_interruption() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let mut original = ExecutionSession::from_semantic_store(&store).unwrap();
    let original_overlay = original
        .begin_required_callback_phase(1.0, [object])
        .unwrap();

    let mut cloned = original.clone();
    assert_eq!(cloned.pending_callback_token(), None);
    let termination = cloned.callback_termination().unwrap();
    assert_eq!(termination.kind(), CallbackTerminationKind::Interrupted);
    assert_ne!(
        original_overlay.token().runtime(),
        termination.token().runtime()
    );
    assert!(matches!(
        cloned.advance_to_callback_barrier(1.0),
        Err(ExecutionSessionCallbackError::Terminated(_))
    ));
    assert_eq!(
        original.pending_callback_token(),
        Some(original_overlay.token())
    );
    assert_eq!(original.frame().time, 0.0);
    assert_eq!(cloned.frame().time, 0.0);
}

#[test]
fn callback_aware_advance_runs_time_zero_phase_once_in_compiler_order() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let unrelated =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 2.0,
        }));
    store.attach_to_scene(object).unwrap();
    store.attach_to_scene(unrelated).unwrap();
    let input = store.insert_semantic_input_signal(0.0_f64).unwrap();
    store
        .bind_semantic_signal(input, object, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    let bound_state_source = NativeStateSource::Control {
        name: "callback-input".to_owned(),
    };
    store
        .bind_semantic_native_state_input(input, bound_state_source.clone())
        .unwrap();
    let event_input = store.insert_semantic_input_signal(0.0_f64).unwrap();
    store
        .bind_semantic_signal(event_input, object, SemanticObjectProperty::RotationZ)
        .unwrap();
    let bound_event_source = NativeEventSource::KeyPress {
        code: "Space".to_owned(),
    };
    store
        .bind_semantic_native_event_input(event_input, bound_event_source.clone())
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(object, HostCallbackId::new(4), 0.0, None);
    transaction.add_updater(object, HostCallbackId::new(2), 0.0, None);
    transaction.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let zero_duration = session.wait_segment(0.0).unwrap();
    let coherent = session.frame().clone();

    assert_eq!(
        session.set_reactive_input(input, 1.0_f32),
        Err(ExecutionSessionInputError::RequiredCallbacksConfigured)
    );
    assert_eq!(
        session.set_native_state_input(bound_state_source, NativeInputValue::Scalar(1.0),),
        Err(ExecutionSessionInputError::RequiredCallbacksConfigured)
    );
    assert_eq!(
        session.emit_native_event(NativeEventOccurrence::new(0, bound_event_source,)),
        Err(ExecutionSessionInputError::RequiredCallbacksConfigured)
    );
    session
        .set_native_state_input(
            NativeStateSource::ViewportSize,
            NativeInputValue::Vec2(Vec2::new(10.0, 10.0)),
        )
        .unwrap();
    session
        .emit_native_event(NativeEventOccurrence::new(
            0,
            NativeEventSource::PointerDown { button: 0 },
        ))
        .unwrap();
    assert_eq!(session.frame(), &coherent);

    assert!(!session.segment_state(zero_duration).is_complete());
    assert_eq!(
        session.advance_to(0.0),
        Err(EvaluationError::RequiredCallbackBarrier)
    );
    assert_eq!(
        session.wake_state().timeline(),
        TimelineWakeState::Continuous
    );
    let (invocations, overlay) = match session.advance_to_callback_barrier(0.0).unwrap() {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => (invocations, overlay),
        CallbackAdvance::Ready(_) => panic!("time-zero updater phase must be required"),
    };
    assert_eq!(
        invocations
            .iter()
            .map(|invocation| invocation.callback_id())
            .collect::<Vec<_>>(),
        vec![HostCallbackId::new(4), HostCallbackId::new(2)]
    );
    assert_eq!(overlay.objects().count(), 1);
    assert!(overlay.object(unrelated).is_none());
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert!(session.segment_state(zero_duration).is_complete());

    assert!(matches!(
        session.advance_to_callback_barrier(0.0).unwrap(),
        CallbackAdvance::Ready(frame) if frame.time == 0.0
    ));
}

#[test]
fn latest_sampled_state_remains_coherent_across_a_required_callback_stall() {
    let mut store = SemanticStore::new();
    let target = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(target).unwrap();
    let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
    let source = NativeStateSource::Control {
        name: "gain".to_owned(),
    };
    store
        .bind_semantic_signal(signal, target, SemanticObjectProperty::ObjectOpacity)
        .unwrap();
    store
        .bind_semantic_native_state_input(signal, source.clone())
        .unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

    // Sampled state is latest-value data. Deliver a bounded burst through the
    // session API, then pin that coherent state behind the existing callback
    // phase instead of introducing another pending-input queue here.
    let latest = 31.0 / 32.0;
    for sample in 0..32 {
        session
            .set_native_state_input(
                source.clone(),
                NativeInputValue::Scalar(sample as f32 / 32.0),
            )
            .unwrap();
    }
    assert_eq!(session.frame().objects[0].style.opacity, latest);
    let overlay = session
        .begin_required_callback_phase(0.0, [target])
        .unwrap();
    assert_eq!(overlay.object(target).unwrap().style.opacity, latest);
    assert_eq!(
        session.set_native_state_input(source, NativeInputValue::Scalar(0.0)),
        Err(ExecutionSessionInputError::RequiredCallbackPending),
    );
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert_eq!(session.frame().objects[0].style.opacity, latest);
}

#[test]
fn required_callback_phase_stays_local_with_ten_thousand_unrelated_objects() {
    let mut store = SemanticStore::new();
    let target = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(target).unwrap();
    for _ in 0..10_000 {
        let unrelated =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 2.0,
            }));
        store.attach_to_scene(unrelated).unwrap();
    }
    let callback = HostCallbackId::new(31);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(target, callback, 0.0, None);
    transaction.apply(&mut store).unwrap();

    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let (invocations, overlay) = match session.advance_to_callback_barrier(0.0).unwrap() {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => (invocations, overlay),
        CallbackAdvance::Ready(_) => panic!("time-zero callback phase must be required"),
    };

    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].callback_id(), callback);
    assert_eq!(overlay.objects().count(), 1);
    assert_eq!(overlay.staged_row_count(), 0);
    assert_eq!(overlay.prior_driver_row_count(), 0);
    assert!(overlay.object(target).is_some());

    // Model an overloaded host by requesting later ticks before its required
    // callback has returned. The pending token is the barrier: these requests
    // must stay rejected and cannot move the visible frame through the callback
    // dependency, even in a large scene.
    let pending = overlay.token();
    let coherent_frame = session.frame().clone();
    let publication = session.publication_context();
    for _ in 0..512 {
        assert!(matches!(
            session.advance_to_callback_barrier(1.0),
            Err(ExecutionSessionCallbackError::Pending(token)) if token == pending
        ));
        assert_eq!(session.pending_callback_token(), Some(pending));
        assert_eq!(session.publication_context(), publication);
    }
    assert_eq!(session.frame(), &coherent_frame);

    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert_eq!(session.frame().objects.len(), 10_001);
    assert_eq!(session.frame().time, 0.0);

    let resumed = match session.advance_to_callback_barrier(1.0).unwrap() {
        CallbackAdvance::HostRequired { overlay, .. } => {
            assert_eq!(overlay.time(), 1.0);
            overlay
        }
        CallbackAdvance::Ready(_) => panic!("active callback must resume at the requested time"),
    };
    session
        .commit_required_callback_phase(resumed.finish())
        .unwrap();
    assert_eq!(session.frame().time, 1.0);
}

#[test]
fn large_advance_stops_at_bounded_callback_activation_before_crossing_it() {
    let mut store = SemanticStore::new();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(object).unwrap();
    let callback = HostCallbackId::new(9);
    let mut add = SemanticMutationTransaction::new();
    add.add_updater(object, callback, 1.0, None);
    add.apply(&mut store).unwrap();
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_updater(object, callback, 1.1);
    remove.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();

    assert_eq!(
        session.wake_state().timeline(),
        TimelineWakeState::Deadline(1.0)
    );
    let overlay = match session.advance_to_callback_barrier(2.0).unwrap() {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => {
            assert_eq!(overlay.time(), 1.0);
            assert_eq!(invocations.len(), 1);
            assert_eq!(invocations[0].callback_id(), callback);
            overlay
        }
        CallbackAdvance::Ready(_) => panic!("bounded updater interval was skipped"),
    };
    assert_eq!(session.frame().time, 0.0);
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    assert_eq!(session.frame().time, 1.0);

    assert!(matches!(
        session.advance_to_callback_barrier(2.0).unwrap(),
        CallbackAdvance::Ready(frame) if frame.time == 2.0
    ));
    assert_eq!(
        session.wake_state().timeline(),
        TimelineWakeState::Quiescent
    );
}
