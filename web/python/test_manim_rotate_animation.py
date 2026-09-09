import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotateAnimationTests(unittest.TestCase):
    def test_public_request_preserves_pivot_capture_and_timing(self) -> None:
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
            import sys
            import types

            fake_js = types.ModuleType("js")

            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_animate  # noqa: F401
            import _manim_rotate
            _manim_rotate.install()

            from noon import Rotate, Mobject, RIGHT, IN, PI
            target = Mobject.__new__(Mobject)
            target.get_center = lambda: (2.0, 1.0)
            animation = Rotate(target, about_edge=RIGHT)
            target.get_center = lambda: (3.0, 1.0)
            assert animation.about_point == (2.0, 1.0)
            assert animation.about_edge is RIGHT
            assert animation.angle == PI
            explicit = Rotate(target, about_point=(0.0, 0.0), axis=IN, run_time=2.0)
            assert explicit.about_point == (0.0, 0.0)
            assert explicit.axis is IN
            assert explicit.anim_args == {"run_time": 2.0}
            assert not hasattr(animation, "target")
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
