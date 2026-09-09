use noon::{FamilyLayoutTarget as Target, ManimNextToArgs, Scene};

#[test]
fn placement_shares_object_family_and_point_targets_with_masks_and_nonunit_directions() {
    let scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.shift(2.0, 0.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let mut target = scene.square(2.0).unwrap();
    target.shift(6.0, 5.0).unwrap();
    let target_family = scene.family(&[(&target).into()]).unwrap();
    let observed = family.layout().unwrap();
    assert_eq!(observed.center(), (1.0, 0.0));
    assert_eq!((observed.width(), observed.height()), (3.0, 1.0));
    observed
        .next_to(
            Target::Mobject(&target),
            ManimNextToArgs {
                direction: (2.0, -3.0),
                buff: 0.5,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 0.25),
            },
        )
        .unwrap();
    assert_eq!(first.center().unwrap(), (8.5, 0.5));
    assert_eq!(second.center().unwrap(), (10.5, 0.5));
    assert_eq!(observed.center(), (1.0, 0.0)); // Immutable observation.
    family
        .layout()
        .unwrap()
        .move_to(
            Target::Family(&target_family.layout().unwrap()),
            (0.0, 0.0),
            (0.5, 0.0),
        )
        .unwrap();
    assert_eq!(first.center().unwrap(), (6.75, 0.5));
    family
        .layout()
        .unwrap()
        .align_to(Target::Point(0.0, 3.0), (0.0, -1.0))
        .unwrap();
    assert_eq!(first.center().unwrap(), (6.75, 3.5));
    assert_eq!(second.center().unwrap(), (8.75, 3.5));
    assert_eq!(target.center().unwrap(), (6.0, 5.0));
}

#[test]
fn invalid_or_foreign_placement_targets_do_not_publish() {
    let scene = Scene::new();
    let object = scene.square(1.0).unwrap();
    let family = scene.family(&[(&object).into()]).unwrap();
    let other_scene = Scene::new();
    let foreign = other_scene.square(1.0).unwrap();
    let foreign_family = other_scene
        .family(&[(&foreign).into()])
        .unwrap()
        .layout()
        .unwrap();
    let observation = family.layout().unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    for target in [
        Target::Mobject(&foreign),
        Target::Family(&foreign_family),
        Target::Point(f64::NAN, 0.0),
    ] {
        assert!(observation.move_to(target, (0.0, 0.0), (1.0, 1.0)).is_err());
    }
    assert!(observation
        .next_to(
            Target::Point(0.0, 0.0),
            ManimNextToArgs {
                direction: (1.0, 0.0),
                buff: f64::INFINITY,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            }
        )
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    assert_eq!(object.center().unwrap(), (0.0, 0.0));
}

#[test]
fn empty_family_observation_has_origin_bounds_without_scene_changes() {
    let scene = Scene::new();
    let family = scene.family(&[]).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    let observation = family.layout().unwrap();
    assert_eq!(observation.bounds(), None);
    assert_eq!(observation.critical_point(1.0, -1.0), (0.0, 0.0));
    assert_eq!((observation.width(), observation.height()), (0.0, 0.0));
    observation.shift(3.0, 4.0).unwrap();
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
}

#[test]
fn object_next_to_and_frame_corner_use_shared_bounds_and_buffers() {
    let scene = Scene::new();
    let mut left = scene.circle(1.0).unwrap();
    left.shift(-2.0, 0.0).unwrap();
    let mut right = scene.square(1.0).unwrap();
    let buffer = f64::from(noon_core::DEFAULT_MOBJECT_TO_MOBJECT_BUFFER);
    right.next_to_handle(&left, 1.0, 0.0, buffer).unwrap();
    let gap = right.critical_point(-1.0, 0.0).unwrap().0 - left.critical_point(1.0, 0.0).unwrap().0;
    assert!((gap - buffer).abs() < 1e-6);

    let buffer = f64::from(noon_core::DEFAULT_MOBJECT_TO_EDGE_BUFFER);
    right.align_on_frame(1.0, 1.0, buffer).unwrap();
    let corner = right.critical_point(1.0, 1.0).unwrap();
    assert!((corner.0 - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - buffer)).abs() < 1e-5);
    assert!((corner.1 - (f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - buffer)).abs() < 1e-5);
}

#[test]
fn object_placement_to_family_anchor_is_shared_and_rejects_foreign_targets() {
    let scene = Scene::new();
    let object = scene.square(1.0).unwrap();
    let mut reference = scene.square(2.0).unwrap();
    reference.shift(5.0, 3.0).unwrap();
    let family = scene.family(&[(&reference).into()]).unwrap();
    let source = noon::LayoutAnchor::from(&object);
    let target = noon::LayoutAnchor::from(&family);
    source
        .layout()
        .unwrap()
        .move_to(Target::Anchor(&target), (0.0, 1.0), (0.5, 1.0))
        .unwrap();
    assert_eq!(object.center().unwrap(), (2.5, 3.5));
    source
        .layout()
        .unwrap()
        .align_to(Target::Anchor(&target), (0.0, -1.0))
        .unwrap();
    assert_eq!(object.center().unwrap(), (2.5, 2.5));
    let other = Scene::new();
    let foreign = noon::LayoutAnchor::from(&other.family(&[]).unwrap());
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(source
        .layout()
        .unwrap()
        .move_to(Target::Anchor(&foreign), (0.0, 0.0), (1.0, 1.0))
        .is_err());
    assert!(source
        .layout()
        .unwrap()
        .align_to(Target::Anchor(&foreign), (1.0, 1.0))
        .is_err());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(object.center().unwrap(), (2.5, 2.5));
}
