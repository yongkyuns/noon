import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimArrowVectorFieldDefaultRangeTests(unittest.TestCase):
    def test_default_and_mixed_ranges_are_resolved_by_rust(self) -> None:
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
                def free(self):
                    calls.append(("free_draft",))

            class Factory:
                @staticmethod
                def vectorField(*args):
                    calls.append(("field", *args))
                    return FakeDraft()
                @staticmethod
                def vectorFieldWithDefaultRanges(*args):
                    calls.append(("field_defaults", *args))
                    return FakeDraft()
                @staticmethod
                def defaultVectorFieldXStart(): return -8.0
                @staticmethod
                def defaultVectorFieldXEnd(): return 8.0
                @staticmethod
                def defaultVectorFieldXStep(): return 0.5
                @staticmethod
                def defaultVectorFieldYStart(): return -4.0
                @staticmethod
                def defaultVectorFieldYEnd(): return 4.0
                @staticmethod
                def defaultVectorFieldYStep(): return 0.5

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

            calls.clear()
            field = arrows.ArrowVectorField(lambda point: (1.0, 0.0, 0.0))
            default_calls = [call for call in calls if call[0] == "field_defaults"]
            assert default_calls == [
                ("field_defaults", True, 0.0, 0.0, 1.0, True, 0.0, 0.0, 1.0, False)
            ]
            assert field.x_range == [-8.0, 8.5, 0.5]
            assert field.y_range == [-4.0, 4.5, 0.5]
            assert calls.count(("publish",)) == 1

            calls.clear()
            field = arrows.ArrowVectorField(
                lambda point: (1.0, 0.0, 0.0),
                x_range=[-1.0, 1.0, 1.0],
            )
            default_calls = [call for call in calls if call[0] == "field_defaults"]
            assert default_calls == [
                ("field_defaults", False, -1.0, 1.0, 1.0, True, 0.0, 0.0, 1.0, False)
            ]
            assert field.x_range == [-1.0, 2.0, 1.0]
            assert field.y_range == [-4.0, 4.5, 0.5]
            assert calls.count(("publish",)) == 1

            calls.clear()
            field = arrows.ArrowVectorField(
                lambda point: (1.0, 0.0, 0.0),
                x_range=[-1.0, 1.0, 1.0],
                y_range=[-2.0, 2.0, 2.0],
            )
            assert [call for call in calls if call[0] == "field_defaults"] == []
            assert [call for call in calls if call[0] == "field"] == [
                ("field", -1.0, 1.0, 1.0, -2.0, 2.0, 2.0, False)
            ]
            assert field.x_range == [-1.0, 2.0, 1.0]
            assert field.y_range == [-2.0, 4.0, 2.0]
            assert calls.count(("publish",)) == 1
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
