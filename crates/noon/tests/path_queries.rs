use noon::{
    AnimationCompositionRequest, AnimationOptions, MobjectTarget, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest, Vec2, VectorPath,
};

fn near(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        (actual.0 - expected.0).abs() < 1e-6 && (actual.1 - expected.1).abs() < 1e-6,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn point_count_keeps_promoted_controls_and_unfinished_anchors() {
    let mut scene = Scene::new();
    let cubic = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
        Vec2::new(1., 2.),
        Vec2::new(2., -2.),
        Vec2::new(3., 0.),
    );
    for (path, expected) in [
        (VectorPath::new(), 0),
        (VectorPath::new().move_to(Vec2::ZERO), 1),
        (
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .move_to(Vec2::new(2., 1.)),
            5,
        ),
        (cubic.clone(), 4),
        (cubic.clone().move_to(Vec2::new(4., 0.)), 5),
        (
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.)),
            4,
        ),
        (
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .quadratic_to(Vec2::new(1., 1.), Vec2::new(2., 0.)),
            4,
        ),
        (
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .cubic_to(Vec2::ZERO, Vec2::ZERO, Vec2::ZERO),
            4,
        ),
    ] {
        let mut object = scene.path(path, Default::default()).unwrap();
        let revision = scene.revision();
        let query = object.path_query().unwrap();
        assert_eq!(query.point_count(), expected);
        assert_eq!(scene.revision(), revision);
        object.set_scale(2., 0.5).unwrap();
        assert_eq!(query.point_count(), expected);
        assert_eq!(object.local_path_query().unwrap().point_count(), expected);
    }
    assert_eq!(
        scene
            .rectangle(2., 1.)
            .unwrap()
            .path_query()
            .unwrap()
            .point_count(),
        16
    );

    let repeated_moves = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .move_to(Vec2::new(2., 1.)),
            Default::default(),
        )
        .unwrap();
    let query = repeated_moves.path_query().unwrap();
    assert_eq!(query.curve_count(), 1);
    assert_eq!(query.point_count(), 5);
    assert_eq!(
        query.points().unwrap(),
        vec![(0., 0.), (0., 0.), (0., 0.), (0., 0.), (2., 1.)]
    );
    assert_eq!(query.curve_points(0).unwrap(), [(0., 0.); 4]);
    assert_eq!(query.curve_length(0, None).unwrap(), 0.);

    let move_then_draw = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .move_to(Vec2::new(2., 1.))
                .line_to(Vec2::new(3., 1.)),
            Default::default(),
        )
        .unwrap();
    let query = move_then_draw.path_query().unwrap();
    assert_eq!(query.curve_count(), 2);
    assert_eq!(query.point_count(), 8);
    assert_eq!(query.curve_points(0).unwrap(), [(0., 0.); 4]);
    assert_eq!(query.curve_points(1).unwrap()[0], (2., 1.));
}

