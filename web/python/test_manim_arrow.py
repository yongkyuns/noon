import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimArrowFacadeTests(unittest.TestCase):
    def test_arrow_family_delegates_constructor_semantics_to_shared_rust(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        source = textwrap.dedent(
            r"""
            import sys
            import types

            fake_js = types.ModuleType("js")
            calls = []

            class FakeLeafHandle:
                def __init__(self, slot):
                    self.semanticSlot = slot
                    self.semanticGeneration = 0

            class FakeFamilyHandle:
                def __init__(self, members):
                    self.members = members
                    self.semanticSlot = 100
                    self.semanticGeneration = 0
                    self.memberCount = len(members)
                def memberKeys(self):
                    return [f"{member.semanticSlot}:{member.semanticGeneration}" for member in self.members]
                def layout(self):
                    return self
                def shiftBy(self, x, y):
                    calls.append(("family_shift", float(x), float(y)))
                def scale(self, x, y):
                    calls.append(("family_scale", float(x), float(y)))
                def layoutAnchor(self, index):
                    return self

            class FakeCreatedArrow:
                def __init__(self, double=False):
                    self._shaft = FakeLeafHandle(1)
                    self._start = FakeLeafHandle(2) if double else None
                    self._end = FakeLeafHandle(3)
                    members = [self._shaft]
                    if self._start is not None:
                        members.append(self._start)
                    members.append(self._end)
                    self._family = FakeFamilyHandle(members)
                    self.hasStartTip = self._start is not None
                def family(self): return self._family
                def shaft(self): return self._shaft
                def startTip(self): return self._start
                def endTip(self): return self._end

            class FakeOptions:
                def __init__(self, kind, start, end):
                    self.kind = kind
                    self.start = start
                    self.end = end
                def _record(self, name, *values):
                    calls.append((name, *values))
                def setBuff(self, value): self._record("buff", float(value))
                def setTipLength(self, value): self._record("tip_length", float(value))
                def setMaxTipLengthToLengthRatio(self, value): self._record("tip_ratio", float(value))
                def setMaxStrokeWidthToLengthRatio(self, value): self._record("stroke_ratio", float(value))
                def setZIndex(self, value): self._record("z_index", float(value))
                def setTranslation(self, x, y): self._record("translation", float(x), float(y))
                def setRotation(self, value): self._record("rotation", float(value))
                def setScale(self, x, y): self._record("scale", float(x), float(y))
                def setStrokeWidth(self, value): self._record("stroke_width", float(value))
                def setStrokeWidthMode(self, value): self._record("stroke_width_mode", str(value))
                def setStrokeJoin(self, value): self._record("stroke_join", str(value))
                def setStrokeCap(self, value): self._record("stroke_cap", str(value))
                def setObjectOpacity(self, value): self._record("object_opacity", float(value))
                def setColor(self, r, g, b, a): self._record("color", float(r), float(g), float(b), float(a))

            class Factory:
                @staticmethod
                def arrow(sx, sy, ex, ey):
                    calls.append(("arrow", float(sx), float(sy), float(ex), float(ey)))
                    return FakeOptions("arrow", (sx, sy), (ex, ey))
                @staticmethod
                def vector(dx, dy):
                    calls.append(("vector", float(dx), float(dy)))
                    return FakeOptions("vector", (0.0, 0.0), (dx, dy))
                @staticmethod
                def doubleArrow(sx, sy, ex, ey):
                    calls.append(("double_arrow", float(sx), float(sy), float(ex), float(ey)))
                    return FakeOptions("double", (sx, sy), (ex, ey))

            def create_arrow(options):
                calls.append(("publish", options.kind))
                return FakeCreatedArrow(options.kind == "double")

            fake_js.noonAuthoringArrowOptions = Factory
            fake_js.noonCreateAuthoringArrowHandle = create_arrow

            # The ordinary compatibility modules import these shared bindings too.
            class EmptyGeometryOptions:
                @staticmethod
                def emptyPath(): return types.SimpleNamespace()
            fake_js.noonAuthoringGeometryOptions = EmptyGeometryOptions
            fake_js.noonAuthoringVectorPath = lambda: None
            fake_js.noonCreateAuthoringGeometryHandle = lambda value: value
            fake_js.noonCreateAuthoringFamilyHandle = lambda batch, z: None
            fake_js.noonAuthoringMembershipBatch = lambda kind: None
            sys.modules["js"] = fake_js

            import _manim_arrow as arrows
            import noon

            # Endpoint queries remain delegated to the established shared path-query module;
            # replace only that projection in this native facade test.
            import _manim_path_queries
            _manim_path_queries.endpoint = lambda value, end: noon.Vec2(
                float(value._semantic_handle.semanticSlot), 1.0 if end else 0.0
            )

            arrow = arrows.Arrow(
                (-2.0, 1.0),
                (4.0, -3.0),
                buff=0.4,
                tip_length=0.5,
                max_tip_length_to_length_ratio=0.2,
                max_stroke_width_to_length_ratio=6.0,
                stroke_width=8.0,
                position=(1.0, 2.0),
                color="#58C4DD",
            )
            vector = arrows.Vector((2.0, 3.0))
            double = arrows.DoubleArrow((-1.0, 0.0), (1.0, 0.0))

            assert calls[0] == ("arrow", -2.0, 1.0, 4.0, -3.0)
            assert ("buff", 0.4) in calls
            assert ("tip_length", 0.5) in calls
            assert ("tip_ratio", 0.2) in calls
            assert ("stroke_ratio", 0.06) in calls
            assert ("stroke_width", 0.08) in calls
            assert ("translation", 1.0, 2.0) in calls
            assert ("publish", "arrow") in calls
            assert ("vector", 2.0, 3.0) in calls
            assert ("publish", "vector") in calls
            assert ("double_arrow", -1.0, 0.0, 1.0, 0.0) in calls
            assert ("publish", "double") in calls

            # Python does not recompute family geometry. Supported whole-object edits
            # route through the existing shared family handle.
            arrow.shift((0.5, -0.25))
            assert ("family_shift", 0.5, -0.25) in calls
            assert len(arrow.submobjects) == 2
            assert len(double.submobjects) == 3
            assert not arrow.has_start_tip()
            assert double.has_start_tip()
            assert arrow.get_tip() is arrow.tip
            assert double.get_start_tip() is double.start_tip
            assert arrow.get_end() == noon.Vec2(3.0, 0.0)

            # Unsupported semantic breadth is rejected rather than approximated in Python.
            before = list(calls)
            for thunk in (
                lambda: arrows.Arrow(path_arc=0.5),
                lambda: arrows.Arrow(tip_shape=object()),
                lambda: arrows.Arrow(preserve_tip_size_when_scaling=False),
                lambda: arrow.scale(2.0),
            ):
                try:
                    thunk()
                except NotImplementedError:
                    pass
                else:
                    raise AssertionError("unsupported Arrow case unexpectedly succeeded")
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
