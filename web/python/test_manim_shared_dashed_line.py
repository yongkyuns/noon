import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedDashedLineTests(unittest.TestCase):
    def test_constructor_stays_on_shared_rust_geometry(self) -> None:
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
                wireTranslationX = 0.0
                wireTranslationY = 0.0
                wireScaleX = 1.0
                wireScaleY = 1.0
                wireRotation = 0.0
                wireHasFill = True
                wireFillRed = wireFillGreen = wireFillBlue = 1.0
                wireFillAlpha = 0.0
                wireHasStroke = True
                wireStrokeRed = wireStrokeGreen = wireStrokeBlue = 1.0
                wireStrokeAlpha = 1.0
                wireStrokeWidth = 0.04
                wireObjectOpacity = 1.0

                def __init__(self, snapshot):
                    self.snapshot = snapshot

                def snapshotJson(self):
                    return json.dumps(self.snapshot)

            def generic_handle(snapshot_json):
                return FakeHandle(json.loads(snapshot_json))

            def dashed_line(sx, sy, ex, ey, dash_length, dashed_ratio):
                calls.append((float(sx), float(sy), float(ex), float(ey), float(dash_length), float(dashed_ratio)))
                return FakeHandle({
                    "geometry": {"vector_path": {"commands": [
                        {"move_to": {"to": {"x": float(sx), "y": float(sy)}}},
                        {"line_to": {"to": {"x": float(ex), "y": float(ey)}}},
                    ]}},
                    "transform": {"translation": {"x": 0.0, "y": 0.0}, "rotation": 0.0, "scale": {"x": 1.0, "y": 1.0}},
                    "style": {"fill": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 0.0}, "stroke": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0}, "stroke_width": 0.04, "stroke_width_mode": "screen_space", "stroke_join": "miter", "stroke_cap": "butt", "opacity": 1.0},
                })

            fake_js.noonCreateAuthoringMobjectHandle = generic_handle
            fake_js.noonCreateAuthoringDashedLineHandle = dashed_line
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()

            _manim_compat._ir.Path = lambda *args, **kwargs: (_ for _ in ()).throw(AssertionError("Python path reconstruction was called"))

            import _manim_dashed_line
            _manim_dashed_line.install()
            from noon import DashedLine, Line

            line = DashedLine(start=(-2, 1), end=(3, -1), dash_length=0.2, dashed_ratio=0.25)
            assert isinstance(line, Line)
            assert calls == [(-2.0, 1.0, 3.0, -1.0, 0.2, 0.25)]
            assert "vector_path" in line.geometry

            before = list(calls)
            for kwargs in ({"dash_length": 0.0}, {"dashed_ratio": -0.1}, {"dashed_ratio": 1.1}):
                try:
                    DashedLine(**kwargs)
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid dash parameters must fail before bridge dispatch")
            assert calls == before
            """
        )
        completed = subprocess.run([sys.executable, "-c", source], cwd=python_dir, env=env, capture_output=True, text=True, check=False)
        self.assertEqual(completed.returncode, 0, f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}")


if __name__ == "__main__":
    unittest.main()
