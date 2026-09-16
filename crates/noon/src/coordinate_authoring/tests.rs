use super::*;
use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveProgramStatus,
    LiveSession, ManimRotationPivot, RateFunction, RustHostCallbackTable,
};

fn near(actual: [f64; 2], expected: [f64; 2]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 2.0e-5, "{actual} != {expected}");
    }
}

fn axes_options() -> ManimAxesOptions {
    ManimAxesOptions::new([-2.0, 2.0, 1.0], [-1.0, 1.0, 1.0], 4.0, 2.0)
}

fn resources(scene: &Scene) -> usize {
    scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len()
}

#[test]
fn number_line_range_and_ticks_live_in_the_semantic_family() {
    let mut scene = Scene::new();
    let mut options = ManimNumberLineOptions::new([2.0, 6.0, 1.0]);
    options.length = Some(8.0);
    let line = scene.number_line(&options).unwrap();
    assert_eq!(line.range().unwrap(), [2.0, 6.0, 1.0]);
    let frame = line.authored_frame().unwrap();
    near(frame.number_to_point(4.0).unwrap(), [0.0, 0.0]);
    assert_eq!(frame.unit_size(), 2.0);
    let ticks = line.ticks().unwrap();
    let store = scene.integration_store().borrow();
    let tick_nodes = store
        .semantic_family_members_checked(ticks.node_id())
        .unwrap();
    assert_eq!(tick_nodes.len(), 5);
    let first = tick_nodes[0];
    drop(store);
    let tick = Mobject::from_node(Rc::clone(scene.integration_store()), first).unwrap();
    assert!((tick.path_query().unwrap().arc_length(None).unwrap() - 0.2).abs() < 1.0e-6);
    assert_eq!(resources(&scene), 0);
}

#[test]
fn axes_positive_negative_ranges_use_numerical_midpoints() {
    let mut scene = Scene::new();
    let axes = scene
        .axes(&ManimAxesOptions::new(
            [2.0, 6.0, 1.0],
            [-6.0, -2.0, 1.0],
            8.0,
            4.0,
        ))
        .unwrap();
    let frame = axes.authored_frame().unwrap();
    near(frame.coords_to_point(4.0, -4.0).unwrap(), [0.0, 0.0]);
    near(frame.coords_to_point(2.0, -6.0).unwrap(), [-4.0, -2.0]);
    near(frame.point_to_coords([4.0, 2.0]).unwrap(), [6.0, -2.0]);
    near(frame.x().start(), [-4.0, 2.0]);
    near(frame.y().start(), [-4.0, -2.0]);
}

#[test]
fn affine_family_edits_keep_ranges_and_round_trips() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    axes.family().scale(-1.5, 2.0).unwrap();
    axes.family()
        .rotate(0.7, ManimRotationPivot::Center)
        .unwrap();
    axes.family().shift(3.0, -2.0).unwrap();
    let frame = axes.authored_frame().unwrap();
    for point in [[0.0, 0.0], [1.0, 0.5], [-3.0, 2.0]] {
        let world = frame.coords_to_point(point[0], point[1]).unwrap();
        near(frame.point_to_coords(world).unwrap(), point);
    }
    assert_eq!(axes.x_axis().unwrap().range().unwrap(), [-2.0, 2.0, 1.0]);
    assert_eq!(resources(&scene), 0);
}

#[test]
fn second_axis_failure_and_tick_budget_are_atomic() {
    let mut scene = Scene::new();
    let sentinel = scene.circle(0.5).unwrap();
    let before = sentinel.state().unwrap();
    let revision = scene.revision();
    let mut options = axes_options();
    options.y_length = f64::NAN;
    assert!(scene.axes(&options).is_err());
    assert_eq!(scene.revision(), revision);
    options.y_length = 2.0;
    options.ticks.limit = 1;
    assert!(scene.axes(&options).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(sentinel.state().unwrap(), before);
    assert_eq!(resources(&scene), 0);
    assert!(scene.axes(&axes_options()).is_ok());
}

#[test]
fn shared_family_copy_reconstructs_ranges_without_wrapper_metadata() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    axes.family().shift(2.0, 3.0).unwrap();
    let copied = axes.family().copy_family().unwrap();
    let copy = ManimAxes::from_family(copied.root().clone()).unwrap();
    assert_ne!(copy.family().node_id(), axes.family().node_id());
    assert_eq!(
        copy.authored_frame().unwrap(),
        axes.authored_frame().unwrap()
    );
    copy.family().shift(1.0, 0.0).unwrap();
    near(
        axes.authored_frame()
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [2.0, 3.0],
    );
    near(
        copy.authored_frame()
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [3.0, 3.0],
    );
}

