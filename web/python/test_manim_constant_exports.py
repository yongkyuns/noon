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
            import _manim_phase_b  # noqa: F401
            import _manim_geometry  # noqa: F401

            namespace = {}
            exec("from noon import *", namespace)
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
