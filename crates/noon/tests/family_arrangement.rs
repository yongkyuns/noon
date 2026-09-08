use noon::{FamilyArrangeOptions, LayoutAnchor, Scene};

fn close(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        (actual.0 - expected.0).abs() < 1e-6 && (actual.1 - expected.1).abs() < 1e-6,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn arrangement_forwards_edge_direction_and_partial_coordinate_mask() {
    let scene = Scene::new();
    let mut first = scene.square(2.0).unwrap();
    first.shift(0.0, 2.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.shift(5.0, -3.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let mut options = FamilyArrangeOptions::new(2.0, 0.0, 0.25, false);
    options.placement.aligned_edge = (0.0, 1.0);
    options.placement.mask = (1.0, 0.5);
    family.arrange_with_options(&options).unwrap();
    close(first.center().unwrap(), (0.0, 2.0));
    close(second.center().unwrap(), (2.0, -0.25));
}

#[test]
fn aliased_members_observe_preceding_moves_and_center_unique_leaves_once() {
    // Same semantic object belongs to two direct members. Manim's arrange
    // observes each preceding move, then centers its deduplicated family.
    let scene = Scene::new();
    let first = scene.square(0.4).unwrap();
    let mut second = scene.square(0.4).unwrap();
    second.shift(2.0, 0.0).unwrap();
    let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let family = scene.family(&[(&first).into(), (&nested).into()]).unwrap();
    let before = scene.store().borrow().scene_revision();
    family.arrange(1.0, 0.0, 0.2, true).unwrap();
    close(first.center().unwrap(), (-1.0, 0.0));
    close(second.center().unwrap(), (1.0, 0.0));
    close(family.layout().unwrap().center(), (0.0, 0.0));
    assert_eq!(
        scene.store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    family.shift(0.25, 0.5).unwrap();
    close(first.center().unwrap(), (-0.75, 0.5));
    close(second.center().unwrap(), (1.25, 0.5));
}

#[test]
fn selected_member_options_are_shared_by_authored_and_live_arrangement() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let mut second = scene.square(1.0).unwrap();
        second.shift(2.0, 0.0).unwrap();
        let mut third = scene.square(1.0).unwrap();
        third.shift(8.0, 0.0).unwrap();
        let mut fourth = scene.square(1.0).unwrap();
        fourth.shift(12.0, 0.0).unwrap();
        let left = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let right = scene.family(&[(&third).into(), (&fourth).into()]).unwrap();
        let outer = scene.family(&[(&left).into(), (&right).into()]).unwrap();
        let mut options = FamilyArrangeOptions::new(1.0, 0.0, 0.25, false);
        options.member_index = Some(-1);
        // Source override wins over source indexing; target indexing still applies.
        options.aligner = Some(LayoutAnchor::from(&third));
        if live_mode {
            for object in [&first, &second, &third, &fourth] {
                scene.add(object).unwrap();
            }
            let mut execution = scene.execution_session().unwrap();
            let mut live = scene.live(&mut execution);
            let result = live.arrange_family_with_options(&outer, &options).unwrap();
            assert_eq!(result.impacts().len(), 2);
            close(live.effective_layout(&third).unwrap().center, (3.25, 0.0));
            close(live.effective_layout(&fourth).unwrap().center, (7.25, 0.0));
        } else {
            outer.arrange_with_options(&options).unwrap();
            close(third.center().unwrap(), (3.25, 0.0));
            close(fourth.center().unwrap(), (7.25, 0.0));
        }
    }
}

#[test]
fn late_invalid_selection_and_foreign_aligner_never_publish_a_prefix() {
    let mut scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let second = scene.square(1.0).unwrap();
    let first_family = scene.family(&[(&first).into()]).unwrap();
    let second_family = scene.family(&[(&second).into()]).unwrap();
    let empty = scene.family(&[]).unwrap();
    let family = scene
        .family(&[
            (&first_family).into(),
            (&second_family).into(),
            (&empty).into(),
        ])
        .unwrap();
    let mut options = FamilyArrangeOptions::new(1.0, 0.0, 0.25, false);
    options.member_index = Some(0);
    let before = scene.store().borrow().scene_revision();
    assert!(family.arrange_with_options(&options).is_err());
    assert_eq!(scene.store().borrow().scene_revision(), before);
    close(second.center().unwrap(), (0.0, 0.0));
    for object in [&first, &second] {
        scene.add(object).unwrap();
    }
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    let before = scene.store().borrow().scene_revision();
    assert!(live.arrange_family_with_options(&family, &options).is_err());
    assert_eq!(scene.store().borrow().scene_revision(), before);
    close(live.effective_layout(&second).unwrap().center, (0.0, 0.0));
    let other = Scene::new();
    options.member_index = None;
    options.aligner = Some(LayoutAnchor::from(&other.square(1.0).unwrap()));
    assert!(live.arrange_family_with_options(&family, &options).is_err());
    assert_eq!(scene.store().borrow().scene_revision(), before);
}
