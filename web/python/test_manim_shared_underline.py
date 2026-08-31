import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedUnderlineTests(unittest.TestCase):
    def test_constructor_uses_target_semantic_handle_and_shared_matcher(self) -> None:
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
                    self.wireStrokeWidth = float(snapshot["style"]["stroke_width"])
                    self.wireObjectOpacity = 1.0

                def snapshotJson(self):
                    return json.dumps(self.snapshot)

                def setStrokeWidth(self, value):
                    self.snapshot["style"]["stroke_width"] = float(value)
                    self.wireStrokeWidth = float(value)

                def setFillColor(self, red, green, blue, alpha):
                    current_alpha = self.snapshot["style"]["fill"]["alpha"]
                    self.snapshot["style"]["fill"] = {
                        "red": float(red),
                        "green": float(green),
                        "blue": float(blue),
                        "alpha": current_alpha,
                    }
                    self.wireFillRed = float(red)
                    self.wireFillGreen = float(green)
                    self.wireFillBlue = float(blue)

                def setStrokeColor(self, red, green, blue, alpha):
                    current_alpha = self.snapshot["style"]["stroke"]["alpha"]
                    self.snapshot["style"]["stroke"] = {
                        "red": float(red),
                        "green": float(green),
                        "blue": float(blue),
                        "alpha": current_alpha,
                    }
                    self.wireStrokeRed = float(red)
                    self.wireStrokeGreen = float(green)
                    self.wireStrokeBlue = float(blue)

            def rectangle_handle(width, height):
                return FakeHandle({
                    "geometry": {"rectangle": {"size": {"x": float(width), "y": float(height)}}},
                    "transform": {
                        "translation": {"x": 0.0, "y": 0.0},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": style(),
                })

            def generic_handle(snapshot_json):
                return FakeHandle(json.loads(snapshot_json))

            def underline_handle(target_handle, buff):
                calls.append((target_handle, float(buff)))
                target = target_handle.snapshot
                size = target["geometry"]["rectangle"]["size"]
                center = target["transform"]["translation"]
                half_width = float(size["x"]) / 2.0
                half_height = float(size["y"]) / 2.0
                y = float(center["y"]) - half_height - float(buff)
                return FakeHandle({
                    "geometry": {"line": {
                        "start": {"x": float(center["x"]) - half_width, "y": y},
                        "end": {"x": float(center["x"]) + half_width, "y": y},
                    }},
                    "transform": {
                        "translation": {"x": 0.0, "y": 0.0},
                        "rotation": 0.0,
                        "scale": {"x": 1.0, "y": 1.0},
                    },
                    "style": style(),
                })

            fake_js.noonCreateAuthoringMobjectHandle = generic_handle
            fake_js.noonCreateAuthoringRectangleHandle = rectangle_handle
            fake_js.noonCreateAuthoringUnderlineHandle = underline_handle
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b
            import _manim_geometry
            import _manim_semantic_handles as handles
            handles.install()

            # Underline geometry and placement must stay below the Python facade.
            _manim_compat._ir.Line = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python Line geometry constructor was called")
            )
            import noon as _base
            _base._bounds = lambda *args, **kwargs: (_ for _ in ()).throw(
                AssertionError("Python bounds computation was called")
            )

            import _manim_shared_geometry
            _manim_shared_geometry.install()
            from noon import BLUE, Line, Rectangle, Underline

            target = Rectangle(width=4.0, height=2.0)
            target_handle = target._semantic_handle
            underline = Underline(
                target,
                buff=0.25,
                color=BLUE,
                stroke_width=2.0,
            )

            assert isinstance(underline, Line)
            assert len(calls) == 1
            assert calls[0][0] is target_handle
            assert calls[0][1] == 0.25
            assert underline.buff == 0.25
            assert underline.geometry["line"] == {
                "start": {"x": -2.0, "y": -1.25},
                "end": {"x": 2.0, "y": -1.25},
            }
            assert underline.style["stroke_width"] == 0.02
            assert underline.style["stroke"]["red"] == BLUE.red
            assert underline.style["stroke"]["green"] == BLUE.green
            assert underline.style["stroke"]["blue"] == BLUE.blue

            before = len(calls)
            target._semantic_handle_fresh = False
            try:
                Underline(target)
            except NotImplementedError as error:
                assert "shared semantic geometry" in str(error)
            else:
                raise AssertionError("stale/non-shared targets must not use Python bounds fallback")
            assert len(calls) == before

            target._semantic_handle_fresh = True
            try:
                Underline(target, buff=float("nan"))
            except ValueError as error:
                assert "buff" in str(error)
            else:
                raise AssertionError("non-finite buff must be rejected before bridge dispatch")
            assert len(calls) == before
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
