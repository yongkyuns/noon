use super::*;

fn options() -> ManimAxesOptions {
    ManimAxesOptions::new([-2.0, 2.0, 1.0], [-1.0, 1.0, 1.0], 4.0, 2.0)
}

fn resources(scene: &Scene) -> (usize, usize, usize) {
    let store = scene.integration_store().borrow();
    (
        store.geometry_resources().len(),
        store.text_resources().len(),
        store.font_resources().len(),
    )
}

fn near(actual: [f64; 2], expected: [f64; 2]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 2e-5, "{actual} != {expected}");
    }
}

#[test]
fn running_coordinates_are_detached_until_admitted_and_keep_identity_on_reentry() {
    let mut scene = Scene::new();
    let sentinel = scene.circle(0.2).unwrap();
    scene.add(&sentinel).unwrap();
    let state = sentinel.state().unwrap();
    let counts = resources(&scene);
    let mut execution = scene.into_execution().unwrap();
    let original_frame = execution.session().frame().objects.clone();
    let axes = execution.scene_mut().axes(&options()).unwrap();
    let line = execution
        .scene_mut()
        .number_line(&ManimNumberLineOptions::new([2.0, 6.0, 1.0]))
        .unwrap();
    near(
        execution
            .scene()
            .effective_axes_frame(&axes)
            .unwrap()
            .coords_to_point(1.0, 0.5)
            .unwrap(),
        [1.0, 0.5],
    );
    near(
        execution
            .scene()
            .effective_number_line_frame(&line)
            .unwrap()
            .number_to_point(4.0)
            .unwrap(),
        [0.0, 0.0],
    );
    assert_eq!(execution.session().frame().objects, original_frame);
    let scene = execution.scene_mut();
    assert_eq!(resources(scene), counts);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![sentinel.node_id()]
    );
    scene.shift_family(axes.family(), 2.0, -1.0).unwrap();
    let frame = scene.effective_axes_frame(&axes).unwrap();
    near(frame.coords_to_point(1.0, 0.5).unwrap(), [3.0, -0.5]);
    near(frame.point_to_coords([3.0, -0.5]).unwrap(), [1.0, 0.5]);
    scene
        .add_many(&[axes.family().into(), line.family().into()])
        .unwrap();
    let identity = axes.family().node_id();
    scene.remove_many(&[axes.family().into()]).unwrap();
    scene.add_many(&[axes.family().into()]).unwrap();
    assert_eq!(axes.family().node_id(), identity);
    near(
        scene
            .effective_axes_frame(&axes)
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [2.0, -1.0],
    );
    assert_eq!(resources(scene), counts);
    assert_eq!(sentinel.state().unwrap(), state);
    assert!(execution.session().frame().objects.len() > original_frame.len());
}

#[test]
fn invalid_coordinate_options_publish_no_partial_family() {
    let mut execution = Scene::new().into_execution().unwrap();
    let before = execution.session().publication_context();
    let scene = execution.scene_mut();
    let revision = scene.revision();
    let count = scene.integration_store().borrow().len();
    let mut invalid = options();
    invalid.y_length = f64::NAN;
    assert!(scene.axes(&invalid).is_err());
    invalid.y_length = 2.0;
    invalid.ticks.limit = 1;
    assert!(scene.axes(&invalid).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert_eq!(resources(scene), (0, 0, 0));
    assert_eq!(execution.session().publication_context(), before);
}

#[test]
fn running_coordinates_reject_stale_and_foreign_publications() {
    let mut execution = Scene::new().into_execution().unwrap();
    let axes = execution.scene_mut().axes(&options()).unwrap();
    let line_options = ManimNumberLineOptions::new([0.0, 2.0, 1.0]);
    let line = execution.scene_mut().number_line(&line_options).unwrap();
    let other = Scene::new().into_execution().unwrap();
    assert!(other.scene().effective_axes_frame(&axes).is_err());
    assert!(other.scene().effective_number_line_frame(&line).is_err());

    let before = execution.session().publication_context();
    let scene = execution.scene_mut();
    // Bypass Scene publication through the raw store to simulate a stale host.
    Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.2).unwrap();
    let revision = scene.revision();
    let count = scene.integration_store().borrow().len();
    assert!(matches!(
        scene.axes(&options()),
        Err(CoordinateAuthoringError::Authoring(
            AuthoringError::ExecutionPublication(_)
        ))
    ));
    assert!(scene.number_line(&line_options).is_err());
    assert!(scene.effective_axes_frame(&axes).is_err());
    assert!(scene.effective_number_line_frame(&line).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert_eq!(execution.session().publication_context(), before);
}