#[test]
fn membership_edits_do_not_leave_a_cached_axis_behind() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    let x = axes.x_axis().unwrap();
    axes.family().remove_many(&[x.family().into()]).unwrap();
    assert!(matches!(
        axes.authored_frame(),
        Err(CoordinateAuthoringError::InvalidTopology)
    ));
}

#[test]
fn cold_scene_effective_query_does_not_fall_back() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    assert!(scene.effective_axes_frame(&axes).is_err());
    assert!(axes.authored_frame().is_ok());
}

#[test]
fn mapped_curves_share_the_coordinate_snapshot_and_data_order() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    axes.family().shift(2.0, 1.0).unwrap();
    let frame = axes.authored_frame().unwrap();
    let sampling = axes.plot_sampling(Some(&[-1.0, 1.0, 0.5])).unwrap();
    let calls = std::cell::Cell::new(0usize);
    let options = ManimGeometryOptions::axes_function_plot(
        frame,
        &sampling,
        |x| {
            calls.set(calls.get() + 1);
            x * x
        },
        false,
    )
    .unwrap();
    let graph = scene.geometry(options).unwrap();
    assert_eq!(calls.get(), 5);
    assert_eq!(graph.path_query().unwrap().start().unwrap(), (1.0, 2.0));
    assert_eq!(graph.path_query().unwrap().end().unwrap(), (3.0, 2.0));
    let data =
        ManimGeometryOptions::axes_sampled_plot(frame, &[[1.0, 0.0], [1.0, 1.0], [-1.0, 0.0]])
            .unwrap();
    let data = scene.geometry(data).unwrap();
    assert_eq!(data.path_query().unwrap().curve_count(), 2);
    assert_eq!(data.path_query().unwrap().start().unwrap(), (3.0, 1.0));
    assert_eq!(data.path_query().unwrap().end().unwrap(), (1.0, 1.0));
    scene
        .add_many(&[axes.family().into(), (&graph).into(), (&data).into()])
        .unwrap();
    let session = scene.execution_session().unwrap();
    assert!(!session.frame().objects.is_empty());
    assert_eq!(calls.get(), 5);
}

struct MovingAxes {
    source: ManimAxes,
    target: MobjectFamily,
    started: bool,
}

impl LiveContinuation for MovingAxes {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.started {
            return Ok(ContinuationStep::Finished);
        }
        self.started = true;
        live.declare_and_activate_family_transform_to(
            self.source.family(),
            &self.target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .map(ContinuationStep::Await)
        .map_err(|error| error.to_string())
    }
}

fn moving_axes() -> (ManimAxes, LiveProgram<MovingAxes>) {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    let copy = axes.family().copy_family().unwrap();
    copy.root().shift(2.0, 4.0).unwrap();
    scene.add_many(&[axes.family().into()]).unwrap();
    let program = scene
        .into_live_program(MovingAxes {
            source: axes.clone(),
            target: copy.root().clone(),
            started: false,
        })
        .unwrap();
    (axes, program)
}

#[test]
fn effective_axis_queries_follow_active_drivers_without_semantic_churn() {
    let (axes, mut program) = moving_axes();
    assert!(matches!(
        program.resume().unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    let revision = axes.family().integration_store().borrow().scene_revision();
    let mut callbacks = RustHostCallbackTable::new();
    assert!(matches!(
        program.drive_to(&mut callbacks, 0.5).unwrap(),
        LiveProgramStatus::Awaiting(_)
    ));
    near(
        axes.authored_frame()
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [0.0, 0.0],
    );
    let frame = axes.effective_frame(program.session()).unwrap();
    near(frame.coords_to_point(0.0, 0.0).unwrap(), [1.0, 2.0]);
    near(frame.point_to_coords([1.5, 2.25]).unwrap(), [0.5, 0.25]);
    assert_eq!(
        axes.family().integration_store().borrow().scene_revision(),
        revision
    );
    let status = program.drive_to(&mut callbacks, 1.0).unwrap();
    assert!(matches!(status, LiveProgramStatus::PublicationPending(_)));
    let context = program.take_renderer_publication().context();
    program.admit_publication(context).unwrap();
    near(
        axes.authored_frame()
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [2.0, 4.0],
    );
}

#[test]
fn direct_and_forward_effective_coordinate_samples_agree() {
    let (direct_axes, mut direct) = moving_axes();
    let (forward_axes, mut forward) = moving_axes();
    direct.resume().unwrap();
    forward.resume().unwrap();
    let mut direct_callbacks = RustHostCallbackTable::new();
    let mut forward_callbacks = RustHostCallbackTable::new();
    direct.drive_to(&mut direct_callbacks, 0.5).unwrap();
    forward.drive_to(&mut forward_callbacks, 0.25).unwrap();
    forward.drive_to(&mut forward_callbacks, 0.5).unwrap();
    assert_eq!(
        direct_axes.effective_frame(direct.session()).unwrap(),
        forward_axes.effective_frame(forward.session()).unwrap(),
    );
}
