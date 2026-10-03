"""ThreeDAxes adapter stays a thin binding over Rust-owned 3D coordinates."""

from types import SimpleNamespace
from unittest import TestCase, main
from unittest.mock import Mock, patch

import _manim_plotting as plotting


class ThreeDAxesAdapterTests(TestCase):
    def setUp(self):
        self.addCleanup(patch.stopall)
        self.options = Mock(name="three-d-options")
        self.real_coordinate_style = plotting._coordinate_style
        self.handle = Mock(name="three-d-family")
        self.context = SimpleNamespace(liveCreateCoordinates=Mock(return_value=self.handle))
        self.factory = Mock(return_value=self.options)
        patch.object(plotting, "_coordinate_constructor_context", return_value=self.context).start()
        patch.object(plotting, "_coordinate_options", SimpleNamespace(threeDAxes=self.factory)).start()
        patch.object(plotting, "_array", side_effect=lambda values: list(values)).start()
        patch.object(plotting, "engine_call", side_effect=lambda fn, *args: fn(*args)).start()
        patch.object(plotting, "_coordinate_style").start()
        self.axes_members = []

        def attach(wrapper, axis, tip):
            wrapper._test_shaft = Mock(name="shaft")
            wrapper._axis_handle = axis
            wrapper.tip = tip
            return wrapper

        patch.object(plotting, "_attach_three_d_axis", side_effect=attach).start()

        def family(wrapper, handle, members):
            wrapper._semantic_family_handle = handle
            wrapper._test_members = list(members)
            self.axes_members[:] = members
            return wrapper

        patch.object(plotting, "_family", side_effect=family).start()
        patch.object(
            plotting.ThreeDAxes,
            "submobjects",
            new_callable=lambda: property(lambda wrapper: wrapper._test_members),
        ).start()
        patch.object(
            plotting.NumberLine,
            "shaft",
            new_callable=lambda: property(lambda wrapper: wrapper._test_shaft),
        ).start()
        self.axes_handles = [Mock(name=f"axis-{i}") for i in range(3)]
        self.tips = [Mock(name=f"tip-{i}") for i in range(3)]
        self.handle.threeDAxesAxis.side_effect = self.axes_handles
        self.handle.threeDAxesTip.side_effect = self.tips

    def construct(self, **kwargs):
        return plotting.ThreeDAxes(
            (-2, 6, 2), (-3, 5, 2), (-4, 4, 2),
            x_length=8, y_length=4, z_length=6, **kwargs,
        )

    def test_ranges_tips_and_tick_policy_are_forwarded_to_rust(self):
        axes = self.construct(color="#ffffff", tips=True)
        self.factory.assert_called_once_with(
            [-2.0, 6.0, 2.0], [-3.0, 5.0, 2.0], [-4.0, 4.0, 2.0], 8.0, 4.0, 6.0,
        )
        self.options.setTicks.assert_called_once_with(True, 0.1, True)
        self.options.setTips.assert_called_once_with(True)
        self.context.liveCreateCoordinates.assert_called_once_with(self.options)
        self.assertIs(axes.x_axis, self.axes_members[0])
        self.assertIs(axes.y_axis, self.axes_members[1])
        self.assertIs(axes.z_axis, self.axes_members[2])

    def test_custom_labels_and_unknown_cairo_options_fail_before_allocation(self):
        with self.assertRaises(NotImplementedError):
            self.construct(labels={"x": "time"})
        with patch.object(plotting, "_coordinate_style", self.real_coordinate_style):
            with self.assertRaises(TypeError):
                self.construct(num_axis_pieces=5)
        self.factory.assert_called_once()
        self.options.free.assert_called_once_with()
        self.context.liveCreateCoordinates.assert_not_called()

    def test_coordinate_math_is_a_rust_frame_call(self):
        axes = self.construct()
        frame = Mock(name="rust-frame")
        frame.coordsToPoint.return_value = [1.0, 2.0, 3.0]
        frame.pointToCoords.return_value = [4.0, 5.0, 6.0]
        axes._semantic_family_handle.threeDAxesFrame.return_value = frame
        with patch.object(plotting, "_coordinate_context", return_value=None):
            self.assertEqual(axes.c2p(1, 2, 3), (1.0, 2.0, 3.0))
            self.assertEqual(axes.p2c((1, 2, 3)), (4.0, 5.0, 6.0))
        frame.coordsToPoint.assert_called_once_with(1.0, 2.0, 3.0)
        frame.pointToCoords.assert_called_once_with(1.0, 2.0, 3.0)


if __name__ == "__main__":
    main()
