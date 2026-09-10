use noon::{AnimationOptions, ManimGeometryOptions, RateFunction, Scene};

#[test]
fn point_matching_reuses_content_and_preserves_identity_paint_and_priority() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.circle(1.).unwrap();
        source.set_fill(0., 1., 0., 0.5).unwrap();
        source.set_z_index(3.5).unwrap();
        let target = scene
            .geometry(ManimGeometryOptions::arc(1., -0.3, 1.8, 3, 2., -1.).unwrap())
            .unwrap();
        let unrelated = scene.square(0.4).unwrap();
        let before = source.state().unwrap();
        let identity = source.node_id();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let other = unrelated.state().unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            scene
                .live(&mut session)
                .match_points(&source, &target)
                .unwrap();
        } else {
            source.match_points(&target).unwrap();
        }
        let after = source.state().unwrap();
        assert_eq!(source.node_id(), identity);
        assert_eq!(after.content, target.state().unwrap().content);
        assert_eq!(after.transform, target.state().unwrap().transform);
        assert_eq!(after.style, before.style);
        assert_eq!(after.presentation(), before.presentation());
        assert_eq!(unrelated.state().unwrap(), other);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
        let revision = scene.revision();
        let alias = source.clone();
        source.match_points(&alias).unwrap();
        assert_eq!(scene.revision(), revision);
        let foreign = Scene::new().circle(2.).unwrap();
        let before = source.state().unwrap();
        assert!(source.match_points(&foreign).is_err());
        assert_eq!(source.state().unwrap(), before);
        assert_eq!(scene.revision(), revision);
    }
}

#[test]
fn live_point_matching_obeys_publication_boundaries_and_captures_effective_seek_state() {
    let mut scene = Scene::new();
    let source = scene.square(1.).unwrap();
    let target = scene.line((-1., 0.), (1., 0.)).unwrap();
    let mut endpoint = target.target_editor().unwrap();
    endpoint.shift(0., 2.).unwrap();
    scene
        .add_many(&[(&source).into(), (&target).into()])
        .unwrap();
    let animation = scene
        .declare_transform_to(
            &target,
            &endpoint,
            AnimationOptions::new()
                .run_time(2.)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.).unwrap();
    let before = source.state().unwrap();
    assert!(live.match_points(&source, &target).is_err());
    assert_eq!(source.state().unwrap(), before);
    live.advance_segment_to(segment, 2.).unwrap();
    live.complete_segment(segment).unwrap();
    session.seek(1.).unwrap();
    {
        let mut live = scene.live(&mut session);
        live.match_points(&source, &target).unwrap();
        assert!((source.center().unwrap().1 - 1.).abs() < 1e-6);
        assert_eq!(
            source.state().unwrap().content,
            target.state().unwrap().content
        );
    }
    session.seek(2.).unwrap();
    let live = scene.live(&mut session);
    assert!((live.effective_layout(&target).unwrap().center.1 - 2.).abs() < 1e-6);
    assert!((live.effective_layout(&source).unwrap().center.1 - 1.).abs() < 1e-6);
}

#[test]
fn paired_point_matching_session_seeks_through_normal_runtime() {
    let mut session = noon::example_scenes::point_matching::session().unwrap();
    session.seek(0.).unwrap();
    assert_eq!(session.frame().objects.len(), 2);
    let start = session.frame().objects.clone();
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, start);
}

#[test]
fn point_matching_rejects_active_render_override_before_mutating_source() {
    let mut scene = Scene::new();
    let source = scene.square(1.).unwrap();
    let target = scene.circle(1.).unwrap();
    scene.add(&source).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.declare_and_activate_create(&target, AnimationOptions::new())
        .unwrap();
    let before = source.state().unwrap();
    let revision = source.integration_store().borrow().scene_revision();
    assert!(live.match_points(&source, &target).is_err());
    assert_eq!(source.state().unwrap(), before);
    assert_eq!(
        source.integration_store().borrow().scene_revision(),
        revision
    );
}
