import unittest

from noon import (
    BLUE,
    DEGREES,
    DEFAULT_MOBJECT_TO_EDGE_BUFFER,
    DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    LEFT,
    PI,
    RED,
    RIGHT,
    UP,
    UR,
    Circle,
    Scene,
    Square,
    VGroup,
    Vec2,
    color_from_hex,
)


class PublicAuthoringTests(unittest.TestCase):
    def test_direction_arithmetic_stays_tuple_compatible(self) -> None:
        point = 2.0 * LEFT + 0.75 * UP
        self.assertIsInstance(point, tuple)
        self.assertEqual(point, (-2.0, 0.75, 0.0))
        self.assertAlmostEqual(90 * DEGREES, PI / 2.0)

    def test_xyz_numpy_arithmetic_preserves_array_types_and_depth(self) -> None:
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy interoperability runs in the pinned utility lane")
        import noon
        point = np.array([4.0, 2.0, 1.125])
        # These seven outcomes match the retained actual ManimCE 0.21.0 probe.
        operations = [
            (lambda: point + noon.RIGHT, [5., 2., 1.125]),
            (lambda: noon.RIGHT + point, [5., 2., 1.125]),
            (lambda: point - noon.UP, [4., 1., 1.125]),
            (lambda: noon.UP - point, [-4., -1., -1.125]),
            (lambda: point * noon.RIGHT, [4., 0., 0.]),
            (lambda: noon.RIGHT * point, [4., 0., 0.]),
            (lambda: np.asarray(noon.ORIGIN) + point, [4., 2., 1.125]),
        ]
        for operation, expected in operations:
            with self.subTest(expected=expected):
                result = operation()
                self.assertIs(type(result), np.ndarray)
                self.assertEqual(result.dtype, np.dtype("float64"))
                self.assertEqual(result.shape, (3,))
                np.testing.assert_array_equal(result, expected)

    def test_direction_constants_do_not_load_numpy_on_cold_import(self) -> None:
        import os
        import subprocess
        import sys
        from pathlib import Path
        source = (
            "import sys, noon; assert 'numpy' not in sys.modules; "
            "assert tuple(noon.RIGHT) == (1., 0., 0.); "
            "assert tuple(noon.UP + noon.LEFT) == (-1., 1., 0.)"
        )
        result = subprocess.run(
            [sys.executable, "-S", "-c", source],
            env={**os.environ, "PYTHONPATH": str(Path(__file__).parent)},
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_direction_arithmetic_respects_array_ufunc_opt_out(self) -> None:
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy interoperability runs in the pinned utility lane")
        import noon
        class DisabledArray(np.ndarray):
            __array_ufunc__ = None
        value = np.zeros(3).view(DisabledArray)
        with self.assertRaises(TypeError):
            noon.RIGHT + value

    def test_named_palette_matches_canonical_values(self) -> None:
        self.assertEqual(BLUE, color_from_hex("#58C4DD"))
        self.assertEqual(RED, color_from_hex(0xFC6255))

    def test_planar_path_commands_accept_xyz_direction_constants(self) -> None:
        import noon
        path = (noon.VectorPath().move_to(noon.ORIGIN).line_to(noon.RIGHT)
                .quadratic_to(noon.UP, noon.LEFT)
                .cubic_to(noon.DOWN, noon.UR, noon.DL))
        expected = (noon.VectorPath().move_to((0., 0.)).line_to((1., 0.))
                    .quadratic_to((0., 1.), (-1., 0.))
                    .cubic_to((0., -1.), (1., 1.), (-1., -1.)))
        self.assertEqual(path.to_ir(), expected.to_ir())

    def test_planar_path_rejects_depth_before_appending_commands(self) -> None:
        import noon
        path = noon.VectorPath().move_to(noon.ORIGIN)
        before = path.to_ir()
        for point in ((1., 2., 1.125), (1., 2., float("nan"))):
            for operation in (lambda: path.line_to(point),
                              lambda: path.quadratic_to(point, noon.RIGHT),
                              lambda: path.cubic_to(noon.UP, point, noon.RIGHT)):
                with self.subTest(point=point), self.assertRaises(ValueError):
                    operation()
                self.assertEqual(path.to_ir(), before)


if __name__ == "__main__":
    unittest.main()