#[test]
fn all_points_preserves_recursive_preorder_and_repeated_family_aliases() {
    let mut scene = Scene::new();
    let first = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.)),
            Default::default(),
        )
        .unwrap();
    let second = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::new(2., 0.))
                .line_to(Vec2::new(3., 0.)),
            Default::default(),
        )
        .unwrap();
    let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let root = scene.family(&[(&nested).into(), (&first).into()]).unwrap();

    let first_points = first.path_query().unwrap().points().unwrap();
    let second_points = second.path_query().unwrap().points().unwrap();
    let mut expected = first_points.clone();
    expected.extend(second_points);
    expected.extend(first_points);
    assert_eq!(MobjectTarget::from(&root).all_points().unwrap(), expected);
    let expected_center = (
        expected.iter().map(|point| point.0).sum::<f64>() / expected.len() as f64,
        expected.iter().map(|point| point.1).sum::<f64>() / expected.len() as f64,
    );
    assert_eq!(
        MobjectTarget::from(&root).center_of_mass().unwrap(),
        expected_center
    );
    assert_eq!(
        MobjectTarget::from(&root).boundary_point(1., 0.).unwrap(),
        (3., 0.)
    );
    assert_eq!(
        MobjectTarget::from(&root).boundary_point(0., 0.).unwrap(),
        expected[0]
    );
    assert_eq!(
        MobjectTarget::from(&root)
            .extremum_along_dim(0, 0.)
            .unwrap(),
        1.5
    );
    assert_eq!(
        MobjectTarget::from(&root)
            .extremum_along_dim(2, 1.)
            .unwrap(),
        0.
    );
    assert_eq!(MobjectTarget::from(&root).length_over_dim(0).unwrap(), 3.);
    assert_eq!(MobjectTarget::from(&root).length_over_dim(1).unwrap(), 0.);
    assert_eq!(MobjectTarget::from(&root).length_over_dim(2).unwrap(), 0.);
    assert!(MobjectTarget::from(&root).length_over_dim(3).is_err());
    assert_eq!(
        MobjectTarget::from(&first).all_points().unwrap(),
        first.path_query().unwrap().points().unwrap()
    );

    let empty = scene.family(&[]).unwrap();
    assert!(MobjectTarget::from(&empty).all_points().unwrap().is_empty());
    let empty_center = MobjectTarget::from(&empty).center_of_mass().unwrap();
    assert!(empty_center.0.is_nan());
    assert!(empty_center.1.is_nan());
    assert_eq!(MobjectTarget::from(&empty).length_over_dim(3).unwrap(), 0.);
    assert!(MobjectTarget::from(&empty).boundary_point(1., 0.).is_err());
}

#[test]
fn point_extremum_reducer_matches_manim_key_selection() {
    let coordinates = [-2., 3., 4., -1.];
    assert_eq!(
        noon::reduce_point_extremum(&coordinates, 2, 0, -1.).unwrap(),
        -2.
    );
    assert_eq!(
        noon::reduce_point_extremum(&coordinates, 2, 0, 0.).unwrap(),
        1.
    );
    assert_eq!(
        noon::reduce_point_extremum(&coordinates, 2, 1, 1.).unwrap(),
        3.
    );
    assert_eq!(
        noon::reduce_point_extremum(&coordinates, 2, 2, 1.),
        Err(noon::AuthoringError::InvalidPointDimension(2))
    );
    assert!(noon::reduce_point_extremum(&[], 2, 0, 0.).is_err());
    assert!(noon::reduce_point_extremum(&[1., 2., 3.], 2, 0, 0.).is_err());
}

#[test]
fn new_path_started_tracks_the_unfinished_cubic_anchor() {
    let mut scene = Scene::new();
    let empty = scene.path(VectorPath::new(), Default::default()).unwrap();
    assert!(!empty.path_query().unwrap().has_new_path_started());

    let anchored = scene
        .path(VectorPath::new().move_to(Vec2::ZERO), Default::default())
        .unwrap();
    assert!(anchored.path_query().unwrap().has_new_path_started());

    let complete = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.)),
            Default::default(),
        )
        .unwrap();
    assert!(!complete.path_query().unwrap().has_new_path_started());

    let reopened = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.))
                .move_to(Vec2::new(2., 0.)),
            Default::default(),
        )
        .unwrap();
    assert!(reopened.path_query().unwrap().has_new_path_started());
}

