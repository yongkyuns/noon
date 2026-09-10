use noon::{ManimBecomeOptions, MobjectFamily, Scene};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

fn leaves(family: &MobjectFamily) -> Vec<noon::SemanticNodeId> {
    family
        .integration_store()
        .borrow()
        .ordered_leaf_nodes(family.node_id())
        .unwrap()
}

fn family(scene: &Scene) -> MobjectFamily {
    let a = scene.square(1.0).unwrap();
    let mut b = scene.rectangle(1.0, 2.0).unwrap();
    b.shift(2.0, 0.0).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    scene.family(&[(&a).into(), (&nested).into()]).unwrap()
}

#[test]
fn family_become_and_restore_preserve_alias_identity_and_share_content() {
    let scene = Scene::new();
    let source = family(&scene);
    let saved = source.copy_family().unwrap();
    let target = source.copy_family().unwrap();
    target.root().shift(4.0, -2.0).unwrap();
    target.root().scale(2.0, 2.0).unwrap();
    let members = leaves(&source);
    let untouched = saved.root().layout_bounds().unwrap();
    let revision = scene.revision();
    source
        .become_family(target.root(), Default::default())
        .unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(
        source.layout_bounds().unwrap(),
        target.root().layout_bounds().unwrap()
    );
    assert_eq!(leaves(&source), members);
    assert_eq!(saved.root().layout_bounds().unwrap(), untouched);
    for (&source, &target) in members.iter().zip(&leaves(target.root())) {
        let store = scene.integration_store().borrow();
        assert_eq!(
            store.semantic_object_state_checked(source).unwrap().content,
            store.semantic_object_state_checked(target).unwrap().content
        );
    }
    source
        .become_family(saved.root(), Default::default())
        .unwrap();
    assert_eq!(source.layout_bounds().unwrap(), untouched);
    let revision = scene.revision();
    source.become_family(&source, Default::default()).unwrap();
    assert_eq!(scene.revision(), revision);
}

#[test]
fn dimension_matching_uses_aggregate_bounds_and_center_not_individual_leaf_sizes() {
    let scene = Scene::new();
    let source = family(&scene);
    let target = family(&scene);
    target.scale(2.0, 3.0).unwrap();
    target.shift(5.0, 3.0).unwrap();
    let before = source.layout().unwrap();
    let target_before = target.layout_bounds().unwrap();
    source
        .become_family(
            &target,
            ManimBecomeOptions {
                stretch: true,
                match_center: true,
                ..Default::default()
            },
        )
        .unwrap();
    let after = source.layout().unwrap();
    close(after.width(), before.width());
    close(after.height(), before.height());
    assert_eq!(after.center(), before.center());
    assert_eq!(target.layout_bounds().unwrap(), target_before);
    source
        .become_family(
            &target,
            ManimBecomeOptions {
                match_height: true,
                match_width: true,
                ..Default::default()
            },
        )
        .unwrap();
    close(source.layout().unwrap().width(), before.width());
    close(source.layout().unwrap().height(), 3.0);
}

#[test]
fn invalid_family_become_is_atomic_and_rotated_stretch_preserves_dimensions() {
    let scene = Scene::new();
    let source = family(&scene);
    let different = scene.family(&[]).unwrap();
    let foreign = family(&Scene::new());
    let saved = source.layout_bounds().unwrap();
    let revision = scene.revision();
    for target in [&different, &foreign] {
        assert!(source.become_family(target, Default::default()).is_err());
        assert_eq!(source.layout_bounds().unwrap(), saved);
        assert_eq!(scene.revision(), revision);
    }
    let target = family(&scene);
    target
        .rotate(0.3, noon::ManimRotationPivot::Center)
        .unwrap();
    source
        .become_family(
            &target,
            ManimBecomeOptions {
                stretch: true,
                match_center: true,
                ..Default::default()
            },
        )
        .unwrap();
    let after = source.layout_bounds().unwrap().unwrap();
    let before = saved.unwrap();
    close(after.width(), before.width());
    close(after.height(), before.height());
    different
        .become_family(&different, Default::default())
        .unwrap();
}

