"""Adapter lifetime/error tests; real numerical/renderer proof is browser CI."""
import unittest

from types import SimpleNamespace
from unittest.mock import Mock, PropertyMock, patch

try:
    import numpy as np
except ImportError:
    np = None

import _manim_plotting as plotting
import _manim_number_labels as number_labels
import noon as public_api
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
    def test_coordinate_label_precision_rejects_explicit_none_before_host_admission(self):
        from _manim_number_plane import NumberPlane

        for constructor in (
            lambda: plotting.NumberLine(decimal_number_config={"num_decimal_places": None}),
            lambda: plotting.Axes(x_axis_config={
                "decimal_number_config": {"num_decimal_places": None}}),
            lambda: NumberPlane(y_axis_config={
                "decimal_number_config": {"num_decimal_places": None}}),
        ):
            with self.subTest(constructor=constructor), patch.object(
                plotting, "_coordinate_constructor_context",
                side_effect=AssertionError("invalid precision reached the host"),
            ):
                with self.assertRaisesRegex(TypeError, "num_decimal_places"):
                    constructor()

    def test_axes_add_coordinates_forwards_positionals_and_rejects_obsolete_aliases(self):
        axes = object.__new__(plotting.Axes)
        with patch.object(number_labels, "add_coordinates", return_value=axes) as add:
            self.assertIs(axes.add_coordinates((1, 2)), axes)
        self.assertEqual(add.call_args.args, (axes, (1, 2)))
        for keyword in ("x_values", "y_values"):
            with self.subTest(keyword=keyword), patch.object(number_labels, "add_coordinates") as add:
                with self.assertRaisesRegex(TypeError, "positional per-axis"):
                    axes.add_coordinates(**{keyword: (2,)})
                add.assert_not_called()

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

    def test_number_line_constructor_requests_rust_retained_default_tip(self):
        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options") as factory, \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=object()), \
             patch.object(plotting, "_attach_number_line") as attach:
            plotting.NumberLine([-2, 2], include_tip=True)

        options = factory.numberLine.return_value
        options.setTips.assert_called_once_with(True)
        options.setNumberLineTipDimensions.assert_called_once_with(
            0.35,
            0.35,
        )
        self.assertTrue(attach.call_args.kwargs["include_tip"])

    def test_number_line_constructor_forwards_logarithmic_scale_to_rust(self):
        scaling = plotting.LogBase(base=2, custom_labels=False)
        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options") as factory, \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=object()), \
             patch.object(plotting, "_attach_number_line"):
            line = plotting.NumberLine((-2, 2, 1), scaling=scaling)

        factory.numberLine.return_value.setNumberLineScale.assert_called_once_with("log", 2.0)
        self.assertIs(line.scaling, scaling)

    def test_number_line_log_custom_labels_use_retained_integers_at_generated_ticks(self):
        from _manim_numbers import Integer

        labels = object()

        def make_labels(line, values, **options):
            line.numbers = labels
            return labels

        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options") as factory, \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=object()), \
             patch.object(plotting, "_attach_number_line"), \
             patch.object(number_labels, "number_mobjects", side_effect=make_labels) as add:
            line = plotting.NumberLine(
                (-2, 2, 1), scaling=plotting.LogBase(base=10),
                include_numbers=True, numbers_to_exclude=[1],
                decimal_number_config={"num_decimal_places": 2},
            )

        factory.numberLine.return_value.setNumberLineScale.assert_called_once_with("log", 10.0)
        self.assertIs(line.labels, labels)
        self.assertIs(line.numbers, labels)
        self.assertEqual(add.call_args.args[:2], (line, None))
        self.assertEqual(add.call_args.kwargs["excluding"], ())
        self.assertEqual(add.call_args.kwargs["log_base"], 10.0)
        self.assertIs(add.call_args.kwargs["number_class"], Integer)
        self.assertEqual(add.call_args.kwargs["config"]["decimal_places"], 2)

    def test_number_line_rejects_unsupported_scaling_before_host_admission(self):
        with patch.object(plotting, "_coordinate_constructor_context") as context, \
             patch.object(plotting, "_coordinate_options") as factory:
            with self.assertRaisesRegex(NotImplementedError, "supports LinearBase and LogBase"):
                plotting.NumberLine((-2, 2, 1), scaling=object())
            context.assert_not_called()
            factory.numberLine.assert_not_called()

    def test_number_line_log_labels_reject_an_explicit_unit_before_host_admission(self):
        with patch.object(plotting, "_coordinate_constructor_context") as context, \
             patch.object(plotting, "_coordinate_options") as factory:
            with self.assertRaisesRegex(TypeError, "supply the exponent unit"):
                plotting.NumberLine(
                    (-2, 2, 1), scaling=plotting.LogBase(), include_numbers=True,
                    decimal_number_config={"unit": "m"},
                )
            context.assert_not_called()
            factory.numberLine.assert_not_called()

    def test_builtin_number_line_scale_helpers_preserve_scalar_and_sequence_semantics(self):
        self.assertIs(public_api.LinearBase, plotting.LinearBase)
        self.assertIs(public_api.LogBase, plotting.LogBase)
        self.assertEqual(plotting.LinearBase(2).function((-1, 0, 1)), [-2.0, 0.0, 2.0])
        self.assertEqual(plotting.LogBase(10).function((-1, 0, 1)), [0.1, 1.0, 10.0])
        self.assertAlmostEqual(plotting.LogBase(10).inverse_function(100), 2.0)
        with self.assertRaises(ValueError):
            plotting.LogBase().inverse_function(0)

    def test_unit_interval_inherits_retained_number_line_tip_support(self):
        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options") as factory, \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=object()), \
             patch.object(plotting, "_attach_number_line") as attach:
            plotting.UnitInterval(include_tip=True)

        options = factory.numberLine.return_value
        options.setTips.assert_called_once_with(True)
        self.assertTrue(attach.call_args.kwargs["include_tip"])

    def test_number_line_wrapper_keeps_rust_tip_as_a_direct_family_member(self):
        wrapper = object.__new__(plotting.NumberLine)
        handle = SimpleNamespace(
            coordinateShaft=Mock(return_value="shaft_handle"),
            coordinateTicks=Mock(return_value="ticks_handle"),
            coordinateTickObjects=Mock(return_value=[]),
            coordinateTip=Mock(return_value="tip_handle"),
        )
        wrapped_shaft = object()
        wrapped_tip = object()
        with patch.object(plotting, "_leaf", side_effect=[wrapped_shaft, wrapped_tip]) as leaf, \
             patch.object(plotting, "_family", return_value="ticks") as family, \
             patch.object(plotting, "_coordinate_family", side_effect=lambda _, __, members: members):
            members = plotting._attach_number_line(wrapper, handle, include_tip=True)

        self.assertEqual(members, [wrapped_shaft, "ticks", wrapped_tip])
        self.assertIs(wrapper.tip, wrapped_tip)
        leaf.assert_any_call("tip_handle", plotting._compat.VMobject)
        family.assert_called_once()

    def test_number_line_rejects_custom_or_invalid_tip_options_before_host_admission(self):
        with patch.object(plotting, "_coordinate_constructor_context") as context:
            with self.assertRaises(NotImplementedError):
                plotting.NumberLine(include_tip=True, tip_shape=object)
            with self.assertRaises(ValueError):
                plotting.NumberLine(include_tip=True, tip_width=0)
            context.assert_not_called()

    def test_number_line_rejects_unavailable_label_constructors_and_configs_early(self):
        with patch.object(plotting, "_coordinate_constructor_context") as context:
            with self.assertRaises(NotImplementedError):
                plotting.NumberLine(label_constructor=object)
            with self.assertRaises(NotImplementedError):
                plotting.NumberLine(decimal_number_config={"mob_class": object})
            with self.assertRaises(TypeError):
                plotting.NumberLine(decimal_number_config=1)
            context.assert_not_called()

    def test_number_line_decimal_precision_uses_step_or_explicit_config(self):
        for values, config, expected in (
            (None, None, 0),
            ([-2, 2], None, 0),
            ([-2, 2, 1], None, 0),
            ([-2, 2, 1.0], None, 1),
            ([-2, 2, 0.25], None, 2),
            ([-2, 2, 0.1], {"num_decimal_places": 3}, 3),
        ):
            with self.subTest(values=values, config=config), \
                 patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
                 patch.object(plotting, "_coordinate_options"), \
                 patch.object(plotting, "_array", side_effect=list), \
                 patch.object(plotting, "_create_coordinates", return_value=object()), \
                 patch.object(plotting, "_attach_number_line"), \
                 patch("_manim_number_labels.number_mobjects") as build:
                line = plotting.NumberLine(
                    values, include_numbers=True,
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

    def test_number_line_decimal_format_options_are_retained_and_used_for_labels(self):
        format_config = {
            "num_decimal_places": 1,
            "include_sign": True,
            "group_with_commas": False,
            "show_ellipsis": True,
            "unit": r"\text{m}",
        }
        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options"), \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=object()), \
             patch.object(plotting, "_attach_number_line"), \
             patch("_manim_number_labels.number_mobjects") as build:
            line = plotting.NumberLine(
                [-2, 2, 0.5], include_numbers=True,
                decimal_number_config=format_config,
            )
            self.assertEqual(line.decimal_number_config, format_config)
            self.assertEqual(build.call_args.kwargs["config"]["decimal_places"], 1)
            self.assertTrue(build.call_args.kwargs["config"]["include_sign"])
            self.assertFalse(build.call_args.kwargs["config"]["group_with_commas"])
            self.assertTrue(build.call_args.kwargs["config"]["show_ellipsis"])
            self.assertEqual(build.call_args.kwargs["config"]["unit"], r"\text{m}")

    def test_axes_label_decimal_precision_preserves_authored_step_type(self):
        for step, expected in ((1, 0), (1.0, 1)):
            with self.subTest(step=step):
                axis = SimpleNamespace(add_numbers=Mock())
                config = {
                    "decimal_places": None,
                    "include_numbers": True,
                    "numbers_to_include": (1,),
                    "numbers_to_exclude": (),
                    "font_size": 36,
                    "label_direction": plotting._base.DOWN,
                    "line_to_number_buff": 0.1,
                }
                plotting._configure_axes_numeric_labels(axis, config, (-2, 2, step))
                self.assertEqual(axis.number_label_decimal_places, expected)
                self.assertEqual(axis.decimal_number_config,
                                 {"num_decimal_places": expected})
                self.assertEqual(axis.add_numbers.call_args.kwargs["decimal_places"],
                                 expected)

    @unittest.skipIf(np is None, "NumPy protocol runs in the pinned reference lane")
    def test_number_line_get_tick_range_delegates_tip_and_origin_rules_to_rust(self):
        frame = SimpleNamespace(
            tickValues=Mock(return_value=[-2.0, -1.0, 1.0]),
            free=Mock(),
        )
        line = object.__new__(plotting.NumberLine)
        line.include_tip = False
        line.exclude_origin_tick = True
        with patch.object(plotting.NumberLine, "_coordinate_frame", return_value=frame):
            values = line.get_tick_range()
        self.assertIsInstance(values, np.ndarray)
        self.assertEqual(values.dtype, np.dtype(float))
        self.assertEqual(values.shape, (3,))
        np.testing.assert_array_equal(values, [-2.0, -1.0, 1.0])
        np.testing.assert_array_equal(values + 1, [-1.0, 0.0, 2.0])
        frame.tickValues.assert_called_once_with(False, True)
        frame.free.assert_called_once_with()

    def test_number_line_operators_use_shared_coordinate_queries(self):
        line = object.__new__(plotting.NumberLine)
        frame = SimpleNamespace(
            numberToPoint=Mock(return_value=[3.0, 4.0]),
            pointToNumber=Mock(return_value=2.0),
            free=Mock(),
        )
        with patch.object(plotting.NumberLine, "_coordinate_frame", return_value=frame):
            point = line @ 2.0
            self.assertEqual((point.x, point.y), (3.0, 4.0))
            self.assertEqual(plotting._base.Vec2(3.0, 4.0) @ line, 2.0)
        frame.numberToPoint.assert_called_once_with(2.0)
        frame.pointToNumber.assert_called_once_with(3.0, 4.0)
        self.assertEqual(frame.free.call_count, 2)

    def test_number_line_line_observations_use_one_shared_frame_query_each(self):
        line = object.__new__(plotting.NumberLine)
        frame = SimpleNamespace(
            start=Mock(return_value=[1.0, 2.0]),
            end=Mock(return_value=[5.0, 2.0]),
            vector=Mock(return_value=[4.0, 0.0]),
            length=Mock(return_value=4.0),
            angle=Mock(return_value=0.0),
            slope=Mock(return_value=0.0),
            projectPoint=Mock(return_value=[3.0, 2.0]),
            free=Mock(),
        )
        with patch.object(plotting.NumberLine, "_coordinate_frame", return_value=frame), \
             patch.object(plotting, "engine_call", side_effect=lambda fn, *args: fn(*args)):
            start, end = line.get_start_and_end()
            self.assertEqual((start, end), (plotting._base.Vec2(1.0, 2.0),
                                            plotting._base.Vec2(5.0, 2.0)))
            self.assertEqual(line.get_vector(), plotting._base.Vec2(4.0, 0.0))
            self.assertEqual(line.get_length(), 4.0)
            self.assertEqual(line.get_angle(), 0.0)
            self.assertEqual(line.get_slope(), 0.0)
            self.assertEqual(line.get_projection((3.0, 8.0)), plotting._base.Vec2(3.0, 2.0))

        frame.start.assert_called_once_with()
        frame.end.assert_called_once_with()
        frame.vector.assert_called_once_with()
        frame.length.assert_called_once_with()
        frame.angle.assert_called_once_with()
        frame.slope.assert_called_once_with()
        frame.projectPoint.assert_called_once_with(3.0, 8.0)
        self.assertEqual(frame.free.call_count, 6)

    def test_number_line_line_mutators_reuse_shared_rotate_and_scale(self):
        line = object.__new__(plotting.NumberLine)
        start = plotting._base.Vec2(-2.0, 3.0)
        about = plotting._base.Vec2(1.0, 2.0)
        line.get_start = Mock(return_value=start)
        line.get_angle = Mock(return_value=0.25)
        line.rotate = Mock(return_value=line)
        line.get_length = Mock(return_value=4.0)
        line.scale = Mock(return_value=line)

        self.assertIs(line.set_angle(1.25), line)
        line.rotate.assert_called_once_with(1.0, about_point=start)

        line.rotate.reset_mock()
        self.assertIs(line.set_angle(0.5, about_point=about), line)
        line.rotate.assert_called_once_with(0.25, about_point=about)

        self.assertIs(line.set_length(10), line)
        line.scale.assert_called_once_with(2.5)

    def test_number_line_rotation_helpers_use_the_requested_data_anchor(self):
        line = object.__new__(plotting.NumberLine)
        point = plotting._base.Vec2(2.0, 0.0)
        result = object()
        rate_function = object()
        with patch.object(plotting.NumberLine, "number_to_point", return_value=point) as n2p, \
             patch.object(plotting.NumberLine, "rotate", return_value=result) as rotate:
            self.assertIs(
                line.rotate_about_zero(1.5, rate_func=rate_function), result
            )
            n2p.assert_called_once_with(0)
            rotate.assert_called_once_with(
                1.5,
                axis=plotting._base.OUT,
                about_point=point,
                rate_func=rate_function,
            )

        with patch.object(plotting.NumberLine, "number_to_point", return_value=point) as n2p, \
             patch.object(plotting.NumberLine, "rotate", return_value=result) as rotate:
            self.assertIs(
                line.rotate_about_number(-2, 0.5, axis=plotting._base.UP), result
            )
            n2p.assert_called_once_with(-2)
            rotate.assert_called_once_with(
                0.5,
                axis=plotting._base.UP,
                about_point=point,
            )

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

    def test_axes_forward_common_and_per_axis_scales_to_rust(self):
        common = plotting.LogBase(base=2, custom_labels=False)
        y_scale = plotting.LinearBase(scale_factor=3)
        x_axis = SimpleNamespace()
        y_axis = SimpleNamespace()
        handle = SimpleNamespace(coordinateAxis=Mock(side_effect=["x-axis", "y-axis"]))
        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options") as factory, \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=handle), \
             patch.object(plotting, "_attach_number_line", side_effect=[x_axis, y_axis]) as attach, \
             patch.object(plotting, "_coordinate_family"), \
             patch.object(plotting._compat, "VGroup", side_effect=lambda *members: members) as retain_axes:
            plotting.Axes(
                axis_config={"scaling": common},
                y_axis_config={"scaling": y_scale},
            )

        options = factory.axes.return_value
        options.setAxesScale.assert_any_call(0, "log", 2.0)
        options.setAxesScale.assert_any_call(1, "linear", 3.0)
        self.assertIs(x_axis.scaling, common)
        self.assertIs(y_axis.scaling, y_scale)
        self.assertFalse(attach.call_args_list[0].kwargs["exclude_origin_tick"])
        self.assertTrue(attach.call_args_list[1].kwargs["exclude_origin_tick"])
        self.assertEqual([call.args[1] for call in attach.call_args_list], ["x-axis", "y-axis"])

    def test_axes_reject_unqualified_axis_config_before_host_admission(self):
        with patch.object(plotting, "_coordinate_constructor_context") as context, \
             patch.object(plotting, "_coordinate_options") as factory:
            with self.assertRaisesRegex(NotImplementedError, "unrecognized_axis_option"):
                plotting.Axes(axis_config={"unrecognized_axis_option": True})
            with self.assertRaisesRegex(NotImplementedError, "supports LinearBase and LogBase"):
                plotting.Axes(x_axis_config={"scaling": object()})
            with self.assertRaisesRegex(TypeError, "supply the exponent unit"):
                plotting.Axes(
                    x_axis_config={
                        "scaling": plotting.LogBase(), "include_numbers": True,
                        "decimal_number_config": {"unit": "m"},
                    }
                )
            context.assert_not_called()
            factory.axes.assert_not_called()

    def test_axes_log_custom_labels_use_retained_integers_and_shared_formatter(self):
        from _manim_numbers import Integer

        labels = object()
        x_axis = object.__new__(plotting.NumberLine)
        y_axis = object.__new__(plotting.NumberLine)
        handle = SimpleNamespace(coordinateAxis=Mock(side_effect=["x-axis", "y-axis"]))

        def build_labels(line, values, **options):
            line.numbers = labels
            return labels

        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options") as factory, \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=handle), \
             patch.object(plotting, "_attach_number_line", side_effect=[x_axis, y_axis]), \
             patch.object(plotting._compat, "VGroup", return_value=object()), \
             patch.object(plotting, "_coordinate_family"), \
             patch.object(number_labels, "number_mobjects", side_effect=build_labels) as build:
            axes = plotting.Axes(
                x_range=(-2, 2, 1), y_range=(-1, 1, 1),
                x_axis_config={
                    "scaling": plotting.LogBase(base=2), "include_numbers": True,
                    "decimal_number_config": {"num_decimal_places": 1},
                },
                tips=False,
            )

        factory.axes.return_value.setAxesScale.assert_any_call(0, "log", 2.0)
        self.assertIs(x_axis.numbers, labels)
        self.assertIs(x_axis.labels, labels)
        self.assertEqual(build.call_args.args[:2], (x_axis, None))
        self.assertEqual(build.call_args.kwargs["log_base"], 2.0)
        self.assertIs(build.call_args.kwargs["number_class"], Integer)
        self.assertEqual(build.call_args.kwargs["config"]["decimal_places"], 1)
        self.assertEqual(build.call_args.kwargs["excluding"], ())

    def test_axes_attach_configured_numeric_labels_with_axis_overrides(self):
        x_axis = SimpleNamespace(add_numbers=Mock())
        y_axis = SimpleNamespace(add_numbers=Mock())
        handle = SimpleNamespace(coordinateAxis=Mock(side_effect=["x-axis", "y-axis"]))
        with patch.object(plotting, "_coordinate_constructor_context", return_value=None), \
             patch.object(plotting, "_coordinate_options"), \
             patch.object(plotting, "_array", side_effect=list), \
             patch.object(plotting, "_create_coordinates", return_value=handle), \
             patch.object(plotting, "_attach_number_line", side_effect=[x_axis, y_axis]), \
             patch.object(plotting, "_coordinate_family"), \
             patch.object(plotting._compat, "VGroup", side_effect=lambda *members: members) as retain_axes:
            axes = plotting.Axes(
                x_range=(-2, 2, 0.5), y_range=(-1, 1, 1),
                axis_config={"include_numbers": True, "font_size": 30,
                             "line_to_number_buff": 0.25},
                x_axis_config={"numbers_to_include": [-1, 1],
                               "label_direction": plotting._base.UP},
                y_axis_config={"numbers_to_include": [0.5], "include_numbers": False,
                               "font_size": 24,
                               "decimal_number_config": {
                                   "num_decimal_places": 2, "include_sign": True,
                                   "group_with_commas": False, "show_ellipsis": True,
                                   "unit": r"\text{m}",
                               }},
            )

        retain_axes.assert_called_once_with(x_axis, y_axis)
        self.assertEqual(axes.axes, (x_axis, y_axis))
        x_axis.add_numbers.assert_called_once_with(
            x_values=(-1.0, 1.0), excluding=[0.0], font_size=30.0,
            direction=plotting._base._as_vec2(plotting._base.UP), buff=0.25,
            decimal_places=1,
        )
        y_axis.add_numbers.assert_called_once_with(
            x_values=(0.5,), excluding=[0.0], font_size=24.0,
            direction=plotting._base._as_vec2(plotting._base.LEFT), buff=0.25,
            decimal_places=2,
        )
        self.assertEqual(y_axis.decimal_number_config, {
            "num_decimal_places": 2, "include_sign": True,
            "group_with_commas": False, "show_ellipsis": True,
            "unit": r"\text{m}",
        })
        self.assertTrue(x_axis.include_numbers)
        self.assertFalse(y_axis.include_numbers)
        self.assertEqual(x_axis.number_label_decimal_places, 1)
        self.assertEqual(y_axis.number_label_decimal_places, 2)

    def test_axes_accessors_preserve_axis_identity_and_current_frame_queries(self):
        x_axis = SimpleNamespace(get_unit_size=Mock(return_value=2.5))
        y_axis = SimpleNamespace(get_unit_size=Mock(return_value=1.25))
        axes = object.__new__(plotting.Axes)
        axes.axes = (x_axis, y_axis)
        origin = plotting._base.Vec2(3, -2)
        with patch.object(axes, "c2p", return_value=origin) as coords_to_point:
            self.assertEqual(axes.get_origin(), origin)
        coords_to_point.assert_called_once_with(0.0, 0.0)
        self.assertIs(axes.get_axes()[0], x_axis)
        self.assertIs(axes.get_axis(1), y_axis)
        self.assertIs(axes.get_x_axis(), x_axis)
        self.assertIs(axes.get_y_axis(), y_axis)
        self.assertEqual(axes.get_x_unit_size(), 2.5)
        self.assertEqual(axes.get_y_unit_size(), 1.25)
        x_axis.get_unit_size.assert_called_once_with()
        y_axis.get_unit_size.assert_called_once_with()

    def test_explicit_empty_decimal_config_uses_decimal_number_default_precision(self):
        for config, expected in ((None, None), ({}, 2), ({"num_decimal_places": 0}, 0)):
            with self.subTest(config=config):
                style = plotting._number_label_style(
                    36, plotting._base.DOWN, 0.25, config, "axis"
                )
                self.assertEqual(style[-1], expected)

    def test_axes_default_and_explicit_label_steps_preserve_authored_precision(self):
        for axis_range, expected in (((), 0), ((-2, 2), 0),
                                     ((-2, 2, 1), 0), ((-2, 2, 1.0), 1)):
            with self.subTest(axis_range=axis_range):
                axis = SimpleNamespace(add_numbers=Mock())
                config = plotting._axes_label_options(
                    {}, ("linear", 1.0, False), "axis", plotting._base.DOWN
                )
                plotting._configure_axes_numeric_labels(axis, config, axis_range)
                self.assertEqual(axis.number_label_decimal_places, expected)

    def test_axes_get_axes_reuses_the_retained_group_without_semantic_construction(self):
        axes = object.__new__(plotting.Axes)
        group = object()
        axes.axes = group
        with patch.object(plotting._compat, "VGroup", side_effect=AssertionError(
                "axis access must not create a family during callbacks")):
            self.assertIs(axes.get_axes(), group)
            self.assertIs(axes.get_axes(), group)

    def test_axis_label_forwards_one_shared_placement_with_default_frame_buffer(self):
        edge = plotting._base.Vec2(1, -1)
        direction = plotting._base.Vec2(1, 0.5)
        label, axis = object(), object()
        axes = object.__new__(plotting.Axes)
        axes.axes = (axis, object())
        with patch.object(plotting, "_axes_label_object", return_value=label), \
             patch.object(plotting._shared, "_next_to_edge_in_frame") as place:
            result = axes.get_x_axis_label("x", edge=edge, direction=direction, buff=0.25)
        self.assertIs(result, label)
        place.assert_called_once_with(label, axis, edge, direction, 0.25,
                                      plotting._base.MED_SMALL_BUFF)

    def test_axes_label_invalid_placement_rejects_before_constructing_text(self):
        axes = object.__new__(plotting.Axes)
        axes.axes = (object(), object())
        for options, error in (
            ({"buff": -0.1}, ValueError),
            ({"buff": float("nan")}, ValueError),
            ({"buff": True}, TypeError),
            ({"edge": object()}, TypeError),
            ({"direction": object()}, TypeError),
        ):
            with self.subTest(options=options), \
                 patch.object(plotting, "_axes_label_object") as construct:
                with self.assertRaises(error):
                    axes.get_x_axis_label("x", **options)
                construct.assert_not_called()

    def test_axes_label_numbers_reuse_mathtex_label_path(self):
        label = object()
        with patch.object(plotting, "_three_d_axis_label_object", return_value=label) as create:
            self.assertIs(plotting._axes_label_object(2.5), label)
        create.assert_called_once_with("2.5")

    def test_axes_label_group_is_retained_on_axes(self):
        x_label, y_label, group = object(), object(), object()
        axes = object.__new__(plotting.Axes)
        axes.get_x_axis_label = Mock(return_value=x_label)
        axes.get_y_axis_label = Mock(return_value=y_label)
        with patch.object(plotting._compat, "VGroup", return_value=group) as make_group:
            self.assertIs(axes.get_axis_labels(), group)
        axes.get_x_axis_label.assert_called_once_with("x")
        axes.get_y_axis_label.assert_called_once_with("y")
        make_group.assert_called_once_with(x_label, y_label)
        self.assertIs(axes.axis_labels, group)

    def test_axes_line_to_point_projects_through_current_coordinate_frame(self):
        point = plotting._base.Vec2(3.0, 4.0)
        projected = plotting._base.Vec2(3.0, 0.0)
        label_color = object()
        line = object()
        line_func = Mock(return_value=line)
        axes = object.__new__(plotting.Axes)
        axis = Mock()
        axis.get_projection.return_value = projected
        axis.point_to_number.side_effect = AssertionError("physical projection must not enter a logarithmic coordinate domain")
        axis.number_to_point.side_effect = AssertionError("physical projection must not remap a numeric coordinate")
        axes.get_axis = Mock(return_value=axis)
        line_config = {"dashed_ratio": 0.75, "stroke_width": 99}

        with patch.object(plotting._compat, "_as_color", return_value=label_color) as parse_color:
            result = axes.get_line_from_axis_to_point(
                0, point, line_func=line_func, line_config=line_config,
                color=plotting._base.RED, stroke_width=2,
            )

        self.assertIs(result, line)
        axes.get_axis.assert_called_once_with(0)
        axis.get_projection.assert_called_once_with(point)
        parse_color.assert_called_once_with("line color", plotting._base.RED)
        line_func.assert_called_once_with(
            projected, point, dashed_ratio=0.75, color=label_color, stroke_width=2,
        )
        self.assertEqual(line_config, {"dashed_ratio": 0.75, "stroke_width": 99})

    def test_axes_line_to_point_rejects_invalid_axis_before_coordinate_queries(self):
        axes = object.__new__(plotting.Axes)
        axes.get_axis = Mock()
        for index, error in ((True, TypeError), (2, IndexError), (0.5, TypeError)):
            with self.subTest(index=index), self.assertRaises(error):
                axes.get_line_from_axis_to_point(index, plotting._base.ORIGIN)
        axes.get_axis.assert_not_called()

    def test_axes_lines_to_point_returns_horizontal_then_vertical(self):
        point = object()
        horizontal, vertical, group = object(), object(), object()
        axes = object.__new__(plotting.Axes)
        axes.get_horizontal_line = Mock(return_value=horizontal)
        axes.get_vertical_line = Mock(return_value=vertical)
        with patch.object(plotting._compat, "VGroup", return_value=group) as make_group:
            result = axes.get_lines_to_point(point, color=plotting._base.BLUE)

        self.assertIs(result, group)
        axes.get_horizontal_line.assert_called_once_with(point, color=plotting._base.BLUE)
        axes.get_vertical_line.assert_called_once_with(point, color=plotting._base.BLUE)
        make_group.assert_called_once_with(horizontal, vertical)

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
