import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimConstantExportTests(unittest.TestCase):
    def test_standard_buffer_constants_are_star_imported(self) -> None:
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
            from noon import Mobject, Scene, Vec2
            assert Vec2(1, 2) + [3, 4, 0] == Vec2(4, 6)
            import _manim_compat
            assert _manim_compat.Mobject is Mobject
            assert _manim_compat.Scene is Scene
            assert _manim_compat.Group.__bases__ == (Mobject,)

            namespace = {}
            exec("from noon import *", namespace)
            import noon
            from importlib import import_module
            for name, module in noon._PUBLIC_EXPORTS.items():
                assert namespace[name] is getattr(import_module(module), name)
                assert getattr(noon, name) is namespace[name]
                assert name in dir(noon)
            for module in set(noon._PUBLIC_EXPORTS.values()):
                assert not hasattr(import_module(module), "install")
            assert "Tex" not in namespace and "MathTex" not in namespace
            assert not issubclass(namespace["Text"], noon.Rectangle)
            assert namespace["SMALL_BUFF"] == 0.1
            assert namespace["MED_SMALL_BUFF"] == 0.25
            assert namespace["MED_LARGE_BUFF"] == 0.5
            assert namespace["LARGE_BUFF"] == 1.0
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
