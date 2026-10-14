use noon::{Color, Mobject, MobjectFamily, Scene};

fn family() -> (Scene, MobjectFamily, [Mobject; 2], Mobject) {
    let mut scene = Scene::new();
    let a = scene.square(0.5).unwrap();
    let b = scene.circle(0.4).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    let unrelated = scene.square(0.2).unwrap();
    (scene, family, [a, b], unrelated)
}

#[test]
fn family_paint_matches_leaf_semantics_in_one_revision_and_leaves_other_objects_alone() {
    let (mut scene, family, [a, b], unrelated) = family();
    let untouched = unrelated.state().unwrap();
    let mut reference = scene.square(0.5).unwrap();
    let before = scene.revision();
    family
        .set_fill(Some(Color::rgba(1.0, 0.0, 0.0, 1.0)), Some(0.3))
        .unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    reference.set_fill(1.0, 0.0, 0.0, 0.3).unwrap();
    family
        .set_stroke(Some(Color::rgba(0.0, 0.0, 1.0, 1.0)), Some(0.1), Some(0.6))
        .unwrap();
    reference.set_stroke_color(0.0, 0.0, 1.0, 1.0).unwrap();
    reference.set_stroke_width(0.1).unwrap();
    reference.set_stroke_opacity(0.6).unwrap();
    family.set_color(0.2, 0.4, 0.8, 1.0).unwrap();
    reference.set_color(0.2, 0.4, 0.8, 1.0).unwrap();
    family.set_opacity(0.4).unwrap();
    reference.set_opacity(0.4).unwrap();
    for member in [&a, &b] {
        assert_eq!(
            member.state().unwrap().style,
            reference.state().unwrap().style
        );
    }
    assert_eq!(unrelated.state().unwrap(), untouched);
}

