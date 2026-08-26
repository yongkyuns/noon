import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


PYTHON_ROOT = Path(__file__).resolve().parent


class ManimDotEllipseTests(unittest.TestCase):
    def test_contract_in_isolated_compatibility_process(self) -> None:
        # The Manim facade intentionally monkeypatches the public Noon module to match
        # browser authoring semantics. Keep those process-global patches out of the
        # native-Noon unittest discovery process.
        source = textwrap.dedent(
            """
            import math

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_geometry  # noqa: F401

            from noon import BLUE, DEFAULT_DOT_RADIUS, Dot, Ellipse, LEFT, RED, WHITE

            default_dot = Dot()
            assert DEFAULT_DOT_RADIUS == 0.08
            assert math.isclose(default_dot.geometry["circle"]["radius"], 0.08)
            assert math.isclose(default_dot.width, 0.16)
            assert math.isclose(default_dot.height, 0.16)
            assert default_dot.get_center() == (0.0, 0.0)
            assert math.isclose(default_dot.style["stroke_width"], 0.0)
            assert math.isclose(default_dot.style["fill"]["alpha"], 1.0)
            assert (
                default_dot.style["fill"]["red"],
                default_dot.style["fill"]["green"],
                default_dot.style["fill"]["blue"],
            ) == (WHITE.red, WHITE.green, WHITE.blue)

            shifted_dot = Dot(point=2 * LEFT, radius=0.2, color=BLUE)
            assert shifted_dot.get_center() == (-2.0, 0.0)
            assert math.isclose(shifted_dot.width, 0.4)
            assert math.isclose(shifted_dot.height, 0.4)
            assert (
                shifted_dot.style["fill"]["red"],
                shifted_dot.style["fill"]["green"],
                shifted_dot.style["fill"]["blue"],
            ) == (BLUE.red, BLUE.green, BLUE.blue)

            default_ellipse = Ellipse()
            assert math.isclose(default_ellipse.geometry["circle"]["radius"], 1.0)
            assert math.isclose(default_ellipse.width, 2.0)
            assert math.isclose(default_ellipse.height, 1.0)
            assert math.isclose(default_ellipse.transform["scale"]["x"], 1.0)
            assert math.isclose(default_ellipse.transform["scale"]["y"], 0.5)
            assert math.isclose(default_ellipse.style["stroke_width"], 0.04)
            assert math.isclose(default_ellipse.style["fill"]["alpha"], 0.0)
            assert (
                default_ellipse.style["stroke"]["red"],
                default_ellipse.style["stroke"]["green"],
                default_ellipse.style["stroke"]["blue"],
            ) == (RED.red, RED.green, RED.blue)

            rotated = Ellipse(width=4.0, height=1.5, color=BLUE).rotate(math.pi / 6)
            # ManimCE v0.21 measures the eight-cubic VMobject control hull rather
            # than the mathematical ellipse extrema for layout width/height.
            assert math.isclose(rotated.width, 3.6630139825174126, abs_tol=1e-12)
            assert math.isclose(rotated.height, 2.46422807100826, abs_tol=1e-12)
            """
        )
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(PYTHON_ROOT)
            if not existing
            else str(PYTHON_ROOT) + os.pathsep + existing
        )
        result = subprocess.run(
            [sys.executable, "-c", source],
            cwd=PYTHON_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            result.returncode,
            0,
            msg=f"isolated compatibility probe failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
