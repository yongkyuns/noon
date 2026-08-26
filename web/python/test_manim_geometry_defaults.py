import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimGeometryDefaultsTests(unittest.TestCase):
    def _run_compat_source(self, source: str) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )

        completed = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(source)],
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

    def test_public_rectangle_and_square_defaults_match_manim_v021(self) -> None:
        self._run_compat_source(
            """
            import inspect

            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs pinned Phase-B semantics
            from noon import Rectangle, Square

            rectangle = Rectangle()
            assert abs(rectangle.width - 4.0) < 1e-12
            assert abs(rectangle.height - 2.0) < 1e-12
            assert rectangle.geometry["rectangle"]["size"] == {"x": 4.0, "y": 2.0}

            signature = inspect.signature(Rectangle)
            assert signature.parameters["width"].default == 4.0
            assert signature.parameters["height"].default == 2.0

            explicit = Rectangle(width=1.5, height=0.75)
            assert abs(explicit.width - 1.5) < 1e-12
            assert abs(explicit.height - 0.75) < 1e-12

            square = Square()
            assert abs(square.width - 2.0) < 1e-12
            assert abs(square.height - 2.0) < 1e-12
            assert square.geometry["rectangle"]["size"] == {"x": 2.0, "y": 2.0}
            """
        )

    def test_circle_line_and_rotated_rectangle_bounds_match_manim_geometry(self) -> None:
        self._run_compat_source(
            """
            import math

            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs pinned Phase-B semantics
            from noon import Circle, LEFT, Line, PI, RIGHT, Rectangle

            circle = Circle(radius=1.25)
            assert abs(circle.width - 2.5) < 1e-12
            assert abs(circle.height - 2.5) < 1e-12
            assert circle.geometry["circle"]["radius"] == 1.25

            line = Line()
            assert line.geometry["line"]["start"] == {"x": LEFT.x, "y": LEFT.y}
            assert line.geometry["line"]["end"] == {"x": RIGHT.x, "y": RIGHT.y}
            assert abs(line.width - 2.0) < 1e-12
            assert abs(line.height) < 1e-12

            rectangle = Rectangle(width=4.0, height=2.0).rotate(PI / 6)
            expected_width = 4.0 * math.cos(PI / 6) + 2.0 * math.sin(PI / 6)
            expected_height = 4.0 * math.sin(PI / 6) + 2.0 * math.cos(PI / 6)
            assert abs(rectangle.width - expected_width) < 1e-12
            assert abs(rectangle.height - expected_height) < 1e-12
            """
        )

    def test_vector_path_bounds_use_bezier_extrema_not_control_hull(self) -> None:
        self._run_compat_source(
            """
            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401 - installs pinned Phase-B semantics
            from noon import Path, VectorPath

            curve = (
                VectorPath()
                .move_to((0.0, 0.0))
                .cubic_to((0.0, 3.0), (2.0, 3.0), (2.0, 0.0))
            )
            path = Path(curve)

            # The cubic reaches y=2.25 at t=0.5. The control hull reaches y=3,
            # so this catches conservative-control-box layout regressions.
            assert abs(path.width - 2.0) < 1e-12
            assert abs(path.height - 2.25) < 1e-12
            """
        )


if __name__ == "__main__":
    unittest.main()
