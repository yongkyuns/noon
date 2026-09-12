use noon::{AnimationOptions, ManimGeometryOptions, Scene, Vec2, VectorPath};

fn geometry_resource_count(scene: &Scene) -> usize {
    scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len()
}

#[test]
fn alignment_changes_only_selected_content_and_preserves_identity_style_and_bounds() {
    let scene = Scene::new();
    let mut a = scene.line((0., 0.), (3., 0.)).unwrap();
    a.shift(1., 2.).unwrap();
    a.set_stroke_color(1., 0., 0., 0.5).unwrap();
    let mut b = scene
        .geometry(
            ManimGeometryOptions::path(VectorPath::new().move_to(Vec2::ZERO).cubic_to(
                Vec2::new(1., 2.),
                Vec2::new(2., 2.),
                Vec2::new(3., 0.),
            ))
            .unwrap(),
        )
        .unwrap();
    b.insert_n_curves(3).unwrap();
    let unrelated = scene.square(1.).unwrap();
    let before = [
        a.state().unwrap(),
        b.state().unwrap(),
        unrelated.state().unwrap(),
    ];
    let bounds = a.layout_bounds().unwrap();
    a.align_points(&b).unwrap();
    assert_eq!(a.path_query().unwrap().curve_count(), 4);
    assert_eq!(b.path_query().unwrap().curve_count(), 4);
    assert_eq!(a.layout_bounds().unwrap(), bounds);
    assert_eq!(a.state().unwrap().style, before[0].style);
    assert_eq!(b.state().unwrap(), before[1]);
    assert_eq!(unrelated.state().unwrap(), before[2]);
    let revision = scene.revision();
    let resources = geometry_resource_count(&scene);
    a.align_points(&b).unwrap();
    a.align_points(&a).unwrap();
    assert_eq!(scene.revision(), revision);
    assert_eq!(geometry_resource_count(&scene), resources);
}

#[test]
fn foreign_or_nonvector_operand_cannot_partially_publish() {
    let scene = Scene::new();
    let a = scene.square(1.).unwrap();
    let foreign = Scene::new().circle(1.).unwrap();
    let revision = scene.revision();
    let resources = geometry_resource_count(&scene);
    let before = a.state().unwrap();
    assert!(a.align_points(&foreign).is_err());
    assert_eq!(a.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(geometry_resource_count(&scene), resources);
}

#[test]
fn live_alignment_publishes_both_operands_coherently() {
    let mut scene = Scene::new();
    let a = scene.line((0., 0.), (3., 0.)).unwrap();
    let b = scene.square(2.).unwrap();
    scene.add(&a).unwrap();
    scene.add(&b).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.shift(&a, 1., 2.).unwrap();
    live.align_points(&a, &b).unwrap();
    assert_eq!(live.effective_path_query(&a).unwrap().curve_count(), 4);
    assert_eq!(live.effective_path_query(&b).unwrap().curve_count(), 4);
    assert_eq!(
        live.effective_path_query(&a).unwrap().start().unwrap(),
        (1., 2.)
    );
}

#[test]
fn live_self_alignment_is_noop_before_unsupported_current_capture() {
    let scene = Scene::new();
    let object = scene.circle(1.).unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .declare_and_activate_create(&object, AnimationOptions::new())
        .unwrap();
    session.take_frame_changes();
    let revision = scene.revision();
    let resources = geometry_resource_count(&scene);

    scene
        .live(&mut session)
        .align_points(&object, &object)
        .unwrap();

    assert_eq!(scene.revision(), revision);
    assert_eq!(geometry_resource_count(&scene), resources);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn live_foreign_alignment_fails_before_resource_or_frame_publication() {
    let mut scene = Scene::new();
    let object = scene.line((0., 0.), (3., 0.)).unwrap();
    scene.add(&object).unwrap();
    let foreign = Scene::new().square(2.).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let before = object.state().unwrap();
    let revision = scene.revision();
    let resources = geometry_resource_count(&scene);

    assert!(scene
        .live(&mut session)
        .align_points(&object, &foreign)
        .is_err());

    assert_eq!(object.state().unwrap(), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(geometry_resource_count(&scene), resources);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn empty_operand_receives_null_geometry_and_closed_contour_stays_closed() {
    let scene = Scene::new();
    let empty = scene
        .geometry(ManimGeometryOptions::path(VectorPath::new()).unwrap())
        .unwrap();
    let square = scene.square(2.).unwrap();
    empty.align_points(&square).unwrap();
    assert_eq!(empty.path_query().unwrap().curve_count(), 4);
    assert_eq!(empty.path_query().unwrap().start().unwrap(), (0., 0.));
    assert!(square.path_query().unwrap().is_closed().unwrap());
}

#[test]
fn paired_alignment_example_runs_through_shared_runtime() {
    let mut session = noon::example_scenes::path_alignment::session().unwrap();
    session.seek(0.1).unwrap();
    assert_eq!(session.frame().objects.len(), 2);
}
