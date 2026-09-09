import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimApplyMethodResolutionTests(unittest.TestCase):
    def test_public_and_reassigned_methods_resolve_public_names(self) -> None:
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
            from _typed_geometry_test_support import identity_only_wrapper as identity
            import _manim_compat
            _manim_compat.install()
            import _manim_geometry

            from noon import Dot

            dot = identity(Dot)
            assert dot.set_color.__name__ == "set_color"
            assert _manim_geometry._public_bound_method_name(dot, dot.set_color) == "set_color"
            assert _manim_geometry._public_bound_method_name(dot, dot.shift) == "shift"
            def alternate_color_implementation(self, color):
                raise AssertionError("method resolution must not invoke the method")
            Dot.set_color = alternate_color_implementation
            assert _manim_geometry._public_bound_method_name(dot, dot.set_color) == "set_color"
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
