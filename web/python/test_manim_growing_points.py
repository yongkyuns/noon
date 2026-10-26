"""Appearance request coercion over shared query results; no scene execution."""
import unittest
from unittest.mock import Mock

import noon
from _manim_growing import GrowFromCenter, GrowFromEdge, GrowFromPoint
from _typed_geometry_test_support import identity_only_wrapper


class ManimGrowingPointTests(unittest.TestCase):
    def test_mobject_point_requests_accept_planar_xyz_query_containers(self):
        target = identity_only_wrapper(noon.Mobject)
        for point in ([2., -1., 0.], (2., -1., 0.), noon.Vec2(2., -1.)):
            with self.subTest(point=point):
                target.get_center = Mock(return_value=point)
                target.get_critical_point = Mock(return_value=point)
                for request in (GrowFromPoint(target, target), GrowFromCenter(target),
                                GrowFromEdge(target, noon.UP)):
                    self.assertIs(request.mobject, target)
                    self.assertEqual(request.point, noon.Vec2(2., -1.))
                self.assertEqual(target.get_center.call_count, 2)
                target.get_critical_point.assert_called_once_with(noon.Vec2(0., 1.))

    def test_planar_request_rejects_nonzero_depth_from_mobject_query(self):
        target = identity_only_wrapper(noon.Mobject)
        target.get_center = Mock(return_value=[2., -1., 1.125])
        with self.assertRaises(NotImplementedError):
            GrowFromPoint(target, target)


if __name__ == "__main__":
    unittest.main()
