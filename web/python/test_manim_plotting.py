"""Adapter lifetime/error tests; real numerical/renderer proof is browser CI."""
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_plotting as plotting
from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT


def _family_handle(slot, members=()):
    """Build the Rust family-membership surface used by wrapper reconciliation."""
    keys = [f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}" for handle, _ in members]
    family = SimpleNamespace(semanticSlot=slot, semanticGeneration=1)
    family.memberKeys = Mock(return_value=keys)
    family.memberIsFamily = Mock(side_effect=lambda index: members[index][1])
    family.memberFamily = Mock(side_effect=lambda index: members[index][0])
    family.memberMobject = Mock(side_effect=lambda index: members[index][0])
    return family


def _handle(slot):
    return SimpleNamespace(semanticSlot=slot, semanticGeneration=1)


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
    def test_two_dimensional_axes_keep_the_existing_plotting_method_set(self):
        for name in (
            "plot",
            "plot_samples",
            "plot_implicit_curve",
            "get_area",
            "get_riemann_rectangles",
            "add_coordinates",
            "time_series_plan",
            "synchronized_series_plan",
            "gapped_series_plan",
        ):
            with self.subTest(method=name):
                self.assertTrue(callable(getattr(plotting.Axes, name, None)))
        self.assertFalse(hasattr(plotting.ThreeDAxes, "plot_samples"))

    def test_bar_labels_wrap_existing_tex_leaves_without_recompiling(self):
        from _manim_latex import _CompiledTexLeaf

        leaf = SimpleNamespace(semanticSlot=2, semanticGeneration=1)
        family = SimpleNamespace(
            semanticSlot=1, semanticGeneration=1,
            numberLabelMembers=Mock(return_value=[(leaf, "-2")]),
            memberKeys=Mock(return_value=["2:1"]),
        )
        context = object()
        with patch("_manim_latex._create") as compile_tex:
            labels = plotting._chart_text_family(family, 24, context)

        label, = labels.submobjects
        self.assertIsInstance(label, _CompiledTexLeaf)
        self.assertIs(label._semantic_handle, leaf)
        self.assertEqual(label.get_tex_string(), "-2")
        self.assertEqual(label.font_size, 24)
        self.assertIs(label._canonical_live_target_context, context)
        compile_tex.assert_not_called()

    def test_unit_interval_adapts_only_constructor_defaults(self):
        with patch.object(plotting.NumberLine, "__init__", return_value=None) as create:
            plotting.UnitInterval()
        create.assert_called_once_with((0, 1, 0.1), unit_size=10,
                                       numbers_with_elongated_ticks=(0, 1))
        with patch.object(plotting.NumberLine, "__init__", return_value=None) as create:
            plotting.UnitInterval(unit_size=4, numbers_with_elongated_ticks=[], include_ticks=False)
        create.assert_called_once_with((0, 1, 0.1), unit_size=4,
                                       numbers_with_elongated_ticks=[], include_ticks=False)

    def test_three_d_axes_axis_config_inherits_and_applies_per_axis_overrides(self):
        options = Mock()
        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options") as factory, \
             patch.object(plotting, "_create_coordinates", side_effect=RuntimeError("stop")):
            factory.threeDAxes.return_value = options
            with self.assertRaisesRegex(RuntimeError, "stop"):
                plotting.ThreeDAxes(
                    tips=False,
                    axis_config={"include_tip": True, "include_ticks": False,
                                 "tick_size": 0.2, "color": plotting._base.RED,
                                 "stroke_width": 4},
                    x_axis_config={"include_tip": False, "tick_size": 0.3,
                                   "stroke_width": 2, "stroke_opacity": 0.25},
                    y_axis_config={"exclude_origin_tick": False, "opacity": 0.5},
                    z_axis_config={"color": plotting._base.BLUE},
                )
        options.setTips.assert_called_once_with(True)
        options.setTicks.assert_called_once_with(False, 0.2, True)
        options.setStrokeWidth.assert_called_once_with(0.04)
        options.setAxisTips.assert_called_once_with(0, False)
        options.setAxisTickSize.assert_called_once_with(0, 0.3)
        options.setAxisStrokeWidth.assert_called_once_with(0, 0.02)
        options.setAxisStrokeOpacity.assert_called_once_with(0, 0.25)
        options.setAxisExcludeOriginTick.assert_called_once_with(1, False)
        options.setAxisOpacity.assert_called_once_with(1, 0.5)
        options.setAxisColor.assert_called_once_with(2, plotting._base.BLUE.red,
                                                     plotting._base.BLUE.green,
                                                     plotting._base.BLUE.blue,
                                                     plotting._base.BLUE.alpha)

    def test_three_d_axes_rejects_unsupported_or_invalid_axis_config_before_publish(self):
        for kwargs, error in (
            ({"x_axis_config": {"numbers_to_include": [1]}}, NotImplementedError),
            ({"axis_config": {"scaling": object()}}, NotImplementedError),
            ({"z_axis_config": {"include_tip": 1}}, TypeError),
            ({"x_axis_config": {"stroke_width": -1}}, ValueError),
            ({"x_axis_config": {"stroke_width": True}}, TypeError),
        ):
            with self.subTest(kwargs=kwargs), \
                 patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
                 patch.object(plotting, "_coordinate_options") as factory, \
                 patch.object(plotting, "_create_coordinates") as publish:
                options = factory.threeDAxes.return_value
                with self.assertRaises(error):
                    plotting.ThreeDAxes(**kwargs)
                publish.assert_not_called()
                if options.free.called:
                    options.free.assert_called_once()

    def test_three_d_axes_labels_send_retained_text_handles_as_one_rust_operation(self):
        class RetainedMobject:
            pass

        labels = [RetainedMobject(), RetainedMobject(), RetainedMobject()]
        handles = [_handle(index + 10) for index in range(3)]
        for label, handle in zip(labels, handles):
            label._semantic_family_handle = handle
        result = _family_handle(40)
        axes = object.__new__(plotting.ThreeDAxes)
        axes._semantic_family_handle = SimpleNamespace(
            threeDAxesLabelFamilies=Mock(return_value=result)
        )
        with patch.object(plotting._base, "Mobject", RetainedMobject), \
             patch.object(plotting, "_three_d_axis_label_object", side_effect=labels), \
             patch.object(plotting._shared, "_family_wrapper_key", side_effect=["10:1", "11:1", "12:1"]):
            group = axes.get_axis_labels(*labels, buff=0.2, fixed_orientation=True)
        axes._semantic_family_handle.threeDAxesLabelFamilies.assert_called_once_with(
            *handles, 7, 0.2, True,
        )
        self.assertIs(group._semantic_family_handle, result)
        self.assertEqual(list(group._semantic_member_wrappers.values()), labels)

        axes._semantic_family_handle.threeDAxesLabelFamilies.reset_mock()
        with patch.object(plotting._base, "Mobject", RetainedMobject), \
             patch.object(plotting, "_three_d_axis_label_object", return_value=labels[1]), \
             patch.object(plotting._shared, "_family_wrapper_key", return_value="11:1"):
            axes.get_y_axis_label(labels[1])
        axes._semantic_family_handle.threeDAxesLabelFamilies.assert_called_once_with(
            handles[1], handles[1], handles[1], 2, 0.1, False,
        )

    def test_three_d_axes_labels_reject_unretained_inputs_before_rust_call(self):
        class RetainedMobject:
            pass

        axes = object.__new__(plotting.ThreeDAxes)
        axes._semantic_family_handle = SimpleNamespace(threeDAxesLabelFamilies=Mock())
        with patch.object(plotting._base, "Mobject", RetainedMobject), \
             patch.object(plotting, "_three_d_axis_label_object", return_value=RetainedMobject()):
            with self.assertRaisesRegex(TypeError, "retained semantic family handles"):
                axes.get_x_axis_label(RetainedMobject())
        axes._semantic_family_handle.threeDAxesLabelFamilies.assert_not_called()

    def test_three_d_axes_string_labels_reuse_the_existing_mathtex_family(self):
        class RetainedTex(plotting._compat.Group):
            pass

        tex = object.__new__(RetainedTex)
        tex._semantic_family_handle = _handle(25)
        with patch("_manim_latex.MathTex", return_value=tex) as make_math:
            self.assertIs(plotting._three_d_axis_label_object("x"), tex)
        make_math.assert_called_once_with("x")

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
        axis_shaft = _handle(10)
        axis_ticks = _family_handle(11)
        other_shaft = _handle(20)
        other_ticks = _family_handle(21)
        axis = _family_handle(12, [(axis_shaft, False), (axis_ticks, True)])
        other_axis = _family_handle(13, [(other_shaft, False), (other_ticks, True)])
        axes_family = _family_handle(2, [(axis, True), (other_axis, True)])
        bars_family = _family_handle(3)
        bars_family.directMobjects = Mock(return_value=[])
        chart_family = _family_handle(1, [(axes_family, True), (bars_family, True)])
        chart.family.return_value = chart_family
        chart.axes.return_value = axes_family
        chart.bars.return_value = bars_family
        axis.coordinateShaft = Mock(return_value=axis_shaft)
        axis.coordinateTicks = Mock(return_value=axis_ticks)
        axis.coordinateTickObjects = Mock(return_value=[])
        other_axis.coordinateShaft = Mock(return_value=other_shaft)
        other_axis.coordinateTicks = Mock(return_value=other_ticks)
        other_axis.coordinateTickObjects = Mock(return_value=[])
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
        axis_shaft = _handle(10)
        axis_ticks = _family_handle(11)
        other_shaft = _handle(20)
        other_ticks = _family_handle(21)
        axis = _family_handle(12, [(axis_shaft, False), (axis_ticks, True)])
        other_axis = _family_handle(13, [(other_shaft, False), (other_ticks, True)])
        axes_family = _family_handle(2, [(axis, True), (other_axis, True)])
        bars_family = _family_handle(3)
        bars_family.directMobjects = Mock(return_value=[])
        chart_family = _family_handle(1, [(axes_family, True), (bars_family, True)])
        chart.family.return_value = chart_family
        chart.axes.return_value = axes_family
        chart.bars.return_value = bars_family
        axis.coordinateShaft = Mock(return_value=axis_shaft)
        axis.coordinateTicks = Mock(return_value=axis_ticks)
        axis.coordinateTickObjects = Mock(return_value=[])
        other_axis.coordinateShaft = Mock(return_value=other_shaft)
        other_axis.coordinateTicks = Mock(return_value=other_ticks)
        other_axis.coordinateTickObjects = Mock(return_value=[])
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
        axis_shaft = _handle(10)
        axis_ticks = _family_handle(11)
        other_shaft = _handle(20)
        other_ticks = _family_handle(21)
        axis = _family_handle(12, [(axis_shaft, False), (axis_ticks, True)])
        other_axis = _family_handle(13, [(other_shaft, False), (other_ticks, True)])
        source_axes = _family_handle(2, [(axis, True), (other_axis, True)])
        source_bars = _family_handle(3)
        source_bars.directMobjects = Mock(return_value=[])
        source_family = _family_handle(1, [(source_axes, True), (source_bars, True)])
        source_chart.family.return_value = source_family
        source_chart.axes.return_value = source_axes
        source_chart.bars.return_value = source_bars
        axis.coordinateShaft = Mock(return_value=axis_shaft)
        axis.coordinateTicks = Mock(return_value=axis_ticks)
        axis.coordinateTickObjects = Mock(return_value=[])
        other_axis.coordinateShaft = Mock(return_value=other_shaft)
        other_axis.coordinateTicks = Mock(return_value=other_ticks)
        other_axis.coordinateTickObjects = Mock(return_value=[])
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
        copied_axes = _family_handle(12)
        copied_bars = _family_handle(13)
        copied_bars.directMobjects = Mock(return_value=[])
        copied_family = _family_handle(10, [(copied_axes, True), (copied_bars, True)])
        copied_family.barChart = Mock(name="copied_chart_handle")
        def copied_family_for(source):
            mapped = {
                id(source_family): copied_family,
                id(source_axes): copied_axes,
                id(source_bars): copied_bars,
            }.get(id(source))
            return mapped if mapped is not None else _family_handle(99)
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
