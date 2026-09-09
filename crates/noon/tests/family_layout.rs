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
