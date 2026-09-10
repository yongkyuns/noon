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

#[test]
fn all_fill_orders_place_members_in_their_cells() {
    use noon::FamilyGridOptions;
    for (flow, expected) in [
        (
            "rd",
            [
                (-1., 0.5),
                (0., 0.5),
                (1., 0.5),
                (-1., -0.5),
                (0., -0.5),
                (1., -0.5),
            ],
        ),
        (
            "ld",
            [
                (1., 0.5),
                (0., 0.5),
                (-1., 0.5),
                (1., -0.5),
                (0., -0.5),
                (-1., -0.5),
            ],
        ),
        (
            "ru",
            [
                (-1., -0.5),
                (0., -0.5),
                (1., -0.5),
                (-1., 0.5),
                (0., 0.5),
                (1., 0.5),
            ],
        ),
        (
            "lu",
            [
                (1., -0.5),
                (0., -0.5),
                (-1., -0.5),
                (1., 0.5),
                (0., 0.5),
                (-1., 0.5),
            ],
        ),
        (
            "dr",
            [
                (-1., 0.5),
                (-1., -0.5),
                (0., 0.5),
                (0., -0.5),
                (1., 0.5),
                (1., -0.5),
            ],
        ),
        (
            "dl",
            [
                (1., 0.5),
                (1., -0.5),
                (0., 0.5),
                (0., -0.5),
                (-1., 0.5),
                (-1., -0.5),
            ],
        ),
        (
            "ur",
            [
                (-1., -0.5),
                (-1., 0.5),
                (0., -0.5),
                (0., 0.5),
                (1., -0.5),
                (1., 0.5),
            ],
        ),
        (
            "ul",
            [
                (1., -0.5),
                (1., 0.5),
                (0., -0.5),
                (0., 0.5),
                (-1., -0.5),
                (-1., 0.5),
            ],
        ),
    ] {
        let scene = Scene::new();
        let members: Vec<_> = (0..6).map(|_| scene.square(0.5).unwrap()).collect();
        let family = scene
            .family(&members.iter().map(Into::into).collect::<Vec<_>>())
            .unwrap();
        family
            .arrange_in_grid_with_options(&FamilyGridOptions {
                rows: Some(2),
                columns: Some(3),
                gap: (0.5, 0.5),
                flow: flow.parse().unwrap(),
                ..Default::default()
            })
            .unwrap();
        for (object, point) in members.iter().zip(expected) {
            assert_eq!(object.center().unwrap(), point, "{flow}");
        }
    }
}

#[test]
fn explicit_sizes_and_alignments_infer_dimensions_and_publish_from_live_bounds() {
    use noon::FamilyGridOptions;
    let (mut scene, family, members, unrelated) = fixture();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = unrelated.state().unwrap();
    let mut live = scene.live(&mut session);
    live.arrange_family_in_grid_with_options(
        &family,
        &FamilyGridOptions {
            gap: (0.5, 0.25),
            cell_alignment: (-1., -1.),
            row_alignments: Some("ud".into()),
            column_alignments: Some("lr".into()),
            row_heights: Some(vec![Some(3.), None]),
            column_widths: Some(vec![None, Some(2.)]),
            flow: noon::GridFlow::DownRight,
            ..Default::default()
        },
    )
    .unwrap();
    // Top-left a, bottom-left b, top-right c, bottom-right d. Explicit
    // alignment lists override the supplied default cell alignment.
    for (object, expected) in
        members
            .iter()
            .zip([(1.75, 3.625), (1.25, 0.125), (5.0, 3.125), (4.75, 0.375)])
    {
        assert_eq!(live.effective_layout(object).unwrap().center, expected);
    }
    assert_eq!(
        live.effective_family_layout(&family).unwrap().center,
        (3., 2.)
    );
    assert_eq!(unrelated.state().unwrap(), before);
}

#[test]
fn malformed_grid_options_are_atomic_and_huge_spare_capacity_keeps_precision() {
    use noon::FamilyGridOptions;
    let (scene, family, members, _) = fixture();
    let before = scene.revision();
    for options in [
        FamilyGridOptions {
            rows: Some(2),
            row_alignments: Some("u".into()),
            ..Default::default()
        },
        FamilyGridOptions {
            row_alignments: Some("ux".into()),
            ..Default::default()
        },
        FamilyGridOptions {
            rows: Some(2),
            row_heights: Some(vec![None]),
            ..Default::default()
        },
        FamilyGridOptions {
            column_widths: Some(vec![Some(f64::NAN)]),
            ..Default::default()
        },
    ] {
        assert!(family.arrange_in_grid_with_options(&options).is_err());
        assert_eq!(scene.revision(), before);
    }
    family
        .arrange_in_grid(Some(usize::MAX), Some(usize::MAX), 0.5, 0.25)
        .unwrap();
    assert_eq!(family.layout().unwrap().center(), (3., 2.));
    assert_eq!(members[0].center().unwrap().1, 2.);
    assert_eq!(
        members[1].center().unwrap().0 - members[0].center().unwrap().0,
        2.
    );
}

#[test]
fn paired_grid_example_uses_the_normal_execution_session() {
    let session = noon::example_scenes::family_grid::session().unwrap();
    assert_eq!(session.frame().objects.len(), 4);
}
