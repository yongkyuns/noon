use noon::{ManimRotationPivot, Mobject, MobjectFamily, Scene};

fn family() -> (Scene, MobjectFamily, [Mobject; 2], Mobject) {
    let mut scene = Scene::new();
    let mut a = scene.square(0.5).unwrap();
    let mut b = scene.square(0.5).unwrap();
    a.shift(-1.0, 0.0).unwrap();
    b.shift(1.0, 0.0).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    let unrelated = scene.circle(0.2).unwrap();
    (scene, family, [a, b], unrelated)
}

#[test]
fn shared_aliases_are_scaled_and_rotated_once_in_one_revision() {
    let (scene, family, [a, b], unrelated) = family();
    let before = scene.revision();
    family.scale(2.0, 1.0).unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(a.center().unwrap(), (-2.0, 0.0));
    assert_eq!(b.center().unwrap(), (2.0, 0.0));
    assert_eq!(a.width().unwrap(), 1.0);
    let before = scene.revision();
    family
        .rotate(std::f64::consts::FRAC_PI_2, ManimRotationPivot::Center)
        .unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert!(a.center().unwrap().0.abs() < 1e-6);
    assert!((a.center().unwrap().1 + 2.0).abs() < 1e-6);
    assert!((b.center().unwrap().1 - 2.0).abs() < 1e-6);
    assert_eq!(unrelated.center().unwrap(), (0.0, 0.0));
}

#[test]
fn late_overflow_and_invalid_pivot_leave_every_leaf_unchanged() {
    let (scene, family, [a, mut b], _) = family();
    b.scale(1.0e20, 1.0).unwrap();
    let before = scene.revision();
    let states = [a.state().unwrap(), b.state().unwrap()];
    assert!(family.scale(1.0e20, 1.0).is_err());
    assert!(family
        .rotate(1.0, ManimRotationPivot::Point(f64::NAN, 0.0))
        .is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!([a.state().unwrap(), b.state().unwrap()], states);
}

#[test]
fn live_family_edits_publish_atomically_and_foreign_family_is_rejected() {
    let (mut scene, family, [a, b], _) = family();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.into_execution().unwrap();
    session.scene_mut().scale_family(&family, 2.0, 1.0).unwrap();
    assert_eq!(
        session
            .live()
            .effective(&a)
            .unwrap()
            .transform
            .translation
            .x,
        -2.0
    );
    assert_eq!(
        session
            .live()
            .effective(&b)
            .unwrap()
            .transform
            .translation
            .x,
        2.0
    );
    session
        .scene_mut()
        .rotate_family(
            &family,
            std::f64::consts::FRAC_PI_2,
            ManimRotationPivot::Point(0.0, 0.0),
        )
        .unwrap();
    assert!(
        (session
            .live()
            .effective(&b)
            .unwrap()
            .transform
            .translation
            .y
            - 2.0)
            .abs()
            < 1e-6
    );
    let before = session.live().effective(&a).unwrap();
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.family(&[]).unwrap();
    assert!(session
        .scene_mut()
        .scale_family(&foreign, 2.0, 2.0)
        .is_err());
    assert!(session
        .scene_mut()
        .scale_family(&family, f64::NAN, 2.0)
        .is_err());
    assert_eq!(session.live().effective(&a).unwrap(), before);
}

#[test]
fn active_affine_driver_rejects_family_edit_without_partial_publication() {
    let (mut scene, family, [a, _], _) = family();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut target = a.target_editor().unwrap();
    target.shift(1.0, 0.0).unwrap();
    let animation = scene
        .declare_transform_to(&a, &target, noon::AnimationOptions::new().run_time(2.0))
        .unwrap();
    let mut session = scene.into_execution().unwrap();
    let segment = session.live().play_animation(&animation).unwrap();
    session.live().advance_segment_to(segment, 1.0).unwrap();
    let before = session.live().effective(&a).unwrap();
    assert!(session.scene_mut().scale_family(&family, 2.0, 2.0).is_err());
    let anchor = noon::LayoutAnchor::from(&family);
    assert!(session
        .scene_mut()
        .flip_layout(
            &anchor,
            noon::SemanticVec3::new(0., 1., 0.),
            ManimRotationPivot::Center
        )
        .is_err());
    assert!(session
        .scene_mut()
        .rotate_layout(&anchor, 0.5, ManimRotationPivot::Point(0., 0.))
        .is_err());
    assert_eq!(session.live().effective(&a).unwrap(), before);
    session
        .live()
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.live().complete_segment(segment).unwrap();
    session.scene_mut().scale_family(&family, 2.0, 2.0).unwrap();
}
