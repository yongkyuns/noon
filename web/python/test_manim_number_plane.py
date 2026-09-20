"""NumberPlane adapter tests; Rust owns geometry and coordinate semantics."""

from types import SimpleNamespace
from unittest import TestCase, main
from unittest.mock import Mock, patch

import _manim_number_plane as number_plane


class NumberPlaneAdapterTests(TestCase):
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
        self.lines = patch.object(
            number_plane, "_lines", side_effect=lambda handle: self.wrapped_parts[(self.lines.call_count - 1) % 2]
        ).start()
        self.axes = patch.object(
            number_plane._plot,
            "_attach_number_line",
            side_effect=lambda wrapper, handle: self.wrapped_parts[2 + ((self.axes.call_count - 1) % 2)],
        ).start()
        self.family = patch.object(number_plane._plot, "_family", side_effect=self._family).start()
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

    def test_unknown_constructor_options_and_tips_reject_before_publication(self):
        with self.assertRaises(TypeError):
            self.construct(unsupported=True)
        with self.assertRaises(NotImplementedError):
            self.construct(axis_config={"include_tip": True})
        number_plane._plot._coordinate_options.numberPlane.assert_not_called()
        self.live.assert_not_called()
        self.cold.assert_not_called()
        self.options.free.assert_not_called()

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
