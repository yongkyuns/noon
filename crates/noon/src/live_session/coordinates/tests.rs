use super::*;
use crate::Scene;

fn options() -> ManimAxesOptions {
    ManimAxesOptions::new([-2.0, 2.0, 1.0], [-1.0, 1.0, 1.0], 4.0, 2.0)
}

fn plane_options() -> crate::ManimNumberPlaneOptions {
    let mut options = crate::ManimNumberPlaneOptions::new();
    options.x_range = [-2.0, 2.0, 1.0];
    options.y_range = [-1.0, 1.0, 1.0];
    options.x_length = Some(4.0);
    options.y_length = Some(2.0);
    options
}
fn resources(scene: &Scene) -> (usize, usize, usize) {
    let store = scene.integration_store().borrow();
    (
        store.geometry_resources().len(),
        store.text_resources().len(),
        store.font_resources().len(),
    )
}
fn near(a: [f64; 2], b: [f64; 2]) {
    for (a, b) in a.into_iter().zip(b) {
        assert!((a - b).abs() < 2e-5, "{a} != {b}");
    }
}

#[test]
fn complete_live_coordinates_stay_detached_until_explicit_admission() {
    let mut scene = Scene::new();
    let sentinel = scene.circle(0.2).unwrap();
    scene.add(&sentinel).unwrap();
    let original = sentinel.state().unwrap();
    let counts = resources(&scene);
    let mut execution = scene.execution_session().unwrap();
    let original_frame = execution.frame().objects.clone();
    let axes;
    let line;
    {
        let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        axes = live.axes(&options()).unwrap();
        line = live
            .number_line(&ManimNumberLineOptions::new([2.0, 6.0, 1.0]))
            .unwrap();
        near(
            live.effective_axes_frame(&axes)
                .unwrap()
                .coords_to_point(1.0, 0.5)
                .unwrap(),
            [1.0, 0.5],
        );
        near(
            live.effective_number_line_frame(&line)
                .unwrap()
                .number_to_point(4.0)
                .unwrap(),
            [0.0, 0.0],
        );
    }
    assert_eq!(execution.frame().objects, original_frame);
    assert_eq!(resources(&scene), counts);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap(),
        vec![sentinel.node_id()]
    );
    {
        let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        live.shift_family(axes.family(), 2.0, -1.0).unwrap();
        let frame = live.effective_axes_frame(&axes).unwrap();
        near(frame.coords_to_point(1.0, 0.5).unwrap(), [3.0, -0.5]);
        near(frame.point_to_coords([3.0, -0.5]).unwrap(), [1.0, 0.5]);
        live.add_many(&[axes.family().into(), line.family().into()])
            .unwrap();
    }
    assert!(execution.frame().objects.len() > original_frame.len());
    assert_eq!(sentinel.state().unwrap(), original);
    assert_eq!(resources(&scene), counts);
}

