"""PolarPlane adapter tests: Python owns syntax while Rust owns the grid."""
from types import SimpleNamespace
from unittest import TestCase, main
from unittest.mock import Mock, patch

import _manim_polar_plane as polar_plane


class PolarPlaneAdapterTests(TestCase):
    def setUp(self):
        self.addCleanup(patch.stopall)
        self.options = Mock(name="polar-options")
        self.context = SimpleNamespace(
            liveCreateCoordinates=Mock(name="live-create", return_value=Mock(name="handle")),
            queryPolarPlaneFrame=Mock(name="polar-frame"),
        )
        self.handle = self.context.liveCreateCoordinates.return_value
        self.parts = [Mock(name=f"part-{index}") for index in range(4)]
        self.handle.polarPlanePart.side_effect = lambda index: self.parts[index]
        patch.object(polar_plane._plot, "_coordinate_constructor_context", return_value=self.context).start()
        patch.object(polar_plane._plot, "_coordinate_options", SimpleNamespace(
            polarPlane=Mock(return_value=self.options),
        )).start()
        patch.object(polar_plane._plot, "_array", side_effect=lambda values: list(values)).start()
        patch.object(polar_plane._plot, "engine_call", side_effect=lambda fn, *args: fn(*args)).start()
        patch.object(polar_plane, "engine_call", side_effect=lambda fn, *args: fn(*args)).start()
        patch.object(polar_plane._plot, "_coordinate_style").start()
        self.members = [Mock(name=f"member-{index}") for index in range(4)]
        lines = patch.object(polar_plane, "_lines", side_effect=self.members[:2]).start()
        axes = patch.object(polar_plane._plot, "_attach_number_line", side_effect=self.members[2:]).start()
        patch.object(polar_plane._plot, "_family", side_effect=self._family).start()
        patch.object(polar_plane.PolarPlane, "submobjects", new_callable=lambda: property(lambda value: value._members)).start()

    def _family(self, wrapper, handle, members):
        wrapper._members = list(members)
        wrapper._semantic_family_handle = handle
        return wrapper

    def test_constructor_forwards_polar_options_and_preserves_part_order(self):
        plane = polar_plane.PolarPlane(
            radius_max=3, size=6, radius_step=.5, azimuth_step=12,
            azimuth_offset=.25, azimuth_direction="CW", faded_line_ratio=2,
        )
        self.assertEqual(
            polar_plane._plot._coordinate_options.polarPlane.call_args.args,
            (3.0, 6.0, .5, 12.0, .25, True, 2),
        )
        self.context.liveCreateCoordinates.assert_called_once_with(self.options)
        self.assertEqual(plane.submobjects, self.members)
        self.assertIs(plane.faded_lines, self.members[0])
        self.assertIs(plane.background_lines, self.members[1])
        self.assertIs(plane.x_axis, self.members[2])
        self.assertIs(plane.y_axis, self.members[3])

    def test_units_only_select_the_shared_default_azimuth_step(self):
        polar_plane.PolarPlane(azimuth_units="degrees")
        self.assertEqual(polar_plane._plot._coordinate_options.polarPlane.call_args.args[3], 36)

    def test_invalid_direction_rejects_before_shared_option_creation(self):
        with self.assertRaisesRegex(ValueError, "azimuth_direction"):
            polar_plane.PolarPlane(azimuth_direction="sideways")
        polar_plane._plot._coordinate_options.polarPlane.assert_not_called()


if __name__ == "__main__":
    main()
