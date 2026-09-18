use super::*;
use crate::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram, LiveProgramStatus,
    LiveSession, ManimRotationPivot, RateFunction, RustHostCallbackTable,
};
use noon_core::{StrokeCap, StrokeJoin};

fn near(actual: [f64; 2], expected: [f64; 2]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 2.0e-5, "{actual} != {expected}");
    }
}

fn axes_options() -> ManimAxesOptions {
    ManimAxesOptions::new([-2.0, 2.0, 1.0], [-1.0, 1.0, 1.0], 4.0, 2.0)
}

fn plane_options() -> ManimNumberPlaneOptions {
    let mut options = ManimNumberPlaneOptions::new();
    options.x_range = [-2.0, 3.0, 1.0];
    options.y_range = [-1.0, 2.0, 1.0];
    options.x_length = Some(5.0);
    options.y_length = Some(3.0);
    options.faded_line_ratio = 2;
    options
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
fn number_plane_uses_pinned_flat_family_order_and_grid_classification() {
    let mut scene = Scene::new();
    let plane = scene.number_plane(&plane_options()).unwrap();
    let root = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(plane.family().node_id())
        .unwrap();
    assert_eq!(root.len(), 4);
    assert_eq!(root[0], plane.faded_lines().unwrap().node_id());
    assert_eq!(root[1], plane.background_lines().unwrap().node_id());
    assert_eq!(root[2], plane.x_axis().unwrap().family().node_id());
    assert_eq!(root[3], plane.y_axis().unwrap().family().node_id());
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .semantic_family_members_checked(plane.background_lines().unwrap().node_id())
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        store
            .semantic_family_members_checked(plane.faded_lines().unwrap().node_id())
            .unwrap()
            .len(),
        10
    );
    drop(store);
    near(
        plane
            .authored_frame()
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [-0.5, -0.5],
    );
    assert_eq!(resources(&scene), 0);
}

#[test]
fn number_plane_defaults_faded_style_from_background_without_changing_color() {
    let mut scene = Scene::new();
    let plane = scene.number_plane(&plane_options()).unwrap();
    let store = scene.integration_store().borrow();
    let background = store
        .semantic_family_members_checked(plane.background_lines().unwrap().node_id())
        .unwrap()[0];
    let faded = store
        .semantic_family_members_checked(plane.faded_lines().unwrap().node_id())
        .unwrap()[0];
    let background = store.semantic_object_state_checked(background).unwrap();
    let faded = store.semantic_object_state_checked(faded).unwrap();
    assert_eq!(background.style.stroke, faded.style.stroke);
    assert_eq!(
        faded.style.stroke_width,
        background.style.stroke_width * 0.5
    );
    assert_eq!(
        faded.style.stroke_opacity,
        background.style.stroke_opacity * 0.5
    );
    for style in [&background.style, &faded.style] {
        assert_eq!(style.stroke_width_mode, StrokeWidthMode::ScreenSpace);
        assert_eq!(style.stroke_join, StrokeJoin::Miter);
        assert_eq!(style.stroke_cap, StrokeCap::Butt);
    }
    let options = ManimNumberPlaneOptions::default();
    assert_eq!(
        options.axis_style.stroke_width_mode,
        StrokeWidthMode::ScreenSpace
    );
    assert_eq!(options.axis_style.stroke_join, StrokeJoin::Miter);
    assert_eq!(options.axis_style.stroke_cap, StrokeCap::Butt);
}

