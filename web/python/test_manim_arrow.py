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
                def startX(self): calls.append(("query", "startX")); return -1.5
                def startY(self): calls.append(("query", "startY")); return 0.5
                def endX(self): calls.append(("query", "endX")); return 3.5
                def endY(self): calls.append(("query", "endY")); return -2.5
                def vectorX(self): calls.append(("query", "vectorX")); return 5.0
                def vectorY(self): calls.append(("query", "vectorY")); return -3.0
                def length(self): calls.append(("query", "length")); return 5.830951894845301
                def unitVectorX(self): calls.append(("query", "unitVectorX")); return 0.8574929257125441
                def unitVectorY(self): calls.append(("query", "unitVectorY")); return -0.5144957554275265
                def angle(self): calls.append(("query", "angle")); return -0.5404195002705842
                def scale(self, factor, scale_tips):
                    calls.append(("arrow_scale", float(factor), bool(scale_tips)))

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
                def setFill(self, r, g, b, a): self._record("fill", float(r), float(g), float(b), float(a))
                def disableFill(self): self._record("disable_fill")
                def setFillOpacity(self, value): self._record("fill_opacity", float(value))
                def setStrokeColor(self, r, g, b, a): self._record("stroke_color", float(r), float(g), float(b), float(a))
                def disableStroke(self): self._record("disable_stroke")
                def setStrokeOpacity(self, value): self._record("stroke_opacity", float(value))
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

            def boundary_options(kind, mode, *values):
                encoded = tuple(
                    value.semanticSlot if isinstance(value, FakeLeafHandle) else float(value)
                    for value in values
                )
                calls.append((f"boundary_{kind}_{mode}", *encoded))
                return FakeOptions(kind, None, None)

            fake_js.noonAuthoringArrowFromMobjects = lambda start, end: boundary_options("arrow", "both", start, end)
            fake_js.noonAuthoringArrowFromMobject = lambda start, ex, ey: boundary_options("arrow", "start", start, ex, ey)
            fake_js.noonAuthoringArrowToMobject = lambda sx, sy, end: boundary_options("arrow", "end", sx, sy, end)
            fake_js.noonAuthoringDoubleArrowFromMobjects = lambda start, end: boundary_options("double", "both", start, end)
            fake_js.noonAuthoringDoubleArrowFromMobject = lambda start, ex, ey: boundary_options("double", "start", start, ex, ey)
            fake_js.noonAuthoringDoubleArrowToMobject = lambda sx, sy, end: boundary_options("double", "end", sx, sy, end)

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

            # Python passes opaque Mobject handles into the Rust boundary constructor;
            # it never queries centers, anchors, or boundary coordinates itself.
            def endpoint(slot):
                wrapper = object.__new__(noon.Mobject)
                arrows._shared._attach_shared_handle(wrapper, FakeLeafHandle(slot))
                return wrapper

            start_mobject = endpoint(20)
            end_mobject = endpoint(21)
            bounded = arrows.Arrow(start_mobject, end_mobject, buff=0.0)
            bounded_from = arrows.Arrow(start_mobject, (5.0, 2.0), buff=0.0)
            bounded_to = arrows.DoubleArrow((-4.0, 1.0), end_mobject, buff=0.0)
            assert ("boundary_arrow_both", 20, 21) in calls
            assert ("boundary_arrow_start", 20, 5.0, 2.0) in calls
            assert ("boundary_double_end", -4.0, 1.0, 21) in calls
            assert len(bounded.submobjects) == 2
            assert len(bounded_from.submobjects) == 2
            assert len(bounded_to.submobjects) == 3

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

            # Endpoint/vector/length/angle observations are also owned by the retained
            # aggregate Rust handle, not recomputed from Python leaf geometry.
            assert arrow.get_start() == noon.Vec2(-1.5, 0.5)
            assert arrow.get_end() == noon.Vec2(3.5, -2.5)
            assert arrow.get_vector() == noon.Vec2(5.0, -3.0)
            assert abs(arrow.get_length() - 5.830951894845301) < 1e-12
            assert arrow.get_unit_vector() == noon.Vec2(0.8574929257125441, -0.5144957554275265)
            assert abs(arrow.get_angle() + 0.5404195002705842) < 1e-12
            assert [call for call in calls if call[0] == "query"] == [
                ("query", "startX"),
                ("query", "startY"),
                ("query", "endX"),
                ("query", "endY"),
                ("query", "vectorX"),
                ("query", "vectorY"),
                ("query", "length"),
                ("query", "unitVectorX"),
                ("query", "unitVectorY"),
                ("query", "angle"),
            ]

            # Arrow-specific dependent scaling is one Rust operation; Python does
            # not pop/rebuild tips or retain the original stroke-width policy.
            assert arrow.scale(2.0) is arrow
            assert arrow.scale(0.5, scale_tips=True) is arrow
            assert ("arrow_scale", 2.0, False) in calls
            assert ("arrow_scale", 0.5, True) in calls

            # Unsupported ManimCE semantic breadth rejects rather than being approximated.
            before = list(calls)
            for thunk in (
                lambda: arrows.Arrow(path_arc=0.5),
                lambda: arrows.Arrow(tip_shape=object()),
                lambda: arrows.Arrow(tip_style={"fill_opacity": 0.0}),
                lambda: arrows.DoubleArrow(tip_shape_start=object()),
                lambda: arrow.scale(2.0, about_point=(0.0, 0.0)),
            ):
                try:
                    thunk()
                except NotImplementedError:
                    pass
                else:
                    raise AssertionError("unsupported Arrow case unexpectedly succeeded")
            assert calls == before

            # Old/non-ManimCE constructor names are no longer advertised as supported.
            publish_count = sum(call[0] == "publish" for call in calls)
            for name, value in (
                ("preserve_tip_size_when_scaling", False),
                ("use_rectangular_stem", True),
            ):
                try:
                    arrows.Arrow(**{name: value})
                except TypeError:
                    pass
                else:
                    raise AssertionError(f"stale Arrow option {name} unexpectedly succeeded")
            assert sum(call[0] == "publish" for call in calls) == publish_count
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
