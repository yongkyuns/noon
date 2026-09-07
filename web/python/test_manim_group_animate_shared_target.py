import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimGroupAnimateSharedTargetTests(unittest.TestCase):
    def test_group_animate_target_uses_shared_family_editor(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )
        source = textwrap.dedent(
            """
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveUniformCompositionSchedule = object()
            fake_js.noonResolveAnimationOptions = object()

            next_id = 1
            def allocate_id():
                global next_id
                value = next_id
                next_id += 1
                return value

            class FakeHandle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)
                    self.semanticSlot = allocate_id()
                    self.semanticGeneration = 0
                    self.calls = []

                def identity(self):
                    return ("mobject", self.semanticSlot, self.semanticGeneration)

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(",", ":"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.calls.append("replaceSnapshotJson")
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    self.calls.append("cloneHandle")
                    clone = FakeHandle(self.snapshotJson())
                    clone.calls.append("cloneHandle")
                    return clone

                def targetEditor(self):
                    self.calls.append("targetEditor")
                    clone = FakeHandle(self.snapshotJson())
                    clone.calls.append("targetEditor")
                    return clone

                def setFillOpacity(self, opacity):
                    fill = self.snapshot["style"].get("fill")
                    if fill is not None:
                        fill["alpha"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot["style"].get("stroke")
                    if stroke is not None:
                        stroke["alpha"] = float(opacity)

                def setFill(self, red, green, blue, opacity):
                    self.calls.append("setFill")
                    self.snapshot["style"]["fill"] = {
                        "red": float(red),
                        "green": float(green),
                        "blue": float(blue),
                        "alpha": float(opacity),
                    }

                def setFillColor(self, red, green, blue, alpha):
                    fill = self.snapshot["style"].get("fill")
                    if fill is None:
                        fill = {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": float(alpha)}
                        self.snapshot["style"]["fill"] = fill
                    fill.update(red=float(red), green=float(green), blue=float(blue))

                def setStrokeColor(self, red, green, blue, alpha):
                    pass

                def shift(self, x, y):
                    self.calls.append(("shift", float(x), float(y)))
                    translation = self.snapshot["transform"]["translation"]
                    translation["x"] += float(x)
                    translation["y"] += float(y)

            class FakeFamilyTargetEditor:
                def __init__(self, source):
                    self.expected = list(source.members)
                    self.index = 0
                    self.target = FakeFamilyHandle()
                    source.calls.append("targetEditor")

                def _accept(self, source, target):
                    assert self.index < len(self.expected)
                    assert self.expected[self.index] == source.identity(), (
                        self.index, self.expected, source.identity()
                    )
                    self.target.members.append(target.identity())
                    self.index += 1

                def acceptMobject(self, source, target):
                    self._accept(source, target)

                def acceptFamily(self, source, target):
                    self._accept(source, target)

                def finish(self):
                    assert self.index == len(self.expected)
                    return self.target

            class FakeFamilyHandle:
                def __init__(self):
                    self.semanticSlot = allocate_id()
                    self.semanticGeneration = 0
                    self.members = []
                    self.calls = []

                def identity(self):
                    return ("family", self.semanticSlot, self.semanticGeneration)

                @property
                def memberCount(self):
                    return len(self.members)

                def addMobject(self, member):
                    identity = member.identity()
                    if identity in self.members:
                        return False
                    self.members.append(identity)
                    return True

                def addFamily(self, member):
                    identity = member.identity()
                    if identity in self.members:
                        return False
                    self.members.append(identity)
                    return True

                def removeMobject(self, member):
                    identity = member.identity()
                    if identity not in self.members:
                        return False
                    self.members.remove(identity)
                    return True

                def removeFamily(self, member):
                    return self.removeMobject(member)

                def targetEditor(self):
                    return FakeFamilyTargetEditor(self)

            fake_js.noonCreateAuthoringMobjectHandle = FakeHandle
            fake_js.noonCreateAuthoringFamilyHandle = FakeFamilyHandle
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_rate_functions
            _manim_rate_functions.install()
            import _manim_phase_b  # noqa: F401
            import _manim_geometry  # installs constructor-free custom Group copy
            import _manim_semantic_handles as handles
            handles.install()
            import _manim_animate  # noqa: F401

            from noon import ORANGE, RIGHT, Square, VGroup

            first = Square()
            second = Square()
            nested = VGroup(second)
            group = VGroup(first, nested)
            source_family = group._semantic_family_handle
            nested_source_family = nested._semantic_family_handle
            first_source_handle = first._semantic_handle
            second_source_handle = second._semantic_handle
            source_first = first._current_raw().to_ir()
            source_second = second._current_raw().to_ir()

            builder = group.animate
            target = builder.target
            assert isinstance(target, VGroup)
            assert target is not group
            assert len(target.submobjects) == 2
            assert isinstance(target[1], VGroup)
            assert source_family.calls == ["targetEditor"], source_family.calls
            assert nested_source_family.calls == ["targetEditor"], nested_source_family.calls
            assert first_source_handle.calls.count("targetEditor") == 1
            assert second_source_handle.calls.count("targetEditor") == 1
            assert "cloneHandle" not in first_source_handle.calls
            assert "cloneHandle" not in second_source_handle.calls

            target_first = target[0]
            target_nested = target[1]
            target_second = target_nested[0]
            assert target._semantic_family_handle.members == [
                target_first._semantic_handle.identity(),
                target_nested._semantic_family_handle.identity(),
            ]
            assert target_nested._semantic_family_handle.members == [
                target_second._semantic_handle.identity()
            ]
            assert target._semantic_family_handle.members != source_family.members

            builder.shift(RIGHT).set_fill(ORANGE, opacity=0.5)
            assert first._current_raw().to_ir() == source_first
            assert second._current_raw().to_ir() == source_second
            assert target_first._current_raw().transform["translation"]["x"] == 1.0
            assert target_second._current_raw().transform["translation"]["x"] == 1.0
            assert target_first._current_raw().style["fill"]["alpha"] == 0.5
            assert target_second._current_raw().style["fill"]["alpha"] == 0.5

            class FakeCanonicalContext:
                def __init__(self):
                    self.calls = []

                def liveExecutionOwnership(self):
                    return "active"

                def liveTargetEditor(self, handle):
                    self.calls.append(("leaf", handle.identity()))
                    return handle.targetEditor()

                def beginLiveFamilyTarget(self, family):
                    self.calls.append(("begin-family", family.identity()))
                    return FakeFamilyTargetEditor(family)

                def finishLiveFamilyTarget(self, editor):
                    self.calls.append(("finish-family", editor.index))
                    return editor.finish()

            context = FakeCanonicalContext()
            scene = types.SimpleNamespace(
                _canonical_authoring_context=context,
                _legacy_geometry_materialized=False,
                _tracks=[],
            )
            first._scene = scene
            first._object = types.SimpleNamespace(id=1)
            second._scene = scene
            second._object = types.SimpleNamespace(id=2)
            live_target = group.animate.target
            assert isinstance(live_target, VGroup)
            assert [call[0] for call in context.calls] == [
                "leaf", "leaf", "begin-family", "finish-family",
                "begin-family", "finish-family",
            ], context.calls
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
            "shared Group.animate target subprocess failed:\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