#[test]
fn curve_length_piece_queries_match_sample_count_and_world_scale() {
    let mut scene = Scene::new();
    let path = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
        Vec2::new(0., 1.),
        Vec2::new(1., 1.),
        Vec2::new(1., 0.),
    );
    let mut object = scene.path(path, Default::default()).unwrap();
    object.set_scale(2., 0.5).unwrap();
    let query = object.path_query().unwrap();

    let pieces = query.curve_length_pieces(0, Some(5)).unwrap();
    let expected = [
        0.420_425_751_471_053_43,
        0.693_862_603_474_203_7,
        0.693_862_603_474_203_7,
        0.420_425_751_471_053_43,
    ];
    assert_eq!(pieces.len(), expected.len());
    for (actual, expected) in pieces.iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
    assert!((query.curve_length(0, Some(5)).unwrap() - pieces.iter().sum::<f64>()).abs() < 1e-12);
    assert_eq!(
        query.curve_length_pieces(0, Some(1)).unwrap(),
        Vec::<f64>::new()
    );
    assert_eq!(query.curve_length(0, Some(1)).unwrap(), 0.0);
    assert!(query.curve_length_pieces(0, Some(0)).is_err());
    assert!(query.curve_length(0, Some(0)).is_err());
    assert!(query.curve_length_pieces(1, None).is_err());
}

#[test]
fn curve_function_queries_evaluate_batches_and_extrapolate_in_snapshot_space() {
    let mut scene = Scene::new();
    let path = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
        Vec2::new(1., 2.),
        Vec2::new(2., 2.),
        Vec2::new(3., 0.),
    );
    let mut object = scene.path(path, Default::default()).unwrap();
    object.set_scale(2., 0.5).unwrap();
    object.set_translation(10., -2.).unwrap();
    let query = object.path_query().unwrap();

    let points = query.curve_points_at(0, &[0., 0.5, 1., 1.5]).unwrap();
    assert_eq!(points.len(), 4);
    for (actual, expected) in points.iter().zip([
        (10., -2., 0.),
        (13., -1.25, 0.),
        (16., -2., 0.),
        (19., -4.25, 0.),
    ]) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-12
                && (actual.1 - expected.1).abs() < 1e-12
                && (actual.2 - expected.2).abs() < 1e-12,
            "{actual:?} != {expected:?}"
        );
    }
    assert!(query.curve_points_at(1, &[0.5]).is_err());
}

#[test]
fn path_direction_matches_manim_anchor_trapezoid_and_snapshot_transform() {
    let mut scene = Scene::new();
    let path = VectorPath::new()
        .move_to(Vec2::new(-1., -1.))
        .line_to(Vec2::new(1., -1.))
        .line_to(Vec2::new(1., 1.))
        .line_to(Vec2::new(-1., 1.))
        .close();
    let mut object = scene.path(path, Default::default()).unwrap();
    assert_eq!(object.path_query().unwrap().direction(), "CCW");
    object.reverse_direction().unwrap();
    assert_eq!(object.path_query().unwrap().direction(), "CW");
    object.set_translation(7., -3.).unwrap();
    assert_eq!(object.path_query().unwrap().direction(), "CCW");
    assert_eq!(object.local_path_query().unwrap().direction(), "CW");
}

#[test]
fn nonuniform_world_measure_and_local_measure_are_distinct_immutable_observations() {
    let mut scene = Scene::new();
    let path = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(1., 0.))
        .line_to(Vec2::new(1., 1.));
    let mut object = scene.path(path, Default::default()).unwrap();
    object.set_scale(3., 1.).unwrap();
    object.set_translation(10., -2.).unwrap();
    let revision = scene.revision();
    let content = object.state().unwrap().content;
    let local = object.local_path_query().unwrap();
    let world = object.path_query().unwrap();
    near(local.point_from_proportion(0.5).unwrap(), (1., 0.));
    near(world.point_from_proportion(0.5).unwrap(), (12., -2.));
    near(world.start().unwrap(), (10., -2.));
    near(world.end().unwrap(), (13., -1.));
    assert!((world.arc_length(None).unwrap() - 4.).abs() < 1e-6);
    assert!((local.arc_length(Some(25)).unwrap() - 2.).abs() < 1e-6);
    assert!(world.point_from_proportion(f64::NAN).is_err());
    assert!(world.point_from_proportion(1.1).is_err());
    assert!(world.arc_length(Some(1)).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(object.state().unwrap().content, content);
    object.shift(1., 0.).unwrap();
    near(world.start().unwrap(), (10., -2.));
    near(object.path_query().unwrap().start().unwrap(), (11., -2.));
}

