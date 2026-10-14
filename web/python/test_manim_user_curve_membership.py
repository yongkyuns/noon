import unittest

try:
    import numpy as np
except ImportError:
    np = None

import _manim_namespace as namespace
import _manim_user_vectors as helpers
import noon


class UserCurveMembership(unittest.TestCase):
    def test_public_callables_and_numpy_discovery(self):
        for name in ("proportions_along_bezier_curve_for_point", "point_lies_on_bezier"):
            self.assertIs(getattr(noon, name), getattr(helpers, name))
            self.assertIn(name, noon.__all__)
            self.assertEqual(namespace.required_packages(f"{name}(p, control)"), ("numpy",))

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_line_membership_and_off_curve_rejection(self):
        controls = np.array([[0., 0., 0.], [1., 1., 1.]])
        point = np.array([.25, .25, .25])
        np.testing.assert_array_equal(noon.proportions_along_bezier_curve_for_point(point, controls), [.25])
        self.assertIs(noon.point_lies_on_bezier(point, controls), True)
        self.assertIs(noon.point_lies_on_bezier([.25, .5, .25], controls), False)

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_revisited_point_retains_both_original_parameters(self):
        controls = np.array([[0., 0., 0.], [1., 0., 0.], [0., 0., 0.]])
        np.testing.assert_array_equal(
            noon.proportions_along_bezier_curve_for_point([.375, 0, 0], controls), [.25, .75])

    @unittest.skipIf(np is None, "NumPy runs in the pinned array-oracle lane")
    def test_shape_errors_and_constant_curve_error_preserve_inputs(self):
        controls = np.array([[0., 0., 0.], [1., 1., 1.]])
        before = controls.copy()
        controls.flags.writeable = False
        with self.assertRaises(ValueError):
            noon.proportions_along_bezier_curve_for_point([1, 2], controls)
        with self.assertRaises(TypeError):
            noon.point_lies_on_bezier([1, 2, 3], [[1, 2, 3]])
        np.testing.assert_array_equal(controls, before)


if __name__ == "__main__":
    unittest.main()
