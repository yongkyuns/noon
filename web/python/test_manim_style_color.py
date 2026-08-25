import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimStyleColorTests(unittest.TestCase):
    def test_set_color_preserves_independent_fill_and_stroke_alpha(self) -> None:
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
            import _manim_compat

            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            from noon import BLUE, GREEN, PINK, YELLOW, Circle, Square

            def assert_rgb(actual, expected):
                assert abs(actual["red"] - expected.red) < 1e-12
                assert abs(actual["green"] - expected.green) < 1e-12
                assert abs(actual["blue"] - expected.blue) < 1e-12

            # Constructor color is implemented through VMobject.set_color in
            # Manim and must not make the default zero-alpha fill visible.
            circle = Circle(color=BLUE)
            assert_rgb(circle.style["fill"], BLUE)
            assert_rgb(circle.style["stroke"], BLUE)
            assert abs(circle.style["fill"]["alpha"] - 0.0) < 1e-12
            assert abs(circle.style["stroke"]["alpha"] - 1.0) < 1e-12

            square = Square(
                fill_color=PINK,
                fill_opacity=0.35,
                stroke_color=BLUE,
                stroke_opacity=0.65,
                stroke_width=8,
            )
            square.set_color(GREEN)
            assert_rgb(square.style["fill"], GREEN)
            assert_rgb(square.style["stroke"], GREEN)
            assert abs(square.style["fill"]["alpha"] - 0.35) < 1e-12
            assert abs(square.style["stroke"]["alpha"] - 0.65) < 1e-12

            # Keep the existing Noon compatibility escape hatch: an explicitly
            # disabled layer stays disabled when the remaining style is recolored.
            square.set_fill()
            square.set_color(YELLOW)
            assert square.style["fill"] is None
            assert_rgb(square.style["stroke"], YELLOW)
            assert abs(square.style["stroke"]["alpha"] - 0.65) < 1e-12
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
