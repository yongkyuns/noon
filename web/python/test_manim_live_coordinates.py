"""Dispatch/option ownership only; Rust and actual Pyodide test the semantics."""
from types import SimpleNamespace
from unittest import TestCase, main
from unittest.mock import Mock, call, patch
import _manim_plotting as plotting
import noon

class LiveCoordinateAdapterTests(TestCase):
    def setUp(self):
        self.addCleanup(patch.stopall)
        self.options=Mock()
        self.cold=Mock(return_value=Mock())
        self.live=Mock(return_value=Mock())
        self.context=SimpleNamespace(liveCreateCoordinates=self.live)
        self.resolve=patch.object(plotting._shared,"_live_constructor_context",return_value=self.context).start()
        self.guard=patch.object(plotting,"_outside_callback").start()
        patch.object(plotting,"_to_js",side_effect=lambda x:x).start()
        patch.object(plotting,"_create_coordinates",self.cold).start()
        patch.object(plotting,"_coordinate_options",SimpleNamespace(
            numberLine=Mock(return_value=self.options),axes=Mock(return_value=self.options))).start()
        patch.object(plotting,"engine_call",side_effect=lambda fn,*args:fn(*args)).start()
        patch.object(plotting,"_coordinate_style").start()
        patch.object(plotting,"_attach_number_line",side_effect=lambda wrapper,handle:wrapper).start()
        patch.object(plotting,"_coordinate_family",side_effect=lambda wrapper,handle,members:wrapper).start()
        patch.object(plotting,"_family",side_effect=lambda wrapper,handle,members:wrapper).start()

    def construct(self, axes=False):
        return plotting.Axes((0,2,1),(0,2,1),x_length=2,y_length=2) if axes else plotting.NumberLine((0,2,1))

    def test_live_number_line_captures_owner_once_and_consumes_options(self):
        self.construct()
        self.resolve.assert_called_once_with("coordinates")
        self.live.assert_called_once_with(self.options)
        self.cold.assert_not_called(); self.options.free.assert_not_called()

    def test_live_axes_use_the_same_dispatch_without_a_cold_fallback(self):
        self.construct(True)
        self.resolve.assert_called_once_with("coordinates")
        self.live.assert_called_once_with(self.options)
        self.cold.assert_not_called(); self.options.free.assert_not_called()

    def test_axes_defaults_and_two_value_ranges_are_forwarded_for_rust_coercion(self):
        plotting.Axes((-1, 10), (-1, 10))
        plotting._coordinate_options.axes.assert_called_once_with(
            [-1.0, 10.0], [-1.0, 10.0], None, None,
        )
        self.options.setTips.assert_called_once_with(True)

    def test_cold_constructor_still_uses_existing_host(self):
        self.resolve.return_value=None
        self.construct()
        self.cold.assert_called_once_with(self.options); self.live.assert_not_called()

    def test_consuming_live_failure_never_retries_cold_or_double_frees(self):
        error=ValueError("stale owner")
        self.live.side_effect=error
        with self.assertRaises(ValueError) as caught:self.construct()
        self.assertIs(caught.exception,error)
        self.cold.assert_not_called(); self.options.free.assert_not_called()

    def test_option_failure_frees_unconsumed_candidate_and_never_publishes(self):
        error=ValueError("bad ticks");self.options.setTicks.side_effect=error
        with self.assertRaises(ValueError) as caught:self.construct()
        self.assertIs(caught.exception,error)
        self.options.free.assert_called_once_with(); self.live.assert_not_called();self.cold.assert_not_called()

    def test_callback_guard_runs_before_context_or_options(self):
        self.guard.side_effect=NotImplementedError("unpinned callback")
        with self.assertRaises(NotImplementedError):self.construct()
        self.resolve.assert_not_called(); plotting._coordinate_options.numberLine.assert_not_called()

    def test_missing_host_does_not_allocate(self):
        plotting._create_coordinates=None
        with self.assertRaises(RuntimeError):self.construct()
        self.resolve.assert_not_called();plotting._coordinate_options.numberLine.assert_not_called()

    def test_number_line_tips_remain_unsupported_but_axes_tips_are_options(self):
        with self.assertRaises(NotImplementedError):
            plotting.NumberLine((0,2,1), include_tip=True)
        plotting._coordinate_options.numberLine.assert_not_called()
        self.construct(True)
        self.options.setTips.assert_called_once_with(True)

    def test_late_queries_require_binding_but_never_choose_authored_fallback(self):
        shaft=object()
        with patch.object(plotting._shared, "_live_mutation_context", return_value=None), \
             patch.object(plotting._shared, "_is_bound", return_value=False) as bound:
            with self.assertRaisesRegex(NotImplementedError, "add live coordinates"):
                plotting._coordinate_context([shaft])
            bound.return_value=True
            self.assertIs(plotting._coordinate_context([shaft]), self.context)
            self.resolve.return_value=None
            bound.return_value=False
            self.assertIsNone(plotting._coordinate_context([shaft]))


    def _plotted_graph(self, axes, callback):
        frame = Mock()
        frame.plotPlan.return_value = Mock()
        axes._coordinate_frame = lambda: frame
        handle = Mock()
        with patch.object(plotting, "_evaluate", return_value=Mock()), \
             patch.object(plotting, "_curve", side_effect=lambda wrapper, *args: self._attach_graph_handle(wrapper, handle)):
            graph = axes.plot(callback, (0, 4, 1))
        return graph, handle

    @staticmethod
    def _attach_graph_handle(graph, handle):
        graph._semantic_handle = handle
        return graph

    def test_graph_query_uses_plot_owners_current_axes_and_re_evaluates_callable(self):
        owner = object.__new__(plotting.Axes)
        receiver = object.__new__(plotting.Axes)
        owner.c2p = Mock(side_effect=lambda x, y: noon.Vec2(x + 10, y + 20))
        receiver.c2p = Mock(return_value=noon.Vec2(-1, -1))
        callback = Mock(side_effect=[2, 3])
        graph, _ = self._plotted_graph(owner, callback)

        first = receiver.i2gp(1, graph)
        second = receiver.input_to_graph_point(1, graph)

        self.assertEqual(first, noon.Vec2(11, 22))
        self.assertEqual(second, noon.Vec2(11, 23))
        self.assertEqual(callback.call_args_list, [call(1.0), call(1.0)])
        self.assertEqual(owner.c2p.call_args_list, [call(1.0, 2.0), call(1.0, 3.0)])
        receiver.c2p.assert_not_called()

    def test_graph_callable_preserves_callback_exception_identity(self):
        axes = object.__new__(plotting.Axes)
        axes.c2p = Mock()
        failure = ValueError("source callable failed")
        graph, _ = self._plotted_graph(axes, Mock(side_effect=failure))
        with self.assertRaises(ValueError) as caught:
            graph.function(2)
        self.assertIs(caught.exception, failure)
        axes.c2p.assert_not_called()

    def test_retained_path_and_invalid_graphs_fail_before_any_sampling(self):
        axes = object.__new__(plotting.Axes)
        with patch.object(plotting, "_outside_callback") as guard:
            with self.assertRaisesRegex(NotImplementedError, "retained-path fallback"):
                axes.input_to_graph_point(1, SimpleNamespace())
            guard.assert_called_once_with()

        callback = Mock()
        with self.assertRaisesRegex(NotImplementedError, "callable-backed graph"):
            axes.input_to_graph_point(1, SimpleNamespace(function=callback, underlying_function=None))
        callback.assert_not_called()

    def test_function_graph_range_is_forwarded_each_time_without_cached_copy(self):
        graph = object.__new__(plotting.FunctionGraph)
        graph._semantic_handle = SimpleNamespace(functionPlotRange=Mock(
            side_effect=([0.0, 3.0], [2.0, 5.0], [2.0, 8.0])))

        self.assertEqual(graph.t_min, 0.0)
        self.assertEqual(graph.t_min, 2.0)
        self.assertEqual(graph.t_max, 8.0)
        self.assertEqual(graph._semantic_handle.functionPlotRange.call_count, 3)
        with self.assertRaises(AttributeError):
            graph.t_min = 9
        with self.assertRaises(AttributeError):
            graph.t_max = 9

    def test_standalone_function_graph_returns_world_coordinate_vec2(self):
        callback = Mock(return_value=4)
        with patch.object(plotting, "_sampling_plan", SimpleNamespace(parametric=Mock(return_value=Mock()))), \
             patch.object(plotting, "_evaluate", return_value=Mock()), \
             patch.object(plotting, "_curve", side_effect=lambda wrapper, *args: wrapper):
            graph = plotting.FunctionGraph(callback, (0, 5, 1))
        self.assertEqual(graph.function(2), noon.Vec2(2, 4))
        callback.assert_called_once_with(2.0)