#[test]
fn live_capture_restores_current_completed_state_in_one_publication() {
    let mut scene = Scene::new();
    let source = family(&scene);
    let target = source.copy_family().unwrap();
    target.root().shift(3.0, 0.0).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live
        .declare_and_activate_family_transform_to(
            &source,
            target.root(),
            noon::AnimationOptions::new()
                .run_time(1.0)
                .rate_func(noon::RateFunction::Linear),
        )
        .unwrap();
    live.advance_segment_to(segment, 0.5).unwrap();
    assert!(live
        .become_family(&source, target.root(), Default::default())
        .is_err());
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    let saved = live.copy_family(&source).unwrap();
    live.shift_family(&source, -5.0, 0.0).unwrap();
    live.become_family(&source, saved.root(), Default::default())
        .unwrap();
    let layout = live.effective_family_layout(&source).unwrap();
    assert_eq!(layout.center, saved.root().layout().unwrap().center());
}

#[test]
fn paired_program_animates_and_restores_through_coherent_completion() {
    use noon::{LiveProgramStatus, RustHostCallbackTable};
    let mut program = noon::example_scenes::family_state::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    let center = |program: &noon::LiveProgram<noon::example_scenes::family_state::FamilyState>| {
        let frame = program.session().frame();
        frame
            .render_geometry(0)
            .unwrap()
            .world_bounds(frame.render_transform(0))
            .unwrap()
            .center()
    };
    let initial = center(&program);
    program.drive_to(&mut callbacks, 0.2).unwrap();
    let midpoint = center(&program);
    close(f64::from(midpoint.x - initial.x), 0.5);
    close(f64::from(midpoint.y - initial.y), 0.5);
    for time in [0.4, 0.6000000000000001] {
        assert!(matches!(
            program.drive_to(&mut callbacks, time).unwrap(),
            LiveProgramStatus::PublicationPending(_)
        ));
        let context = program.take_renderer_publication().context();
        program.admit_publication(context).unwrap();
        program.resume().unwrap();
    }
    close(f64::from(center(&program).x), -2.0);
    close(f64::from(center(&program).y), 0.0);
}

#[test]
fn pairing_memoizes_shared_family_dags_and_rejects_different_family_aliases() {
    let scene = Scene::new();
    let a = scene.square(1.0).unwrap();
    let mut root = scene.family(&[(&a).into()]).unwrap();
    for _ in 0..20 {
        let left = scene.family(&[(&root).into()]).unwrap();
        let right = scene.family(&[(&root).into()]).unwrap();
        root = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    }
    let copy = root.copy_family().unwrap();
    root.become_family(copy.root(), Default::default()).unwrap();
    assert_eq!(leaves(&root), [a.node_id()]);

    let shared = scene.family(&[(&a).into()]).unwrap();
    let left = scene.family(&[(&shared).into()]).unwrap();
    let right = scene.family(&[(&shared).into()]).unwrap();
    let source = scene.family(&[(&left).into(), (&right).into()]).unwrap();
    let target = source.copy_family().unwrap();
    let copied_right = target.family(&right).unwrap();
    let copied_shared = target.family(&shared).unwrap();
    let distinct = scene
        .family(&[(&target.mobject(&a).unwrap()).into()])
        .unwrap();
    copied_right.remove((&copied_shared).into()).unwrap();
    copied_right.add((&distinct).into()).unwrap();
    assert!(source
        .become_family(target.root(), Default::default())
        .is_err());
}

#[test]
fn cross_alias_targets_observe_staged_writes_unless_matching_captures_a_copy() {
    for match_center in [false, true] {
        let scene = Scene::new();
        let mut a = scene.square(1.0).unwrap();
        let mut b = scene.square(1.0).unwrap();
        a.shift(-1.0, 0.0).unwrap();
        b.shift(1.0, 0.0).unwrap();
        let source = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        let target = scene.family(&[(&b).into(), (&a).into()]).unwrap();
        source
            .become_family(
                &target,
                ManimBecomeOptions {
                    match_center,
                    ..Default::default()
                },
            )
            .unwrap();
        close(a.center().unwrap().0, 1.0);
        close(b.center().unwrap().0, if match_center { -1.0 } else { 1.0 });
    }
}
