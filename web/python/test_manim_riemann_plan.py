"""Transport ownership/error paths only; Rust and real WASM test geometry."""
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
        patch.object(plotting._shared, '_gradient_components', return_value=[]).start()
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
