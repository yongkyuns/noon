use super::*;
use crate::{Mobject, Scene};
use noon_core::{
    GeometryRef, GeometryResourceArena, GeometryResourceHandle, GeometryResourceLookup, Vec2,
    VectorPath,
};

fn scene(count: usize) -> (Scene, Vec<Mobject>, ExecutionSession) {
    let mut scene = Scene::new();
    let objects = (0..count)
        .map(|_| {
            let object = scene.circle(1.0).unwrap();
            scene.add(&object).unwrap();
            object
        })
        .collect::<Vec<_>>();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    (scene, objects, session)
}

fn path(width: f32) -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(width, 0.0))
        .line_to(Vec2::new(0.0, 1.0))
        .close()
}

fn handle(session: &ExecutionSession, target: SemanticNodeId) -> GeometryResourceHandle {
    let object = session.execution_object_id(target).unwrap();
    let row = session.runtime.frame_index_for_object(object).unwrap();
    let ObjectContentRef::Geometry(GeometryRef::External(id)) =
        session.frame().objects[row].content
    else {
        panic!("expected callback-owned retained geometry");
    };
    session.geometry_resources().current_handle(id).unwrap()
}

#[test]
fn mixed_callback_content_publishes_once_reuses_equal_paths_and_updates_only_changed_targets() {
    let (scene, objects, mut session) = scene(600);
    let targets = [
        objects[2].node_id(),
        objects[301].node_id(),
        objects[500].node_id(),
    ];
    let semantic_revision = scene.integration_store().borrow().scene_revision();
    let node_count = scene.integration_store().borrow().len();
    let mut overlay = session.begin_required_callback_phase(0.0, targets).unwrap();
    overlay
        .write(EffectiveSemanticPropertyWrite::Translation {
            object: targets[0],
            translation: Vec2::new(3.0, 0.0),
        })
        .unwrap();
    let before = session.publication_context();
    let leases = session
        .commit_required_callback_phase_with_owned_contents(
            overlay.finish(),
            vec![
                (targets[0], CallbackContent::Path(path(2.0))),
                (
                    targets[1],
                    CallbackContent::Inline(ObjectContentRef::Geometry(GeometryRef::circle(2.0))),
                ),
                (targets[2], CallbackContent::Path(path(4.0))),
            ],
        )
        .unwrap();
    assert_eq!(leases.len(), 3);
    assert_ne!(leases[0], leases[1]);
    assert_ne!(leases[1], leases[2]);
    for (target, lease) in targets.into_iter().zip(leases.iter()) {
        assert_eq!(lease.object(), session.execution_object_id(target).unwrap());
        assert_eq!(session.effective_content_lease(target), Some(*lease));
    }
    assert_eq!(
        session.publication_context().frame_epoch().get(),
        before.frame_epoch().get() + 1
    );
    assert_eq!(
        session.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        before.execution_revision()
    );
    let changed = session
        .take_frame_changes()
        .object_indices()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(changed, BTreeSet::from([2, 301, 500]));
    assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
    assert!(session.last_spatial_update_stats().leaves_upserted <= 3);
    let first = handle(&session, targets[0]);
    let second = handle(&session, targets[2]);
    assert_ne!(first.id, second.id);
    assert_eq!(session.callback_geometry_sources.slot_capacity(), 1);
    assert_eq!(session.callback_geometry_sources.stats().live_resources, 0);

    // Two value-equal paths in one subsequent phase are still the exact live
    // resources, including their handles and allocations. Omitted targets hold.
    // Keep the same effective property output so this isolates content reuse
    // from changes to callback property-driver ownership.
    let mut overlay = session.begin_required_callback_phase(0.0, targets).unwrap();
    overlay
        .write(EffectiveSemanticPropertyWrite::Translation {
            object: targets[0],
            translation: Vec2::new(3.0, 0.0),
        })
        .unwrap();
    let before = session.publication_context();
    let repeat = session
        .commit_required_callback_phase_with_owned_contents(
            overlay.finish(),
            vec![
                (targets[2], CallbackContent::Path(path(4.0))),
                (targets[0], CallbackContent::Path(path(2.0))),
            ],
        )
        .unwrap();
    assert_eq!(repeat, vec![leases[2], leases[0]]);
    assert_eq!(handle(&session, targets[0]), first);
    assert_eq!(handle(&session, targets[2]), second);
    assert_eq!(session.publication_context(), before);
    assert!(session.take_frame_changes().object_indices().is_empty());

    for version in 1..=64 {
        let previous = handle(&session, targets[0]);
        let mut overlay = session
            .begin_required_callback_phase(f64::from(version) / 100.0, targets)
            .unwrap();
        overlay
            .write(EffectiveSemanticPropertyWrite::Translation {
                object: targets[0],
                translation: Vec2::new(3.0, 0.0),
            })
            .unwrap();
        let updated = session
            .commit_required_callback_phase_with_owned_contents(
                overlay.finish(),
                vec![
                    (
                        targets[0],
                        CallbackContent::Path(path(2.0 + version as f32 / 10.0)),
                    ),
                    (targets[2], CallbackContent::Path(path(4.0))),
                ],
            )
            .unwrap();
        assert_eq!(updated, vec![leases[0], leases[2]]);
        assert_ne!(handle(&session, targets[0]), previous);
        assert!(session.geometry_resources().get(previous).is_none());
        assert_eq!(handle(&session, targets[2]), second);
        assert_eq!(session.take_frame_changes().object_indices(), &[2]);
        assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
        assert!(session.last_spatial_update_stats().leaves_upserted <= 1);
        assert_eq!(session.callback_geometry_sources.slot_capacity(), 1);
        assert_eq!(session.callback_geometry_sources.stats().live_resources, 0);
    }
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        semantic_revision
    );
    assert_eq!(scene.integration_store().borrow().len(), node_count);
    let last = handle(&session, targets[0]);
    session.release_effective_content(leases[0]).unwrap();
    assert!(session.geometry_resources().get(last).is_none());
    assert!(session.geometry_resources().get(second).is_some());
    session.release_effective_content(leases[2]).unwrap();
    assert!(session.geometry_resources().get(second).is_none());
}

