import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimArrowVectorFieldColorTests(unittest.TestCase):
    def test_static_color_inputs_are_prepared_then_delegated_to_rust(self) -> None:
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

            class FakeLeaf:
                def __init__(self, slot):
                    self.semanticSlot = slot
                    self.semanticGeneration = 0

            class FakeFamily:
                def __init__(self, slot, members=()):
                    self.semanticSlot = slot
                    self.semanticGeneration = 0
                    self.members = list(members)
                    self.memberCount = len(self.members)
                def memberKeys(self):
                    return [f"{member.semanticSlot}:{member.semanticGeneration}" for member in self.members]
                def layout(self): return self
                def layoutAnchor(self, index): return self

            class FakeDraft:
                sampleCount = 2
                def sampleX(self, index): return float(index)
                def sampleY(self, index): return 0.0
                def setVector(self, index, x, y):
                    calls.append(("vector", int(index), float(x), float(y)))
                def setDisplayLength(self, index, value):
                    calls.append(("length", int(index), float(value)))
                def setFieldColor(self, r, g, b, a):
                    calls.append(("single", float(r), float(g), float(b), float(a)))
                def setColorGradient(self, minimum, maximum, custom):
                    calls.append(("gradient", float(minimum), float(maximum), bool(custom)))
                def addColorGradientStop(self, r, g, b, a):
                    calls.append(("stop", float(r), float(g), float(b), float(a)))
                def setColorValue(self, index, value):
                    calls.append(("color_value", int(index), float(value)))
                def free(self):
                    calls.append(("free_draft",))

            class Factory:
                @staticmethod
                def vectorField(*args):
                    calls.append(("field", *args))
                    return FakeDraft()

            class FakeCreated:
                vectorCount = 2
                def __init__(self):
                    self._shafts = [FakeLeaf(10), FakeLeaf(20)]
                    self._tips = [FakeLeaf(11), FakeLeaf(21)]
                    self._families = [
                        FakeFamily(100, (self._shafts[0], self._tips[0])),
                        FakeFamily(200, (self._shafts[1], self._tips[1])),
                    ]
                    self._family = FakeFamily(300, self._families)
                def family(self): return self._family
                def vectorFamily(self, index): return self._families[index]
                def vectorShaft(self, index): return self._shafts[index]
                def vectorEndTip(self, index): return self._tips[index]

            def create_arrow(draft):
                calls.append(("publish",))
                return FakeCreated()

            fake_js.noonAuthoringArrowOptions = Factory
            fake_js.noonCreateAuthoringArrowHandle = create_arrow
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

            calls.clear()
            scheme_calls = []
            field = arrows.ArrowVectorField(
                lambda point: (point[0] + 1.0, 0.0, 0.0),
                color="#D147BD",
                color_scheme=lambda vector: scheme_calls.append(vector) or 99.0,
                colors=[noon.RED, noon.BLUE],
                min_color_scheme_value=10,
                max_color_scheme_value=20,
                x_range=[0.0, 1.0, 1.0],
                y_range=[0.0, 0.0, 1.0],
            )
            assert field.single_color is True
            assert scheme_calls == []
            assert any(call[0] == "single" for call in calls)
            assert not any(call[0] in {"gradient", "stop", "color_value"} for call in calls)
            assert calls.count(("publish",)) == 1

            calls.clear()
            field = arrows.ArrowVectorField(
                lambda point: (point[0] + 1.0, 0.0, 0.0),
                colors=[noon.RED, noon.BLUE],
                min_color_scheme_value=1.0,
                max_color_scheme_value=2.0,
                x_range=[0.0, 1.0, 1.0],
                y_range=[0.0, 0.0, 1.0],
            )
            assert field.single_color is False
            assert ("gradient", 1.0, 2.0, False) in calls
            stops = [call for call in calls if call[0] == "stop"]
            assert len(stops) == 2
            assert not any(call[0] == "color_value" for call in calls)
            assert calls.count(("publish",)) == 1

            calls.clear()
            seen = []
            field = arrows.ArrowVectorField(
                lambda point: (point[0] + 1.0, point[0], 0.0),
                color_scheme=lambda vector: seen.append(tuple(vector)) or vector[0] - vector[1],
                colors=[noon.GREEN, noon.YELLOW, noon.RED],
                min_color_scheme_value=0.0,
                max_color_scheme_value=2.0,
                length_func=lambda norm: norm * 0.25,
                x_range=[0.0, 1.0, 1.0],
                y_range=[0.0, 0.0, 1.0],
            )
            assert seen == [(1.0, 0.0, 0.0), (2.0, 1.0, 0.0)]
            assert ("gradient", 0.0, 2.0, True) in calls
            assert [call for call in calls if call[0] == "color_value"] == [
                ("color_value", 0, 1.0),
                ("color_value", 1, 1.0),
            ]
            assert len([call for call in calls if call[0] == "length"]) == 2
            assert calls.count(("publish",)) == 1

            before = list(calls)
            for thunk in (
                lambda: arrows.ArrowVectorField(
                    lambda point: (1.0, 0.0, 0.0),
                    colors=[],
                    x_range=[0.0, 0.0, 1.0],
                    y_range=[0.0, 0.0, 1.0],
                ),
                lambda: arrows.ArrowVectorField(
                    lambda point: (1.0, 0.0, 0.0),
                    colors=[noon.RED, noon.BLUE],
                    min_color_scheme_value=2.0,
                    max_color_scheme_value=2.0,
                    x_range=[0.0, 0.0, 1.0],
                    y_range=[0.0, 0.0, 1.0],
                ),
            ):
                try:
                    thunk()
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid color configuration unexpectedly published")
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
