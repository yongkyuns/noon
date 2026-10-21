import unittest
from unittest.mock import Mock, patch

import noon
import _manim_compat as compat
from _typed_geometry_test_support import identity_only_wrapper


class PointPredicateDispatchTests(unittest.TestCase):
    def test_default_predicates_do_not_dispatch_get_num_points_override(self):
        for cls in (noon.Mobject, compat.VMobject, compat.Circle, compat.Square,
                    compat.VectorizedPoint, compat.Group, compat.VGroup):
            for count in (0, 4):
                with self.subTest(cls=cls.__name__, count=count):
                    root = identity_only_wrapper(cls)
                    root.get_num_points = Mock(side_effect=LookupError("count hook"))
                    with patch("_manim_path_queries.point_count", return_value=count) as native:
                        self.assertIs(root.has_points(), count != 0)
                        self.assertIs(root.has_no_points(), count == 0)
                    root.get_num_points.assert_not_called()
                    self.assertEqual(native.call_count, 2)

    def test_has_no_points_keeps_the_public_has_points_hook(self):
        for result in (False, True, LookupError("point hook")):
            with self.subTest(result=result):
                root = identity_only_wrapper(noon.Mobject)
                root.has_points = Mock(**({"side_effect": result} if isinstance(result, Exception)
                                          else {"return_value": result}))
                if isinstance(result, Exception):
                    with self.assertRaisesRegex(LookupError, "point hook"):
                        root.has_no_points()
                else:
                    self.assertIs(root.has_no_points(), not result)
                root.has_points.assert_called_once_with()
