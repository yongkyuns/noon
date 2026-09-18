use crate::{
    AnimationOptions, AuthoringError, FamilyArrangeOptions, FamilyGridOptions, LiveLayoutTarget,
    MobjectTarget, RateFunction, Scene, UnsupportedAuthoringOperation,
};

#[test]
fn scene_family_shift_uses_the_same_route_before_and_after_bootstrap() {
    let mut scene = Scene::new();
    let mut left = scene.square(2.0).unwrap();
    let mut right = scene.square(2.0).unwrap();
    left.set_translation(-2.0, 0.0).unwrap();
    right.set_translation(2.0, 0.0).unwrap();
    let family = scene
        .family(&[MobjectTarget::Object(&left), MobjectTarget::Object(&right)])
        .unwrap();
    scene
        .add_many(&[MobjectTarget::Object(&left), MobjectTarget::Object(&right)])
        .unwrap();

    let revision = scene.revision();
    scene.shift_family(&family, -1.5, 0.5).unwrap();
    // A persistent edit must not bootstrap execution or relabel authored state.
    let error = scene.effective_family_layout(&family).unwrap_err();
    assert!(matches!(
        error,
        AuthoringError::Unsupported(UnsupportedAuthoringOperation::EffectiveStateUnavailable)
    ));
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(left.center().unwrap(), (-3.5, 0.5));
    assert_eq!(right.center().unwrap(), (0.5, 0.5));

    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let before = scene.revision();
    scene.shift_family(&family, 1.5, -0.5).unwrap();
    assert_eq!(scene.revision().get(), before.get() + 1);
    assert_eq!(left.center().unwrap(), (-2.0, 0.0));
    assert_eq!(right.center().unwrap(), (2.0, 0.0));

    let layout = scene.effective_family_layout(&family).unwrap();
    assert_eq!(layout.center, (0.0, 0.0));
    assert_eq!((layout.width, layout.height), (6.0, 2.0));
    assert_eq!(
        layout.publication,
        scene.owned_execution().publication_context()
    );
}

#[test]
fn scene_family_arrangement_and_placement_publish_through_one_owner() {
    let mut scene = Scene::new();
    let first = scene.square(2.0).unwrap();
    let second = scene.square(2.0).unwrap();
    let third = scene.square(2.0).unwrap();
    let family = scene
        .family(&[
            MobjectTarget::Object(&first),
            MobjectTarget::Object(&second),
            MobjectTarget::Object(&third),
        ])
        .unwrap();
    scene
        .add_many(&[
            MobjectTarget::Object(&first),
            MobjectTarget::Object(&second),
            MobjectTarget::Object(&third),
        ])
        .unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);

    let before = scene.revision();
    scene
        .arrange_family_with_options(&family, &FamilyArrangeOptions::new(1.0, 0.0, 1.0, false))
        .unwrap();
    assert_eq!(scene.revision().get(), before.get() + 1);
    assert_eq!(first.center().unwrap(), (0.0, 0.0));
    assert_eq!(second.center().unwrap(), (3.0, 0.0));
    assert_eq!(third.center().unwrap(), (6.0, 0.0));

    let before = scene.revision();
    scene
        .move_family_to(
            &family,
            LiveLayoutTarget::Point(5.0, 3.0),
            (0.0, 0.0),
            (1.0, 1.0),
        )
        .unwrap();
    assert_eq!(scene.revision().get(), before.get() + 1);
    assert_eq!(
        scene.effective_family_layout(&family).unwrap().center,
        (5.0, 3.0)
    );

    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.square(1.0).unwrap();
    let revision = scene.revision();
    let publication = scene.owned_execution().publication_context();
    let first_before = first.state().unwrap();
    let error = scene
        .move_family_to(
            &family,
            LiveLayoutTarget::Mobject(&foreign),
            (0.0, 0.0),
            (1.0, 1.0),
        )
        .unwrap_err();
    assert!(matches!(error, AuthoringError::ForeignStore));
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.owned_execution().publication_context(), publication);
    assert_eq!(first.state().unwrap(), first_before);
}

