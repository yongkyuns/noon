import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimCyclicReplaceTests(unittest.TestCase):
    def test_requests_are_inert_and_keep_default_or_negative_path_arc(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH")) if part
        )
        source = textwrap.dedent(
            """
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = object()
            fake_js.noonResolveTransformAnimationOptions = object()
            sys.modules["js"] = fake_js

            import noon
            import _manim_animate as animate

            a = object.__new__(noon.Mobject)
            b = object.__new__(noon.Mobject)
            swap = animate.Swap(a, b)
            assert swap.mobjects == (a, b)
            assert swap.anim_args["path_arc"] == noon.PI / 2.0

            cyclic = animate.CyclicReplace(a, b, path_arc=-0.75, run_time=2.0)
            assert cyclic.mobjects == (a, b)
            assert cyclic.anim_args == {"path_arc": -0.75, "run_time": 2.0}
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"CyclicReplace request subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
