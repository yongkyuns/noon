from pathlib import Path
p = Path('crates/noon/src/coordinate_authoring/area/tests.rs')
p.write_text(p.read_text() + r'''

fn quadratic_fixture() -> (Scene, ManimAxes, Mobject) {
    let mut scene = Scene::new();
    let mut options = ManimAxesOptions::new([-1.0, 1.0, 1.0], [-1.0, 1.0, 1.0], 2.0, 2.0);
    options.ticks.enabled = false;
    let axes = scene.axes(&options).unwrap();
    let graph = axes.plot(|x| x * x, Some(&[-1.0, 1.0, 1.0]), false).unwrap();
    (scene, axes, graph)
}

fn midpoint_options() -> RiemannRectangleOptions {
    RiemannRectangleOptions { dx: 0.5, sample: RiemannSample::Center, width_scale_factor: 1.0, ..Default::default() }
}

fn family_top_points(scene: &Scene, family: &MobjectFamily) -> Vec<(f64, f64)> {
    let members = scene.integration_store().borrow().semantic_family_members_checked(family.node_id()).unwrap();
    members.into_iter().map(|id| {
        Mobject::from_node(Rc::clone(scene.integration_store()), id).unwrap().path_query().unwrap().start().unwrap()
    }).collect()
}

#[test]
fn exact_plan_geometry_is_not_coarse_retained_interpolation() {
    let (scene, axes, graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, midpoint_options()).unwrap();
    assert_eq!(plan.starts(), &[-1.0, -0.5, 0.0, 0.5]);
    assert_eq!(plan.samples().collect::<Vec<_>>(), [-0.75, -0.25, 0.25, 0.75]);
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
        let plan = axes.riemann_plan(&graph, RiemannRectangleOptions { sample, ..midpoint_options() }).unwrap();
        assert_eq!(plan.samples().collect::<Vec<_>>(), expected);
        assert_eq!(plan.starts(), &[-1.0, -0.5, 0.0, 0.5]);
    }
}

#[test]
fn exact_and_retained_sources_are_selected_independently() {
    let (_, axes, graph) = quadratic_fixture();
    let bound = axes.plot(|_| -0.25, Some(&[-1.0, 1.0, 1.0]), false).unwrap();
    let plan = axes.riemann_plan(&graph, RiemannRectangleOptions {
        bounded_graph: Some(bound.node_id()), ..midpoint_options()
    }).unwrap();
    let exact = [0.5625, 0.0625, 0.0625, 0.5625];
    let lower = [-0.25; 4];
    assert_eq!(plan.paths(Some(&exact), None).unwrap(), plan.paths(Some(&exact), Some(&lower)).unwrap());
    assert_eq!(plan.paths(None, Some(&lower)).unwrap(), plan.paths(None, None).unwrap());
    assert_ne!(plan.paths(Some(&exact), None).unwrap(), plan.paths(None, None).unwrap());
}

#[test]
fn plan_keeps_its_original_axes_and_graph_snapshot_after_source_edits() {
    let (_, axes, mut graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, midpoint_options()).unwrap();
    let retained = plan.paths(None, None).unwrap();
    let exact = plan.paths(Some(&[0.5625, 0.0625, 0.0625, 0.5625]), None).unwrap();
    axes.family().shift(3.0, 2.0).unwrap();
    graph.shift(-2.0, 0.75).unwrap();
    assert_eq!(plan.paths(None, None).unwrap(), retained);
    assert_eq!(plan.paths(Some(&[0.5625, 0.0625, 0.0625, 0.5625]), None).unwrap(), exact);
}

#[test]
fn invalid_values_publish_neither_resources_nor_nodes() {
    let (scene, axes, graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, midpoint_options()).unwrap();
    let revision = scene.revision();
    let nodes = scene.integration_store().borrow().len();
    let resources = scene.integration_store().borrow().geometry_resources().len();
    for values in [vec![], vec![1.0], vec![1.0; 5], vec![f64::NAN; 4], vec![f64::INFINITY; 4]] {
        assert!(plan.publish(Some(&values), None).is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        assert_eq!(scene.integration_store().borrow().geometry_resources().len(), resources);
    }
    assert!(plan.publish(None, Some(&[0.0; 4])).is_err());
    assert_eq!(scene.revision(), revision);
}

#[test]
fn tiny_exact_negative_area_keeps_signed_paint() {
    let (_, axes, graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, RiemannRectangleOptions {
        colors: vec![noon_core::RED], ..midpoint_options()
    }).unwrap();
    let paths = plan.paths(Some(&[-1e-10; 4]), None).unwrap();
    let red = noon_core::RED;
    let inverted = noon_core::Color::rgba(1.0 - red.red, 1.0 - red.green, 1.0 - red.blue, red.alpha);
    assert!(paths.iter().all(|(_, style)| style.fill == Some(SemanticPaint::Solid(inverted))));
}

#[test]
fn exact_right_sample_can_exceed_retained_domain_without_extrapolating_it() {
    let (_, axes, graph) = quadratic_fixture();
    let plan = axes.riemann_plan(&graph, RiemannRectangleOptions {
        x_range: Some([0.0, 1.0]), dx: 0.6, sample: RiemannSample::Right, ..Default::default()
    }).unwrap();
    assert_eq!(plan.samples().collect::<Vec<_>>(), [0.6, 1.2]);
    assert!(plan.paths(None, None).is_err());
    assert!(plan.paths(Some(&[0.36, 1.44]), None).is_ok());
}

#[test]
fn plan_rejects_foreign_bounded_source_before_host_evaluation() {
    let (_, axes, graph) = quadratic_fixture();
    let (_, _, foreign) = quadratic_fixture();
    let result = RiemannRectanglePlan::from_snapshot(
        axes.authored_frame().unwrap(), &graph, &graph.path_query().unwrap(),
        Some((&foreign, &foreign.path_query().unwrap())), midpoint_options(),
    );
    assert!(matches!(result, Err(CoordinateAuthoringError::Authoring(AuthoringError::ForeignStore))));
}

#[test]
fn effective_plan_values_use_one_existing_live_publication_transaction() {
    let (mut scene, axes, graph) = quadratic_fixture();
    scene.add_many(&[axes.family().into(), (&graph).into()]).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let plan = scene.effective_riemann_plan(&axes, &graph, midpoint_options()).unwrap();
    let revision = scene.revision();
    let paths = plan.paths(Some(&[0.5625, 0.0625, 0.0625, 0.5625]), None).unwrap();
    let family = crate::scene::publish_path_family(&mut scene, paths).unwrap();
    assert_eq!(scene.revision().get(), revision.get() + 1);
    assert_eq!(family_top_points(&scene, &family), [(-0.5, 0.5625), (0.0, 0.0625), (0.5, 0.0625), (1.0, 0.5625)]);
}
''')

