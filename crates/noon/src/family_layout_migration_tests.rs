use crate::{
    AnimationOptions, AuthoringError, FamilyArrangeOptions, LiveLayoutTarget, MobjectTarget,
    RateFunction, Scene, UnsupportedAuthoringOperation,
};

#[test]
fn scene_family_shift_requires_running_execution_and_publishes_once() {
    let mut scene = Scene::new();
    let mut left = scene.square(2.0).unwrap();
    let mut right = scene.square(2.0).unwrap();
    left.set_translation(-2.0, 0.0).unwrap();
    right.set_translation(2.0, 0.0).unwrap();
    let family = scene
        .family(&[
            MobjectTarget::Object(&left),
            MobjectTarget::Object(&right),
        ])
        .unwrap();
    scene
        .add_many(&[
            MobjectTarget::Object(&left),
            MobjectTarget::Object(&right),
        ])
        .unwrap();

    let revision = scene.revision();
    let error = scene.shift_family(&family, 1.5, -0.5).unwrap_err();
    assert!(matches!(
        error,
        AuthoringError::Unsupported(UnsupportedAuthoringOperation::EffectiveStateUnavailable)
    ));
    assert_eq!(scene.revision(), revision);
    assert_eq!(left.center().unwrap(), (-2.0, 0.0));
    assert_eq!(right.center().unwrap(), (2.0, 0.0));

    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let before = scene.revision();
    scene.shift_family(&family, 1.5, -0.5).unwrap();
    assert_eq!(scene.revision().get(), before.get() + 1);
    assert_eq!(left.center().unwrap(), (-0.5, -0.5));
    assert_eq!(right.center().unwrap(), (3.5, -0.5));

    let layout = scene.effective_family_layout(&family).unwrap();
    assert_eq!(layout.center, (1.5, -0.5));
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
    assert_eq!(scene.effective_family_layout(&family).unwrap().center, (5.0, 3.0));

    let foreign_scene = Scene::new();
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
    let family = scene
        .family(&[MobjectTarget::Object(&object)])
        .unwrap();
    scene.add(&object).unwrap();
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
    assert_eq!(object.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.owned_execution().publication_context(), publication);
}
