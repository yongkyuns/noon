import json
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyIdentityTests(unittest.TestCase):
    def test_group_wrapper_mirrors_shared_family_membership(self) -> None:
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

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles


            class FakeObjectHandle:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.allocate()
                    self.snapshot = json.loads(snapshot_json)

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

                def replaceSnapshotJson(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def cloneHandle(self):
                    clone = FakeObjectHandle(self.store, self.snapshotJson())
                    return clone

                def targetEditor(self):
                    return self.cloneHandle()

                def setFillOpacity(self, opacity):
                    fill = self.snapshot[\"style\"][\"fill\"]
                    if fill is not None:
                        fill[\"alpha\"] = float(opacity)

                def setStrokeOpacity(self, opacity):
                    stroke = self.snapshot[\"style\"][\"stroke\"]
                    if stroke is not None:
                        stroke[\"alpha\"] = float(opacity)


            class FakeLayoutSession:
                def __init__(self, store):
                    self.store = store
                    self.members = []
                    store.layout_sessions += 1

                def includeMobject(self, member):
                    self.members.append(member.identity)

                def _complete(self):
                    assert len(self.members) == 2

                def criticalX(self, direction_x, direction_y):
                    del direction_y
                    self._complete()
                    return -3.0 if direction_x < 0 else (5.0 if direction_x > 0 else 1.0)

                def criticalY(self, direction_x, direction_y):
                    del direction_x
                    self._complete()
                    return -2.0 if direction_y < 0 else (4.0 if direction_y > 0 else 1.0)


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

                def _add(self, member):
                    key = member.identity
                    if key in self.members:
                        return False
                    self.members.append(key)
                    return True

                def addMobject(self, member):
                    assert member.store is self.store
                    return self._add(member)

                def addFamily(self, member):
                    assert member.store is self.store
                    return self._add(member)

                def _remove(self, member):
                    key = member.identity
                    if key not in self.members:
                        return False
                    self.members.remove(key)
                    return True

                def removeMobject(self, member):
                    return self._remove(member)

                def removeFamily(self, member):
                    return self._remove(member)


            class FakeStore:
                def __init__(self):
                    self.next_identity = 0
                    self.layout_sessions = 0

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

            from noon import Circle, Square, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            family = VGroup(first, second)
            assert len(family) == 2
            assert family._semantic_family_handle.memberCount == 2

            # The shared family graph owns duplicate suppression. Python mirrors the
            # returned decision rather than independently appending another wrapper.
            family.add(first)
            assert len(family) == 2
            assert family._semantic_family_handle.memberCount == 2

            family.remove(first)
            assert list(family) == [second]
            assert family._semantic_family_handle.memberCount == 1
            family.remove(first)
            assert list(family) == [second]
            assert family._semantic_family_handle.memberCount == 1

            nested = VGroup(first)
            outer = VGroup(nested, second)
            assert outer._semantic_family_handle.memberCount == 2
            assert nested._semantic_family_handle.memberCount == 1

            center = outer.get_center()
            assert center.x == 1.0 and center.y == 1.0
            assert outer.width == 8.0
            assert outer.height == 6.0
            assert store.layout_sessions == 3

            clone = outer.copy()
            assert clone is not outer
            assert clone._semantic_family_handle is not outer._semantic_family_handle
            assert clone._semantic_family_handle.memberCount == 2
            assert clone[0] is not nested
            assert clone[1] is not second
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
