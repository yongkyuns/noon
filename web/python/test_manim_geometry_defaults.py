import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimGeometryDefaultsTests(unittest.TestCase):
    def test_public_rectangle_and_square_defaults_match_manim_v021(self) -> None:
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
