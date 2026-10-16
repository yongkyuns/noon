"""NumberPlane adapter tests; Rust owns geometry and coordinate semantics."""

from types import SimpleNamespace
from unittest import TestCase, main
from unittest.mock import Mock, patch

import _manim_number_plane as number_plane


class NumberPlaneAdapterTests(TestCase):
    def test_format_precision_preserves_manims_authored_range_step(self):
        precision = number_plane._coordinate_decimal_places
        self.assertEqual(precision((-2, 2, 1)), 0)
        self.assertEqual(precision((-2.0, 2.0, 1.0)), 1)
        self.assertEqual(precision((-2, 2, 0.25)), 2)
        self.assertEqual(precision((-2, 2, 0.125)), 3)
        self.assertEqual(precision((-2, 2)), 0)
        self.assertEqual(precision(None), 0)

    def setUp(self):
        self.addCleanup(patch.stopall)
        self.options = Mock(name="inert-options")
        self.cold = Mock(name="cold-create", return_value=Mock(name="cold-handle"))
        self.live = Mock(name="live-create", return_value=Mock(name="live-handle"))
        self.context = SimpleNamespace(
            liveCreateCoordinates=self.live,
            queryNumberPlaneFrame=Mock(name="query-plane-frame"),
        )
        self.parts = [Mock(name=f"part-{index}") for index in range(4)]
        self.handle = self.live.return_value
        self.handle.numberPlanePart.side_effect = lambda index: self.parts[index]
        self.cold.return_value = self.handle

        self.resolve = patch.object(
            number_plane._plot, "_coordinate_constructor_context", return_value=self.context
        ).start()
        patch.object(number_plane._plot, "_coordinate_options", SimpleNamespace(
            numberPlane=Mock(name="numberPlane-options", return_value=self.options),
        )).start()
        patch.object(number_plane._plot, "_create_coordinates", self.cold).start()
        patch.object(number_plane._plot, "_array", side_effect=lambda values: list(values)).start()
        patch.object(number_plane._plot, "engine_call", side_effect=lambda fn, *args: fn(*args)).start()
        patch.object(number_plane, "engine_call", side_effect=lambda fn, *args: fn(*args)).start()
        patch.object(number_plane._plot, "_coordinate_style", Mock(name="axis-style")).start()
        self.wrapped_parts = [Mock(name=f"wrapped-{index}") for index in range(4)]
        vec2 = number_plane._plot._base.Vec2

        def line(start, end):
            result = Mock(name=f"line-{start}-{end}")
            result.get_start.return_value = vec2(*start)
            result.get_end.return_value = vec2(*end)
            return result

        self.grid_lines = [
            line((-2, 0), (2, 0)),
            line((0, -1), (0, 1)),
            line((-2, 0.5), (2, 0.5)),
            line((0.5, -1), (0.5, 1)),
        ]
        self.wrapped_parts[1].submobjects = self.grid_lines
        self.wrapped_parts[2].shaft = line((-2, 0), (2, 0))
        self.wrapped_parts[3].shaft = line((0, -1), (0, 1))
        self.lines = patch.object(
            number_plane, "_lines", side_effect=lambda handle: self.wrapped_parts[(self.lines.call_count - 1) % 2]
        ).start()
        self.axes = patch.object(
            number_plane._plot,
            "_attach_number_line",
            side_effect=lambda wrapper, handle, **kwargs:
                self.wrapped_parts[2 + ((self.axes.call_count - 1) % 2)],
        ).start()
        self.family = patch.object(number_plane._plot, "_family", side_effect=self._family).start()
        patch.object(number_plane._plot._compat, "VGroup", side_effect=lambda *members: members).start()
        patch.object(
            number_plane.NumberPlane,
            "submobjects",
            new_callable=lambda: property(lambda wrapper: wrapper._test_members),
        ).start()

    def _family(self, wrapper, handle, members):
        wrapper._test_members = list(members)
        wrapper._semantic_family_handle = handle
        return wrapper

    def construct(self, **kwargs):
        return number_plane.NumberPlane(
            (0, 2, 0.5), (1, 5, 2), x_length=8, y_length=4, **kwargs
        )

    def test_live_and_cold_routes_consume_one_inert_options_value(self):
        self.construct()
        options_factory = number_plane._plot._coordinate_options.numberPlane
        options_factory.assert_called_once()
        self.resolve.assert_called_once_with()
        self.live.assert_called_once_with(self.options)
        self.cold.assert_not_called()
        self.options.free.assert_not_called()
        self.assertEqual(
            [call.args for call in self.options.setPlaneAxisTicks.call_args_list],
            [(0, False, 0.1, True), (1, False, 0.1, True)],
        )
        self.assertEqual(
            [call.args for call in self.options.setPlaneAxisTips.call_args_list],
            [(0, False, 0.35, 0.35), (1, False, 0.35, 0.35)],
        )

        self.resolve.return_value = None
        self.live.reset_mock()
        self.cold.reset_mock()
        number_plane._plot._coordinate_options.numberPlane.reset_mock()
        number_plane.NumberPlane((0, 2), (1, 5), x_length=8, y_length=4)
        self.cold.assert_called_once_with(self.options)
        self.live.assert_not_called()

    def test_live_failure_is_not_retried_with_cold_constructor(self):
        error = ValueError("stale live owner")
        self.live.side_effect = error
        with self.assertRaises(ValueError) as caught:
            self.construct()
        self.assertIs(caught.exception, error)
        self.cold.assert_not_called()
        self.options.free.assert_not_called()

    def test_invalid_style_frees_options_before_publication(self):
        with self.assertRaisesRegex(TypeError, "unsupported NumberPlane line style"):
            self.construct(background_line_style={"unknown": 1})
        self.options.free.assert_called_once_with()
        self.live.assert_not_called()
        self.cold.assert_not_called()

    def test_unknown_constructor_options_reject_but_axis_tips_are_supported(self):
        with self.assertRaises(TypeError):
            self.construct(unsupported=True)
        plane = self.construct(axis_config={"include_tip": True})
        self.assertEqual(
            [call.args for call in self.options.setPlaneAxisTips.call_args_list],
            [(0, True, 0.35, 0.35), (1, True, 0.35, 0.35)],
        )
        self.assertTrue(self.axes.call_args_list[0].kwargs["include_tip"])
        self.assertTrue(self.axes.call_args_list[1].kwargs["include_tip"])
        self.assertIs(plane.x_axis, self.wrapped_parts[2])
        number_plane._plot._coordinate_options.numberPlane.assert_called_once()
        self.live.assert_called_once_with(self.options)

    def test_constructor_attaches_numeric_labels_with_shared_and_per_axis_options(self):
        plane = number_plane.NumberPlane(
            (0, 2, 0.25), (1, 5, 2),
            axis_config={
                "include_numbers": True,
                "numbers_to_exclude": [0, 1],
                "font_size": 28,
                "line_to_number_buff": 0.3,
            },
            x_axis_config={"numbers_to_include": [0.5, 1.5]},
            y_axis_config={
                "include_numbers": False,
                "numbers_to_include": [2],
                "label_direction": number_plane._plot._base.LEFT,
                "decimal_number_config": {"num_decimal_places": 3},
            },
        )

        x_axis, y_axis = plane.x_axis, plane.y_axis
        x_axis.add_numbers.assert_called_once_with(
            x_values=(0.5, 1.5), excluding=(0.0, 1.0), font_size=28.0,
            direction=number_plane._plot._base.DR, buff=0.3,
            decimal_places=2,
        )
        y_axis.add_numbers.assert_called_once_with(
            x_values=(2.0,), excluding=(0.0, 1.0), font_size=28.0,
            direction=number_plane._plot._base.LEFT, buff=0.3,
            decimal_places=3,
        )
        self.assertEqual(x_axis.number_label_decimal_places, 2)
        self.assertEqual(y_axis.number_label_decimal_places, 3)
        self.assertIsInstance(x_axis.scaling, number_plane._plot.LinearBase)
        self.assertIsInstance(y_axis.scaling, number_plane._plot.LinearBase)

    def test_constructor_forwards_independent_axis_tick_configuration(self):
        plane = self.construct(
            axis_config={"include_ticks": True, "tick_size": 0.2},
            x_axis_config={"exclude_origin_tick": True},
            y_axis_config={"tick_size": 0.3, "exclude_origin_tick": False},
        )

        self.assertEqual(
            [call.args for call in self.options.setPlaneAxisTicks.call_args_list],
            [(0, True, 0.2, True), (1, True, 0.3, True)],
        )
        self.assertEqual(self.axes.call_args_list[0].kwargs, {
            "tick_size": 0.2, "include_ticks": True, "include_tip": False,
            "exclude_origin_tick": True,
        })
        self.assertEqual(self.axes.call_args_list[1].kwargs, {
            "tick_size": 0.3, "include_ticks": True, "include_tip": False,
            "exclude_origin_tick": True,
        })

    def test_constructor_forwards_independent_axis_stroke_styles(self):
        plane = self.construct(
            axis_config={"stroke_width": 2, "stroke_opacity": 0.8, "opacity": 0.9},
            x_axis_config={
                "color": number_plane._plot._base.GREEN,
                "stroke_width": 4,
                "stroke_opacity": 0.4,
                "opacity": 0.6,
            },
            y_axis_config={"stroke_color": number_plane._plot._base.BLUE},
        )

        green = number_plane._plot._compat._as_color("color", number_plane._plot._base.GREEN)
        blue = number_plane._plot._compat._as_color("color", number_plane._plot._base.BLUE)
        self.assertEqual(
            [call.args for call in self.options.setPlaneAxisStyle.call_args_list],
            [
                (0, [green.red, green.green, green.blue, green.alpha], 0.04, 0.4, 0.6),
                (1, [blue.red, blue.green, blue.blue, blue.alpha], None, None, None),
            ],
        )
        self.assertIs(plane.x_axis, self.wrapped_parts[2])

    def test_constructor_forwards_independent_tip_options(self):
        self.construct(
            axis_config={"include_tip": True, "tip_width": 0.4, "tip_height": 0.3},
            x_axis_config={"include_tip": False},
            y_axis_config={"tip_width": 0.6, "tip_height": 0.2},
        )

        self.assertEqual(
            [call.args for call in self.options.setPlaneAxisTips.call_args_list],
            [(0, False, 0.4, 0.3), (1, True, 0.6, 0.2)],
        )
        self.assertFalse(self.axes.call_args_list[0].kwargs["include_tip"])
        self.assertTrue(self.axes.call_args_list[1].kwargs["include_tip"])
        self.assertEqual(self.wrapped_parts[2].tip_width, 0.4)
        self.assertEqual(self.wrapped_parts[2].tip_height, 0.3)
        self.assertEqual(self.wrapped_parts[3].tip_width, 0.6)
        self.assertEqual(self.wrapped_parts[3].tip_height, 0.2)

    def test_invalid_axis_stroke_styles_fail_before_host_admission(self):
        for kwargs in (
            {"x_axis_config": {"stroke_width": -1}},
            {"x_axis_config": {"stroke_opacity": 1.1}},
            {"y_axis_config": {"opacity": -0.1}},
            {"y_axis_config": {"color": "not-a-color"}},
        ):
            with self.subTest(kwargs=kwargs), self.assertRaises((TypeError, ValueError)):
                self.construct(**kwargs)
        self.resolve.assert_not_called()
        number_plane._plot._coordinate_options.numberPlane.assert_not_called()
        self.live.assert_not_called()
        self.cold.assert_not_called()

    def test_tick_configuration_is_validated_before_host_admission(self):
        for kwargs in (
            {"axis_config": {"include_ticks": 1}},
            {"x_axis_config": {"exclude_origin_tick": 1}},
            {"y_axis_config": {"tick_size": -0.1}},
        ):
            with self.subTest(kwargs=kwargs):
                with self.assertRaises((TypeError, ValueError)):
                    self.construct(**kwargs)
        self.resolve.assert_not_called()
        number_plane._plot._coordinate_options.numberPlane.assert_not_called()
        self.live.assert_not_called()
        self.cold.assert_not_called()

    def test_tip_configuration_is_validated_before_host_admission(self):
        for kwargs in (
            {"axis_config": {"include_tip": 1}},
            {"x_axis_config": {"tip_width": 0}},
            {"y_axis_config": {"tip_height": float("inf")}},
        ):
            with self.subTest(kwargs=kwargs), self.assertRaises((TypeError, ValueError)):
                self.construct(**kwargs)
        self.resolve.assert_not_called()
        number_plane._plot._coordinate_options.numberPlane.assert_not_called()
        self.live.assert_not_called()
        self.cold.assert_not_called()

    def test_constructor_preserves_integer_and_float_step_label_precision(self):
        for axis_range, expected in (
            ((-2, 2, 1), 0),
            ((-2, 2, 1.0), 1),
            (None, 0),
        ):
            with self.subTest(axis_range=axis_range):
                plane = number_plane.NumberPlane(
                    axis_range, axis_range,
                    axis_config={"include_numbers": True},
                )
                self.assertEqual(plane.x_axis.number_label_decimal_places, expected)
                self.assertEqual(plane.y_axis.number_label_decimal_places, expected)
                self.assertEqual(
                    plane.x_axis.add_numbers.call_args.kwargs["decimal_places"], expected
                )
                self.assertEqual(
                    plane.y_axis.add_numbers.call_args.kwargs["decimal_places"], expected
                )

    def test_unqualified_axis_config_fails_before_host_admission(self):
        for kwargs, message in (
            ({"axis_config": {"unknown": True}}, "unsupported axis_config"),
            ({"y_axis_config": {"scaling": number_plane._plot.LogBase()}}, "nonlinear"),
        ):
            with self.subTest(kwargs=kwargs), self.assertRaisesRegex(NotImplementedError, message):
                self.construct(**kwargs)
        self.resolve.assert_not_called()
        number_plane._plot._coordinate_options.numberPlane.assert_not_called()
        self.live.assert_not_called()
        self.cold.assert_not_called()

    def test_ranges_and_lengths_are_forwarded_without_coordinate_math(self):
        self.construct()
        args = number_plane._plot._coordinate_options.numberPlane.call_args.args
        self.assertEqual(args[:4], ([0, 2, 0.5], [1, 5, 2], 8.0, 4.0))
        self.assertEqual(args[4], 1)

    def test_omitted_styles_are_distinct_from_explicit_empty_styles(self):
        self.construct()
        self.options.setPlaneLineStyle.assert_not_called()

        self.options.reset_mock()
        number_plane.NumberPlane(
            (0, 2), (1, 5), x_length=8, y_length=4,
            background_line_style={}, faded_line_style={},
        )
        self.assertEqual(self.options.setPlaneLineStyle.call_count, 2)
        self.assertEqual(
            [call.args[:2] for call in self.options.setPlaneLineStyle.call_args_list],
            [(False, []), (True, [])],
        )

    def test_query_routes_through_plane_frame_with_only_two_axis_shafts(self):
        plane = self.construct()
        plane.x_axis.shaft = object()
        plane.y_axis.shaft = object()
        with patch.object(
            number_plane._plot, "_coordinate_context", return_value=self.context
        ) as coordinate_context:
            plane._coordinate_frame()
        coordinate_context.assert_called_once_with(
            [plane.x_axis.shaft, plane.y_axis.shaft]
        )
        self.context.queryNumberPlaneFrame.assert_called_once_with(
            plane._semantic_family_handle
        )

    def test_inherited_axis_accessors_exclude_grid_line_siblings(self):
        plane = self.construct()
        x_axis, y_axis = plane.x_axis, plane.y_axis
        self.assertEqual(plane.get_axes(), (x_axis, y_axis))
        self.assertIs(plane.get_axis(0), x_axis)
        self.assertIs(plane.get_axis(1), y_axis)
        self.assertIs(plane.get_x_axis(), x_axis)
        self.assertIs(plane.get_y_axis(), y_axis)

    def test_x_and_y_line_aliases_select_major_grid_lines_by_axis_direction(self):
        plane = self.construct()
        self.assertEqual(plane.x_lines, (self.grid_lines[0], self.grid_lines[2]))
        self.assertEqual(plane.y_lines, (self.grid_lines[1], self.grid_lines[3]))

    def test_grid_line_aliases_follow_rotated_current_axis_endpoints(self):
        vec2 = number_plane._plot._base.Vec2

        def line(start, end):
            return SimpleNamespace(get_start=lambda: vec2(*start), get_end=lambda: vec2(*end))

        lines = SimpleNamespace(submobjects=[
            line((0, -2), (0, 2)),
            line((-1, 0), (1, 0)),
        ])
        x_axis = line((0, -2), (0, 2))
        y_axis = line((1, 0), (-1, 0))
        self.assertEqual(number_plane._parallel_grid_lines(lines, x_axis), (lines.submobjects[0],))
        self.assertEqual(number_plane._parallel_grid_lines(lines, y_axis), (lines.submobjects[1],))

    def test_group_members_preserve_authoritative_part_identity_and_order(self):
        plane = self.construct()
        self.assertEqual(plane.submobjects, self.wrapped_parts)
        self.assertIs(plane.faded_lines, self.wrapped_parts[0])
        self.assertIs(plane.background_lines, self.wrapped_parts[1])
        self.assertIs(plane.x_axis, self.wrapped_parts[2])
        self.assertIs(plane.y_axis, self.wrapped_parts[3])
        self.assertEqual(
            [call.args[0] for call in self.family.call_args_list],
            [plane],
        )


if __name__ == "__main__":
    main()