p = Path('web/python/test_manim_riemann_plan.py')
p.write_text(r'''"""Transport ownership/error paths only; Rust and real WASM test geometry."""
from types import SimpleNamespace
from unittest import TestCase, main
from unittest.mock import Mock, patch
import _manim_plotting as plotting


class RiemannPlanOwnershipTests(TestCase):
    def setUp(self):
        self.addCleanup(patch.stopall)
        self.options = Mock()
        self.family = SimpleNamespace(directMobjects=Mock(return_value=[]))
        self.plan = SimpleNamespace(
            starts=Mock(return_value=[-1, -0.5, 0, 0.5]),
            samples=Mock(return_value=[-0.75, -0.25, 0.25, 0.75]),
            publish=Mock(return_value=self.family), free=Mock())
        self.prepare = Mock(return_value=self.plan)
        self.axes = SimpleNamespace(
            x_axis=SimpleNamespace(shaft=object()), y_axis=SimpleNamespace(shaft=object()),
            _semantic_family_handle=SimpleNamespace(riemannSamplePlan=self.prepare))
        self.graph = SimpleNamespace(_semantic_handle=object(), underlying_function=lambda x: x*x)
        self.context = patch.object(plotting, '_coordinate_context', return_value=None).start()
        self.factory = Mock(return_value=self.options)
        patch.object(plotting, '_coordinate_options', SimpleNamespace(riemann=self.factory)).start()
        patch.object(plotting, '_to_js', side_effect=lambda values: values).start()
        patch.object(plotting, 'engine_call', side_effect=lambda fn, *args: fn(*args)).start()
        patch.object(plotting, '_family', side_effect=lambda wrapper, handle, members: handle).start()

    def invoke(self, **kwargs):
        return plotting.Axes.get_riemann_rectangles(self.axes, self.graph, **kwargs)

    def test_cold_plan_is_released_after_borrowed_publication(self):
        self.assertIs(self.invoke(), self.family)
        self.plan.publish.assert_called_once_with([0.5625, 0.0625, 0.0625, 0.5625], [])
        self.plan.free.assert_called_once_with()
        self.options.free.assert_not_called()

    def test_mixed_sources_and_top_then_lower_callback_order(self):
        calls = []
        def top(x): calls.append(('top', x)); return x*x
        def lower(x): calls.append(('lower', x)); return -0.25
        self.graph.underlying_function = top
        bound = SimpleNamespace(_semantic_handle=object(), underlying_function=lower)
        self.invoke(bounded_graph=bound)
        self.assertEqual(calls, [(kind, x) for start, sample in zip(self.plan.starts(), self.plan.samples())
                                for kind, x in [('top', sample), ('lower', start)]])
        del bound.underlying_function
        self.invoke(bounded_graph=bound)
        self.assertEqual(self.plan.publish.call_args.args[1], [])
        del self.graph.underlying_function
        bound.underlying_function = lower
        self.invoke(bounded_graph=bound)
        self.assertEqual(self.plan.publish.call_args.args, ([], [-0.25]*4))

    def test_callback_error_keeps_exception_identity_and_frees_plan(self):
        failure = RuntimeError('callback failed')
        self.graph.underlying_function = Mock(side_effect=failure)
        with self.assertRaises(RuntimeError) as caught: self.invoke()
        self.assertIs(caught.exception, failure)
        self.plan.free.assert_called_once_with()
        self.plan.publish.assert_not_called()
        self.options.free.assert_not_called()

    def test_plan_observation_error_releases_plan(self):
        self.plan.starts.side_effect = RuntimeError('plan read failed')
        with self.assertRaises(RuntimeError): self.invoke()
        self.plan.free.assert_called_once_with()
        self.plan.publish.assert_not_called()

    def test_paint_error_frees_unconsumed_options(self):
        self.options.setPaint.side_effect = ValueError('paint')
        with self.assertRaises(ValueError): self.invoke()
        self.options.free.assert_called_once_with()
        self.prepare.assert_not_called()

    def test_consuming_plan_failure_does_not_double_free_or_retry(self):
        self.prepare.side_effect = ValueError('foreign graph')
        with self.assertRaises(ValueError): self.invoke()
        self.prepare.assert_called_once()
        self.options.free.assert_not_called()
        self.plan.free.assert_not_called()

    def test_publication_failure_releases_plan_without_retry(self):
        self.plan.publish.side_effect = ValueError('sample values')
        with self.assertRaises(ValueError): self.invoke()
        self.plan.publish.assert_called_once()
        self.plan.free.assert_called_once_with()

    def test_live_plan_uses_only_the_selected_execution_owner(self):
        live = Mock(return_value=self.family)
        self.context.return_value = SimpleNamespace(
            liveEffectiveRiemannSamplePlan=self.prepare, livePublishRiemannPlan=live)
        self.assertIs(self.invoke(), self.family)
        live.assert_called_once_with(self.plan, [0.5625, 0.0625, 0.0625, 0.5625], [])
        self.plan.publish.assert_not_called()
        self.plan.free.assert_called_once_with()

    def test_missing_graph_handle_does_not_allocate_options(self):
        del self.graph._semantic_handle
        with self.assertRaises(AttributeError): self.invoke()
        self.factory.assert_not_called()


if __name__ == '__main__': main()
''')
compile(p.read_text(), str(p), 'exec')
