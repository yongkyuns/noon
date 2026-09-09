import json
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyRelativePlacementTests(unittest.TestCase):
    def test_group_next_to_and_align_to_dispatch_to_shared_family_session(self) -> None:
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
            import _manim_semantic_handles as handles


            class FakeObjectHandle:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.allocate()
                    store.entities[self.identity] = self
                    self.snapshot = json.loads(snapshot_json)
                    self.shift_calls = []

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

                def cloneHandle(self):
                    return FakeObjectHandle(self.store, self.snapshotJson())

                def targetEditor(self):
                    return self.cloneHandle()

                def layoutAnchor(self, index=None):
                    assert index is None
                    return self

                def shift(self, x, y):
                    self.shift_calls.append((float(x), float(y)))
                    translation = self.snapshot[\"transform\"][\"translation\"]
                    translation[\"x\"] += float(x)
                    translation[\"y\"] += float(y)

                def setFillOpacity(self, opacity):
                    pass

                def setStrokeOpacity(self, opacity):
                    pass


            class FakeLayoutObservation:
                def __init__(self, store, members):
                    self.store = store
                    self.members = list(members)

                def _apply(self, dx, dy):
                    for member in self.members:
                        member.shift(float(dx), float(dy))
                        self.store.applied.append(member.identity)
                    self.store.finishes += 1


                def nextToPoint(self, px, py, aligner, *args):
                    assert aligner.members == self.members
                    self.store.next_to_point.append(tuple(float(value) for value in (px, py, *args)))
                    return self._apply(2.0, -1.0)

                def nextTo(self, target, aligner, *args):
                    assert aligner.members == self.members
                    self.store.next_to_family.append(tuple(float(value) for value in args))
                    assert len(target.members) == 1
                    return self._apply(3.0, 0.5)

                def alignToPoint(self, *args):
                    self.store.align_to_point.append(tuple(float(value) for value in args))
                    return self._apply(0.0, 4.0)

                def alignToFamily(self, target, *args):
                    self.store.align_to_family.append(tuple(float(value) for value in args))
                    assert len(target.members) == 1
                    return self._apply(-2.0, 0.0)

                def criticalX(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive family relative-placement deltas")

                def criticalY(self, direction_x, direction_y):
                    raise AssertionError("Python must not derive family relative-placement deltas")


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate()
                    store.entities[self.identity] = self
                    self.members = []

                def layoutAnchor(self, index=None):
                    assert index is None
                    return self.layout()

                def layout(self):
                    return FakeLayoutObservation(self.store, [self.store.entities[key] for key in self.members])

                @property
                def memberCount(self):
                    return len(self.members)

            class FakeStore:
                def __init__(self):
                    self.next_identity = 0
                    self.entities = {}
                    self.next_to_point = []
                    self.next_to_family = []
                    self.align_to_point = []
                    self.align_to_family = []
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
            import _typed_geometry_test_support as _geometry_test
            _geometry_test.install_module_bridge(handles, store.createMobject)
            import _typed_family_test_support as _family_test
            _family_test.install_bridge(handles, store.createFamily, FakeFamilyHandle, FakeObjectHandle)
            handles.install()

            from noon import Circle, RIGHT, Square, UP, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            family = VGroup(first, second)
            ids = [first._semantic_handle.identity, second._semantic_handle.identity]

            family.next_to((5.0, 6.0, 0.0), 2.0 * RIGHT, buff=0.25)
            assert store.next_to_point == [(5.0, 6.0, 2.0, 0.0, 0.25, 0.0, 0.0, 1.0, 1.0)]
            assert store.applied == ids
            assert first._semantic_handle.shift_calls[-1] == (2.0, -1.0)
            assert second._semantic_handle.shift_calls[-1] == (2.0, -1.0)

            target_leaf = Circle(radius=0.1)
            target = VGroup(target_leaf)
            before = len(store.applied)
            family.next_to(target, RIGHT, buff=0.5)
            assert store.next_to_family == [(1.0, 0.0, 0.5, 0.0, 0.0, 1.0, 1.0)]
            assert store.applied[before:] == ids

            before = len(store.applied)
            family.align_to((9.0, 8.0, 0.0), UP)
            assert store.align_to_point == [(9.0, 8.0, 0.0, 1.0)]
            assert store.applied[before:] == ids
            assert first._semantic_handle.shift_calls[-1] == (0.0, 4.0)

            before = len(store.applied)
            family.align_to(target, RIGHT)
            assert store.align_to_family == [(1.0, 0.0)]
            assert store.applied[before:] == ids
            assert first._semantic_handle.shift_calls[-1] == (-2.0, 0.0)
            assert store.finishes == 4
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
