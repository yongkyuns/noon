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
            import _manim_geometry  # noqa: F401

            from noon import Circle, VGroup
            from _typed_geometry_test_support import identity_only_wrapper as identity

            class CustomFamily(VGroup):
                def __init__(self):
                    raise AssertionError("copy replayed user constructor")

            # This pass copies Python metadata only. Rust family-copy tests own
            # semantic duplication and independent geometry/mutation behavior.
            leaf = identity(Circle)
            family = identity(CustomFamily, submobjects=[leaf], selected=leaf)
            nested = identity(VGroup, submobjects=[family], selected=family)
            _manim_compat.Group.__deepcopy__ = _manim_compat.deepcopy_semantic_wrapper
            _manim_compat._BaseMobject.__deepcopy__ = _manim_compat.deepcopy_semantic_wrapper
            clone, pairs = _manim_compat.prepare_family_wrapper_copy(nested, lambda value: set())
            assert isinstance(clone[0], CustomFamily)
            assert clone is not nested and clone[0] is not family
            assert clone.selected is clone[0]
            assert clone[0].selected is clone[0][0]
            assert clone[0][0] is not leaf
            assert len(pairs) == 3
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
