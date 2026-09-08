use noon::semantic_mobject::ManimNextToArgs;
use noon::{AnimationOptions, LiveLayoutTarget as Target, RateFunction, Scene};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn family_queries_mix_effective_and_detached_bounds_without_mutation() {
    let mut scene = Scene::new();
    let animated = scene.square(1.0).unwrap();
    let mut detached = scene.circle(0.5).unwrap();
    detached.shift(10.0, 0.0).unwrap();
    let nested = scene
        .family(&[(&animated).into(), (&detached).into()])
        .unwrap();
    let aliases = scene
        .family(&[(&animated).into(), (&nested).into()])
        .unwrap();
    let empty = scene.family(&[]).unwrap();
    let mut target = animated.target_editor().unwrap();
    target.shift(4.0, 0.0).unwrap();
    scene.add(&animated).unwrap();
    let animation = scene
        .declare_transform_to(
            &animated,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    let revision = scene.store().borrow().scene_revision();
    let halfway = live.effective_family_layout(&aliases).unwrap();
    close(halfway.center.0, 6.0);
    close(halfway.width, 9.0);
    close(aliases.layout().unwrap().center().0, 5.0);
    assert_eq!(
        live.effective_family_layout(&empty).unwrap().center,
        (0.0, 0.0)
    );
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    assert!(live
        .move_family_to(&aliases, Target::Point(0.0, 0.0), (0.0, 0.0), (1.0, 1.0))
        .is_err());
    assert_eq!(live.effective_family_layout(&aliases).unwrap(), halfway);
    live.advance_segment_to(segment, 2.0).unwrap();
    live.complete_segment(segment).unwrap();
    close(
        live.effective_family_layout(&aliases).unwrap().center.0,
        7.0,
    );
    close(halfway.center.0, 6.0); // Retained query results never become mutable state.
}

#[test]
fn relative_placement_uses_shared_masks_targets_and_one_local_publication() {
    let mut scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.shift(2.0, 0.0).unwrap();
    let mut anchor = scene.square(2.0).unwrap();
    anchor.shift(6.0, 3.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let target = scene.family(&[(&anchor).into()]).unwrap();
    for object in [&first, &second, &anchor] {
        scene.add(object).unwrap();
    }
    let mut execution = scene.execution_session().unwrap();
    execution.take_frame_changes();
    {
        let mut live = scene.live(&mut execution);
        let before = scene.store().borrow().scene_revision();
        let result = live
            .move_family_to(&family, Target::Point(4.0, 9.0), (1.0, 0.0), (1.0, 0.0))
            .unwrap();
        assert_eq!(result.impacts().len(), 2);
        assert_eq!(
            scene.store().borrow().scene_revision(),
            before.checked_next().unwrap()
        );
        live.next_family_to(
            &family,
            Target::Mobject(&anchor),
            ManimNextToArgs {
                direction: (2.0, 0.0),
                buff: 0.25,
                aligned_edge: (0.0, 0.0),
                mask: (1.0, 0.0),
            },
        )
        .unwrap();
        live.align_family_to(&family, Target::Family(&target), (0.0, 1.0))
            .unwrap();
        let layout = live.effective_family_layout(&family).unwrap();
        close(layout.center.0, 9.0);
        close(layout.center.1, 3.5);
        assert_eq!(anchor.center().unwrap(), (6.0, 3.0));
    }
    assert_eq!(execution.take_frame_changes().object_indices(), &[0, 1]);
}

#[test]
fn empty_foreign_stale_and_unpublished_families_obey_query_and_mutation_validation() {
    let scene = Scene::new();
    let empty = scene.family(&[]).unwrap();
    let other = Scene::new();
    let foreign = other.family(&[]).unwrap();
    let stale = scene.family(&[]).unwrap();
    scene
        .store()
        .borrow_mut()
        .remove_node(stale.node_id())
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    let before = live.effective_family_layout(&empty).unwrap();
    for invalid in [&foreign, &stale] {
        assert!(live.effective_family_layout(invalid).is_err());
        assert!(live
            .align_family_to(&empty, Target::Family(invalid), (1.0, 0.0))
            .is_err());
    }
    live.move_family_to(&empty, Target::Point(2.0, 3.0), (0.0, 0.0), (1.0, 1.0))
        .unwrap();
    assert_eq!(live.effective_family_layout(&empty).unwrap(), before);
    let _unpublished = scene.family(&[]).unwrap();
    assert!(live.effective_family_layout(&empty).is_err());
}
