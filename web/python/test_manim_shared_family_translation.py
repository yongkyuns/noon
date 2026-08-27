import json
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyTranslationTests(unittest.TestCase):
    def test_group_translation_uses_shared_family_session(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )

        source = textwrap.dedent(
            """
            import json

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles


            class FakeObjectHandle:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.allocate()
                    self.snapshot = json.loads(snapshot_json)
                    self.shift_calls = []

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    return FakeObjectHandle(self.store, self.snapshotJson())

                def targetEditor(self):
                    return self.cloneHandle()

                def shift(self, x, y):
                    self.shift_calls.append((float(x), float(y)))
                    translation = self.snapshot[\"transform\"][\"translation\"]
                    translation[\"x\"] += float(x)
                    translation[\"y\"] += float(y)

                def setFillOpacity(self, opacity):
                    fill = self.snapshot[\"style\"][\"fill\"]
                    if fill is not None:
                        fill[\"alpha\"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot[\"style\"][\"stroke\"]
                    if stroke is not None:
                        stroke[\"alpha\"] = float(opacity)


            class FakeTranslation:
                def __init__(self, store, members, dx, dy):
                    self.store = store
                    self.expected = [member.identity for member in members]
                    self.next_index = 0
                    self.dx = float(dx)
                    self.dy = float(dy)

                def applyMobject(self, member):
                    assert member.identity == self.expected[self.next_index]
                    self.store.applied.append(member.identity)
                    member.shift(self.dx, self.dy)
                    self.next_index += 1

                def finish(self):
                    assert self.next_index == len(self.expected)
                    self.store.finishes += 1


            class FakeLayoutSession:
                def __init__(self, store):
                    self.store = store
                    self.members = []
                    store.layout_sessions += 1

                def includeMobject(self, member):
                    self.members.append(member)

                def shiftBy(self, dx, dy):
                    self.store.shift_by.append((float(dx), float(dy)))
                    return FakeTranslation(self.store, self.members, dx, dy)

                def moveToPoint(self, x, y, edge_x, edge_y, mask_x, mask_y):
                    self.store.move_to_point.append(
                        (float(x), float(y), float(edge_x), float(edge_y), float(mask_x), float(mask_y))
                    )
                    # The fake family has center (1, 1) for aligned_edge == ORIGIN.
                    return FakeTranslation(
                        self.store,
                        self.members,
                        (float(x) - 1.0) * float(mask_x),
                        (float(y) - 1.0) * float(mask_y),
                    )

                def criticalX(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive shared family move_to delta")

                def criticalY(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive shared family move_to delta")


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate()
                    self.members = []

                def layoutSession(self):
                    return FakeLayoutSession(self.store)

                @property
                def memberCount(self):
                    return len(self.members)

                def addMobject(self, member):
                    if member.identity in self.members:
                        return False
                    self.members.append(member.identity)
                    return True

                def addFamily(self, member):
                    if member.identity in self.members:
                        return False
                    self.members.append(member.identity)
                    return True

                def removeMobject(self, member):
                    if member.identity not in self.members:
                        return False
                    self.members.remove(member.identity)
                    return True

                def removeFamily(self, member):
                    return self.removeMobject(member)


            class FakeStore:
                def __init__(self):
                    self.next_identity = 0
                    self.layout_sessions = 0
                    self.shift_by = []
                    self.move_to_point = []
                    self.applied = []
                    self.finishes = 0

                def allocate(self):
                    value = self.next_identity
                    self.next_identity += 1
                    return value

                def createMobject(self, snapshot_json):
                    return FakeObjectHandle(self, snapshot_json)

                def createFamily(self):
                    return FakeFamilyHandle(self)


            store = FakeStore()
            handles._create_handle = store.createMobject
            handles._create_family_handle = store.createFamily
            handles.install()

            from noon import Circle, RIGHT, Square, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            family = VGroup(first, second)
            ids = [first._semantic_handle.identity, second._semantic_handle.identity]

            family.shift(RIGHT)
            assert store.shift_by == [(1.0, 0.0)]
            assert store.applied == ids
            assert first._semantic_handle.shift_calls == [(1.0, 0.0)]
            assert second._semantic_handle.shift_calls == [(1.0, 0.0)]

            before = len(store.applied)
            family.move_to((5.0, 4.0, 0.0))
            assert store.move_to_point == [(5.0, 4.0, 0.0, 0.0, 1.0, 1.0)]
            assert store.applied[before:] == ids
            assert first._semantic_handle.shift_calls[-1] == (4.0, 3.0)
            assert second._semantic_handle.shift_calls[-1] == (4.0, 3.0)
            assert store.finishes == 2
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
            msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
