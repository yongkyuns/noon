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
            import _manim_compat
            _manim_compat.install()
            import _manim_geometry  # noqa: F401

            namespace = {}
            exec("from noon import *", namespace)
            import noon
            import _manim_shared_geometry
            for name in noon._GEOMETRY_EXPORTS:
                assert namespace[name] is getattr(_manim_shared_geometry, name)
                assert getattr(noon, name) is namespace[name]
                assert name in dir(noon)
            assert not hasattr(_manim_shared_geometry, "install")
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
