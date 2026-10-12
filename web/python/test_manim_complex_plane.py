"""ComplexPlane keeps coordinate conversion in the shared Rust frame adapter."""

from unittest import TestCase
from unittest.mock import patch

import _manim_complex_plane as complex_plane
import noon


class ComplexPlaneAdapterTests(TestCase):
    def test_complex_round_trip_uses_shared_frame_queries(self):
        plane = object.__new__(complex_plane.ComplexPlane)
        with patch.object(
            plane, "coords_to_point", return_value=noon.Vec2(3.5, -2.0)
        ) as to_point:
            self.assertEqual(plane.n2p(2 + 1j), noon.Vec2(3.5, -2.0))
            to_point.assert_called_once_with(2.0, 1.0)
        with patch.object(
            plane, "point_to_coords", return_value=noon.Vec2(-4.0, 0.25)
        ) as from_point:
            self.assertEqual(plane.p2n(noon.Vec2(7, 8)), complex(-4.0, 0.25))
            from_point.assert_called_once_with(noon.Vec2(7, 8))

    def test_queries_are_not_cached_across_transformed_frame_reads(self):
        plane = object.__new__(complex_plane.ComplexPlane)
        world_offset = [0.0]

        def coords_to_point(x, y):
            return noon.Vec2(x + world_offset[0], y)

        with patch.object(plane, "coords_to_point", side_effect=coords_to_point):
            self.assertEqual(plane.n2p(1 + 2j), noon.Vec2(1.0, 2.0))
            world_offset[0] = 3.0
            self.assertEqual(plane.n2p(1 + 2j), noon.Vec2(4.0, 2.0))
