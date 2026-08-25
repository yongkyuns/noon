import math
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimLayoutBoundsTests(unittest.TestCase):
    def test_curve_extrema_and_affine_bounds_drive_layout(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )

        source = textwrap.dedent(
            """
            import math

            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs Manim bounds semantics
            from noon import Circle, PI, Path, Square, UP, VectorPath

            quadratic = Path(
                VectorPath()
                .move_to((-1.0, 0.0))
                .quadratic_to((0.0, 2.0), (1.0, 0.0))
            )
            assert abs(quadratic.width - 2.0) < 1e-12
            assert abs(quadratic.height - 1.0) < 1e-12
            assert abs(quadratic.get_top().y - 1.0) < 1e-12

            above = Square(1.0).next_to(quadratic, UP, buff=0.5)
            gap = (above.get_center().y - above.height * 0.5) - quadratic.get_top().y
            assert abs(gap - 0.5) < 1e-12

            cubic = Path(
                VectorPath()
                .move_to((0.0, 0.0))
                .cubic_to((0.0, 3.0), (2.0, 3.0), (2.0, 0.0))
            )
            assert abs(cubic.height - 2.25) < 1e-12

            rotated_curve = Path(
                VectorPath()
                .move_to((-1.0, 0.0))
                .quadratic_to((0.0, 2.0), (1.0, 0.0))
            ).rotate(PI / 4.0)
            expected_rotated_extent = 9.0 * math.sqrt(2.0) / 8.0
            assert abs(rotated_curve.width - expected_rotated_extent) < 1e-12
            assert abs(rotated_curve.height - expected_rotated_extent) < 1e-12

            ellipse = Circle(1.0).scale((2.0, 1.0)).rotate(PI / 4.0)
            expected_ellipse_extent = math.sqrt(10.0)
            assert abs(ellipse.width - expected_ellipse_extent) < 1e-12
            assert abs(ellipse.height - expected_ellipse_extent) < 1e-12
            """
        )

        completed = subprocess.run(
            [sys.executable, "-c", source],
            check=False,
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"compatibility subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
