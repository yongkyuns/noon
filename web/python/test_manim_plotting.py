"""Adapter lifetime/error tests; real numerical/renderer proof is browser CI."""
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_plotting as plotting
from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT


class Plan:
    def __init__(self):
        self.freed = 0
        self.samples = None
        self.result = object()

    def free(self):
        self.freed += 1

    def parameters(self):
        return [2.0, 1.0, 1.0]

    def functionSamples(self, values, smooth):
        self.samples = (values, smooth)
        return self.result

    parametricSamples = functionSamples


class PlottingAdapterTests(unittest.TestCase):
    def test_unit_interval_adapts_only_constructor_defaults(self):
        with patch.object(plotting.NumberLine, "__init__", return_value=None) as create:
            plotting.UnitInterval()
        create.assert_called_once_with((0, 1, 0.1), unit_size=10,
                                       numbers_with_elongated_ticks=(0, 1))
        with patch.object(plotting.NumberLine, "__init__", return_value=None) as create:
            plotting.UnitInterval(unit_size=4, numbers_with_elongated_ticks=[], include_ticks=False)
        create.assert_called_once_with((0, 1, 0.1), unit_size=4,
                                       numbers_with_elongated_ticks=[], include_ticks=False)

    def setUp(self):
        self.array_bridge = patch.object(plotting, "_to_js", lambda values: values)
        self.array_bridge.start()
        self.addCleanup(self.array_bridge.stop)
        self.label_options = patch("_manim_number_labels._options", return_value=(Mock(), 36, plotting._base.WHITE))
        self.label_options.start()
        self.addCleanup(self.label_options.stop)

    def test_function_uses_each_shared_parameter_once_and_releases_plan(self):
        plan = Plan()
        calls = []

        def function(value):
            calls.append(value)
            return value + 3

        result = plotting._evaluate(plan, function, False, parametric=False)
        self.assertIs(result, plan.result)
        self.assertEqual(calls, [2.0, 1.0, 1.0])
        self.assertEqual(plan.samples, ([5.0, 4.0, 4.0], False))
        self.assertEqual(plan.freed, 1)

    def test_parametric_results_only_coerce_components_without_resampling(self):
        plan = Plan()
        result = plotting._evaluate(plan, lambda t: (t, -t), True, parametric=True)
        self.assertIs(result, plan.result)
        self.assertEqual(plan.samples, ([2.0, -2.0, 1.0, -1.0, 1.0, -1.0], True))
        self.assertEqual(plan.freed, 1)

    def test_callback_failure_preserves_exception_and_does_not_submit_samples(self):
        plan = Plan()
        failure = RuntimeError("user function failed")

        def function(_):
            raise failure

        with self.assertRaises(RuntimeError) as caught:
            plotting._evaluate(plan, function, True, parametric=False)
        self.assertIs(caught.exception, failure)
        self.assertIsNone(plan.samples)
        self.assertEqual(plan.freed, 1)

    def test_invalid_return_releases_plan_before_geometry_creation(self):
        plan = Plan()
        with self.assertRaises(TypeError):
            plotting._evaluate(plan, lambda _: object(), True, parametric=False)
        self.assertIsNone(plan.samples)
        self.assertEqual(plan.freed, 1)

    def test_style_failure_releases_inert_options_without_publication(self):
        options = Mock()
        failure = TypeError("bad style")
        with patch.object(plotting._shared, "_apply_shared_constructor_options", side_effect=failure), \
             patch.object(plotting._shared, "_attach_geometry_options") as publish:
            with self.assertRaises(TypeError) as caught:
                plotting._curve(object(), options, None, {"unknown": 1})
        self.assertIs(caught.exception, failure)
        options.free.assert_called_once_with()
        publish.assert_not_called()

    def test_mixed_execution_contexts_are_rejected(self):
        with patch.object(plotting._shared, "_live_mutation_context", side_effect=[object(), object()]):
            with self.assertRaisesRegex(RuntimeError, "different execution contexts"):
                plotting._coordinate_context([object(), object()])

    def test_callback_overlay_reads_fail_before_unpinned_observation(self):
        token = _ACTIVE_CANONICAL_CONTEXT.set(object())
        try:
            with patch.object(plotting._shared, "_live_mutation_context") as observe:
                with self.assertRaisesRegex(NotImplementedError, "pinned coordinate reads"):
                    plotting._coordinate_context([object()])
                observe.assert_not_called()
        finally:
            _ACTIVE_CANONICAL_CONTEXT.reset(token)

    def test_bar_chart_forwards_rust_default_sentinels_and_style(self):
        options = Mock()
        chart = Mock()
        axes_family = SimpleNamespace(semanticSlot=2, semanticGeneration=1,
                                      memberKeys=Mock(return_value=["12:1", "13:1"]))
        bars_family = SimpleNamespace(semanticSlot=3, semanticGeneration=1,
                                      directMobjects=Mock(return_value=[]),
                                      memberKeys=Mock(return_value=[]))
        chart_family = SimpleNamespace(
            semanticSlot=1, semanticGeneration=1,
            memberKeys=Mock(return_value=["2:1", "3:1"]),
        )
        chart.family.return_value = chart_family
        chart.axes.return_value = axes_family
        chart.bars.return_value = bars_family
        axis = SimpleNamespace(semanticSlot=12, semanticGeneration=1,
            coordinateShaft=Mock(return_value=SimpleNamespace(semanticSlot=10, semanticGeneration=1)),
            coordinateTicks=Mock(return_value=SimpleNamespace(semanticSlot=11, semanticGeneration=1, memberKeys=Mock(return_value=[]))),
            coordinateTickObjects=Mock(return_value=[]), memberKeys=Mock(return_value=["10:1", "11:1"]),
        )
        other_axis = SimpleNamespace(semanticSlot=13, semanticGeneration=1,
            coordinateShaft=Mock(return_value=SimpleNamespace(semanticSlot=20, semanticGeneration=1)),
            coordinateTicks=Mock(return_value=SimpleNamespace(semanticSlot=21, semanticGeneration=1, memberKeys=Mock(return_value=[]))),
            coordinateTickObjects=Mock(return_value=[]), memberKeys=Mock(return_value=["20:1", "21:1"]))
        axes_family.coordinateAxis = Mock(side_effect=[axis, other_axis])
        chart.xLabels.return_value = None
        chart.yLabels.return_value = SimpleNamespace(semanticSlot=40, semanticGeneration=1,
            numberLabelMembers=Mock(return_value=[]), memberKeys=Mock(return_value=[]))
        chart.barPrefix.return_value = []

        with patch.object(plotting, "_bar_chart_options", Mock(return_value=options)) as make_options, \
             patch.object(plotting, "_create_bar_chart", Mock(return_value=chart)) as create_chart, \
             patch.object(plotting._shared, "_live_constructor_context", return_value=None):
            result = plotting.BarChart((1, 2))

        make_options.assert_called_once_with([1.0, 2.0], [], 0.0, 0.0)
        options.setStyle.assert_called_once_with(0.6, 0.7, 3.0)
        create_chart.assert_called_once_with(options, unittest.mock.ANY, None)
        self.assertIs(result._bar_chart_handle, chart)
        self.assertIs(result.axes._semantic_family_handle, axes_family)
        self.assertIs(result.bars._semantic_family_handle, bars_family)

    def test_bar_chart_change_values_uses_live_context(self):
        context = Mock()
        chart = Mock()
        options = Mock()
        axes_family = SimpleNamespace(semanticSlot=2, semanticGeneration=1,
                                      memberKeys=Mock(return_value=["12:1", "13:1"]))
        bars_family = SimpleNamespace(semanticSlot=3, semanticGeneration=1,
                                      directMobjects=Mock(return_value=[]),
                                      memberKeys=Mock(return_value=[]))
        chart_family = SimpleNamespace(
            semanticSlot=1, semanticGeneration=1,
            memberKeys=Mock(return_value=["2:1", "3:1"]),
        )
        chart.family.return_value = chart_family
        chart.axes.return_value = axes_family
        chart.bars.return_value = bars_family
        axis = SimpleNamespace(semanticSlot=12, semanticGeneration=1,
            coordinateShaft=Mock(return_value=SimpleNamespace(semanticSlot=10, semanticGeneration=1)),
            coordinateTicks=Mock(return_value=SimpleNamespace(semanticSlot=11, semanticGeneration=1, memberKeys=Mock(return_value=[]))),
            coordinateTickObjects=Mock(return_value=[]), memberKeys=Mock(return_value=["10:1", "11:1"]),
        )
        other_axis = SimpleNamespace(semanticSlot=13, semanticGeneration=1,
            coordinateShaft=Mock(return_value=SimpleNamespace(semanticSlot=20, semanticGeneration=1)),
            coordinateTicks=Mock(return_value=SimpleNamespace(semanticSlot=21, semanticGeneration=1, memberKeys=Mock(return_value=[]))),
            coordinateTickObjects=Mock(return_value=[]), memberKeys=Mock(return_value=["20:1", "21:1"]))
        axes_family.coordinateAxis = Mock(side_effect=[axis, other_axis])
        chart.xLabels.return_value = None
        chart.yLabels.return_value = SimpleNamespace(semanticSlot=40, semanticGeneration=1,
            numberLabelMembers=Mock(return_value=[]), memberKeys=Mock(return_value=[]))
        chart.barPrefix.return_value = []

        with patch.object(plotting, "_bar_chart_options", Mock(return_value=options)), \
             patch.object(plotting, "_create_bar_chart", Mock(return_value=chart)) as create_chart, \
             patch.object(plotting._shared, "_live_constructor_context", return_value=context), \
             patch.object(plotting, "_coordinate_context", return_value=context):
            result = plotting.BarChart((1, 2))
            result.change_bar_values((3, 4), update_colors=False)

        create_chart.assert_called_once_with(options, unittest.mock.ANY, context)
        context.liveChangeBarValues.assert_called_once_with(chart, [3.0, 4.0], False)
        chart.changeBarValues.assert_not_called()

    def test_bar_chart_copy_rebuilds_chart_and_child_wrappers_from_copied_family(self):
        source_chart = Mock()
        source_family = SimpleNamespace(semanticSlot=1, semanticGeneration=1,
                                        memberKeys=Mock(return_value=["2:1", "3:1"]))
        source_axes = SimpleNamespace(semanticSlot=2, semanticGeneration=1,
                                      memberKeys=Mock(return_value=["12:1", "13:1"]))
        source_bars = SimpleNamespace(semanticSlot=3, semanticGeneration=1,
                                      directMobjects=Mock(return_value=[]),
                                      memberKeys=Mock(return_value=[]))
        source_chart.family.return_value = source_family
        source_chart.axes.return_value = source_axes
        source_chart.bars.return_value = source_bars
        axis = SimpleNamespace(semanticSlot=12, semanticGeneration=1,
            coordinateShaft=Mock(return_value=SimpleNamespace(semanticSlot=10, semanticGeneration=1)),
            coordinateTicks=Mock(return_value=SimpleNamespace(semanticSlot=11, semanticGeneration=1, memberKeys=Mock(return_value=[]))),
            coordinateTickObjects=Mock(return_value=[]), memberKeys=Mock(return_value=["10:1", "11:1"]),
        )
        other_axis = SimpleNamespace(semanticSlot=13, semanticGeneration=1,
            coordinateShaft=Mock(return_value=SimpleNamespace(semanticSlot=20, semanticGeneration=1)),
            coordinateTicks=Mock(return_value=SimpleNamespace(semanticSlot=21, semanticGeneration=1, memberKeys=Mock(return_value=[]))),
            coordinateTickObjects=Mock(return_value=[]), memberKeys=Mock(return_value=["20:1", "21:1"]))
        source_axes.coordinateAxis = Mock(side_effect=[axis, other_axis])
        source_chart.xLabels.return_value = None
        source_chart.yLabels.return_value = SimpleNamespace(semanticSlot=40, semanticGeneration=1,
            numberLabelMembers=Mock(return_value=[]), memberKeys=Mock(return_value=[]))
        context = Mock()

        with patch.object(plotting, "_bar_chart_options", Mock(return_value=Mock())), \
             patch.object(plotting, "_create_bar_chart", Mock(return_value=source_chart)), \
             patch.object(plotting._shared, "_live_constructor_context", return_value=None):
            source = plotting.BarChart((1, 2))

        copied_root = Mock()
        copied_family = SimpleNamespace(semanticSlot=10, semanticGeneration=1,
                                        memberKeys=Mock(return_value=[]),
                                        barChart=Mock(name="copied_chart_handle"))
        copied_axes = SimpleNamespace(semanticSlot=12, semanticGeneration=1,
                                     memberKeys=Mock(return_value=[]))
        copied_bars = SimpleNamespace(semanticSlot=13, semanticGeneration=1,
                                     memberKeys=Mock(return_value=[]))
        def copied_family_for(source):
            mapped = {
                id(source_family): copied_family,
                id(source_axes): copied_axes,
                id(source_bars): copied_bars,
            }.get(id(source))
            return mapped if mapped is not None else SimpleNamespace(
                semanticSlot=99, semanticGeneration=1, memberKeys=Mock(return_value=[]))
        copied_root.familyFor.side_effect = copied_family_for
        copied_root.mobjectFor.side_effect = lambda _: SimpleNamespace(
            semanticSlot=11, semanticGeneration=1
        )
        copied_root.barChart.return_value = Mock(name="copied_chart")
        batch = Mock()
        with patch.object(plotting._shared, "_group_target_context", return_value=context), \
             patch.object(plotting._shared, "_family_membership_batch", return_value=batch):
            context.liveCopyFamily.return_value = copied_root
            copied = source.copy()

        context.liveCopyFamily.assert_called_once_with(source._semantic_family_handle, batch)
        self.assertIsNot(copied._semantic_family_handle, source._semantic_family_handle)
        self.assertIs(copied.axes._semantic_family_handle, copied_axes)
        self.assertIs(copied.bars._semantic_family_handle, copied_bars)
        self.assertIs(copied._bar_chart_handle, copied_family.barChart.return_value)
        self.assertFalse(hasattr(copied, "_bar_chart_context"))


if __name__ == "__main__":
    unittest.main()