#[test]
fn canonical_primitives_and_subpath_breaks_keep_their_path_order() {
    let mut scene = Scene::new();
    let rectangle = scene.rectangle(4., 2.).unwrap();
    let query = rectangle.path_query().unwrap();
    near(query.start().unwrap(), (2., 1.));
    near(query.point_from_proportion(0.25).unwrap(), (-1., 1.));
    near(query.end().unwrap(), (2., 1.));
    assert!((query.arc_length(None).unwrap() - 12.).abs() < 1e-6);
    let circle = scene.circle(1.).unwrap();
    near(
        circle
            .path_query()
            .unwrap()
            .point_from_proportion(0.125)
            .unwrap(),
        (2f64.sqrt() / 2., 2f64.sqrt() / 2.),
    );
    let object = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1., 0.))
                .move_to(Vec2::new(10., 0.))
                .line_to(Vec2::new(11., 0.)),
            Default::default(),
        )
        .unwrap();
    let query = object.path_query().unwrap();
    assert!((query.arc_length(None).unwrap() - 2.).abs() < 1e-6);
    near(query.point_from_proportion(0.75).unwrap(), (10.5, 0.));
}

#[test]
fn effective_queries_capture_one_live_affine_publication() {
    let mut scene = Scene::new();
    let object = scene.rectangle(2., 1.).unwrap();
    let mut target = object.target_editor().unwrap();
    target.set_translation(0., 2.).unwrap();
    scene.add(&object).unwrap();
    let animation = scene
        .declare_transform_to(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(2.)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.)
        .unwrap();
    let query =
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap();
    near(query.start().unwrap(), (1., 1.5));
    near(object.path_query().unwrap().start().unwrap(), (1., 0.5));
    scene
        .live(&mut session)
        .advance_segment_to(segment, 2.)
        .unwrap();
    near(query.start().unwrap(), (1., 1.5));
    near(
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap()
            .start()
            .unwrap(),
        (1., 2.5),
    );
}

#[test]
fn effective_all_points_uses_the_live_publication_and_keeps_family_aliases() {
    let mut scene = Scene::new();
    let object = scene.rectangle(2., 1.).unwrap();
    let family = scene.family(&[(&object).into()]).unwrap();
    let mut target = object.target_editor().unwrap();
    target.set_translation(0., 2.).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let animation = scene
        .declare_transform_to(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(2.)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.)
        .unwrap();

    let points = noon::integration::effective_all_points(
        scene.integration_store(),
        &session,
        MobjectTarget::from(&family),
    )
    .unwrap();
    let expected =
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap()
            .points()
            .unwrap();
    assert_eq!(points, expected);
    assert_ne!(points, MobjectTarget::from(&family).all_points().unwrap());
    let expected_center = (
        points.iter().map(|point| point.0).sum::<f64>() / points.len() as f64,
        points.iter().map(|point| point.1).sum::<f64>() / points.len() as f64,
    );
    assert_eq!(
        noon::integration::effective_center_of_mass(
            scene.integration_store(),
            &session,
            MobjectTarget::from(&family),
        )
        .unwrap(),
        expected_center
    );
    let effective_top = noon::integration::effective_boundary_point(
        scene.integration_store(),
        &session,
        MobjectTarget::from(&family),
        0.,
        1.,
    )
    .unwrap();
    assert_eq!(effective_top.1, 1.5);
    assert_eq!(
        noon::integration::effective_length_over_dim(
            scene.integration_store(),
            &session,
            MobjectTarget::from(&family),
            0,
        )
        .unwrap(),
        2.
    );
    assert_eq!(
        noon::integration::effective_length_over_dim(
            scene.integration_store(),
            &session,
            MobjectTarget::from(&family),
            1,
        )
        .unwrap(),
        1.
    );
    assert_eq!(
        noon::integration::effective_length_over_dim(
            scene.integration_store(),
            &session,
            MobjectTarget::from(&family),
            2,
        )
        .unwrap(),
        0.
    );
    assert_eq!(
        MobjectTarget::from(&family)
            .extremum_along_dim(1, 1.)
            .unwrap(),
        0.5
    );
    assert_eq!(
        noon::integration::effective_extremum_along_dim(
            scene.integration_store(),
            &session,
            MobjectTarget::from(&family),
            1,
            1.,
        )
        .unwrap(),
        1.5
    );
    assert_eq!(
        noon::integration::effective_extremum_along_dim(
            scene.integration_store(),
            &session,
            MobjectTarget::from(&family),
            2,
            1.,
        )
        .unwrap(),
        0.
    );
}

