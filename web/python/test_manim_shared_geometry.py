import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedGeometryTests(unittest.TestCase):
    def test_dot_and_triangle_bypass_python_geometry_construction(self) -> None:
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
            calls = []

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                def snapshotJson(self): return json.dumps(self.snapshot)
                def setStrokeWidth(self, value): self.snapshot["style"]["stroke_width"] = float(value)
                def setFillOpacity(self, value): self.snapshot["style"]["fill"]["alpha"] = float(value)
                def setFillColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["fill"]["alpha"]
                    self.snapshot["style"]["fill"] = {"red": float(r), "green": float(g), "blue": float(b), "alpha": alpha}
                def setStrokeColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["stroke"]["alpha"]
                    self.snapshot["style"]["stroke"] = {"red": float(r), "green": float(g), "blue": float(b), "alpha": alpha}

            def base_style(color, *, fill_alpha=0.0, stroke_width=0.04):
                return {
                    "fill": {"red": color[0], "green": color[1], "blue": color[2], "alpha": fill_alpha},
                    "stroke": {"red": color[0], "green": color[1], "blue": color[2], "alpha": 1.0},
                    "stroke_width": stroke_width,
                    "stroke_width_mode": "screen_space",
                    "stroke_join": "miter",
                    "stroke_cap": "butt",
                    "opacity": 1.0,
                }

            def snapshot(geometry, style, x=0.0, y=0.0):
                return {
                    "geometry": geometry,
                    "transform": {
                        "translation": {"x": float(x), "y": float(y)},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": style,
                }

            WHITE = (1.0, 1.0, 1.0)
            BLUE = (0x58/255, 0xC4/255, 0xDD/255)

            def generic_snapshot(value):
                return FakeHandle(json.loads(value))

            def dot(x, y, radius):
                calls.append(("dot", float(x), float(y), float(radius)))
                return FakeHandle(snapshot(
                    {"circle": {"radius": float(radius)}},
                    base_style(WHITE, fill_alpha=1.0, stroke_width=0.0),
                    x,
                    y,
                ))

            def triangle():
                calls.append(("triangle",))
                return FakeHandle(snapshot(
                    {"vector_path": {"commands": [
                        {"move_to": {"to": {"x": 0.0, "y": 1.0}}},
                        {"line_to": {"to": {"x": -0.8660254, "y": -0.5}}},
                        {"line_to": {"to": {"x": 0.8660254, "y": -0.5}}},
                        "close",
                    ]}},
                    base_style(BLUE),
                ))

            fake_js.noonCreateAuthoringMobjectHandle = generic_snapshot
            fake_js.noonCreateAuthoringDotHandle = dot
            fake_js.noonCreateAuthoringTriangleHandle = triangle
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()

            _manim_compat._ir.Circle = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python Circle geometry constructor was called")
            )
            _manim_compat._ir.Path = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python Path geometry constructor was called")
            )

            import _manim_shared_geometry
            _manim_shared_geometry.install()
            from noon import Dot, Triangle

            dot_obj = Dot((2.0, -3.0, 0.0), radius=0.2)
            triangle_obj = Triangle()

            assert calls == [("dot", 2.0, -3.0, 0.2), ("triangle",)]
            assert dot_obj.radius == 0.2
            assert dot_obj.geometry == {"circle": {"radius": 0.2}}
            assert dot_obj.transform["translation"] == {"x": 2.0, "y": -3.0}
            assert dot_obj.style["fill"]["alpha"] == 1.0
            assert dot_obj.style["stroke_width"] == 0.0
            assert triangle_obj.style["stroke"]["red"] == BLUE[0]
            assert "vector_path" in triangle_obj.geometry
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
