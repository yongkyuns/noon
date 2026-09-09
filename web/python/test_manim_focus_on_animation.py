import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimFocusOnAnimationTests(unittest.TestCase):
    def test_focus_on_is_an_inert_request_with_no_python_spotlight(self) -> None:
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

            import noon

            from noon import FocusOn, Mobject
            def forbidden(*args, **kwargs):
                raise AssertionError("FocusOn constructed Python spotlight geometry")
            noon.Circle = forbidden
            focus = FocusOn((2.0, 1.0))
            assert focus.focus_point == (2.0, 1.0)
            assert focus.opacity == 0.2
            assert focus.anim_args["run_time"] == 2.0
            assert not hasattr(focus, "mobject") and not hasattr(focus, "target")
            target = Mobject.__new__(Mobject)
            target.get_center = lambda: (3.0, -2.0)
            observed = FocusOn(target)
            assert observed.focus_point == (3.0, -2.0)
            assert observed.focus_mobject is target
            for kwargs in ({"opacity": -0.1}, {"opacity": float("nan")}, {"run_time": 0.0}):
                try:
                    FocusOn((0.0, 0.0), **kwargs)
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid focus arguments were accepted")
            for kwargs in ({"remover": False}, {"introducer": False}, {"path_arc": 1.0}, {"lag_ratio": 0.2}):
                try:
                    FocusOn((0.0, 0.0), **kwargs)
                except NotImplementedError:
                    pass
                else:
                    raise AssertionError("unsupported focus arguments were accepted")
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
