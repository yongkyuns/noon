use noon::{Mobject, MobjectFamily, Scene};

fn fixture() -> (Scene, MobjectFamily, Vec<Mobject>, Mobject) {
    let scene = Scene::new();
    let mut members = Vec::new();
    for (w, h) in [(2.0, 1.0), (1.0, 0.5), (0.5, 2.0), (1.0, 1.0)] {
        let mut object = scene.rectangle(w, h).unwrap();
        object.shift(3.0, 2.0).unwrap();
        members.push(object);
    }
    let family = scene
        .family(&members.iter().map(Into::into).collect::<Vec<_>>())
        .unwrap();
    let unrelated = scene.square(0.1).unwrap();
    (scene, family, members, unrelated)
}

#[test]
fn grid_sizes_each_row_and_column_and_preserves_center_in_one_revision() {
    let (scene, family, members, unrelated) = fixture();
    let before = scene.revision();
    family.arrange_in_grid(Some(2), Some(2), 0.5, 0.25).unwrap();
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(family.layout().unwrap().center(), (3.0, 2.0));
    let expected = [(2.25, 3.125), (4.25, 3.125), (2.25, 1.375), (4.25, 1.375)];
    for (member, point) in members.iter().zip(expected) {
        assert_eq!(member.center().unwrap(), point);
    }
    assert_eq!(unrelated.center().unwrap(), (0.0, 0.0));
}

#[test]
fn insufficient_capacity_zero_and_nonfinite_gaps_do_not_mutate() {
    let (scene, family, members, _) = fixture();
    let before = scene.revision();
    let states: Vec<_> = members.iter().map(|m| m.state().unwrap()).collect();
    assert!(family.arrange_in_grid(Some(1), Some(2), 0.2, 0.2).is_err());
    assert!(family.arrange_in_grid(None, Some(0), 0.2, 0.2).is_err());
    assert!(family.arrange_in_grid(None, None, f64::NAN, 0.2).is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!(
        members
            .iter()
            .map(|m| m.state().unwrap())
            .collect::<Vec<_>>(),
        states
    );
    // Empty and oversized capacities do not allocate a rows*columns grid.
    let empty = scene.family(&[]).unwrap();
    empty
        .arrange_in_grid(Some(usize::MAX), Some(usize::MAX), 0.2, 0.2)
        .unwrap();
    family
        .arrange_in_grid(Some(usize::MAX), Some(usize::MAX), 0.2, 0.2)
        .unwrap();
}

#[test]
fn nested_aliases_use_one_live_transaction_and_foreign_grids_are_rejected() {
    let mut scene = Scene::new();
    let mut a = scene.square(0.5).unwrap();
    let mut b = scene.square(0.5).unwrap();
    a.shift(-1.0, 0.0).unwrap();
    b.shift(1.0, 0.0).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&nested).into(), (&a).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let result = live
        .arrange_family_in_grid(&family, Some(1), None, 0.25, 0.25)
        .unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(
        live.effective_family_layout(&family).unwrap().center,
        (0.0, 0.0)
    );
    assert_eq!(live.effective_layout(&a).unwrap().center, (0.375, 0.0));
    assert_eq!(live.effective_layout(&b).unwrap().center, (-0.375, 0.0));
    let before = live.effective(&a).unwrap();
    let foreign = Scene::new().family(&[]).unwrap();
    assert!(live
        .arrange_family_in_grid(&foreign, None, None, 0.2, 0.2)
        .is_err());
    assert_eq!(live.effective(&a).unwrap(), before);
}
