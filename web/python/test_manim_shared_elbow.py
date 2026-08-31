import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedElbowTests(unittest.TestCase):
    def test_constructor_stays_on_shared_rust_geometry(self) -> None:
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
            calls = []

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                    self.wireTranslationX = 0.0
                    self.wireTranslationY = 0.0
                    self.wireScaleX = 1.0
                    self.wireScaleY = 1.0
                    self.wireRotation = 0.0
                    self.wireHasFill = True
                    self.wireFillRed = 1.0
                    self.wireFillGreen = 1.0
                    self.wireFillBlue = 1.0
                    self.wireFillAlpha = 0.0
                    self.wireHasStroke = True
                    self.wireStrokeRed = 1.0
                    self.wireStrokeGreen = 1.0
                    self.wireStrokeBlue = 1.0
                    self.wireStrokeAlpha = 1.0
                    self.wireStrokeWidth = 0.04
                    self.wireObjectOpacity = 1.0

                def snapshotJson(self):
                    return json.dumps(self.snapshot)

                def setStrokeWidth(self, value):
                    self.snapshot["style"]["stroke_width"] = float(value)
                    self.wireStrokeWidth = float(value)

                def setFillColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["fill"]["alpha"]
                    self.snapshot["style"]["fill"] = {
                        "red": float(r), "green": float(g), "blue": float(b), "alpha": alpha
                    }
                    self.wireFillRed = float(r)
                    self.wireFillGreen = float(g)
                    self.wireFillBlue = float(b)

                def setStrokeColor(self, r, g, b, a):
                    alpha = self.snapshot["style"]["stroke"]["alpha"]
                    self.snapshot["style"]["stroke"] = {
                        "red": float(r), "green": float(g), "blue": float(b), "alpha": alpha
                    }
                    self.wireStrokeRed = float(r)
                    self.wireStrokeGreen = float(g)
                    self.wireStrokeBlue = float(b)

            def style():
                return {
                    "fill": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 0.0},
                    "stroke": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0},
                    "stroke_width": 0.04,
                    "stroke_width_mode": "screen_space",
                    "stroke_join": "miter",
                    "stroke_cap": "butt",
                    "opacity": 1.0,
                }

            def generic_handle(snapshot_json):
                return FakeHandle(json.loads(snapshot_json))

            def elbow(width, angle):
                calls.append(("elbow", float(width), float(angle)))
                return FakeHandle({
                    "geometry": {"vector_path": {"commands": [
                        {"move_to": {"to": {"x": 0.0, "y": float(width)}}},
                        {"line_to": {"to": {"x": float(width), "y": float(width)}}},
                        {"line_to": {"to": {"x": float(width), "y": 0.0}}},
                    ]}},
                    "transform": {
                        "translation": {"x": 0.0, "y": 0.0},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": style(),
                })

            fake_js.noonCreateAuthoringMobjectHandle = generic_handle
            fake_js.noonCreateAuthoringElbowHandle = elbow
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()

            _manim_compat._ir.Path = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python Path geometry constructor was called")
            )

            import _manim_shared_geometry
            _manim_shared_geometry.install()
            from noon import BLUE, Elbow, VMobject

            shape = Elbow(width=-0.5, angle=math.pi / 3.0, color=BLUE, stroke_width=2.0)
            assert isinstance(shape, VMobject)
            assert calls == [("elbow", -0.5, math.pi / 3.0)]
            assert "vector_path" in shape.geometry
            assert shape.style["stroke_width"] == 0.02
            assert shape.style["stroke"]["red"] == BLUE.red
            assert shape.style["stroke"]["green"] == BLUE.green
            assert shape.style["stroke"]["blue"] == BLUE.blue

            before = list(calls)
            try:
                Elbow(width=float("nan"))
            except ValueError as error:
                assert "width" in str(error)
            else:
                raise AssertionError("non-finite width must be rejected before bridge dispatch")
            assert calls == before

            try:
                Elbow(angle=float("inf"))
            except ValueError as error:
                assert "angle" in str(error)
            else:
                raise AssertionError("non-finite angle must be rejected before bridge dispatch")
            assert calls == before
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
