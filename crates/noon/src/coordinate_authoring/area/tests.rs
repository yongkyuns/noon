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

fn quadratic_fixture() -> (Scene, ManimAxes, Mobject) {
    let mut scene = Scene::new();
    let mut options = ManimAxesOptions::new([-1.0, 1.0, 1.0], [-1.0, 1.0, 1.0], 2.0, 2.0);
    options.ticks.enabled = false;
    let axes = scene.axes(&options).unwrap();
    let graph = axes
        .plot(|x| x * x, Some(&[-1.0, 1.0, 1.0]), false)
        .unwrap();
    (scene, axes, graph)
}

fn midpoint_options() -> RiemannRectangleOptions {
    RiemannRectangleOptions {
        dx: 0.5,
        sample: RiemannSample::Center,
        width_scale_factor: 1.0,
        ..Default::default()
    }
}

fn family_top_points(scene: &Scene, family: &MobjectFamily) -> Vec<(f64, f64)> {
    let members = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .unwrap();
    members
        .into_iter()
        .map(|id| {
            Mobject::from_node(Rc::clone(scene.integration_store()), id)
                .unwrap()
                .path_query()
                .unwrap()
                .start()
                .unwrap()
        })
        .collect()
}

#[test]
fn exact_plan_geometry_is_not_coarse_retained_interpolation() {
    let (scene, axes, graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, midpoint_options()).unwrap();
    assert_eq!(plan.starts(), &[-1.0, -0.5, 0.0, 0.5]);
    assert_eq!(
        plan.samples().collect::<Vec<_>>(),
        [-0.75, -0.25, 0.25, 0.75]
    );
    let values = plan.samples().map(|x| x * x).collect::<Vec<_>>();
    let exact = plan.publish(Some(&values), None).unwrap();
    let retained = plan.publish(None, None).unwrap();
    let exact_points = family_top_points(&scene, &exact);
    let retained_points = family_top_points(&scene, &retained);
    for (i, (actual, coarse)) in exact_points.iter().zip(retained_points).enumerate() {
        assert!((actual.0 - (-0.5 + i as f64 * 0.5)).abs() < 1e-6);
        assert!((actual.1 - values[i]).abs() < 1e-6);
        assert!((coarse.1 - actual.1 - 0.1875).abs() < 1e-5);
    }
}

#[test]
fn plan_selects_left_right_and_center_without_replanning() {
    let (_, axes, graph) = quadratic_fixture();
    for (sample, expected) in [
        (RiemannSample::Left, [-1.0, -0.5, 0.0, 0.5]),
        (RiemannSample::Right, [-0.5, 0.0, 0.5, 1.0]),
        (RiemannSample::Center, [-0.75, -0.25, 0.25, 0.75]),
    ] {
        let plan = axes
            .riemann_plan(
                &graph,
                RiemannRectangleOptions {
                    sample,
                    ..midpoint_options()
                },
            )
            .unwrap();
        assert_eq!(plan.samples().collect::<Vec<_>>(), expected);
        assert_eq!(plan.starts(), &[-1.0, -0.5, 0.0, 0.5]);
    }
}

#[test]
fn exact_and_retained_sources_are_selected_independently() {
    let (_, axes, graph) = quadratic_fixture();
    let bound = axes
        .plot(|_| -0.25, Some(&[-1.0, 1.0, 1.0]), false)
        .unwrap();
    let plan = axes
        .riemann_plan(
            &graph,
            RiemannRectangleOptions {
                bounded_graph: Some(bound.node_id()),
                ..midpoint_options()
            },
        )
        .unwrap();
    let exact = [0.5625, 0.0625, 0.0625, 0.5625];
    let lower = [-0.25; 4];
    assert_eq!(
        plan.paths(Some(&exact), None).unwrap(),
        plan.paths(Some(&exact), Some(&lower)).unwrap()
    );
    assert_eq!(
        plan.paths(None, Some(&lower)).unwrap(),
        plan.paths(None, None).unwrap()
    );
    assert_ne!(
        plan.paths(Some(&exact), None).unwrap(),
        plan.paths(None, None).unwrap()
    );
}

