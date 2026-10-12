use noon::{ManimGeometryOptions, Scene};

#[test]
fn point_matching_reuses_content_and_preserves_identity_paint_and_priority() {
    for scene_api in [false, true] {
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
        if scene_api {
            scene.match_points(&source, &target).unwrap();
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
fn paired_point_matching_session_seeks_through_normal_runtime() {
    let mut session = noon::example_scenes::point_matching::session().unwrap();
    session.seek(0.).unwrap();
    assert_eq!(session.frame().objects.len(), 2);
    let start = session.frame().objects.clone();
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, start);
}