#[test]
fn effective_reveal_queries_keep_curve_boundaries_transforms_and_snapshots() {
    let mut scene = Scene::new();
    let mut object = scene
        .path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(4., 0.))
                .move_to(Vec2::new(10., 0.))
                .line_to(Vec2::new(10., 2.)),
            Default::default(),
        )
        .unwrap();
    object.set_scale(2., 1.).unwrap();
    object.shift(1., 3.).unwrap();
    let mut session = scene.execution_session().unwrap();
    let snapshot;
    {
        let segment = scene
            .live(&mut session)
            .declare_and_activate_create(
                &object,
                AnimationOptions::new()
                    .run_time(2.)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let initial =
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap();
        near(initial.start().unwrap(), (1., 3.));
        near(initial.end().unwrap(), (1., 3.));
        scene
            .live(&mut session)
            .advance_segment_to(segment, 1.5)
            .unwrap();
        snapshot =
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap();
        assert_eq!(snapshot.curve_count(), 2);
        assert_eq!(snapshot.subpaths().len(), 2);
        near(snapshot.end().unwrap(), (21., 4.));
        assert!((snapshot.arc_length(None).unwrap() - 9.).abs() < 1e-6);
        scene
            .live(&mut session)
            .advance_segment_to(segment, 2.)
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        near(
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap()
                .end()
                .unwrap(),
            (21., 5.),
        );
        near(snapshot.end().unwrap(), (21., 4.));
    }
    let committed_revision = scene.revision();
    session.seek(1.5).unwrap();
    let replay =
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap();
    assert_eq!(replay.anchors_and_handles(), snapshot.anchors_and_handles());
    assert_eq!(scene.revision(), committed_revision);
    near(object.path_query().unwrap().end().unwrap(), (21., 5.));
}

#[test]
fn paired_query_example_uses_the_normal_execution_session() {
    let session = noon::example_scenes::path_queries::session().unwrap();
    assert_eq!(session.frame().objects.len(), 10);
}

#[test]
fn cubic_constructor_reference_keeps_six_manim_point_stroke() {
    let session = noon::example_scenes::cubic_bezier::session().unwrap();
    assert_eq!(session.frame().objects.len(), 1);
    assert!((session.frame().objects[0].style.stroke_width - 0.06).abs() < 1e-6);
}

#[test]
fn effective_morph_queries_interpolate_controls_and_keep_snapshot_and_seek_coherent() {
    let mut scene = Scene::new();
    let mut object = scene.square(2.).unwrap();
    object.rotate(0.37).unwrap();
    object.shift(-1., 2.).unwrap();
    scene.add(&object).unwrap();
    let target = object.target_editor().unwrap();
    noon::LayoutAnchor::from(&target)
        .stretch(
            2.,
            noon::LayoutDimension::Width,
            noon::ManimRotationPivot::Center,
        )
        .unwrap();
    let source_query = object.path_query().unwrap();
    let target_query = target.path_query().unwrap();
    let animation = scene
        .declare_transform_to(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(2.)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let midpoint;
    {
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        scene
            .live(&mut session)
            .advance_segment_to(segment, 1.)
            .unwrap();
        midpoint =
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap();
        assert_eq!(midpoint.curve_count(), 4);
        for index in 0..4 {
            let start = source_query.curve_points(index).unwrap();
            let end = target_query.curve_points(index).unwrap();
            for (point, (a, b)) in midpoint
                .curve_points(index)
                .unwrap()
                .into_iter()
                .zip(start.into_iter().zip(end))
            {
                near(point, ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5));
            }
        }
        scene
            .live(&mut session)
            .advance_segment_to(segment, 2.)
            .unwrap();
        scene.live(&mut session).complete_segment(segment).unwrap();
        near(
            noon::integration::effective_path_query(scene.integration_store(), &session, &object)
                .unwrap()
                .start()
                .unwrap(),
            target_query.start().unwrap(),
        );
    }
    session.seek(1.).unwrap();
    let replay =
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap();
    for index in 0..4 {
        assert_eq!(
            replay.curve_points(index).unwrap(),
            midpoint.curve_points(index).unwrap()
        );
    }
}

