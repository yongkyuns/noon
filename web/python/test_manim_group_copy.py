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
            from unittest.mock import patch
            import _manim_semantic_handles as handles
            with patch.object(handles, "_group_members", side_effect=lambda value: value.__dict__["submobjects"]):
                clone, pairs, members = _manim_compat.prepare_family_wrapper_copy(nested, lambda value: set())
            cloned = {id(original): target for original, target in pairs}
            assert isinstance(cloned[id(family)], CustomFamily)
            assert clone is not nested and cloned[id(family)] is not family
            assert clone.selected is cloned[id(family)]
            assert cloned[id(family)].selected is cloned[id(leaf)]
            assert cloned[id(leaf)] is not leaf
            assert dict((id(target), children) for target, children in members)[id(clone)] == [cloned[id(family)]]
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
