import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimMoveToTargetTests(unittest.TestCase):
    def test_exact_example_preserves_target_transform_contract(self) -> None:
        python_dir = Path(__file__).resolve().parent
        repo_root = python_dir.parent.parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            part for part in (str(python_dir), env.get("PYTHONPATH", "")) if part
        )
        source = textwrap.dedent(
            f"""
            import runpy
            from _typed_geometry_test_support import identity_only_wrapper as identity
            import _manim_compat
            from _test_manim_membership import install_test_membership
            install_test_membership(_manim_compat)
            import _manim_rate_functions
            from noon import Circle, MoveToTarget, RIGHT, Scene, Transform, UP, VGroup

            missing = identity(Circle)
            try:
                MoveToTarget(missing)
                raise AssertionError("missing target must fail")
            except ValueError as error:
                assert str(error) == "MoveToTarget called on mobject without attribute 'target'"

            group = identity(VGroup, submobjects=[])
            group.generate_target = lambda: None
            try:
                MoveToTarget(group)
                raise AssertionError("group target must fail")
            except NotImplementedError:
                pass

            # Executable target endpoints are covered by shared-authoring-smoke;
            # this unit test protects Python target-editor selection and rollback.
            # Canonical installation supplies this factory. `generate_target` must
            # select it rather than Python's ordinary `copy`, so MoveToTarget
            # receives the opaque target-editor result.
            class CanonicalSource:
                def __init__(self):
                    self.calls = []
                def _copy_for_animate_target(self):
                    assert self.target is None
                    self.calls.append("target-editor")
                    return object()
                def copy(self):
                    raise AssertionError("generate_target must not use raw copy")

            canonical = CanonicalSource()
            captured = _manim_compat._mobject_generate_target(canonical)
            assert captured is canonical.target
            assert canonical.calls == ["target-editor"]
            recaptured = _manim_compat._mobject_generate_target(canonical)
            assert recaptured is canonical.target and recaptured is not captured
            def rejected_capture():
                assert canonical.target is None
                raise ValueError("capture rejected")
            canonical._copy_for_animate_target = rejected_capture
            try:
                _manim_compat._mobject_generate_target(canonical)
            except ValueError:
                pass
            else:
                raise AssertionError("rejected target capture succeeded")
            assert canonical.target is recaptured

            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
            cwd=repo_root,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
