use noon::{ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

#[test]
fn partial_uses_curve_count_and_preserves_destination_identity_and_style() {
    for live_mode in [false, true] {
        let mut scene = Scene::new();
        let mut source = scene
            .geometry(
                ManimGeometryOptions::path(
                    VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(2., 0.))
                        .move_to(Vec2::new(10., 0.))
                        .line_to(Vec2::new(18., 0.)),
                )
                .unwrap(),
            )
            .unwrap();
        source.shift(0., 2.).unwrap();
        let mut destination = scene.circle(1.).unwrap();
        destination.set_z_index(3.5).unwrap();
        destination.set_fill(0., 1., 0., 0.4).unwrap();
        let before = destination.state().unwrap();
        let source_before = source.state().unwrap();
        let id = destination.node_id();
        scene
            .add_many(&[(&source).into(), (&destination).into()])
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        if live_mode {
            scene
                .live(&mut session)
                .pointwise_become_partial(&destination, &source, 0.25, 0.75)
                .unwrap();
        } else {
            destination
                .pointwise_become_partial(&source, 0.25, 0.75)
                .unwrap();
        }
        let query = destination.path_query().unwrap();
        assert_eq!(query.start().unwrap(), (1., 2.));
        assert_eq!(query.end().unwrap(), (14., 2.));
        assert_eq!(query.arc_length(None).unwrap(), 5.);
        assert_eq!(destination.node_id(), id);
        assert_eq!(destination.state().unwrap().style, before.style);
        assert_eq!(
            destination.state().unwrap().presentation(),
            before.presentation()
        );
        assert_eq!(source.state().unwrap(), source_before);
        if live_mode {
            let mut live = scene.live(&mut session);
            live.reverse_direction(&destination).unwrap();
            assert_eq!(
                live.effective_path_query(&destination)
                    .unwrap()
                    .start()
                    .unwrap(),
                (14., 2.)
            );
        } else {
            destination.reverse_direction().unwrap();
        }
        assert_eq!(destination.path_query().unwrap().end().unwrap(), (1., 2.));
    }
}

#[test]
fn partial_alias_empty_source_and_invalid_interval_are_atomic() {
    let scene = Scene::new();
    let mut object = scene.line((0., 0.), (4., 0.)).unwrap();
    let alias = object.clone();
    object.pointwise_become_partial(&alias, 0.25, 0.75).unwrap();
    assert_eq!(object.path_query().unwrap().start().unwrap(), (1., 0.));
    assert_eq!(object.path_query().unwrap().end().unwrap(), (3., 0.));
    let empty = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    let before = object.state().unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    object.pointwise_become_partial(&empty, 0.2, 0.8).unwrap();
    assert_eq!(object.state().unwrap(), before);
    for (a, b) in [(0.8, 0.2), (f64::NAN, 0.5), (0., 1.1)] {
        assert!(object.pointwise_become_partial(&alias, a, b).is_err());
        assert_eq!(object.state().unwrap(), before);
    }
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    object.pointwise_become_partial(&alias, 0.5, 0.5).unwrap();
    assert_eq!(
        object
            .path_query()
            .unwrap()
            .point_from_proportion(0.3)
            .unwrap(),
        (2., 0.)
    );
    assert_eq!(object.path_query().unwrap().arc_length(None).unwrap(), 0.);
}

#[test]
fn authored_boundary_curves_survive_subsequent_selection() {
    let scene = Scene::new();
    let source = scene
        .geometry(
            ManimGeometryOptions::path(
                VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(2., 0.))
                    .line_to(Vec2::new(4., 0.)),
            )
            .unwrap(),
        )
        .unwrap();
    let mut selected = source.copy_handle().unwrap();
    selected.pointwise_become_partial(&source, 0., 0.5).unwrap();
    let alias = selected.clone();
    // First extraction retains the second curve collapsed at its first anchor.
    selected.pointwise_become_partial(&alias, 0.5, 1.).unwrap();
    assert_eq!(selected.path_query().unwrap().start().unwrap(), (2., 0.));
    assert_eq!(selected.path_query().unwrap().arc_length(None).unwrap(), 0.);
}

#[test]
fn paired_path_selection_runs_on_shared_runtime() {
    let mut session = noon::example_scenes::path_selection::session().unwrap();
    session.seek(0.).unwrap();
    let initial = session.frame().objects.clone();
    assert_eq!(initial.len(), 2);
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, initial);
}

#[test]
fn live_partial_rejects_unrepresentable_source_before_allocating() {
    use noon::AnimationOptions;
    let mut scene = Scene::new();
    let source = scene.circle(1.).unwrap();
    let destination = scene.square(1.).unwrap();
    scene.add(&destination).unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = destination.state().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live
        .declare_and_activate_create(&source, AnimationOptions::new().run_time(1.))
        .unwrap();
    live.advance_segment_to(segment, 0.5).unwrap();
    let count = source
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    assert!(live
        .pointwise_become_partial(&destination, &source, 0.2, 0.8)
        .is_err());
    assert_eq!(destination.state().unwrap(), before);
    assert_eq!(
        source
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        count
    );
}
