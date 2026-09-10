use noon::{AnimationOptions, AuthoringError, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

#[test]
fn corner_replacement_preserves_identity_paint_and_copies_with_local_publication() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene.circle(1.).unwrap();
        source.shift(3., 4.).unwrap();
        source.rotate(0.6).unwrap();
        source.set_fill(0., 1., 0., 0.5).unwrap();
        source.set_z_index(3.5).unwrap();
        let copy = source.copy_handle().unwrap();
        let unrelated = scene.square(0.4).unwrap();
        scene
            .add_many(&[(&source).into(), (&copy).into(), (&unrelated).into()])
            .unwrap();
        let before = source.state().unwrap();
        let other = unrelated.state().unwrap();
        let identity = source.node_id();
        let revision = scene.revision();
        let count = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let corners = [Vec2::new(-2., -1.), Vec2::new(0., 1.), Vec2::new(2., -1.)];
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            scene
                .live(&mut session)
                .set_points_as_corners(&source, &corners)
                .unwrap();
        } else {
            source.set_points_as_corners(&corners).unwrap();
        }
        let after = source.state().unwrap();
        assert_ne!(scene.revision(), revision);
        assert_eq!(source.node_id(), identity);
        assert_eq!(after.style, before.style);
        assert_eq!(after.presentation(), before.presentation());
        assert_eq!(copy.state().unwrap().content, before.content);
        assert_eq!(copy.state().unwrap().transform, before.transform);
        assert_eq!(unrelated.state().unwrap(), other);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            count + 1
        );
        assert_eq!(source.path_query().unwrap().start().unwrap(), (-2., -1.));
        assert_eq!(source.path_query().unwrap().end().unwrap(), (2., -1.));
        let revision = scene.revision();
        if live_mode {
            let mut live = scene.live(&mut session);
            assert_eq!(
                live.effective_path_query(&source).unwrap().end().unwrap(),
                (2., -1.)
            );
            live.set_points_as_corners(&source, &corners).unwrap();
        } else {
            source.set_points_as_corners(&corners).unwrap();
        }
        assert_eq!(
            scene.revision(),
            revision,
            "identical content must not republish"
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            count + 1
        );
    }
}

#[test]
fn empty_corner_inputs_and_nonfinite_rejection_follow_shared_semantics() {
    let scene = Scene::new();
    let mut object = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    let revision = scene.revision();
    let count = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    for corners in [vec![], vec![Vec2::new(1., 2.)]] {
        object.set_points_as_corners(&corners).unwrap();
        assert_eq!(object.path_query().unwrap().arc_length(None).unwrap(), 0.);
        assert!(object.path_query().unwrap().start().is_err());
        assert_eq!(scene.revision(), revision);
    }
    let before = object.state().unwrap();
    assert!(matches!(
        object.set_points_as_corners(&[Vec2::new(f32::NAN, 0.)]),
        Err(AuthoringError::NonFiniteGeometry)
    ));
    assert_eq!(object.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        count
    );
}

#[test]
fn rejected_live_edit_does_not_allocate_or_publish_and_completion_allows_edit() {
    let scene = Scene::new();
    let object = scene.square(1.).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live
        .declare_and_activate_create(&object, AnimationOptions::new())
        .unwrap();
    live.advance_segment_to(segment, 0.5).unwrap();
    let before = object.state().unwrap();
    let revision = object.integration_store().borrow().scene_revision();
    let resources = object
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    let corners = [Vec2::ZERO, Vec2::new(1., 1.)];
    assert!(live.set_points_as_corners(&object, &corners).is_err());
    assert_eq!(object.state().unwrap(), before);
    assert_eq!(
        object.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(
        object
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    live.set_points_as_corners(&object, &corners).unwrap();
    assert_eq!(
        live.effective_path_query(&object).unwrap().end().unwrap(),
        (1., 1.)
    );
}

#[test]
fn paired_path_editing_uses_the_shared_execution_session() {
    let mut session = noon::example_scenes::path_editing::session().unwrap();
    session.seek(0.).unwrap();
    assert_eq!(session.frame().objects.len(), 3);
    let start = session.frame().objects.clone();
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, start);
}