#[test]
fn invalid_second_axis_and_tick_budget_publish_no_partial_family() {
    let scene = Scene::new();
    let mut execution = scene.execution_session().unwrap();
    let revision = scene.revision();
    let count = scene.integration_store().borrow().len();
    let context = execution.publication_context();
    let mut invalid = options();
    invalid.y_length = f64::NAN;
    let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
    assert!(live.axes(&invalid).is_err());
    invalid.y_length = 2.0;
    invalid.ticks.limit = 1;
    assert!(live.axes(&invalid).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert_eq!(execution.publication_context(), context);
    assert_eq!(resources(&scene), (0, 0, 0));
}

#[test]
fn running_number_plane_construction_is_detached_atomic_and_local() {
    let mut scene = Scene::new();
    let sentinel = scene.circle(0.2).unwrap();
    scene.add(&sentinel).unwrap();
    scene.wait(0.25).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let before_time = scene.time();
    let before_frame = execution.frame().objects.clone();
    let before_context = execution.publication_context();
    let before_revision = scene.revision();
    let before_sentinel = sentinel.state().unwrap();
    let plane;
    {
        let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        plane = live.number_plane(&plane_options()).unwrap();
        near(
            live.effective_number_plane_frame(&plane)
                .unwrap()
                .coords_to_point(1.0, 0.5)
                .unwrap(),
            [1.0, 0.5],
        );
        live.add_many(&[plane.family().into()]).unwrap();
        live.shift_family(plane.family(), 2.0, -1.0).unwrap();
        near(
            live.effective_number_plane_frame(&plane)
                .unwrap()
                .coords_to_point(0.0, 0.0)
                .unwrap(),
            [2.0, -1.0],
        );
    }
    assert_eq!(scene.time(), before_time);
    assert_eq!(sentinel.state().unwrap(), before_sentinel);
    assert!(execution.frame().objects.len() > before_frame.len());
    assert_ne!(execution.publication_context(), before_context);

    let revision = scene.revision();
    let frame = execution.frame().objects.clone();
    let mut invalid = plane_options();
    invalid.x_length = Some(f64::NAN);
    let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
    assert!(live.number_plane(&invalid).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(execution.frame().objects, frame);
    assert!(scene.revision().get() > before_revision.get());
}

#[test]
fn stale_execution_rejects_valid_coordinates_before_committing_nodes() {
    let mut scene = Scene::new();
    let mut execution = scene.execution_session().unwrap();
    let sentinel = scene.circle(0.2).unwrap(); // Deliberately bypass live routing.
    let before = sentinel.state().unwrap();
    let revision = scene.revision();
    let count = scene.integration_store().borrow().len();
    let context = execution.publication_context();
    let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
    assert!(matches!(
        live.axes(&options()),
        Err(CoordinateAuthoringError::Live(_))
    ));
    assert!(live
        .number_line(&ManimNumberLineOptions::new([0.0, 2.0, 1.0]))
        .is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert_eq!(sentinel.state().unwrap(), before);
    assert_eq!(execution.publication_context(), context);
}

#[test]
fn foreign_scene_root_does_not_receive_coordinates_from_another_execution() {
    let scene = Scene::new();
    let other = Scene::new();
    let mut execution = scene.execution_session().unwrap();
    let count = other.integration_store().borrow().len();
    let revision = other.revision();
    let mut live = LiveSession::new(other.integration_store(), other.root(), &mut execution);
    assert!(live.axes(&options()).is_err());
    assert_eq!(other.integration_store().borrow().len(), count);
    assert_eq!(other.revision(), revision);
}

#[test]
fn live_coordinate_copy_and_readd_keep_identity_and_frame_mapping() {
    let scene = Scene::new();
    let mut execution = scene.execution_session().unwrap();
    let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
    let axes = live.axes(&options()).unwrap();
    live.add_many(&[axes.family().into()]).unwrap();
    let copy = live.copy_family(axes.family()).unwrap();
    let copy = ManimAxes::from_family(copy.root().clone()).unwrap();
    live.shift_family(copy.family(), 3.0, 2.0).unwrap();
    near(
        live.effective_axes_frame(&copy)
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [3.0, 2.0],
    );
    let identity = axes.family().node_id();
    live.remove_many(&[axes.family().into()]).unwrap();
    live.add_many(&[axes.family().into()]).unwrap();
    assert_eq!(axes.family().node_id(), identity);
    near(
        live.effective_axes_frame(&axes)
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [0.0, 0.0],
    );
}

#[test]
fn detached_coordinate_queries_reject_stale_and_foreign_publications() {
    let mut scene = Scene::new();
    let mut execution = scene.execution_session().unwrap();
    let axes;
    let line;
    {
        let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        axes = live.axes(&options()).unwrap();
        line = live
            .number_line(&ManimNumberLineOptions::new([0.0, 2.0, 1.0]))
            .unwrap();
    }
    let other = Scene::new();
    let mut foreign = other.execution_session().unwrap();
    {
        let live = LiveSession::new(other.integration_store(), other.root(), &mut foreign);
        assert!(live.effective_axes_frame(&axes).is_err());
        assert!(live.effective_number_line_frame(&line).is_err());
    }
    let context = execution.publication_context();
    scene.circle(0.2).unwrap(); // Deliberate out-of-band edit after construction.
    let revision = scene.revision();
    let live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
    assert!(live.effective_axes_frame(&axes).is_err());
    assert!(live.effective_number_line_frame(&line).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(execution.publication_context(), context);
}
