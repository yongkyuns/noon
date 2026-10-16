"""Adapter lifetime/error tests; real numerical/renderer proof is browser CI."""
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, PropertyMock, patch

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
    def test_number_line_default_and_two_value_range_delegate_without_python_defaults(self):
        for values, expected in ((None, []), ([-2, 2], [-2, 2]), ([-2, 2, .5], [-2, 2, .5])):
            with self.subTest(values=values), \
                 patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
                 patch.object(plotting, "_coordinate_options") as factory, \
                 patch.object(plotting, "_array", side_effect=list), \
                 patch.object(plotting, "_create_coordinates", return_value=object()), \
                 patch.object(plotting, "_attach_number_line"):
                plotting.NumberLine(values)
                self.assertEqual(factory.numberLine.call_args.args[0], expected)

    def test_number_line_rejects_explicit_empty_or_malformed_range_before_host_admission(self):
        with patch.object(plotting, "_coordinate_constructor_context") as context:
            for values in ([], [0], [0, 1, 2, 3]):
                with self.subTest(values=values), self.assertRaises(ValueError):
                    plotting.NumberLine(values)
            context.assert_not_called()

    def test_number_line_constructor_attaches_requested_numeric_labels(self):
        for include_numbers, numbers_to_include, expected in (
            (True, None, None),
            (False, [1.0, -1.0], [1.0, -1.0]),
            (False, None, None),
        ):
            with self.subTest(expected=expected), \
                 patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
                 patch.object(plotting, "_coordinate_options") as factory, \
                 patch.object(plotting, "_array", side_effect=list), \
                 patch.object(plotting, "_create_coordinates", return_value=object()), \
                 patch.object(plotting, "_attach_number_line"), \
                 patch.object(plotting.NumberLine, "add_numbers") as add_numbers:
                line = plotting.NumberLine(
                    [-2, 2, 1], include_numbers=include_numbers,
                    numbers_to_include=numbers_to_include,
                    numbers_to_exclude=[0], font_size=30,
                    label_direction=plotting._base.UP,
                    line_to_number_buff=0.4,
                )
                if include_numbers or numbers_to_include is not None:
                    add_numbers.assert_called_once_with(
                        x_values=expected, excluding=[0]
                    )
                    self.assertEqual(line.font_size, 30)
                    self.assertEqual(line.label_direction, plotting._base.UP)
                    self.assertEqual(line.line_to_number_buff, 0.4)
                else:
                    add_numbers.assert_not_called()

    def test_number_line_rejects_unavailable_label_constructors_and_configs_early(self):
        with patch.object(plotting, "_coordinate_constructor_context") as context:
            with self.assertRaises(NotImplementedError):
                plotting.NumberLine(label_constructor=object)
            with self.assertRaises(NotImplementedError):
                plotting.NumberLine(decimal_number_config={"group_with_commas": True})
            with self.assertRaises(TypeError):
                plotting.NumberLine(decimal_number_config=1)
            context.assert_not_called()

    def test_number_line_decimal_precision_uses_step_or_explicit_config(self):
        for step, config, expected in (
            (0.25, None, 2),
            (0.1, {"num_decimal_places": 3}, 3),
        ):
            with self.subTest(step=step, config=config), \
                 patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
                 patch.object(plotting, "_coordinate_options"), \
                 patch.object(plotting, "_array", side_effect=list), \
                 patch.object(plotting, "_create_coordinates", return_value=object()), \
                 patch.object(plotting, "_attach_number_line"), \
                 patch("_manim_number_labels.number_mobjects") as build:
                line = plotting.NumberLine(
                    [-2, 2, step], include_numbers=True,
                    decimal_number_config=config,
                )
                self.assertEqual(line.decimal_number_config,
                                 {"num_decimal_places": expected})
                build.assert_called_once()
                self.assertEqual(build.call_args.kwargs["config"]["decimal_places"],
                                 expected)

        with self.assertRaises(TypeError):
            plotting.NumberLine(decimal_number_config={"num_decimal_places": True})
        with self.assertRaises(ValueError):
            plotting.NumberLine(decimal_number_config={"num_decimal_places": 13})

    def test_number_line_get_tick_range_delegates_tip_and_origin_rules_to_rust(self):
        frame = SimpleNamespace(
            tickValues=Mock(return_value=[-2.0, -1.0, 1.0]),
            free=Mock(),
        )
        line = object.__new__(plotting.NumberLine)
        line.include_tip = False
        line.exclude_origin_tick = True
        with patch.object(plotting.NumberLine, "_coordinate_frame", return_value=frame):
            self.assertEqual(line.get_tick_range(), (-2.0, -1.0, 1.0))
        frame.tickValues.assert_called_once_with(False, True)
        frame.free.assert_called_once_with()

    def test_number_line_get_tick_uses_current_frame_and_matches_shaft_style(self):
        frame = SimpleNamespace(tickSegment=Mock(return_value=[1.0, 2.0, 3.0, 4.0]),
                                free=Mock())
        shaft = object()
        tick = Mock()
        line = object.__new__(plotting.NumberLine)
        line.tick_size = 0.15
        with patch.object(plotting.NumberLine, "_coordinate_frame", return_value=frame), \
             patch.object(plotting.NumberLine, "shaft", new_callable=PropertyMock,
                          return_value=shaft), \
             patch.object(plotting._compat, "Line", return_value=tick) as create:
            result = line.get_tick(2.5)
        self.assertIs(result, tick)
        start, end = create.call_args.args
        self.assertEqual((start.x, start.y), (1.0, 2.0))
        self.assertEqual((end.x, end.y), (3.0, 4.0))
        frame.tickSegment.assert_called_once_with(2.5, 0.15)
        frame.free.assert_called_once_with()
        tick.match_style.assert_called_once_with(shaft)

    def test_number_line_get_tick_accepts_explicit_size_and_tick_marks_alias(self):
        frame = SimpleNamespace(tickSegment=Mock(return_value=[0.0, -0.5, 0.0, 0.5]),
                                free=Mock())
        ticks = object()
        line = object.__new__(plotting.NumberLine)
        line.tick_size = 0.1
        with patch.object(plotting.NumberLine, "_coordinate_frame", return_value=frame), \
             patch.object(plotting.NumberLine, "shaft", new_callable=PropertyMock,
                          return_value=object()), \
             patch.object(plotting.NumberLine, "ticks", new_callable=PropertyMock,
                          return_value=ticks), \
             patch.object(plotting._compat, "Line", return_value=Mock()):
            line.get_tick(-1, size=0.5)
            self.assertIs(line.get_tick_marks(), ticks)
        frame.tickSegment.assert_called_once_with(-1.0, 0.5)

    def test_number_line_get_tick_rejects_boolean_and_nonfinite_arguments(self):
        line = object.__new__(plotting.NumberLine)
        line.tick_size = 0.1
        with patch.object(plotting.NumberLine, "_coordinate_frame") as frame:
            for args in ((True,), (1, float("nan"))):
                with self.subTest(args=args), self.assertRaises((TypeError, ValueError)):
                    line.get_tick(*args)
            frame.assert_not_called()

    def test_number_line_single_and_group_label_accessors_keep_zero_semantics(self):
        line = object.__new__(plotting.NumberLine)
        expected = object()
        family = SimpleNamespace(submobjects=[expected])
        with patch("_manim_number_labels.number_mobjects", return_value=family) as build:
            self.assertIs(line.get_number_mobject(0), expected)
        build.assert_called_once_with(line, (0.0,), attach=False,
                config={
                    "direction": plotting._base.DOWN,
                    "buff": plotting._base.MED_SMALL_BUFF,
                    "font_size": 36,
                    "decimal_places": 0,
                    "exclude_zero": False,
                })

        family = object()
        with patch("_manim_number_labels.number_mobjects", return_value=family) as build:
            self.assertIs(line.get_number_mobjects(0, 1), family)
            build.assert_called_once_with(line, (0, 1), attach=False,
                                         config={
                                             "direction": plotting._base.DOWN,
                                             "buff": plotting._base.MED_SMALL_BUFF,
                                             "font_size": 36,
                                             "decimal_places": 0,
                                             "exclude_zero": False,
                                         })

        with patch.object(plotting.NumberLine, "get_number_mobjects", return_value=family) as get:
            self.assertIs(line.get_labels(), family)
        get.assert_called_once_with()

    def test_number_line_number_labels_preserve_zero_unless_explicitly_excluded(self):
        line = object.__new__(plotting.NumberLine)
        with patch("_manim_number_labels.number_mobjects") as build:
            self.assertIs(line.add_numbers((0, 1)), line)
            build.assert_called_once_with(line, (0, 1), attach=True,
                                         config={
                                             "direction": plotting._base.DOWN,
                                             "buff": plotting._base.MED_SMALL_BUFF,
                                             "font_size": 36,
                                             "decimal_places": 0,
                                             "exclude_zero": False,
                                         }, excluding=())
        with patch("_manim_number_labels.number_mobjects") as build:
            line.numbers_to_exclude = (0,)
            self.assertIs(line.add_numbers(), line)
            build.assert_called_once_with(line, None, attach=True,
                                         config={
                                             "direction": plotting._base.DOWN,
                                             "buff": plotting._base.MED_SMALL_BUFF,
                                             "font_size": 36,
                                             "decimal_places": 0,
                                             "exclude_zero": False,
                                         }, excluding=(0,))
        with patch("_manim_number_labels.number_mobjects") as build:
            self.assertIs(line.add_numbers((0, 1), excluding=(), exclude_zero=True), line)
            build.assert_called_once_with(line, (0, 1), attach=True,
                                         config={
                                             "direction": plotting._base.DOWN,
                                             "buff": plotting._base.MED_SMALL_BUFF,
                                             "font_size": 36,
                                             "decimal_places": 0,
                                             "exclude_zero": True,
                                         }, excluding=())

    def test_number_line_add_labels_uses_native_text_subset_and_returns_self(self):
        line = object.__new__(plotting.NumberLine)
        labels = {2: "Tuesday"}
        with patch("_manim_number_labels.custom_labels") as build:
            self.assertIs(line.add_labels(labels), line)
        build.assert_called_once_with(line, labels, config={
            "direction": plotting._base.DOWN,
            "buff": plotting._base.MED_SMALL_BUFF,
            "font_size": 36,
            "decimal_places": 0,
        })
        with patch("_manim_number_labels.custom_labels") as build:
            with self.assertRaises(NotImplementedError):
                line.add_labels(labels, label_constructor=object)
            build.assert_not_called()

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

    def test_three_d_axis_reconciles_rust_owned_pieces_and_keeps_known_wrappers(self):
        shaft_handle = _handle(10)
        tick_handles = [_handle(11), _handle(12)]
        tick_family_handle = _family_handle(13, [(tick_handles[0], False),
                                                  (tick_handles[1], False)])
        tip_handle = _handle(14)
        cairo_piece_handles = [
            SimpleNamespace(semanticSlot=18, semanticGeneration=0),
            SimpleNamespace(semanticSlot=19, semanticGeneration=0),
        ]
        axis_handle = _family_handle(15, [
            (shaft_handle, False), (tick_family_handle, True),
            (tip_handle, False), *[(piece, False) for piece in cairo_piece_handles],
        ])
        axis_handle.coordinateShaft = Mock(return_value=shaft_handle)
        axis_handle.coordinateTicks = Mock(return_value=tick_family_handle)
        axis_handle.coordinateTickObjects = Mock(return_value=tick_handles)

        created_leaves = {}
        created_families = {}
        real_leaf = plotting._leaf
        real_family = plotting._family

        def capture_leaf(handle, kind=plotting._compat.Line):
            member = real_leaf(handle, kind)
            key = plotting._shared._family_wrapper_key(member)
            created_leaves[key] = member
            return member

        def capture_family(wrapper, handle, members):
            family = real_family(wrapper, handle, members)
            created_families[plotting._shared._family_wrapper_key(family)] = family
            return family

        axis = object.__new__(plotting.NumberLine)
        with patch.object(plotting, "_leaf", side_effect=capture_leaf), \
             patch.object(plotting, "_family", side_effect=capture_family):
            result = plotting._attach_three_d_axis(axis, axis_handle, tip_handle)

        self.assertIs(result, axis)
        self.assertIs(axis.shaft, created_leaves["10:1"])
        self.assertIs(axis.ticks, created_families["13:1"])
        self.assertIs(axis.tip, created_leaves["14:1"])
        self.assertIs(axis.ticks.submobjects[0], created_leaves["11:1"])
        self.assertIs(axis.ticks.submobjects[1], created_leaves["12:1"])
        self.assertEqual(
            [plotting._shared._family_wrapper_key(member) for member in axis.submobjects],
            ["10:1", "13:1", "14:1", "18:0", "19:0"],
        )
        for key, piece in zip(("18:0", "19:0"), cairo_piece_handles):
            self.assertIn(key, axis._semantic_member_wrappers)
            self.assertIs(axis._semantic_member_wrappers[key]._semantic_handle, piece)

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
