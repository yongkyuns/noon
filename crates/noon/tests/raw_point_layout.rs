use noon::{integration::BorrowedPathEdit, AuthoringError, Scene};
use noon_core::VectorPath;
use noon_geometry::PathProportionError;

fn unpaired(error: AuthoringError) {
    assert!(matches!(
        error,
        AuthoringError::PathQuery(PathProportionError::UnpairedAnchors { .. })
    ));
}

#[test]
fn raw_layout_preserves_semantic_coordinates_and_handle_extrema() {
    let delta = 2.0_f64.powi(-40);
    let mut scene = Scene::new();
    let point = scene
        .path(
            VectorPath::from_manim_points_f64(vec![(1.0 + delta, -delta); 4]),
            Default::default(),
        )
        .unwrap();
    assert_eq!(point.center().unwrap(), (1.0 + delta, -delta));
    assert_eq!(
        point.critical_point(1.0, 0.0).unwrap(),
        (1.0 + delta, -delta)
    );
    let points = vec![
        (1.0, 0.0),
        (1.0 + 5.0 * delta, -4.0 * delta),
        (1.0 - 2.0 * delta, 6.0 * delta),
        (1.0 + 3.0 * delta, 0.0),
    ];
    let path = scene
        .path(
            VectorPath::from_manim_points_f64(points),
            Default::default(),
        )
        .unwrap();
    assert_eq!(path.center().unwrap(), (1.0 + 1.5 * delta, 0.0));
    assert_eq!(path.width().unwrap(), 7.0 * delta);
    assert_eq!(path.height().unwrap(), 10.0 * delta);
}

#[test]
fn raw_anchor_selection_and_all_columns_match_every_incomplete_prefix() {
    let mut scene = Scene::new();
    for count in 0_usize..10 {
        let points = (0..count)
            .map(|i| (i as f64, (i * 2) as f64))
            .collect::<Vec<_>>();
        let object = scene
            .path(
                VectorPath::from_manim_points_f64(points.clone()),
                Default::default(),
            )
            .unwrap();
        let query = object.path_query().unwrap();
        for (column, actual) in query.anchors_and_handles().into_iter().enumerate() {
            assert_eq!(
                actual,
                points
                    .iter()
                    .copied()
                    .skip(column)
                    .step_by(4)
                    .collect::<Vec<_>>()
            );
        }
        if count != 1 && !count.is_multiple_of(4) {
            unpaired(query.anchors().unwrap_err());
            unpaired(object.center().unwrap_err());
            unpaired(object.critical_point(1.0, 0.0).unwrap_err());
        } else {
            let indices = if count == 1 {
                vec![0]
            } else {
                (0..count / 4).flat_map(|i| [4 * i, 4 * i + 3]).collect()
            };
            assert_eq!(
                query.anchors().unwrap(),
                indices.into_iter().map(|i| points[i]).collect::<Vec<_>>()
            );
        }
        assert_eq!(object.width().unwrap(), count.saturating_sub(1) as f64);
    }
}

#[test]
fn effective_raw_layout_uses_current_precise_resource_and_rejects_unpaired_boundaries() {
    let delta = 2.0_f64.powi(-40);
    let mut scene = Scene::new();
    let object = scene
        .path(
            VectorPath::from_manim_points_f64(vec![(1.0 + delta, 0.0); 4]),
            Default::default(),
        )
        .unwrap();
    scene.add(&object).unwrap();
    let mut execution = scene.execution_session().unwrap();
    assert_eq!(
        scene
            .live(&mut execution)
            .effective_layout(&object)
            .unwrap()
            .center,
        (1.0 + delta, 0.0)
    );
    let new_points = [(2.0 + delta, -delta); 4];
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut execution,
        BorrowedPathEdit::RawPoints {
            object: &object,
            points: &new_points,
        },
    )
    .unwrap();
    assert_eq!(
        scene
            .live(&mut execution)
            .effective_layout(&object)
            .unwrap()
            .center,
        (2.0 + delta, -delta)
    );
    noon::integration::publish_borrowed_path_edit(
        scene.integration_store(),
        scene.root(),
        &mut execution,
        BorrowedPathEdit::RawPoints {
            object: &object,
            points: &new_points[..2],
        },
    )
    .unwrap();
    assert!(scene
        .live(&mut execution)
        .effective_layout(&object)
        .is_err());
    assert_eq!(object.width().unwrap(), 0.0);
}

#[test]
fn raw_boundary_query_failure_is_read_only_and_does_not_invalidate_snapshots() {
    let mut scene = Scene::new();
    let mut object = scene
        .path(
            VectorPath::from_manim_points_f64(vec![(0.0, 0.0); 4]),
            Default::default(),
        )
        .unwrap();
    let snapshot = object.path_query().unwrap();
    object.set_points_f64(&[(1.0, 0.0), (2.0, 0.0)]).unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    unpaired(object.center().unwrap_err());
    unpaired(object.path_query().unwrap().anchors().unwrap_err());
    assert_eq!(snapshot.anchors().unwrap(), [(0.0, 0.0); 2]);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
}

#[test]
fn semantic_scaling_and_translation_do_not_reintroduce_rounded_bounds() {
    let delta = 2.0_f64.powi(-40);
    let scale = 2.0_f64.powi(40);
    let mut scene = Scene::new();
    let mut object = scene
        .path(
            VectorPath::from_manim_points_f64(
                (0..4).map(|i| (1.0 + i as f64 * delta, 0.0)).collect(),
            ),
            Default::default(),
        )
        .unwrap();
    object.set_scale(scale, 1.0).unwrap();
    object.shift(-scale, 0.0).unwrap();
    assert_eq!(object.center().unwrap(), (1.5, 0.0));
    assert_eq!(object.width().unwrap(), 3.0);
    assert_eq!(object.critical_point(1.0, 0.0).unwrap(), (3.0, 0.0));
}
