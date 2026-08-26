import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimGroupCopyTests(unittest.TestCase):
    def test_custom_group_subclasses_clone_without_replaying_constructor(self) -> None:
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
            import _manim_phase_b  # noqa: F401 - installs pinned style/geometry semantics
            import _manim_geometry  # noqa: F401

            from noon import Arrow, ORIGIN, RIGHT, VGroup

            arrow = Arrow(ORIGIN, 2 * RIGHT)
            clone = arrow.copy()
            assert isinstance(clone, Arrow)
            assert clone is not arrow
            assert len(clone) == len(arrow)
            assert clone._shaft is clone[0]
            assert clone._tip is clone[1]
            assert clone._shaft is not arrow._shaft
            assert clone._tip is not arrow._tip

            original_center = arrow.get_center()
            clone.shift(RIGHT)
            assert arrow.get_center() == original_center
            assert clone.get_center() != original_center

            family = VGroup(arrow)
            family_clone = family.copy()
            nested_arrow = family_clone[0]
            assert isinstance(nested_arrow, Arrow)
            assert nested_arrow._shaft is nested_arrow[0]
            assert nested_arrow._tip is nested_arrow[1]
            assert nested_arrow is not arrow
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
