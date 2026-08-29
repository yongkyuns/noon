import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedMatchXYTests(unittest.TestCase):
    def test_match_xy_uses_shared_directional_placement(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing
            else os.pathsep.join((str(python_dir), existing))
        )
        source = textwrap.dedent(
            r"""
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")
            move_calls = []

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                def snapshotJson(self):
                    return json.dumps(self.snapshot)
                def setFillOpacity(self, value):
                    if self.snapshot["style"]["fill"] is not None:
                        self.snapshot["style"]["fill"]["alpha"] = float(value)
                def setStrokeOpacity(self, value):
                    if self.snapshot["style"]["stroke"] is not None:
                        self.snapshot["style"]["stroke"]["alpha"] = float(value)
                def shift(self, x, y):
                    translation = self.snapshot["transform"]["translation"]
                    translation["x"] += float(x)
                    translation["y"] += float(y)
                def manimMoveToHandle(self, other, edge_x, edge_y, mask_x, mask_y):
                    move_calls.append((
                        float(edge_x),
                        float(edge_y),
                        float(mask_x),
                        float(mask_y),
                    ))
                    translation = self.snapshot["transform"]["translation"]
                    target = other.snapshot["transform"]["translation"]
                    if mask_x:
                        translation["x"] = target["x"]
                    if mask_y:
                        translation["y"] = target["y"]

            def generic_snapshot(value):
                return FakeHandle(json.loads(value))

            fake_js.noonCreateAuthoringMobjectHandle = generic_snapshot
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()
            import _manim_shared_geometry
            _manim_shared_geometry.install()

            from noon import Circle, RIGHT, UP

            source = Circle(1.0).shift((-2.0, -1.0))
            target = Circle(1.0).shift((4.0, 3.0))
            fail = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python critical-point placement was used")
            )
            source.get_critical_point = fail
            target.get_critical_point = fail

            assert source.match_x(target, RIGHT) is source
            assert source.match_y(target, UP) is source
            assert move_calls == [
                (1.0, 0.0, 1.0, 0.0),
                (0.0, 1.0, 0.0, 1.0),
            ]
            assert source.transform["translation"] == {"x": 4.0, "y": 3.0}
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
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