#[test]
fn number_plane_preparation_is_atomic_and_budgeted() {
    let mut scene = Scene::new();
    let sentinel = scene.circle(0.5).unwrap();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let mut invalid = plane_options();
    invalid.y_length = Some(f64::NAN);
    assert!(scene.number_plane(&invalid).is_err());
    invalid.y_length = Some(3.0);
    invalid.line_limit = 2;
    assert!(scene.number_plane(&invalid).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert!(sentinel.validate().is_ok());
}

#[test]
fn number_plane_coordinate_queries_ignore_appended_family_members() {
    let mut scene = Scene::new();
    let plane = scene.number_plane(&plane_options()).unwrap();
    let extra = scene.circle(0.25).unwrap();
    plane.family().add_many(&[(&extra).into()]).unwrap();
    near(
        plane
            .authored_frame()
            .unwrap()
            .coords_to_point(1.0, -0.5)
            .unwrap(),
        [0.5, -1.0],
    );
}

#[test]
fn mapped_curves_share_the_coordinate_snapshot_and_data_order() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    axes.family().shift(2.0, 1.0).unwrap();
    let calls = std::cell::Cell::new(0usize);
    let graph = axes
        .plot(
            |x| {
                calls.set(calls.get() + 1);
                x * x
            },
            Some(&[-1.0, 1.0, 0.5]),
            false,
        )
        .unwrap();
    assert_eq!(calls.get(), 5);
    assert_eq!(graph.path_query().unwrap().start().unwrap(), (1.0, 2.0));
    assert_eq!(graph.path_query().unwrap().end().unwrap(), (3.0, 2.0));
    let data = axes
        .plot_samples(&[[1.0, 0.0], [1.0, 1.0], [-1.0, 0.0]])
        .unwrap();
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

#[test]
fn axes_plot_creates_one_detached_object_in_the_axes_store() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    let nodes_before = scene.integration_store().borrow().len();
    let resources_before = resources(&scene);

    let graph = axes.plot(|x| x, Some(&[-1.0, 1.0, 1.0]), false).unwrap();

    assert!(Rc::ptr_eq(
        graph.integration_store(),
        axes.family().integration_store()
    ));
    let store = scene.integration_store().borrow();
    assert_eq!(store.len(), nodes_before + 1);
    assert_eq!(store.geometry_resources().len(), resources_before + 1);
    assert!(store
        .semantic_family_members_checked(scene.root())
        .unwrap()
        .is_empty());
    assert!(store.semantic_object_state_checked(graph.node_id()).is_ok());
}

#[test]
fn axes_plot_preparation_errors_allocate_no_identity_or_resource() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let resource_count = resources(&scene);
    let calls = std::cell::Cell::new(0usize);

    assert!(axes
        .plot(
            |x| {
                calls.set(calls.get() + 1);
                x
            },
            Some(&[-1.0, 1.0, 0.0]),
            false,
        )
        .is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(resources(&scene), resource_count);
}

