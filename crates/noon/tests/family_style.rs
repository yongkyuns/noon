use noon::{Color, Mobject, MobjectFamily, Scene};

fn family() -> (Scene, MobjectFamily, [Mobject; 2], Mobject) {
    let scene = Scene::new();
    let a = scene.square(0.5).unwrap();
    let b = scene.circle(0.4).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    let unrelated = scene.square(0.2).unwrap();
    (scene, family, [a, b], unrelated)
}

#[test]
fn family_paint_matches_leaf_semantics_in_one_revision_and_leaves_other_objects_alone() {
    let (scene, family, [a, b], unrelated) = family();
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
    let (scene, family, [a, b], _) = family();
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
    let foreign = Scene::new().family(&[]).unwrap();
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
    let (scene, source, [a, b], _) = family();
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
    assert!(source
        .match_style(&Scene::new().family(&[]).unwrap())
        .is_err());
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