#[test]
fn invalid_combined_edit_changes_nothing_and_empty_families_still_validate() {
    let (mut scene, family, [a, b], _) = family();
    let before = scene.revision();
    let states = [a.state().unwrap(), b.state().unwrap()];
    assert!(family
        .set_stroke(Some(Color::rgba(0.0, 0.0, 1.0, 1.0)), Some(-1.0), Some(0.4))
        .is_err());
    assert!(family
        .set_fill(Some(Color::rgba(1.0, 0.0, 0.0, 1.0)), Some(f64::NAN))
        .is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!([a.state().unwrap(), b.state().unwrap()], states);
    let empty = scene.family(&[]).unwrap();
    assert!(empty.set_opacity(2.0).is_err());
}

#[test]
fn live_paint_batches_unique_leaves_and_rejects_foreign_or_invalid_edits_atomically() {
    let (mut scene, family, [a, b], _) = family();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let result = live
        .set_family_fill(&family, Some(Color::rgba(1.0, 0.0, 0.0, 1.0)), Some(0.5))
        .unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(live.effective(&a).unwrap().fill_opacity(), 0.5);
    assert_eq!(live.effective(&b).unwrap().fill_opacity(), 0.5);
    live.set_family_stroke(
        &family,
        Some(Color::rgba(0.0, 0.0, 1.0, 1.0)),
        Some(0.1),
        Some(0.25),
    )
    .unwrap();
    let before = [live.effective(&a).unwrap(), live.effective(&b).unwrap()];
    assert!(live
        .set_family_stroke(
            &family,
            Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
            Some(-1.0),
            None
        )
        .is_err());
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.family(&[]).unwrap();
    assert!(live.set_family_opacity(&foreign, 0.2).is_err());
    assert_eq!(
        [live.effective(&a).unwrap(), live.effective(&b).unwrap()],
        before
    );
    live.set_family_fill(&family, None, None).unwrap();
    live.set_family_opacity(&family, 0.5).unwrap();
    assert_eq!(live.effective(&a).unwrap().fill_opacity(), 0.5);
    assert_eq!(live.effective(&a).unwrap().stroke_opacity(), 0.5);
}

#[test]
fn live_recursive_alias_fade_publishes_one_atomic_style_only_update() {
    let (mut scene, family, [mut a, mut b], _) = family();
    a.set_fill(1.0, 0.0, 0.0, 0.8).unwrap();
    a.set_stroke_color(0.0, 0.0, 1.0, 1.0).unwrap();
    a.set_stroke_opacity(0.6).unwrap();
    a.set_object_opacity(0.7).unwrap();
    b.set_fill(0.0, 1.0, 0.0, 0.5).unwrap();
    let a_content = a.state().unwrap().content;
    let b_content = b.state().unwrap().content;
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let before_revision = scene.revision();
    let result = live.fade_family(&family, 0.25).unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(scene.revision(), before_revision.checked_next().unwrap());
    let faded_a = live.effective(&a).unwrap();
    let faded_b = live.effective(&b).unwrap();
    // Pinned Manim recursively visits a through both nested and direct paths:
    // its paint is multiplied by 0.75 twice, while b is visited once.
    assert!((faded_a.fill_opacity() - 0.45).abs() < 1e-7);
    assert!((faded_a.stroke_opacity() - 0.3375).abs() < 1e-7);
    assert_eq!(faded_a.style.opacity, 0.7_f32);
    assert!((faded_b.fill_opacity() - 0.375).abs() < 1e-7);
    assert_eq!(a.state().unwrap().content, a_content);
    assert_eq!(b.state().unwrap().content, b_content);

    let before_invalid = [faded_a, faded_b];
    let revision = scene.revision();
    assert!(live.fade_family(&family, 1.1).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        [live.effective(&a).unwrap(), live.effective(&b).unwrap()],
        before_invalid
    );
}

#[test]
fn combined_style_is_atomic_and_omission_is_a_noop() {
    use noon::StyleUpdate;
    let (scene, family, [mut a, b], unrelated) = family();
    let untouched = unrelated.state().unwrap();
    let before = scene.revision();
    let update = StyleUpdate {
        fill_color: Some(Color::RED),
        fill_opacity: Some(0.4),
        stroke_color: Some(Color::BLUE),
        stroke_width: Some(0.08),
        stroke_opacity: Some(0.7),
    };
    family.set_style(update).unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(a.state().unwrap().style, b.state().unwrap().style);
    assert_eq!(unrelated.state().unwrap(), untouched);
    let before = scene.revision();
    family.set_fill(None, None).unwrap();
    family.set_stroke(None, None, None).unwrap();
    family.set_style(Default::default()).unwrap();
    a.set_style(Default::default()).unwrap();
    a.match_style(&a).unwrap();
    family.match_style(&family).unwrap();
    assert_eq!(scene.revision(), before);
    let old = a.state().unwrap();
    let invalid = StyleUpdate {
        fill_color: Some(Color::GREEN),
        stroke_width: Some(-1.0),
        ..Default::default()
    };
    assert!(family.set_style(invalid).is_err());
    assert!(a.set_style(invalid).is_err());
    assert_eq!(a.state().unwrap(), old);
    assert_eq!(scene.revision(), before);
}

#[test]
fn style_matching_preserves_source_identity_geometry_and_nonpaint_fields() {
    use noon::StyleUpdate;
    let (mut scene, source, [a, b], _) = family();
    let target = source.copy_family().unwrap().root().clone();
    target
        .set_style(StyleUpdate {
            fill_color: Some(Color::BLUE),
            fill_opacity: Some(0.2),
            stroke_width: Some(0.15),
            ..Default::default()
        })
        .unwrap();
    let a_before = a.state().unwrap();
    let b_before = b.state().unwrap();
    let revision = scene.revision();
    source.match_style(&target).unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    for (leaf, before) in [(&a, a_before), (&b, b_before)] {
        let after = leaf.state().unwrap();
        assert_eq!(after.content, before.content);
        assert_eq!(after.transform, before.transform);
        assert_eq!(after.presentation(), before.presentation());
        assert_eq!(after.style.object_opacity, before.style.object_opacity);
        assert_eq!(
            after.style.stroke_width_mode,
            before.style.stroke_width_mode
        );
        assert_eq!(after.style.stroke_join, before.style.stroke_join);
        assert_eq!(after.style.fill_opacity, 0.2);
        assert_eq!(after.style.stroke_width, 0.15);
    }
    let revision = scene.revision();
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.family(&[]).unwrap();
    assert!(source.match_style(&foreign).is_err());
    assert!(source
        .match_style(&scene.family(&[(&a).into()]).unwrap())
        .is_err());
    // Creating the intentionally incompatible family itself creates one revision.
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
}

#[test]
fn cross_alias_matching_observes_staged_styles_and_live_updates_are_atomic() {
    use noon::StyleUpdate;
    let mut scene = Scene::new();
    let a = scene.square(0.3).unwrap();
    let mut b = scene.circle(0.2).unwrap();
    b.set_style(StyleUpdate {
        fill_color: Some(Color::BLUE),
        fill_opacity: Some(0.25),
        ..Default::default()
    })
    .unwrap();
    let source = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let target = scene.family(&[(&b).into(), (&a).into()]).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let matched = live.match_family_style(&source, &target).unwrap();
    assert_eq!(matched.impacts().len(), 1);
    assert_eq!(live.effective(&a).unwrap().fill_opacity(), 0.25);
    assert_eq!(
        live.effective(&a).unwrap().style.fill,
        live.effective(&b).unwrap().style.fill
    );
    let update = StyleUpdate {
        stroke_color: Some(Color::RED),
        stroke_width: Some(0.05),
        ..Default::default()
    };
    assert_eq!(
        live.set_family_style(&source, update)
            .unwrap()
            .impacts()
            .len(),
        2
    );
    let before = live.effective(&a).unwrap();
    assert!(live
        .set_style(
            &a,
            StyleUpdate {
                stroke_width: Some(f64::NAN),
                ..Default::default()
            }
        )
        .is_err());
    assert_eq!(live.effective(&a).unwrap(), before);
}

#[test]
fn paired_style_example_uses_shared_runtime_with_local_paint_results() {
    let session = noon::example_scenes::style_operations::session().unwrap();
    let objects = &session.frame().objects;
    assert_eq!(objects.len(), 3);
    let colors: Vec<_> = objects
        .iter()
        .map(|object| object.style.fill.unwrap())
        .collect();
    assert_eq!(colors[0], colors[1]);
    assert_eq!(colors[0].red, 1.0);
    assert_eq!(colors[0].blue, 0.0);
    assert_eq!(colors[2].blue, 1.0);
    assert!((colors[2].alpha - 0.7).abs() < 1e-6);
}

#[test]
fn family_own_color_is_independent_of_leaves_and_recursive_writes_include_nested_roots() {
    let mut scene = Scene::new();
    let mut red = scene.square(1.0).unwrap();
    red.set_color(1.0, 0.0, 0.0, 1.0).unwrap();
    let mut green = scene.circle(0.5).unwrap();
    green.set_color(0.0, 1.0, 0.0, 1.0).unwrap();
    let nested = scene.family(&[(&red).into()]).unwrap();
    let root = scene.family(&[(&nested).into(), (&green).into()]).unwrap();
    let empty = scene.family(&[]).unwrap();
    assert_eq!(root.own_color().unwrap(), Color::WHITE);
    assert_eq!(nested.own_color().unwrap(), Color::WHITE);
    assert_eq!(empty.own_color().unwrap(), Color::WHITE);
    root.set_own_color(0.0, 0.0, 1.0, 1.0).unwrap();
    assert_eq!(root.own_color().unwrap(), Color::rgb(0.0, 0.0, 1.0));
    assert_eq!(nested.own_color().unwrap(), Color::WHITE);
    assert_eq!(red.manim_color().unwrap(), Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(green.manim_color().unwrap(), Color::rgb(0.0, 1.0, 0.0));
    root.set_color(1.0, 0.0, 0.0, 1.0).unwrap();
    assert_eq!(root.own_color().unwrap(), Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(nested.own_color().unwrap(), Color::rgb(1.0, 0.0, 0.0));
    assert_eq!(green.manim_color().unwrap(), Color::rgb(1.0, 0.0, 0.0));
    let copy = root.copy_family().unwrap();
    assert_eq!(copy.root().own_color().unwrap(), root.own_color().unwrap());
    copy.root().set_own_color(0.0, 1.0, 0.0, 1.0).unwrap();
    assert_eq!(root.own_color().unwrap(), Color::rgb(1.0, 0.0, 0.0));
}

#[test]
fn live_root_only_color_preserves_leaf_paint_and_publishes_coherently() {
    let mut scene = Scene::new();
    let mut leaf = scene.square(1.0).unwrap();
    leaf.set_color(1.0, 0.0, 0.0, 1.0).unwrap();
    let root = scene.family(&[(&leaf).into()]).unwrap();
    scene.add_many(&[(&root).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .set_family_own_color(&root, 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    assert_eq!(root.own_color().unwrap(), Color::rgb(0.0, 0.0, 1.0));
    assert_eq!(leaf.manim_color().unwrap(), Color::rgb(1.0, 0.0, 0.0));
    let before = scene.revision();
    assert!(scene
        .live(&mut session)
        .set_family_own_color(&root, f64::NAN, 0.0, 0.0, 1.0)
        .is_err());
    assert_eq!(scene.revision(), before);
    session.advance_to(0.1).unwrap();
}

#[test]
fn copied_and_become_family_metadata_preserve_aliases_and_independence() {
    let mut scene = Scene::new();
    let leaf = scene.square(0.5).unwrap();
    let nested = scene.family(&[(&leaf).into()]).unwrap();
    let source = scene.family(&[(&nested).into(), (&nested).into()]).unwrap();
    source.set_own_color(1.0, 0.0, 0.0, 1.0).unwrap();
    nested.set_own_color(0.0, 0.0, 1.0, 1.0).unwrap();
    let copied = source.copy_family().unwrap();
    assert_eq!(
        copied.root().own_color().unwrap(),
        Color::rgba(1.0, 0.0, 0.0, 1.0)
    );
    assert_eq!(
        copied.family(&nested).unwrap().own_color().unwrap(),
        Color::rgba(0.0, 0.0, 1.0, 1.0)
    );
    copied.root().set_own_color(0.0, 1.0, 0.0, 1.0).unwrap();
    assert_eq!(source.own_color().unwrap(), Color::rgba(1.0, 0.0, 0.0, 1.0));
    let target_leaf = scene.square(0.8).unwrap();
    let target_nested = scene.family(&[(&target_leaf).into()]).unwrap();
    let target = scene
        .family(&[(&target_nested).into(), (&target_nested).into()])
        .unwrap();
    target.set_own_color(0.0, 1.0, 0.0, 1.0).unwrap();
    target_nested.set_own_color(1.0, 0.0, 0.0, 1.0).unwrap();
    source
        .become_family(&target, noon::ManimBecomeOptions::default())
        .unwrap();
    assert_eq!(source.own_color().unwrap(), Color::rgba(0.0, 1.0, 0.0, 1.0));
    assert_eq!(nested.own_color().unwrap(), Color::rgba(1.0, 0.0, 0.0, 1.0));
    let empty = scene.family(&[]).unwrap();
    let empty_target = scene.family(&[]).unwrap();
    empty_target.set_own_color(0.0, 0.0, 1.0, 1.0).unwrap();
    empty
        .become_family(&empty_target, noon::ManimBecomeOptions::default())
        .unwrap();
    assert_eq!(empty.own_color().unwrap(), Color::rgba(0.0, 0.0, 1.0, 1.0));
}
