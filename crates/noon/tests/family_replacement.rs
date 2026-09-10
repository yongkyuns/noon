use noon::{
    LayoutAnchor,
    LayoutDimension::{Height, Width},
    Scene,
};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn replacement_preserves_nested_alias_identity_content_and_unrelated_state() {
    let scene = Scene::new();
    let mut first = scene.rectangle(2.0, 1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    first.shift(-2.0, 0.0).unwrap();
    second.shift(2.0, 0.0).unwrap();
    let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let family = scene.family(&[(&first).into(), (&nested).into()]).unwrap();
    let mut target = scene.rectangle(11.0, 4.0).unwrap();
    target.shift(3.0, -1.0).unwrap();
    let unrelated = scene.circle(0.5).unwrap();
    let previous = [first.state().unwrap(), second.state().unwrap()];
    let unaffected = [target.state().unwrap(), unrelated.state().unwrap()];
    let members = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .unwrap();
    let revision = scene.revision();
    LayoutAnchor::from(&family)
        .replace_layout(&(&target).into(), Width, true)
        .unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    close(family.layout().unwrap().width(), 11.0);
    close(family.layout().unwrap().height(), 4.0);
    assert_eq!(family.layout().unwrap().center(), target.center().unwrap());
    close(first.width().unwrap(), 4.0);
    close(second.width().unwrap(), 2.0);
    for (object, old) in [(&first, &previous[0]), (&second, &previous[1])] {
        assert_eq!(object.state().unwrap().content, old.content);
        assert_eq!(object.state().unwrap().style, old.style);
    }
    assert_eq!(
        [target.state().unwrap(), unrelated.state().unwrap()],
        unaffected
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .unwrap(),
        members
    );
}

#[test]
fn shared_target_leaf_observes_staged_scaling_before_the_center_shift() {
    let scene = Scene::new();
    let mut first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    first.shift(-1.0, 0.0).unwrap();
    second.shift(1.0, 0.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    LayoutAnchor::from(&family)
        .replace_layout(&(&first).into(), Width, false)
        .unwrap();
    close(first.center().unwrap().0, -2.0 / 3.0);
    close(second.center().unwrap().0, 0.0);
    close(family.layout().unwrap().center().0, -1.0 / 3.0);
    close(family.layout().unwrap().width(), 1.0);
}

#[test]
fn object_can_replace_a_family_and_zero_source_extent_does_not_divide_by_zero() {
    let scene = Scene::new();
    let mut target = scene.rectangle(4.0, 6.0).unwrap();
    target.shift(3.0, 2.0).unwrap();
    let family = scene.family(&[(&target).into()]).unwrap();
    let mut source = scene.line((0.0, 0.0), (0.0, 2.0)).unwrap();
    LayoutAnchor::from(&source)
        .replace_layout(&(&family).into(), Width, true)
        .unwrap();
    close(source.width().unwrap(), 0.0);
    close(source.height().unwrap(), 6.0);
    assert_eq!(source.center().unwrap(), target.center().unwrap());
    // The ordinary Rust object API delegates to the same implementation.
    source.replace_handle(&target, 0, false).unwrap();
    close(source.height().unwrap(), 6.0);
}

#[test]
fn invalid_targets_fail_atomically_and_rotated_family_replacement_succeeds() {
    let scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.rotate(0.3).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let empty = scene.family(&[]).unwrap();
    let foreign = Scene::new().square(2.0).unwrap();
    let target = scene.rectangle(4.0, 1.0).unwrap();
    let before = [first.state().unwrap(), second.state().unwrap()];
    let revision = scene.revision();
    for target in [LayoutAnchor::from(&empty), (&foreign).into()] {
        assert!(LayoutAnchor::from(&family)
            .replace_layout(&target, Height, true)
            .is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!([first.state().unwrap(), second.state().unwrap()], before);
    }
    // Empty sources and replacing an object with itself do not publish a revision.
    LayoutAnchor::from(&empty)
        .replace_layout(&(&target).into(), Width, false)
        .unwrap();
    LayoutAnchor::from(&first)
        .replace_layout(&(&first).into(), Width, false)
        .unwrap();
    assert_eq!(scene.revision(), revision);
    LayoutAnchor::from(&family)
        .replace_layout(&(&target).into(), Height, true)
        .unwrap();
    close(family.layout().unwrap().width(), 4.0);
    close(family.layout().unwrap().height(), 1.0);
}

#[test]
fn live_replacement_uses_completed_effective_bounds_and_preserves_other_objects() {
    let mut scene = Scene::new();
    let source = scene.square(1.0).unwrap();
    let target = scene.square(1.0).unwrap();
    let family = scene.family(&[(&source).into()]).unwrap();
    let unrelated = scene.circle(0.5).unwrap();
    scene
        .add_many(&[(&family).into(), (&target).into(), (&unrelated).into()])
        .unwrap();
    let mut destination = target.target_editor().unwrap();
    destination.scale(3.0, 3.0).unwrap();
    destination.shift(2.0, 1.0).unwrap();
    let animation = scene
        .declare_transform_to(
            &target,
            &destination,
            noon::AnimationOptions::new()
                .run_time(2.0)
                .rate_func(noon::RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    let before = live.effective(&target).unwrap();
    assert!(live
        .replace_layout(&(&target).into(), &(&family).into(), Width, false)
        .is_err());
    assert_eq!(live.effective(&target).unwrap(), before);
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    let before_unrelated = live.effective(&unrelated).unwrap();
    live.replace_layout(&(&family).into(), &(&target).into(), Width, true)
        .unwrap();
    let source_bounds = live.effective_family_layout(&family).unwrap();
    let target_bounds = live.effective_layout(&target).unwrap();
    close(source_bounds.width, target_bounds.width);
    close(source_bounds.height, target_bounds.height);
    assert_eq!(source_bounds.center, target_bounds.center);
    let after = live.effective(&unrelated).unwrap();
    assert_eq!(after.transform, before_unrelated.transform);
    assert_eq!(after.style, before_unrelated.style);
    assert_eq!(after.appearance, before_unrelated.appearance);
    assert_eq!(after.publication, source_bounds.publication);
}

#[test]
fn paired_example_builds_the_normal_shared_execution_session() {
    noon::example_scenes::family_replacement::session().unwrap();
}
