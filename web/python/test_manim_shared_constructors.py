import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedConstructorTests(unittest.TestCase):
    def test_primitive_constructors_bypass_python_ir_snapshots(self) -> None:
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
            calls = []

            class FakeHandle:
                def __init__(self, geometry, color):
                    self.snapshot = {
                        "geometry": geometry,
                        "transform": {"translation": {"x": 0.0, "y": 0.0}, "rotation": 0.0, "scale": {"x": 1.0, "y": 1.0}},
                        "style": {
                            "fill": {"red": color[0], "green": color[1], "blue": color[2], "alpha": 0.0},
                            "stroke": {"red": color[0], "green": color[1], "blue": color[2], "alpha": 1.0},
                            "stroke_width": 0.04,
                            "stroke_width_mode": "screen_space",
                            "stroke_join": "miter",
                            "stroke_cap": "butt",
                            "opacity": 1.0,
                        },
                    }
                def snapshotJson(self): return json.dumps(self.snapshot)
                def setTranslation(self, x, y): self.snapshot["transform"]["translation"] = {"x": float(x), "y": float(y)}
                def setScale(self, x, y): self.snapshot["transform"]["scale"] = {"x": float(x), "y": float(y)}
                def setRotation(self, value): self.snapshot["transform"]["rotation"] = float(value)
                def setStrokeWidth(self, value): self.snapshot["style"]["stroke_width"] = float(value)
                def setStrokeWidthMode(self, value): self.snapshot["style"]["stroke_width_mode"] = str(value)
                def setStrokeJoin(self, value): self.snapshot["style"]["stroke_join"] = str(value)
                def setStrokeCap(self, value): self.snapshot["style"]["stroke_cap"] = str(value)
                def setObjectOpacity(self, value): self.snapshot["style"]["opacity"] = float(value)
                def setFill(self, r, g, b, a): self.snapshot["style"]["fill"] = {"red": float(r), "green": float(g), "blue": float(b), "alpha": float(a)}
                def disableFill(self): self.snapshot["style"]["fill"] = None
                def setFillOpacity(self, value):
                    if self.snapshot["style"]["fill"] is None:
                        self.snapshot["style"]["fill"] = {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": float(value)}
                    else: self.snapshot["style"]["fill"]["alpha"] = float(value)
                def setFillColor(self, r, g, b, a):
                    current = self.snapshot["style"]["fill"]
                    alpha = float(a) if current is None else current["alpha"]
                    self.snapshot["style"]["fill"] = {"red": float(r), "green": float(g), "blue": float(b), "alpha": alpha}
                def setStrokeColor(self, r, g, b, a):
                    current = self.snapshot["style"]["stroke"]
                    alpha = float(a) if current is None else current["alpha"]
                    self.snapshot["style"]["stroke"] = {"red": float(r), "green": float(g), "blue": float(b), "alpha": alpha}
                def setStrokeOpacity(self, value):
                    if self.snapshot["style"]["stroke"] is None:
                        self.snapshot["style"]["stroke"] = {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": float(value)}
                    else: self.snapshot["style"]["stroke"]["alpha"] = float(value)
                def disableStroke(self): self.snapshot["style"]["stroke"] = None
                @property
                def wireHasFill(self): return self.snapshot["style"]["fill"] is not None
                @property
                def wireHasStroke(self): return self.snapshot["style"]["stroke"] is not None

            def generic_snapshot(*args, **kwargs):
                raise AssertionError("primitive constructor must not create a Python snapshot handle")

            RED = (0xFC/255, 0x62/255, 0x55/255)
            WHITE = (1.0, 1.0, 1.0)
            def circle(radius):
                calls.append("circle")
                return FakeHandle({"circle": {"radius": float(radius)}}, RED)
            def square(side):
                calls.append("square")
                return FakeHandle({"rectangle": {"size": {"x": float(side), "y": float(side)}}}, WHITE)
            def rectangle(width, height):
                calls.append("rectangle")
                return FakeHandle({"rectangle": {"size": {"x": float(width), "y": float(height)}}}, WHITE)
            def line(sx, sy, ex, ey):
                calls.append("line")
                return FakeHandle({"line": {"start": {"x": float(sx), "y": float(sy)}, "end": {"x": float(ex), "y": float(ey)}}}, WHITE)

            fake_js.noonCreateAuthoringMobjectHandle = generic_snapshot
            fake_js.noonCreateAuthoringCircleHandle = circle
            fake_js.noonCreateAuthoringSquareHandle = square
            fake_js.noonCreateAuthoringRectangleHandle = rectangle
            fake_js.noonCreateAuthoringLineHandle = line
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_semantic_handles as handles
            handles.install()

            for name in ("Circle", "Rectangle", "Line"):
                setattr(_manim_compat._ir, name, lambda *args, **kwargs: (_ for _ in ()).throw(AssertionError("Python IR constructor was called")))

            from noon import BLUE, Circle, Line, Rectangle, Square

            c = Circle(1.5)
            r = Rectangle()
            s = Square(
                2.0,
                fill_color="#112233",
                fill_opacity=0.25,
                stroke_color=BLUE,
                stroke_opacity=0.5,
                stroke_width=7,
                position=(2.0, 3.0),
                rotation=0.4,
                scale=(2.0, 0.5),
                stroke_join="bevel",
                stroke_cap="square",
                opacity=0.8,
            )
            l = Line((-2.0, 1.0, 0.0), (3.0, -1.0, 0.0))

            assert calls == ["circle", "rectangle", "square", "line"]
            assert abs(c.style["stroke_width"] - 0.04) < 1e-12
            assert c.style["stroke_width_mode"] == "screen_space"
            assert c.style["stroke_join"] == "miter"
            assert c.style["stroke_cap"] == "butt"
            assert abs(c.style["fill"]["red"] - RED[0]) < 1e-12 and c.style["fill"]["alpha"] == 0.0
            assert r.width_value == 4.0 and r.height_value == 2.0
            assert abs(s.style["stroke_width"] - 0.07) < 1e-12
            assert s.style["stroke_join"] == "bevel" and s.style["stroke_cap"] == "square"
            assert abs(s.style["fill"]["alpha"] - 0.25) < 1e-12
            assert abs(s.style["stroke"]["alpha"] - 0.5) < 1e-12
            assert s.transform["translation"] == {"x": 2.0, "y": 3.0}
            assert s.transform["scale"] == {"x": 2.0, "y": 0.5}
            assert abs(s.transform["rotation"] - 0.4) < 1e-12
            assert abs(s.style["opacity"] - 0.8) < 1e-12
            assert l.start.x == -2.0 and l.end.x == 3.0
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(completed.returncode, 0, f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}")


if __name__ == "__main__":
    unittest.main()
