import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedPlacementHandleTests(unittest.TestCase):
    def test_leaf_placement_math_stays_behind_shared_handle(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = str(python_dir) if not existing else os.pathsep.join((str(python_dir), existing))
        source = textwrap.dedent(
            r"""
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")

            class FakeHandle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)
                    self.calls = []

                def snapshotJson(self):
                    raise AssertionError("placement must not materialize snapshot JSON")

                @property
                def centerX(self): return float(self.snapshot["transform"]["translation"]["x"])
                @property
                def centerY(self): return float(self.snapshot["transform"]["translation"]["y"])
                @property
                def width(self):
                    geometry = self.snapshot["geometry"]
                    base = float(geometry["rectangle"]["size"]["x"])
                    return abs(base * float(self.snapshot["transform"]["scale"]["x"]))
                @property
                def height(self):
                    geometry = self.snapshot["geometry"]
                    base = float(geometry["rectangle"]["size"]["y"])
                    return abs(base * float(self.snapshot["transform"]["scale"]["y"]))
                def criticalX(self, x, y):
                    return self.centerX + (-self.width / 2 if x < 0 else self.width / 2 if x > 0 else 0.0)
                def criticalY(self, x, y):
                    return self.centerY + (-self.height / 2 if y < 0 else self.height / 2 if y > 0 else 0.0)
                def setFillOpacity(self, value): pass
                def setStrokeOpacity(self, value): pass
                def shift(self, x, y):
                    t = self.snapshot["transform"]["translation"]
                    t["x"] += float(x); t["y"] += float(y)
                def manimMoveToHandle(self, other, ex, ey, mx, my):
                    self.calls.append("manimMoveToHandle")
                    self.shift((other.criticalX(ex, ey) - self.criticalX(ex, ey)) * mx,
                               (other.criticalY(ex, ey) - self.criticalY(ex, ey)) * my)
                def manimMoveToPoint(self, px, py, ex, ey, mx, my):
                    self.calls.append("manimMoveToPoint")
                    self.shift((px - self.criticalX(ex, ey)) * mx,
                               (py - self.criticalY(ex, ey)) * my)
                def manimNextToHandle(self, other, dx, dy, buff, ex, ey, mx, my):
                    self.calls.append("manimNextToHandle")
                    self.shift((other.criticalX(ex + dx, ey + dy) - self.criticalX(ex - dx, ey - dy) + dx * buff) * mx,
                               (other.criticalY(ex + dx, ey + dy) - self.criticalY(ex - dx, ey - dy) + dy * buff) * my)
                def manimNextToPoint(self, px, py, dx, dy, buff, ex, ey, mx, my):
                    self.calls.append("manimNextToPoint")
                    self.shift((px - self.criticalX(ex - dx, ey - dy) + dx * buff) * mx,
                               (py - self.criticalY(ex - dx, ey - dy) + dy * buff) * my)
                def alignToHandle(self, other, dx, dy):
                    self.calls.append("alignToHandle")
                    self.shift(other.criticalX(dx, dy) - self.criticalX(dx, dy) if dx != 0 else 0.0,
                               other.criticalY(dx, dy) - self.criticalY(dx, dy) if dy != 0 else 0.0)
                def alignToPoint(self, px, py, dx, dy):
                    self.calls.append("alignToPoint")
                    self.shift(px - self.criticalX(dx, dy) if dx != 0 else 0.0,
                               py - self.criticalY(dx, dy) if dy != 0 else 0.0)

            fake_js.noonCreateAuthoringMobjectHandle = FakeHandle
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles
            handles._create_handle = FakeHandle
            handles.install()

            from noon import LEFT, RIGHT, Square, UP, UR

            reference = Square(2.0)
            diagonal = Square(2.0).next_to(reference, UR, buff=0.25)
            assert diagonal._semantic_handle.calls[-1] == "manimNextToHandle"
            assert abs(diagonal.get_center().x - 2.25) < 1e-12
            assert abs(diagonal.get_center().y - 2.25) < 1e-12

            moved = Square(1.0).shift((0.0, -2.0, 0.0))
            moved.move_to(reference, aligned_edge=UP + LEFT, coor_mask=(1.0, 0.0, 0.0))
            assert moved._semantic_handle.calls[-1] == "manimMoveToHandle"
            assert abs(moved.get_left().x - reference.get_left().x) < 1e-12
            assert abs(moved.get_center().y + 2.0) < 1e-12

            point = Square(1.0).move_to((3.0, 4.0, 0.0), coor_mask=(1.0, 0.0, 0.0))
            assert point._semantic_handle.calls[-1] == "manimMoveToPoint"
            assert abs(point.get_center().x - 3.0) < 1e-12
            assert abs(point.get_center().y) < 1e-12

            aligned = Square(1.0).shift((0.0, -1.0, 0.0)).align_to(reference, RIGHT)
            assert aligned._semantic_handle.calls[-1] == "alignToHandle"
            assert abs(aligned.get_right().x - reference.get_right().x) < 1e-12
            assert abs(aligned.get_center().y + 1.0) < 1e-12
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(completed.returncode, 0, f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}")


if __name__ == "__main__":
    unittest.main()
