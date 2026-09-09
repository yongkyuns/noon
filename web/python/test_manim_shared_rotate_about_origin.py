import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedRotateAboutOriginTests(unittest.TestCase):
    def test_rotate_about_origin_uses_shared_explicit_pivot(self) -> None:
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
            import math
            import sys
            import types

            fake_js = types.ModuleType("js")
            rotations = []

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
                def rotateAboutPoint(self, angle, x, y):
                    rotations.append((float(angle), float(x), float(y)))
                    self.snapshot["transform"]["rotation"] += float(angle)

            def generic_snapshot(value):
                return FakeHandle(json.loads(value))

            import _typed_geometry_test_support as _geometry_test

            _geometry_test.install_js_bridge(fake_js, generic_snapshot)
            sys.modules["js"] = fake_js

            import noon
            geometry_methods = {
                name: getattr(noon.Mobject, name)
                for name in ("set_coord", "match_coord", "match_x", "match_y", "rotate_about_origin")
            }
            import _manim_compat
            _manim_compat.install()
            import _manim_geometry
            import _manim_semantic_handles as handles

            import _manim_shared_geometry
            _manim_shared_geometry.install()
            assert all(getattr(noon.Mobject, name) is method for name, method in geometry_methods.items())

            from noon import Circle

            circle = Circle(1.0).shift((3.0, -2.0))
            circle.get_center = lambda: (_ for _ in ()).throw(
                AssertionError("rotate_about_origin queried Python center")
            )
            assert circle.rotate_about_origin(math.pi / 3.0) is circle
            assert rotations == [(math.pi / 3.0, 0.0, 0.0)]
            assert abs(circle.transform["rotation"] - math.pi / 3.0) < 1e-12
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
