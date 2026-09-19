use super::*;

fn partition([start, end, dx]: [f64; 3]) -> Vec<f64> {
    let plan = riemann_partition_plan([start, end], dx).unwrap();
    let mut parameters = plan.parameters().to_vec();
    assert_eq!(parameters.pop(), Some(end));
    parameters
}

#[test]
fn partition_removes_only_the_appended_terminal_endpoint() {
    assert_eq!(partition([0.0, 1.0, 0.25]), [0.0, 0.25, 0.5, 0.75]);
    assert_eq!(partition([0.0, 1.1, 0.5]), [0.0, 0.5, 1.0]);
}

#[test]
fn nonempty_underflow_partition_keeps_its_start() {
    for range in [[0.0, 1.0e-300, 1.0e100], [0.0, f64::from_bits(1), 2.0]] {
        assert_eq!(partition(range), [range[0]]);
    }
}

#[test]
fn one_rectangle_does_not_compute_an_unused_overflowing_increment() {
    assert_eq!(partition([1.0e308, 1.25e308, 1.0e308]), [1.0e308]);
}

#[test]
fn partition_preserves_numpy_representable_increment_and_rounded_end() {
    for start in [1.0e16, -1.0e16] {
        assert_eq!(
            partition([start, start + 8.0, 3.0]),
            [start, start + 4.0, start + 8.0]
        );
    }
}

#[test]
fn partition_does_not_truncate_repeated_regular_samples() {
    assert_eq!(
        partition([1.0, 1.0 + f64::EPSILON, f64::EPSILON / 4.0]),
        [1.0; 4]
    );
}

#[test]
fn rectangle_budget_does_not_charge_the_planners_appended_endpoint() {
    let limit = noon_geometry::DEFAULT_PLOT_SAMPLE_LIMIT;
    let plan = riemann_partition_plan([0.0, limit as f64], 1.0).unwrap();
    assert_eq!(plan.parameters().len(), limit + 1);
    assert_eq!(plan.parameters()[limit - 1], (limit - 1) as f64);
    assert_eq!(plan.parameters()[limit], limit as f64);
    assert_eq!(
        riemann_partition_plan([0.0, (limit + 1) as f64], 1.0),
        Err(PlotPreparationError::SampleLimitExceeded)
    );
}

fn constant_graph(range: [f64; 3], length: f64) -> (Scene, ManimAxes, Mobject) {
    let mut scene = Scene::new();
    let mut options = ManimAxesOptions::new(range, [0.0, 2.0, 1.0], length, 2.0);
    options.ticks.enabled = false;
    let axes = scene.axes(&options).unwrap();
    let graph = axes.plot(|_| 1.0, Some(&range), false).unwrap();
    (scene, axes, graph)
}

fn assert_rectangle_starts(scene: &Scene, family: &MobjectFamily, expected_x: &[f64]) {
    let members = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .unwrap();
    assert_eq!(members.len(), expected_x.len());
    for (&id, &x) in members.iter().zip(expected_x) {
        let leaf = Mobject::from_node(Rc::clone(scene.integration_store()), id).unwrap();
        let point = leaf.path_query().unwrap().anchors()[0];
        assert!((point.0 - x).abs() < 1.0e-6, "{point:?} != ({x}, 0)");
        assert!(point.1.abs() < 1.0e-6, "{point:?} != ({x}, 0)");
    }
}

#[test]
fn cold_and_live_rectangles_use_the_same_rounded_partition_geometry() {
    for start in [1.0e16, -1.0e16] {
        let (mut scene, axes, graph) = constant_graph([start, start + 12.0, 4.0], 12.0);
        let options = RiemannRectangleOptions {
            x_range: Some([start, start + 8.0]),
            dx: 3.0,
            width_scale_factor: 1.0,
            ..Default::default()
        };
        let cold = axes
            .get_riemann_rectangles(&graph, options.clone())
            .unwrap();
        assert_rectangle_starts(&scene, &cold, &[-2.0, 2.0, 6.0]);

        scene
            .add_many(&[axes.family().into(), (&graph).into()])
            .unwrap();
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        let revision = scene.revision();
        let live = scene
            .effective_riemann_rectangles(&axes, &graph, options)
            .unwrap();
        assert_eq!(scene.revision().get(), revision.get() + 1);
        assert_rectangle_starts(&scene, &live, &[-2.0, 2.0, 6.0]);
    }
}

#[test]
fn rounded_duplicate_samples_keep_distinct_rectangle_family_members() {
    let (scene, axes, graph) = constant_graph([1.0, 1.0 + f64::EPSILON, f64::EPSILON], 4.0);
    let rectangles = axes
        .get_riemann_rectangles(
            &graph,
            RiemannRectangleOptions {
                dx: f64::EPSILON / 4.0,
                width_scale_factor: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
    assert_rectangle_starts(&scene, &rectangles, &[-2.0; 4]);
}

#[test]
fn partition_and_late_sample_failures_leave_cold_and_live_stores_unchanged() {
    for live in [false, true] {
        let (mut scene, axes, graph) = constant_graph([0.0, 1.0, 0.5], 4.0);
        if live {
            scene
                .add_many(&[axes.family().into(), (&graph).into()])
                .unwrap();
            let execution = scene.execution_session().unwrap();
            scene.install_execution(execution);
        }
        for (dx, sample) in [
            (f64::MIN_POSITIVE, RiemannSample::Left),
            (0.6, RiemannSample::Right),
        ] {
            let revision = scene.revision();
            let nodes = scene.integration_store().borrow().len();
            let resources = scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len();
            let options = RiemannRectangleOptions {
                dx,
                sample,
                ..Default::default()
            };
            // Right sampling prepares one rectangle before the next sample
            // exceeds the retained domain. Neither failure may publish a prefix.
            let result = if live {
                scene.effective_riemann_rectangles(&axes, &graph, options)
            } else {
                axes.get_riemann_rectangles(&graph, options)
            };
            assert!(result.is_err());
            assert_eq!(scene.revision(), revision);
            let store = scene.integration_store().borrow();
            assert_eq!(store.len(), nodes);
            assert_eq!(store.geometry_resources().len(), resources);
        }
    }
}