#[test]
fn effective_combined_morph_reveal_uses_authored_curve_progress_and_direct_seek() {
    let mut scene = Scene::new();
    let source = VectorPath::new()
        .move_to(Vec2::ZERO)
        .cubic_to(Vec2::new(0., 5.), Vec2::new(2., -5.), Vec2::new(2., 0.))
        .line_to(Vec2::new(3., 0.));
    let target_path = VectorPath::new()
        .move_to(Vec2::ZERO)
        .cubic_to(Vec2::new(5., 0.), Vec2::new(-5., 2.), Vec2::new(0., 2.))
        .line_to(Vec2::new(0., 3.));
    let object = scene.path(source, Default::default()).unwrap();
    let target = scene.path(target_path, Default::default()).unwrap();
    let linear = AnimationOptions::new()
        .run_time(2.)
        .rate_func(RateFunction::Linear);
    let request = AnimationCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        children: vec![
            AnimationCompositionRequest::Create {
                target: &object,
                options: linear,
            },
            AnimationCompositionRequest::TransformTo(TransformToRequest::point_correspondence(
                &object, &target, linear,
            )),
        ],
        options: AnimationOptions::new(),
    };
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.)
        .unwrap();
    let midpoint =
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap();
    // Persistent partial geometry keeps the zero-length boundary curve so later
    // path alignment observes the same authored two-curve topology.
    assert_eq!(midpoint.curve_count(), 2);
    near(midpoint.start().unwrap(), (0., 0.));
    near(midpoint.end().unwrap(), (1., 1.));

    session.seek(0.25).unwrap();
    session.seek(1.).unwrap();
    let replay =
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap();
    assert_eq!(replay.anchors_and_handles(), midpoint.anchors_and_handles());
    assert_eq!(replay.end().unwrap(), midpoint.end().unwrap());
}

#[test]
fn native_object_scaled_strokes_use_the_same_morph_controls() {
    let mut scene = Scene::new();
    let path = VectorPath::new()
        .move_to(Vec2::new(-1., -1.))
        .line_to(Vec2::new(1., -1.))
        .line_to(Vec2::new(1., 1.))
        .line_to(Vec2::new(-1., 1.))
        .line_to(Vec2::new(-1., -1.))
        .close();
    let object = scene
        .path(path, noon_core::SemanticStyle::default())
        .unwrap();
    scene.add(&object).unwrap();
    let mut target = object.target_editor().unwrap();
    target
        .set_points_as_corners(&[
            Vec2::new(-2., -1.),
            Vec2::new(2., -1.),
            Vec2::new(2., 1.),
            Vec2::new(-2., 1.),
            Vec2::new(-2., -1.),
        ])
        .unwrap();
    let animation = scene
        .declare_transform_to(
            &object,
            &target,
            AnimationOptions::new()
                .run_time(2.)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.)
        .unwrap();
    near(
        noon::integration::effective_path_query(scene.integration_store(), &session, &object)
            .unwrap()
            .start()
            .unwrap(),
        (-1.5, -1.),
    );
}