#[test]
fn invalid_later_callback_content_keeps_exact_phase_retryable_and_scratch_empty() {
    let (_scene, objects, mut session) = scene(3);
    let targets = [
        objects[0].node_id(),
        objects[1].node_id(),
        objects[2].node_id(),
    ];
    let mut overlay = session.begin_required_callback_phase(0.5, targets).unwrap();
    overlay
        .write(EffectiveSemanticPropertyWrite::Translation {
            object: targets[0],
            translation: Vec2::new(9.0, 0.0),
        })
        .unwrap();
    let token = overlay.token();
    let batch = overlay.finish();
    let before = session.publication_context();
    for duplicate in [false, true] {
        let result = session.commit_required_callback_phase_with_owned_contents(
            batch.clone(),
            vec![
                (targets[0], CallbackContent::Path(path(2.0))),
                if duplicate {
                    (targets[0], CallbackContent::Path(path(3.0)))
                } else {
                    (
                        targets[1],
                        CallbackContent::Inline(ObjectContentRef::Geometry(GeometryRef::circle(
                            f32::NAN,
                        ))),
                    )
                },
            ],
        );
        if duplicate {
            assert!(matches!(
                result,
                Err(ExecutionSessionCallbackError::Content(
                    EffectiveContentError::DuplicateTarget(_)
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(ExecutionSessionCallbackError::Content(
                    EffectiveContentError::Invalid(_)
                ))
            ));
        }
        assert_eq!(session.pending_callback_token(), Some(token));
        assert_eq!(session.publication_context(), before);
        assert_eq!(session.frame().time, 0.0);
        assert_eq!(session.frame().objects[0].transform.translation, Vec2::ZERO);
        assert!(session.callback_content_leases.is_empty());
        assert_eq!(session.callback_geometry_sources.slot_capacity(), 1);
        assert_eq!(session.callback_geometry_sources.stats().live_resources, 0);
        assert!(session.take_frame_changes().object_indices().is_empty());
    }
    let leases = session
        .commit_required_callback_phase_with_owned_contents(
            batch,
            vec![
                (targets[0], CallbackContent::Path(path(2.0))),
                (targets[1], CallbackContent::Path(path(3.0))),
            ],
        )
        .unwrap();
    assert_eq!(session.pending_callback_token(), None);
    assert_eq!(session.frame().time, 0.5);
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(9.0, 0.0)
    );
    let first = handle(&session, targets[0]);
    let foreign = session
        .prepare_effective_content_replacement(
            targets[2],
            ObjectContentRef::Geometry(GeometryRef::circle(3.0)),
            None,
            None,
        )
        .unwrap();
    let foreign_lease = session
        .commit_effective_content_replacement(foreign)
        .unwrap();
    let overlay = session.begin_required_callback_phase(0.6, targets).unwrap();
    let token = overlay.token();
    let before = session.publication_context();
    assert!(matches!(
        session.commit_required_callback_phase_with_owned_contents(
            overlay.finish(),
            vec![
                (targets[0], CallbackContent::Path(path(4.0))),
                (targets[2], CallbackContent::Path(path(5.0))),
            ],
        ),
        Err(ExecutionSessionCallbackError::Content(
            EffectiveContentError::DriverConflict(_)
        ))
    ));
    assert_eq!(session.pending_callback_token(), Some(token));
    assert_eq!(session.publication_context(), before);
    assert_eq!(handle(&session, targets[0]), first);
    assert_eq!(session.effective_content_lease(targets[0]), Some(leases[0]));
    assert_eq!(
        session.effective_content_lease(targets[2]),
        Some(foreign_lease)
    );
    assert_eq!(session.callback_geometry_sources.stats().live_resources, 0);
}

#[test]
fn explicit_producer_conflicts_reject_whole_batch_and_raw_paths_avoid_reserved_ids() {
    let (_scene, objects, mut session) = scene(3);
    let targets = objects.iter().map(Mobject::node_id).collect::<Vec<_>>();
    let mut first_source = GeometryResourceArena::new();
    let first = first_source.insert_path(path(2.0));
    let mut second_source = GeometryResourceArena::new();
    let colliding = second_source.insert_path(path(3.0));
    let corrected = second_source.insert_path(path(4.0));
    assert_eq!(first.id, colliding.id);
    let overlay = session
        .begin_required_callback_phase(0.5, targets.iter().copied())
        .unwrap();
    let token = overlay.token();
    let batch = overlay.finish();
    let before = session.publication_context();
    let contents = |second| {
        vec![
            (targets[0], CallbackContent::Path(path(1.0))),
            (
                targets[1],
                CallbackContent::Geometry {
                    handle: first,
                    source: &first_source,
                },
            ),
            (
                targets[2],
                CallbackContent::Geometry {
                    handle: second,
                    source: &second_source,
                },
            ),
        ]
    };
    assert!(
        matches!(session.commit_required_callback_phase_with_owned_contents(
        batch.clone(), contents(colliding),
    ), Err(ExecutionSessionCallbackError::ContentCommit(
        PreparedFrameContentCommitError::Content(EffectiveContentError::GeometryResourceConflict(id))
    )) if id == first.id)
    );
    assert_eq!(session.pending_callback_token(), Some(token));
    assert_eq!(session.publication_context(), before);
    assert!(session.geometry_resources().get(first).is_none());
    assert!(session.geometry_resources().get(colliding).is_none());
    assert_eq!(session.callback_geometry_sources.stats().live_resources, 0);
    let leases = session
        .commit_required_callback_phase_with_owned_contents(batch, contents(corrected))
        .unwrap();
    let raw = handle(&session, targets[0]);
    assert_ne!(raw.id, first.id);
    assert_ne!(raw.id, corrected.id);
    assert_eq!(handle(&session, targets[1]), first);
    assert_eq!(handle(&session, targets[2]), corrected);
    drop(first_source);
    drop(second_source);
    for (target, lease) in targets.into_iter().zip(leases) {
        let retained = handle(&session, target);
        assert!(session.geometry_resources().get(retained).is_some());
        session.release_effective_content(lease).unwrap();
        assert!(session.geometry_resources().get(retained).is_none());
    }
    assert_eq!(session.callback_geometry_sources.slot_capacity(), 1);
    assert_eq!(session.callback_geometry_sources.stats().live_resources, 0);
}

#[test]
fn failed_terminal_content_can_retry_its_original_region_but_not_change_accepted_writes() {
    let (scene, objects, _) = scene(2);
    let targets = [objects[0].node_id(), objects[1].node_id()];
    let mut register = noon_core::SemanticMutationTransaction::new();
    register.add_updater(targets[0], noon_core::HostCallbackId::new(1), 0.0, None);
    register.add_updater(targets[1], noon_core::HostCallbackId::new(2), 0.0, None);
    register
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let CallbackAdvance::HostRequired { mut overlay, .. } =
        session.advance_to_callback_barrier(0.5).unwrap()
    else {
        panic!("a required host region must be pending")
    };
    overlay
        .write(EffectiveSemanticPropertyWrite::Translation {
            object: targets[0],
            translation: Vec2::new(5.0, 0.0),
        })
        .unwrap();
    let original = overlay.finish();
    let before = session.publication_context();
    session
        .require_terminal_callback_content_region(original.token(), original.region())
        .unwrap();
    let CallbackRegionAdvance::Complete(batch) = session
        .submit_required_callback_region(original.clone())
        .unwrap()
    else {
        panic!("the final region must complete its property batch")
    };
    assert_ne!(batch.region(), original.region());
    assert!(session
        .commit_required_callback_phase_with_owned_contents(
            batch,
            vec![
                (targets[0], CallbackContent::Path(path(2.0))),
                (
                    targets[1],
                    CallbackContent::Inline(ObjectContentRef::Geometry(GeometryRef::circle(
                        f32::NAN
                    )))
                ),
            ]
        )
        .is_err());
    assert_eq!(session.pending_callback_token(), Some(original.token()));
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.frame().objects[0].transform.translation, Vec2::ZERO);

    session
        .require_terminal_callback_content_region(original.token(), original.region())
        .unwrap();
    assert!(matches!(
        session.require_terminal_callback_content_region(original.token(), original.region() + 2,),
        Err(ExecutionSessionCallbackError::StaleRegion { .. })
    ));
    let altered = EffectivePropertyBatch::new(
        original.token(),
        [EffectiveSemanticPropertyWrite::Translation {
            object: targets[0],
            translation: Vec2::new(6.0, 0.0),
        }],
    )
    .with_region(original.region());
    assert!(matches!(
        session.submit_required_callback_region(altered),
        Err(ExecutionSessionCallbackError::IncompleteRegion)
    ));
    assert_eq!(session.publication_context(), before);
    let CallbackRegionAdvance::Complete(batch) =
        session.submit_required_callback_region(original).unwrap()
    else {
        panic!("the exact accepted properties remain retryable")
    };
    session
        .commit_required_callback_phase_with_owned_contents(
            batch,
            vec![
                (targets[0], CallbackContent::Path(path(2.0))),
                (targets[1], CallbackContent::Path(path(3.0))),
            ],
        )
        .unwrap();
    assert_eq!(session.pending_callback_token(), None);
    assert_eq!(
        session.frame().objects[0].transform.translation,
        Vec2::new(5.0, 0.0)
    );
    assert_eq!(
        session.publication_context().frame_epoch().get(),
        before.frame_epoch().get() + 1
    );
    assert_eq!(session.callback_geometry_sources.stats().live_resources, 0);
}