#[test]
fn scene_family_placement_rejects_active_affine_driver_atomically() {
    let mut scene = Scene::new();
    let object = scene.square(2.0).unwrap();
    let neighbor = scene.square(2.0).unwrap();
    let family = scene
        .family(&[(&object).into(), (&neighbor).into()])
        .unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut endpoint = object.target_editor().unwrap();
    endpoint.shift(4.0, 0.0).unwrap();
    let animation = scene
        .declare_transform_to(
            &object,
            &endpoint,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    {
        let mut live = scene.owned_live();
        let segment = live.play_animation(&animation).unwrap();
        live.advance_segment_to(segment, 1.0).unwrap();
    }

    let before = object.state().unwrap();
    let revision = scene.revision();
    let publication = scene.owned_execution().publication_context();
    let error = scene
        .move_family_to(
            &family,
            LiveLayoutTarget::Point(10.0, 0.0),
            (0.0, 0.0),
            (1.0, 1.0),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        AuthoringError::Unsupported(UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver)
    ));
    let frame = scene.owned_execution().frame().objects.clone();
    for error in [
        scene
            .arrange_family_with_options(&family, &FamilyArrangeOptions::new(1.0, 0.0, 0.5, true))
            .unwrap_err(),
        scene
            .arrange_family_in_grid_with_options(
                &family,
                &FamilyGridOptions {
                    columns: Some(2),
                    ..Default::default()
                },
            )
            .unwrap_err(),
    ] {
        assert!(matches!(
            error,
            AuthoringError::Unsupported(
                UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver
            )
        ));
    }
    assert_eq!(object.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.owned_execution().publication_context(), publication);
    assert_eq!(scene.owned_execution().frame().objects, frame);
}

#[test]
fn scene_arrangement_is_mode_independent_and_preserves_detached_admission() {
    for running in [false, true] {
        for detached in [false, true] {
            for grid in [false, true] {
                let mut scene = Scene::new();
                let first = scene.square(2.0).unwrap();
                let second = scene.square(2.0).unwrap();
                let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
                let ids = (first.node_id(), second.node_id(), family.node_id());
                let unrelated = scene.circle(0.5).unwrap();
                scene.add(&unrelated).unwrap();
                if !detached {
                    scene.add_many(&[(&family).into()]).unwrap();
                }
                let resources = scene
                    .integration_store()
                    .borrow()
                    .geometry_resources()
                    .stats();
                let unrelated_before = unrelated.state().unwrap();
                if running {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                }
                let frame_before = running.then(|| scene.owned_execution().frame().objects.clone());
                let revision = scene.revision();
                // The operation call is identical in cold and running scenes.
                let result = if grid {
                    scene.arrange_family_in_grid_with_options(
                        &family,
                        &FamilyGridOptions {
                            columns: Some(2),
                            gap: (1.0, 1.0),
                            ..Default::default()
                        },
                    )
                } else {
                    scene.arrange_family_with_options(
                        &family,
                        &FamilyArrangeOptions::new(1.0, 0.0, 1.0, true),
                    )
                }
                .unwrap();
                assert_eq!(result.impacts().len(), 2);
                assert_eq!(scene.revision(), revision.checked_next().unwrap());
                assert_eq!(first.center().unwrap(), (-1.5, 0.0));
                assert_eq!(second.center().unwrap(), (1.5, 0.0));
                assert_eq!((first.node_id(), second.node_id(), family.node_id()), ids);
                assert_eq!(unrelated.state().unwrap(), unrelated_before);
                assert_eq!(
                    scene
                        .integration_store()
                        .borrow()
                        .geometry_resources()
                        .stats(),
                    resources
                );
                if running {
                    assert_eq!(
                        scene
                            .owned_execution()
                            .publication_context()
                            .scene_revision(),
                        scene.revision()
                    );
                    // Detached placement does not touch any rendered row.
                    if detached {
                        assert_eq!(
                            scene.owned_execution().frame().objects,
                            frame_before.unwrap()
                        );
                    }
                } else {
                    assert!(matches!(
                        scene.effective(&first),
                        Err(AuthoringError::Unsupported(
                            UnsupportedAuthoringOperation::EffectiveStateUnavailable
                        ))
                    ));
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                }
                if detached {
                    scene.add_many(&[(&family).into()]).unwrap();
                }
                assert_eq!(scene.owned_execution().frame().objects.len(), 3);
                assert_eq!(
                    scene.effective(&first).unwrap().transform.translation.x,
                    -1.5
                );
                assert_eq!(
                    scene.effective(&second).unwrap().transform.translation.x,
                    1.5
                );
                let layout = scene.effective_family_layout(&family).unwrap();
                assert_eq!(layout.center, (0.0, 0.0));
                assert_eq!(
                    layout.publication,
                    scene.owned_execution().publication_context()
                );
            }
        }
    }
}