#[test]
fn plan_keeps_its_original_axes_and_graph_snapshot_after_source_edits() {
    let (_, axes, mut graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, midpoint_options()).unwrap();
    let retained = plan.paths(None, None).unwrap();
    let exact = plan
        .paths(Some(&[0.5625, 0.0625, 0.0625, 0.5625]), None)
        .unwrap();
    axes.family().shift(3.0, 2.0).unwrap();
    graph.shift(-2.0, 0.75).unwrap();
    assert_eq!(plan.paths(None, None).unwrap(), retained);
    assert_eq!(
        plan.paths(Some(&[0.5625, 0.0625, 0.0625, 0.5625]), None)
            .unwrap(),
        exact
    );
}

#[test]
fn invalid_values_publish_neither_resources_nor_nodes() {
    let (scene, axes, graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, midpoint_options()).unwrap();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    for values in [
        vec![],
        vec![1.0],
        vec![1.0; 5],
        vec![f64::NAN; 4],
        vec![f64::INFINITY; 4],
    ] {
        assert!(plan.publish(Some(&values), None).is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }
    assert!(plan.publish(None, Some(&[0.0; 4])).is_err());
    assert_eq!(scene.revision(), revision);
}

#[test]
fn tiny_exact_negative_area_keeps_signed_paint() {
    let (_, axes, graph) = quadratic_fixture();
    let plan = axes
        .riemann_plan(
            &graph,
            RiemannRectangleOptions {
                colors: vec![noon_core::RED],
                ..midpoint_options()
            },
        )
        .unwrap();
    let paths = plan.paths(Some(&[-1e-10; 4]), None).unwrap();
    let red = noon_core::RED;
    let inverted =
        noon_core::Color::rgba(1.0 - red.red, 1.0 - red.green, 1.0 - red.blue, red.alpha);
    assert!(paths
        .iter()
        .all(|(_, style)| style.fill == Some(SemanticPaint::Solid(inverted))));
}

#[test]
fn exact_right_sample_can_exceed_retained_domain_without_extrapolating_it() {
    let (_, axes, graph) = quadratic_fixture();
    let plan = axes
        .riemann_plan(
            &graph,
            RiemannRectangleOptions {
                x_range: Some([0.0, 1.0]),
                dx: 0.6,
                sample: RiemannSample::Right,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(plan.samples().collect::<Vec<_>>(), [0.6, 1.2]);
    assert!(plan.paths(None, None).is_err());
    assert!(plan.paths(Some(&[0.36, 1.44]), None).is_ok());
}

#[test]
fn plan_rejects_foreign_bounded_source_before_host_evaluation() {
    let (_, axes, graph) = quadratic_fixture();
    let (_, _, foreign) = quadratic_fixture();
    let result = RiemannRectanglePlan::from_snapshot(
        axes.authored_frame().unwrap(),
        &graph,
        &graph.path_query().unwrap(),
        Some((&foreign, &foreign.path_query().unwrap())),
        midpoint_options(),
    );
    assert!(matches!(
        result,
        Err(CoordinateAuthoringError::Authoring(
            AuthoringError::ForeignStore
        ))
    ));
}

#[test]
fn effective_plan_values_use_one_existing_live_publication_transaction() {
    let (mut scene, axes, graph) = quadratic_fixture();
    scene
        .add_many(&[axes.family().into(), (&graph).into()])
        .unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let plan = scene
        .effective_riemann_plan(&axes, &graph, midpoint_options())
        .unwrap();
    let revision = scene.revision();
    let paths = plan
        .paths(Some(&[0.5625, 0.0625, 0.0625, 0.5625]), None)
        .unwrap();
    let family = crate::scene::publish_path_family(&mut scene, paths).unwrap();
    assert_eq!(scene.revision().get(), revision.get() + 1);
    assert_eq!(
        family_top_points(&scene, &family),
        [(-0.5, 0.5625), (0.0, 0.0625), (0.5, 0.0625), (1.0, 0.5625)]
    );
}
