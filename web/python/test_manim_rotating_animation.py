import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimRotatingAnimationTests(unittest.TestCase):
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

            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_animate  # noqa: F401
            import _manim_rotate
            _manim_rotate.install()

            from noon import Rotating, Mobject, RIGHT, TAU, linear
            target = Mobject.__new__(Mobject)
            def forbidden():
                raise AssertionError("Rotating eagerly captured its pivot")
            target.get_center = forbidden
            animation = Rotating(target)
            assert animation.about_point is None
            assert animation.about_edge is None
            assert animation.angle == TAU
            assert animation.anim_args == {"run_time": 5.0, "rate_func": linear}
            edge = Rotating(target, about_edge=RIGHT, run_time=2.0)
            assert edge.about_point is None and edge.about_edge is RIGHT
            assert edge.anim_args["run_time"] == 2.0
            import _manim_updaters

            import noon
            assert noon.Rotating is Rotating
            from noon import Group, Rotate
            family = Group.__new__(Group)
            for kind in (Rotate, Rotating):
                try:
                    kind(family)
                except NotImplementedError as error:
                    assert "family pivot" in str(error)
                else:
                    raise AssertionError("unsupported family rotation constructed a fallback animation")
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
