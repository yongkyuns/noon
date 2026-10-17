use noon::{FamilyLayoutTarget as Target, ManimNextToArgs, Scene};

#[test]
fn placement_shares_object_family_and_point_targets_with_masks_and_nonunit_directions() {
    let mut scene = Scene::new();
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
    let mut scene = Scene::new();
    let object = scene.square(1.0).unwrap();
    let family = scene.family(&[(&object).into()]).unwrap();
    let mut other_scene = Scene::new();
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
    let mut scene = Scene::new();
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
    let mut scene = Scene::new();
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
    let mut scene = Scene::new();
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
    let mut other = Scene::new();
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

#[test]
fn frame_alignment_deduplicates_nested_aliases_and_rejects_invalid_input_atomically() {
    let mut scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.shift(2.0, 0.0).unwrap();
    let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let family = scene.family(&[(&first).into(), (&nested).into()]).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    family
        .layout()
        .unwrap()
        .align_on_frame((2.0, -1.0), 0.25)
        .unwrap();
    let bounds = family.layout().unwrap().bounds().unwrap();
    assert!((bounds.max_x - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.5)).abs() < 1e-6);
    assert!((bounds.min_y + f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - 0.25).abs() < 1e-6);
    assert!((second.center().unwrap().0 - first.center().unwrap().0 - 2.0).abs() < 1e-6);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );

    let revision = scene.integration_store().borrow().scene_revision();
    for (direction, buff) in [((f64::NAN, 1.0), 0.0), ((1.0, 0.0), f64::INFINITY)] {
        assert!(family
            .layout()
            .unwrap()
            .align_on_frame(direction, buff)
            .is_err());
    }
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(family.layout().unwrap().bounds().unwrap(), bounds);

    let y = first.center().unwrap().1;
    family
        .layout()
        .unwrap()
        .align_on_frame((-1.0, 0.0), 0.5)
        .unwrap();
    assert_eq!(first.center().unwrap().1, y);
    assert!(
        (family.layout().unwrap().bounds().unwrap().min_x
            + f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5
            - 0.5)
            .abs()
            < 1e-6
    );
}

#[test]
fn paired_frame_placement_example_executes_the_typed_rust_path() {
    noon::example_scenes::family_placement::session().unwrap();
}

#[test]
fn edge_placement_fits_the_default_frame_and_translates_aliases_once() {
    let mut scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.shift(1.0, 0.0).unwrap();
    let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let label = scene.family(&[(&first).into(), (&nested).into()]).unwrap();
    let mut target = scene.square(2.0).unwrap();
    target.shift(7.0, 4.0).unwrap();
    let revision = scene.revision();
    noon::LayoutAnchor::from(&label)
        .next_to_edge_in_frame(
            &noon::LayoutAnchor::from(&target),
            (1.0, 1.0),
            ManimNextToArgs {
                direction: (2.0, 1.0),
                buff: 0.1,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            },
            0.25,
        )
        .unwrap();
    let corner = label.layout().unwrap().critical_point(1.0, 1.0);
    assert!((corner.0 - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.25)).abs() < 1e-6);
    assert!((corner.1 - (f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - 0.25)).abs() < 1e-6);
    assert!((second.center().unwrap().0 - first.center().unwrap().0 - 1.0).abs() < 1e-6);
    assert_eq!(target.center().unwrap(), (7.0, 4.0));
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
}

#[test]
fn oversized_frame_fitting_keeps_manim_cardinal_order_and_rejects_invalid_inputs_atomically() {
    let mut scene = Scene::new();
    let label = scene.square(20.0).unwrap();
    let target = scene.square(1.0).unwrap();
    let source = noon::LayoutAnchor::from(&label);
    let target = noon::LayoutAnchor::from(&target);
    let args = ManimNextToArgs {
        direction: (1.0, 1.0),
        buff: 0.1,
        aligned_edge: (0.0, 0.0),
        mask: (1.0, 1.0),
    };
    source
        .next_to_edge_in_frame(&target, (1.0, 1.0), args, 0.25)
        .unwrap();
    let bounds = source.layout().unwrap();
    assert!(
        (bounds.critical_point(1.0, 0.0).0
            - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.25))
            .abs()
            < 1e-6
    );
    assert!(
        (bounds.critical_point(0.0, -1.0).1 + f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5
            - 0.25)
            .abs()
            < 1e-6
    );
    let revision = scene.revision();
    let center = label.center().unwrap();
    for (edge, buff) in [
        ((f64::NAN, 0.0), 0.25),
        ((1.0, 1.0), f64::INFINITY),
        ((1.0, 1.0), -0.25),
    ] {
        assert!(source
            .next_to_edge_in_frame(&target, edge, args, buff)
            .is_err());
    }
    let mut other = Scene::new();
    let foreign = other.square(1.0).unwrap();
    assert!(source
        .next_to_edge_in_frame(&noon::LayoutAnchor::from(&foreign), (1.0, 1.0), args, 0.25)
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(label.center().unwrap(), center);
}

#[test]
fn live_edge_placement_observes_effective_target_and_publishes_once() {
    let mut scene = Scene::new();
    let label = scene.square(1.0).unwrap();
    let target = scene.square(2.0).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    let position = scene
        .position_from_tracker(
            &tracker,
            noon::SemanticVec3::new(1.0, 0.0, 0.0),
            noon::SemanticVec3::new(5.0, 0.0, 0.0),
        )
        .unwrap();
    scene.bind_position(&target, &position).unwrap();
    scene.add(&label).unwrap();
    scene.add(&target).unwrap();
    let source = noon::LayoutAnchor::from(&label);
    let target_anchor = noon::LayoutAnchor::from(&target);
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    live.set_value(&tracker, 2.0).unwrap();
    let revision = scene.revision();
    let result = live
        .next_layout_to_edge_in_frame(
            &source,
            &target_anchor,
            (1.0, 0.0),
            ManimNextToArgs {
                direction: (1.0, 0.0),
                buff: 0.2,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 1.0),
            },
            0.25,
        )
        .unwrap();
    assert_eq!(result.impacts().len(), 1);
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    let layout = live.effective_layout(&label).unwrap();
    assert!(
        (layout.center.0 + layout.width * 0.5
            - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.25))
            .abs()
            < 1e-6
    );
    assert_eq!(target.center().unwrap(), (0.0, 0.0));
    assert_eq!(live.effective_layout(&target).unwrap().center, (7.0, 0.0));
}