#[test]
fn axes_plot_rejects_invalid_topology_before_running_the_callback() {
    let mut scene = Scene::new();
    let axes = scene.axes(&axes_options()).unwrap();
    let x = axes.x_axis().unwrap();
    axes.family().remove_many(&[x.family().into()]).unwrap();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let resource_count = resources(&scene);
    let calls = std::cell::Cell::new(0usize);

    assert!(matches!(
        axes.plot(
            |x| {
                calls.set(calls.get() + 1);
                x
            },
            None,
            false,
        ),
        Err(CoordinateAuthoringError::InvalidTopology)
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
    assert_eq!(resources(&scene), resource_count);
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

#[test]
fn large_offset_axes_map_retained_data_without_midpoint_rounding() {
    let mut scene = Scene::new();
    let low = 1.0e16;
    let high = low + 2.0;
    let mut options = ManimAxesOptions::new([low, high, 2.0], [-1.0, 1.0, 1.0], 8.0, 4.0);
    options.ticks.enabled = false;
    let axes = scene.axes(&options).unwrap();
    let frame = axes.authored_frame().unwrap();
    near(frame.coords_to_point(low, -1.0).unwrap(), [-4.0, -2.0]);
    near(frame.coords_to_point(high, 1.0).unwrap(), [4.0, 2.0]);
    let points = [[low, -1.0], [high, 1.0]];
    let data = ManimGeometryOptions::axes_sampled_plot(frame, &points).unwrap();
    let curve = scene.geometry(data).unwrap();
    assert_eq!(curve.path_query().unwrap().start().unwrap(), (-4.0, -2.0));
    assert_eq!(curve.path_query().unwrap().end().unwrap(), (4.0, 2.0));
    assert_eq!(axes.x_axis().unwrap().range().unwrap(), [low, high, 2.0]);
}

#[test]
fn exact_tick_capacity_constructs_and_one_less_rejects_atomically() {
    let mut scene = Scene::new();
    let mut options = ManimNumberLineOptions::new([-2.0, 2.0, 1.0]);
    options.ticks.limit = 5;
    let line = scene.number_line(&options).unwrap();
    let ticks = line.ticks().unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(ticks.node_id())
            .unwrap()
            .len(),
        5
    );
    let revision = scene.revision();
    let shaft_before = line.shaft().unwrap().state().unwrap();
    options.ticks.limit = 4;
    assert!(scene.number_line(&options).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(line.shaft().unwrap().state().unwrap(), shaft_before);
    assert_eq!(resources(&scene), 0);
}

#[test]
fn number_plane_effective_coordinates_seek_without_resource_or_revision_churn() {
    let mut scene = Scene::new();
    let plane = scene.number_plane(&plane_options()).unwrap();
    let target = plane.family().copy_family().unwrap();
    target.root().shift(2.0, 4.0).unwrap();
    scene.add_many(&[plane.family().into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    scene
        .live(&mut execution)
        .declare_and_activate_family_transform_to(
            plane.family(),
            target.root(),
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .stats();
    execution.seek(0.5).unwrap();
    let direct = plane.effective_frame(&execution).unwrap();
    near(direct.coords_to_point(0.0, 0.0).unwrap(), [0.5, 1.5]);
    near(
        plane
            .authored_frame()
            .unwrap()
            .coords_to_point(0.0, 0.0)
            .unwrap(),
        [-0.5, -0.5],
    );
    execution.seek(0.0).unwrap();
    execution.seek(0.25).unwrap();
    execution.seek(0.5).unwrap();
    assert_eq!(plane.effective_frame(&execution).unwrap(), direct);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats(),
        resources
    );
}

#[test]
fn number_plane_thirds_preserve_numpy_arange_boundary_classification() {
    let mut scene = Scene::new();
    let plane = scene
        .number_plane(&ManimNumberPlaneOptions {
            x_range: [-6.0, -2.0, 1.0],
            y_range: [-2.0, 2.0, 1.0],
            faded_line_ratio: 3,
            ..Default::default()
        })
        .unwrap();
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .node(plane.background_lines().unwrap().node_id())
            .unwrap()
            .member_count(),
        7
    );
    assert_eq!(
        store
            .node(plane.faded_lines().unwrap().node_id())
            .unwrap()
            .member_count(),
        18
    );
}

#[test]
fn number_plane_grid_offsets_respect_nonuniform_axis_units() {
    let mut scene = Scene::new();
    let plane = scene
        .number_plane(&ManimNumberPlaneOptions {
            x_range: [2.0, 6.0, 1.0],
            y_range: [-3.0, 1.0, 0.75],
            x_length: Some(5.0),
            y_length: Some(3.0),
            faded_line_ratio: 0,
            ..Default::default()
        })
        .unwrap();
    let members = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(plane.background_lines().unwrap().node_id())
        .unwrap();
    let horizontal = Mobject::from_node(Rc::clone(scene.integration_store()), members[1]).unwrap();
    let vertical = Mobject::from_node(Rc::clone(scene.integration_store()), members[6]).unwrap();
    let (x, y) = horizontal.path_query().unwrap().start().unwrap();
    near([x, y], [-2.5, 1.3125]);
    let (x, y) = vertical.path_query().unwrap().start().unwrap();
    near([x, y], [-1.25, -1.5]);
}
