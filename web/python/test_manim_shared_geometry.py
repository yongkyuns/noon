import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedGeometryTests(unittest.TestCase):
    def _run_source(self, source: str) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing
            else os.pathsep.join((str(python_dir), existing))
        )
        completed = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(source)],
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

    def test_dot_and_triangle_bypass_python_geometry_construction(self) -> None:
        self._run_source(
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

    def test_arc_uses_shared_constructor_metadata_and_current_snapshot_queries(self) -> None:
        self._run_source(
            r"""
            import json
            import math
            import sys
            import types
            from types import SimpleNamespace

            fake_js = types.ModuleType("js")
            calls = []
            queries = []

            WHITE = (1.0, 1.0, 1.0)

            def base_style():
                return {
                    "fill": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 0.0},
                    "stroke": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0},
                    "stroke_width": 0.04,
                    "stroke_width_mode": "screen_space",
                    "stroke_join": "miter",
                    "stroke_cap": "butt",
                    "opacity": 1.0,
                }

            def arc_snapshot():
                return {
                    "geometry": {"vector_path": {"commands": [
                        {"move_to": {"to": {"x": 1.0, "y": 0.0}}},
                        {"cubic_to": {
                            "control1": {"x": 1.0, "y": 0.5},
                            "control2": {"x": 0.5, "y": 1.0},
                            "to": {"x": 0.0, "y": 1.0},
                        }},
                    ]}},
                    "transform": {
                        "translation": {"x": 0.0, "y": 0.0},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": base_style(),
                }

            class FakeHandle:
                def __init__(self, snapshot): self.snapshot = snapshot
                def snapshotJson(self): return json.dumps(self.snapshot)
                def shift(self, x, y):
                    self.snapshot["transform"]["translation"]["x"] += float(x)
                    self.snapshot["transform"]["translation"]["y"] += float(y)
                def setStrokeWidth(self, value): self.snapshot["style"]["stroke_width"] = float(value)
                def setFillOpacity(self, value): self.snapshot["style"]["fill"]["alpha"] = float(value)
                def setStrokeOpacity(self, value): self.snapshot["style"]["stroke"]["alpha"] = float(value)
                def setFillColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["fill"]["alpha"]
                    self.snapshot["style"]["fill"] = {"red": float(r), "green": float(g), "blue": float(b), "alpha": alpha}
                def setStrokeColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["stroke"]["alpha"]
                    self.snapshot["style"]["stroke"] = {"red": float(r), "green": float(g), "blue": float(b), "alpha": alpha}

            def generic_snapshot(value):
                return FakeHandle(json.loads(value))

            def arc_spec(radius, start_angle, angle, num_components, center_x, center_y):
                calls.append((
                    "arc",
                    float(radius),
                    float(start_angle),
                    float(angle),
                    int(num_components),
                    float(center_x),
                    float(center_y),
                ))
                snap = arc_snapshot()
                return SimpleNamespace(
                    snapshotJson=json.dumps(snap),
                    radius=float(radius),
                    startAngle=float(start_angle),
                    angle=float(angle),
                    numComponents=int(num_components),
                )

            def between_spec(start_x, start_y, end_x, end_y, angle, radius, num_components):
                calls.append((
                    "between",
                    float(start_x),
                    float(start_y),
                    float(end_x),
                    float(end_y),
                    float(angle),
                    None if radius is None else float(radius),
                    int(num_components),
                ))
                return SimpleNamespace(
                    snapshotJson=json.dumps(arc_snapshot()),
                    radius=3.0,
                    startAngle=0.0,
                    angle=-math.pi / 2.0,
                    numComponents=int(num_components),
                )

            def query_arc(value):
                snap = json.loads(str(value))
                queries.append(snap)
                translation = snap["transform"]["translation"]
                x = float(translation["x"])
                y = float(translation["y"])
                return SimpleNamespace(
                    startX=1.0 + x,
                    startY=y,
                    endX=x,
                    endY=1.0 + y,
                    centerX=x,
                    centerY=y,
                    stopAngle=math.pi / 2.0,
                )

            fake_js.noonCreateAuthoringMobjectHandle = generic_snapshot
            fake_js.noonCreateAuthoringArcSpec = arc_spec
            fake_js.noonCreateAuthoringArcBetweenPointsSpec = between_spec
            fake_js.noonQueryAuthoringArc = query_arc
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()
            import _manim_shared_geometry as shared_geometry
            shared_geometry.install()

            shared_geometry._fallback_arc_path = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python Arc geometry fallback was called")
            )
            shared_geometry._fallback_arc_between_points = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python ArcBetweenPoints geometry fallback was called")
            )

            from noon import Arc, ArcBetweenPoints, Vec2

            arc = Arc(
                radius=2.0,
                start_angle=0.25,
                angle=1.5,
                num_components=7,
                arc_center=(3.0, -4.0, 0.0),
            )
            between = ArcBetweenPoints((-2.0, 0.0), (2.0, 0.0), radius=-3.0)

            assert calls[0] == ("arc", 2.0, 0.25, 1.5, 7, 3.0, -4.0)
            assert calls[1][0] == "between"
            assert arc.radius == 2.0
            assert arc.start_angle == 0.25
            assert arc.angle == 1.5
            assert arc.num_components == 7
            assert between.radius == 3.0
            assert between.angle < 0.0

            arc.shift((2.0, 3.0))
            assert arc.get_start() == Vec2(3.0, 3.0)
            assert arc.get_end() == Vec2(2.0, 4.0)
            assert arc.get_arc_center() == Vec2(2.0, 3.0)
            assert math.isclose(arc.stop_angle(), math.pi / 2.0)
            assert queries
            assert queries[-1]["transform"]["translation"] == {"x": 2.0, "y": 3.0}
            """
        )


if __name__ == "__main__":
    unittest.main()