class CoordinateFamilyWrapperTests(TestCase):
    def setUp(self):
        self.addCleanup(patch.stopall)

    def test_coordinate_wrapper_reconciles_new_rust_family_members(self):
        shaft_handle = SimpleNamespace(semanticSlot=1, semanticGeneration=0)
        tip_handle = SimpleNamespace(semanticSlot=2, semanticGeneration=0)
        shaft = object.__new__(plotting._compat.VMobject)
        shaft._semantic_handle = shaft_handle
        family = Mock()
        family.memberKeys.return_value = ["1:0", "2:0"]
        family.memberIsFamily.side_effect = [False, False]
        family.memberMobject.side_effect = [shaft_handle, tip_handle]
        owner = object.__new__(plotting._compat.Group)

        def attach_shared_handle(wrapper, handle):
            wrapper._semantic_handle = handle
            return wrapper

        with patch.object(plotting._shared, "engine_call", side_effect=lambda function, *args, **kwargs: function(*args, **kwargs)), \
             patch.object(plotting._shared, "_attach_shared_handle", side_effect=attach_shared_handle):
            plotting._coordinate_family(owner, family, [shaft])

        self.assertEqual(list(owner._semantic_member_wrappers), ["1:0", "2:0"])
        self.assertIs(owner._semantic_member_wrappers["1:0"], shaft)
        self.assertIs(owner._semantic_member_wrappers["2:0"]._semantic_handle, tip_handle)
        family.memberKeys.assert_called_once_with(operation="family.members")

if __name__=="__main__":main()
