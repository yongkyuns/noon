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
            import _manim_compat; _manim_compat.install()
            import _manim_rate_functions; _manim_rate_functions.install()
            from noon import Circle, MoveToTarget, RIGHT, Scene, Transform, UP, VGroup

            missing = Circle()
            try:
                MoveToTarget(missing)
                raise AssertionError("missing target must fail")
            except ValueError as error:
                assert str(error) == "MoveToTarget called on mobject without attribute 'target'"

            group = VGroup(Circle(), Circle())
            group.generate_target = lambda: None
            try:
                MoveToTarget(group)
                raise AssertionError("group target must fail")
            except NotImplementedError:
                pass

            namespace = runpy.run_path({str(repo_root / "web/python/examples/manim_example_move_to_target.py")!r})
            scene = namespace["MoveToTargetExample"]()
            scene.construct()
            assert abs(scene.time - 1.0) < 1e-12
            tracks = [t for t in scene._tracks if t["property"] == "transform"]
            assert len(tracks) == 1
            assert abs(tracks[0]["timing"]["duration"] - 1.0) < 1e-12
            target = tracks[0]["values"]["object"]["to"]
            assert abs(target["transform"]["translation"]["x"] - 2.0) < 1e-12
            assert abs(target["transform"]["translation"]["y"] - 1.0) < 1e-12
            assert abs(target["transform"]["scale"]["x"] - 0.5) < 1e-12

            c = Circle(); c.generate_target(); pending = MoveToTarget(c); c.target.shift(RIGHT)
            scene2 = Scene(); scene2.add(c); scene2.play(pending)
            target2 = [t for t in scene2._tracks if t["property"] == "transform"][0]["values"]["object"]["to"]
            assert abs(target2["transform"]["translation"]["x"] - 1.0) < 1e-12

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