#[test]
fn scene_family_shift_translates_aliases_once_in_both_modes() {
    for running in [false, true] {
        let mut scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let family = scene.family(&[(&first).into(), (&nested).into()]).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        if running {
            let execution = scene.execution_session().unwrap();
            scene.install_execution(execution);
        }
        let revision = scene.revision();
        let result = scene.shift_family(&family, 2.0, -1.0).unwrap();
        assert_eq!(result.impacts().len(), 2);
        assert_eq!(scene.revision(), revision.checked_next().unwrap());
        assert_eq!(first.center().unwrap(), (2.0, -1.0));
        assert_eq!(second.center().unwrap(), (2.0, -1.0));
        if running {
            assert_eq!(scene.owned_execution().frame().objects.len(), 2);
            assert_eq!(
                scene.effective_family_layout(&family).unwrap().center,
                (2.0, -1.0)
            );
        }
    }
}

#[test]
fn scene_family_edits_reject_invalid_options_and_foreign_families_atomically() {
    for running in [false, true] {
        let mut scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let mut other = Scene::new();
        let foreign_object = other.square(1.0).unwrap();
        let foreign = other.family(&[(&foreign_object).into()]).unwrap();
        if running {
            let execution = scene.execution_session().unwrap();
            scene.install_execution(execution);
        }
        let revision = scene.revision();
        let states = (first.state().unwrap(), second.state().unwrap());
        let foreign_before = foreign_object.state().unwrap();
        let publication = running.then(|| scene.owned_execution().publication_context());
        let frame = running.then(|| scene.owned_execution().frame().objects.clone());
        assert!(scene.shift_family(&family, f64::NAN, 0.0).is_err());
        assert!(scene
            .arrange_family_with_options(
                &family,
                &FamilyArrangeOptions::new(1.0, 0.0, f64::NAN, true)
            )
            .is_err());
        assert!(scene
            .arrange_family_in_grid_with_options(
                &family,
                &FamilyGridOptions {
                    rows: Some(1),
                    columns: Some(1),
                    ..Default::default()
                }
            )
            .is_err());
        assert!(matches!(
            scene.shift_family(&foreign, 1.0, 0.0),
            Err(AuthoringError::ForeignStore)
        ));
        assert!(matches!(
            scene.arrange_family_with_options(
                &foreign,
                &FamilyArrangeOptions::new(1.0, 0.0, 0.5, true)
            ),
            Err(AuthoringError::ForeignStore)
        ));
        assert!(matches!(
            scene.arrange_family_in_grid_with_options(&foreign, &FamilyGridOptions::default()),
            Err(AuthoringError::ForeignStore)
        ));
        assert_eq!(scene.revision(), revision);
        assert_eq!((first.state().unwrap(), second.state().unwrap()), states);
        assert_eq!(foreign_object.state().unwrap(), foreign_before);
        if running {
            assert_eq!(
                scene.owned_execution().publication_context(),
                publication.unwrap()
            );
            assert_eq!(scene.owned_execution().frame().objects, frame.unwrap());
        }
    }
}

#[test]
fn scene_family_edits_reject_a_stale_owned_execution_without_publication() {
    let mut scene = Scene::new();
    let mut first = scene.square(1.0).unwrap();
    let second = scene.square(1.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    first.set_translation(3.0, 0.0).unwrap(); // Deliberately bypass publication.
    let revision = scene.revision();
    let publication = scene.owned_execution().publication_context();
    let frame = scene.owned_execution().frame().objects.clone();
    let states = (first.state().unwrap(), second.state().unwrap());
    assert!(scene.shift_family(&family, 1.0, 0.0).is_err());
    assert!(scene
        .arrange_family_with_options(&family, &FamilyArrangeOptions::new(1.0, 0.0, 0.5, true))
        .is_err());
    assert!(scene
        .arrange_family_in_grid_with_options(&family, &FamilyGridOptions::default())
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!((first.state().unwrap(), second.state().unwrap()), states);
    assert_eq!(scene.owned_execution().publication_context(), publication);
    assert_eq!(scene.owned_execution().frame().objects, frame);
}
