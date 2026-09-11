import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimTangentLineFacadeTests(unittest.TestCase):
    def test_tangent_line_delegates_sampling_and_length_to_shared_rust(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            r"""
            import sys
            import types

            calls = []
            fake_js = types.ModuleType("js")

            class FakeOptions:
                def setZIndex(self, value): calls.append(("z_index", float(value)))
                def setTranslation(self, x, y): calls.append(("translation", float(x), float(y)))
                def setRotation(self, value): calls.append(("rotation", float(value)))
                def setScale(self, x, y): calls.append(("scale", float(x), float(y)))
                def setStrokeWidth(self, value): calls.append(("stroke_width", float(value)))
                def setStrokeWidthMode(self, value): calls.append(("stroke_width_mode", str(value)))
                def setStrokeJoin(self, value): calls.append(("stroke_join", str(value)))
                def setStrokeCap(self, value): calls.append(("stroke_cap", str(value)))
                def setObjectOpacity(self, value): calls.append(("opacity", float(value)))
                def setColor(self, r, g, b, a): calls.append(("color", float(r), float(g), float(b), float(a)))
                def disableFill(self): calls.append(("disable_fill",))
                def setFill(self, r, g, b, a): calls.append(("fill", float(r), float(g), float(b), float(a)))
                def setFillColor(self, r, g, b, a): calls.append(("fill_color", float(r), float(g), float(b), float(a)))
                def setFillOpacity(self, value): calls.append(("fill_opacity", float(value)))
                def disableStroke(self): calls.append(("disable_stroke",))
                def setStroke(self, r, g, b, a): calls.append(("stroke", float(r), float(g), float(b), float(a)))
                def setStrokeColor(self, r, g, b, a): calls.append(("stroke_color", float(r), float(g), float(b), float(a)))
                def setStrokeOpacity(self, value): calls.append(("stroke_opacity", float(value)))

            class FakeSourceHandle:
                semanticSlot = 1
                semanticGeneration = 0
                def tangentLineOptions(self, alpha, length, d_alpha):
                    calls.append(("tangent", float(alpha), float(length), float(d_alpha)))
                    return FakeOptions()

            class FakeCreatedHandle:
                semanticSlot = 2
                semanticGeneration = 0

            class EmptyGeometryOptions:
                @staticmethod
                def emptyPath(): return FakeOptions()

            fake_js.noonAuthoringGeometryOptions = EmptyGeometryOptions
            fake_js.noonAuthoringVectorPath = lambda: None
            fake_js.noonCreateAuthoringGeometryHandle = lambda options: (
                calls.append(("publish",)) or FakeCreatedHandle()
            )
            fake_js.noonCreateAuthoringFamilyHandle = lambda batch, z: None
            fake_js.noonAuthoringMembershipBatch = lambda kind: None
            sys.modules["js"] = fake_js

            import noon
            import _manim_compat as compat
            from _manim_geometry import TangentLine

            circle = object.__new__(compat.Circle)
            circle._raw = None
            circle._scene = None
            circle._object = None
            circle._semantic_handle = FakeSourceHandle()
            circle._semantic_handle_fresh = True

            tangent = TangentLine(
                circle,
                0.4,
                length=3.0,
                d_alpha=1e-5,
                color="#29ABCA",
                stroke_width=6.0,
            )
            assert calls[0] == ("tangent", 0.4, 3.0, 1e-5)
            assert ("stroke_width", 0.06) in calls
            assert any(call[0] == "color" for call in calls)
            assert ("publish",) in calls
            assert tangent.length == 3.0
            assert tangent.d_alpha == 1e-5
            assert tangent._semantic_handle.semanticSlot == 2

            before = list(calls)
            try:
                TangentLine(object(), 0.5)
            except TypeError:
                pass
            else:
                raise AssertionError("non-VMobject TangentLine source unexpectedly succeeded")
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
