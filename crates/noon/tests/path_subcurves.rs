use noon::{ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

#[test]
fn subcurve_copies_preserve_style_source_and_coherent_live_state() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.line((0., 0.), (8., 0.)).unwrap();
        source.shift(2., 3.).unwrap();
        source.set_z_index(2.5).unwrap();
        source.set_stroke_color(1., 0., 0., 0.5).unwrap();
        scene.add(&source).unwrap();
        let before = source.state().unwrap();
        let mut session = scene.execution_session().unwrap();
        let selected = if live_mode {
            let mut live = scene.live(&mut session);
            let copy = live.subcurve(&source, 0.25, 0.75).unwrap();
            assert!(!live.contains(&copy).unwrap());
            live.add(&copy).unwrap();
            assert_eq!(
                live.effective_path_query(&copy).unwrap().start().unwrap(),
                (4., 3.)
            );
            copy
        } else {
            source.subcurve(0.25, 0.75).unwrap()
        };
        assert_ne!(selected.node_id(), source.node_id());
        assert_eq!(selected.path_query().unwrap().end().unwrap(), (8., 3.));
        assert_eq!(selected.state().unwrap().style, before.style);
        assert_eq!(
            selected.state().unwrap().presentation().z_index,
            before.presentation().z_index
        );
        assert_eq!(source.state().unwrap(), before);
        let full = if live_mode {
            scene.live(&mut session).subcurve(&source, 0., 1.).unwrap()
        } else {
            source.subcurve(0., 1.).unwrap()
        };
        assert_eq!(full.state().unwrap().content, before.content);
    }
}

#[test]
fn closed_subcurve_wraps_seam_and_keeps_curve_count_parameterization() {
    let scene = Scene::new();
    let source = scene
        .geometry(
            ManimGeometryOptions::path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(4., 0.))
                    .line_to(Vec2::new(4., 2.))
                    .line_to(Vec2::new(0., 2.))
                    .close(),
            )
            .unwrap(),
        )
        .unwrap();
    let selected = source.subcurve(0.875, 0.125).unwrap();
    let query = selected.path_query().unwrap();
    assert_eq!(query.start().unwrap(), (0., 1.));
    assert_eq!(query.end().unwrap(), (2., 0.));
    assert_eq!(query.curve_count(), 2);
    assert_eq!(query.subpaths().len(), 1);
    assert!(!query.is_closed().unwrap());
    assert!(source.path_query().unwrap().is_closed().unwrap());
    let point = source.subcurve(0.25, 0.25).unwrap();
    assert_eq!(point.path_query().unwrap().start().unwrap(), (4., 0.));
    assert_eq!(point.path_query().unwrap().end().unwrap(), (4., 0.));
}

#[test]
fn invalid_copy_inputs_publish_no_identity_or_resource_and_singleton_is_retained() {
    let scene = Scene::new();
    let source = scene.line((0., 0.), (4., 0.)).unwrap();
    let empty = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    for (a, b) in [(0.8, 0.2), (f64::NAN, 0.5), (-0.1, 0.5), (0.2, 1.1)] {
        assert!(source.subcurve(a, b).is_err());
    }
    assert!(empty.subcurve(0., 1.).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    let singleton = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new().move_to(Vec2::new(2., 3.))).unwrap())
        .unwrap();
    assert_eq!(
        singleton
            .subcurve(0.8, 0.2)
            .unwrap()
            .state()
            .unwrap()
            .content,
        singleton.state().unwrap().content
    );
}

#[test]
fn subpath_observations_use_world_tolerance_and_keep_snapshot_immutable() {
    let scene = Scene::new();
    let mut source = scene
        .geometry(
            ManimGeometryOptions::path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(1., 0.))
                    .move_to(Vec2::new(1., 0.))
                    .quadratic_to(Vec2::new(2., 2.), Vec2::new(3., 0.))
                    .move_to(Vec2::new(5., 0.))
                    .line_to(Vec2::new(6., 0.))
                    .move_to(Vec2::new(6., 0.)),
            )
            .unwrap(),
        )
        .unwrap();
    let query = source.path_query().unwrap();
    assert_eq!(
        query.subpaths().iter().map(Vec::len).collect::<Vec<_>>(),
        [8, 5]
    );
    source.shift(0., 2.).unwrap();
    assert_eq!(query.subpaths()[0][0], (0., 0.));
    assert_eq!(source.path_query().unwrap().subpaths()[0][0], (0., 2.));
    source.start_new_path(Vec2::new(9., 0.)).unwrap();
    assert_eq!(source.path_query().unwrap().subpaths().len(), 2);
}

#[test]
fn paired_subcurve_example_runs_through_the_shared_runtime() {
    let mut session = noon::example_scenes::path_subcurves::session().unwrap();
    session.seek(0.1).unwrap();
    assert_eq!(session.frame().objects.len(), 2);
}

#[test]
fn live_subcurve_captures_effective_transform_and_rejects_reveal_override() {
    use noon::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let source = scene.line((0., 0.), (4., 0.)).unwrap();
    let mut target = source.target_editor().unwrap();
    target.shift(0., 2.).unwrap();
    scene.add(&source).unwrap();
    let animation = scene
        .declare_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(2.)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.).unwrap();
    let revision = scene.revision();
    assert!(live.subcurve(&source, 0.25, 0.75).is_err());
    assert_eq!(scene.revision(), revision);
    live.advance_segment_to(segment, 2.).unwrap();
    live.complete_segment(segment).unwrap();
    session.seek(1.).unwrap();
    let selected = scene
        .live(&mut session)
        .subcurve(&source, 0.25, 0.75)
        .unwrap();
    assert_eq!(selected.path_query().unwrap().start().unwrap(), (1., 1.));
    assert_eq!(source.path_query().unwrap().start().unwrap(), (0., 2.));
    session.seek(2.).unwrap();
    assert_eq!(selected.path_query().unwrap().start().unwrap(), (1., 1.));

    let scene = Scene::new();
    let source = scene.circle(1.).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.declare_and_activate_create(&source, AnimationOptions::new())
        .unwrap();
    let revision = scene.revision();
    assert!(live.subcurve(&source, 0.2, 0.8).is_err());
    assert_eq!(scene.revision(), revision);
}
