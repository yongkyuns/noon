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
    assert_eq!(live.effective(&a).unwrap().style.fill, None);
    assert_eq!(live.effective(&a).unwrap().stroke_opacity(), 0.5);
}
