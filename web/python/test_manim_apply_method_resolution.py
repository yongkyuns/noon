import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimApplyMethodResolutionTests(unittest.TestCase):
    def test_monkey_patched_bound_method_resolves_public_name(self) -> None:
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
            import _manim_geometry

            from noon import Dot

            dot = Dot()
            assert dot.set_color.__name__ == "_vmobject_set_color"
            assert _manim_geometry._public_bound_method_name(dot, dot.set_color) == "set_color"
            assert _manim_geometry._public_bound_method_name(dot, dot.shift) == "shift"
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
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
