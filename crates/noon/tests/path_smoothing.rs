use noon::{ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

fn corners() -> [Vec2; 3] {
    [Vec2::new(-2., 0.), Vec2::new(-1., 1.), Vec2::new(0., 0.)]
}

#[test]
fn smoothing_and_jagged_edits_preserve_identity_style_and_world_anchors() {
    for live_mode in [false, true] {
        let scene = Scene::new();
        let mut source = scene
            .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
            .unwrap();
        source.set_points_as_corners(&corners()).unwrap();
        source.shift(2., 3.).unwrap();
        let before = source.state().unwrap();
        let id = source.node_id();
        let anchors = source.path_query().unwrap().anchors();
        let copy = source.copy_handle().unwrap();
        let copy_before = copy.state().unwrap();
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            scene.live(&mut session).make_smooth(&source).unwrap();
        } else {
            source.make_smooth().unwrap();
        }
        assert_eq!(source.node_id(), id);
        assert_eq!(source.state().unwrap().style, before.style);
        assert_eq!(source.path_query().unwrap().anchors(), anchors);
        assert_eq!(copy.state().unwrap(), copy_before);
        assert_ne!(source.state().unwrap().content, before.content);
        if live_mode {
            scene.live(&mut session).make_jagged(&source).unwrap();
        } else {
            source.make_jagged().unwrap();
        }
        assert_eq!(
            source.path_query().unwrap().first_handles()[0],
            (1. / 3., 3. + 1. / 3.)
        );
        let revision = scene.revision();
        if live_mode {
            scene.live(&mut session).make_jagged(&source).unwrap();
        } else {
            source.make_jagged().unwrap();
        }
        assert_eq!(scene.revision(), revision);
    }
}

#[test]
fn family_smoothing_deduplicates_aliases_and_publishes_once() {
    for live_mode in [false, true] {
        let scene = Scene::new();
        let mut first = scene
            .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
            .unwrap();
        first.set_points_as_corners(&corners()).unwrap();
        let second = scene.square(2.).unwrap();
        let unrelated = scene.square(1.).unwrap();
        let before_unrelated = unrelated.state().unwrap();
        let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let family = scene.family(&[(&first).into(), (&nested).into()]).unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            scene
                .live(&mut session)
                .make_family_smooth(&family)
                .unwrap();
        } else {
            family.make_smooth().unwrap();
        }
        assert_eq!(scene.revision(), revision.checked_next().unwrap());
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources + 2
        );
        assert_eq!(unrelated.state().unwrap(), before_unrelated);
        let revision = scene.revision();
        if live_mode {
            scene
                .live(&mut session)
                .make_family_smooth(&family)
                .unwrap();
        } else {
            family.make_smooth().unwrap();
        }
        assert_eq!(scene.revision(), revision);
    }
}

#[test]
fn batch_resource_errors_reclaim_every_unpublished_path() {
    use noon_core::{GeometryResourceError, SemanticStore};
    let mut store = SemanticStore::new();
    let path = VectorPath::new()
        .move_to(Vec2::ZERO)
        .line_to(Vec2::new(1., 0.));
    let existing = store.insert_geometry_path(path.clone()).unwrap();
    let count = store.geometry_resources().len();
    let failed: Result<(), GeometryResourceError> = store.with_geometry_paths(
        [
            path.clone(),
            VectorPath::new().move_to(Vec2::new(f32::NAN, 0.)),
        ],
        |_, _| panic!("invalid batch must not publish"),
    );
    assert!(failed.is_err());
    assert_eq!(store.geometry_resources().len(), count);
    let failed: Result<(), GeometryResourceError> = store
        .with_geometry_paths([path.clone(), path], |_, _| {
            Err(GeometryResourceError::NonFinitePath)
        });
    assert!(failed.is_err());
    assert_eq!(store.geometry_resources().len(), count);
    assert!(store.geometry_resources().get(existing).is_some());
}

#[test]
fn invalid_smooth_corners_leave_content_and_resources_unchanged() {
    let scene = Scene::new();
    let mut source = scene.square(2.).unwrap();
    let before = source.state().unwrap();
    let revision = scene.revision();
    let count = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    assert!(source
        .set_points_smoothly(&[Vec2::ZERO, Vec2::new(f32::NAN, 1.)])
        .is_err());
    assert_eq!(source.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        count
    );
    source.set_points_smoothly(&corners()).unwrap();
    assert_eq!(source.path_query().unwrap().curve_count(), 2);
}

#[test]
fn paired_family_smoothing_example_uses_shared_execution() {
    let mut session = noon::example_scenes::path_smoothing::session().unwrap();
    session.seek(0.).unwrap();
    let objects = session.frame().objects.clone();
    assert_eq!(objects.len(), 4);
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, objects);
}
