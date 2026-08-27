import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyArrangeTests(unittest.TestCase):
    def test_group_arrange_dispatches_order_and_spacing_to_shared_family_plan(self) -> None:
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
                    self.identity = store.allocate(self)
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
                    pass

                def setStrokeOpacity(self, opacity):
                    pass


            class FakeLayoutSession:
                def __init__(self, family):
                    self.family = family
                    self.members = []

                def includeMobject(self, member):
                    self.members.append(member)


            class FakeTranslation:
                def __init__(self, store, expected, delta):
                    self.store = store
                    self.expected = list(expected)
                    self.delta = delta
                    self.next_index = 0

                def applyMobject(self, member):
                    assert member.identity == self.expected[self.next_index]
                    member.shift(*self.delta)
                    self.store.applied.append(member.identity)
                    self.next_index += 1

                def finish(self):
                    assert self.next_index == len(self.expected)


            class FakeArrange:
                def __init__(self, family, direction_x, direction_y, buff, center):
                    self.family = family
                    self.store = family.store
                    self.expected = list(family.members)
                    self.next_include = 0
                    self.next_translation = 0
                    self.store.arrange_calls.append(
                        (family.identity, float(direction_x), float(direction_y), float(buff), bool(center))
                    )

                def _accept(self, identity, kind):
                    assert identity == self.expected[self.next_include]
                    self.store.arrange_includes.append((kind, identity))
                    self.next_include += 1

                def includeMobject(self, member):
                    self._accept(member.identity, \"mobject\")

                def includeFamily(self, layout):
                    self._accept(layout.family.identity, \"family\")

                def nextTranslation(self):
                    assert self.next_include == len(self.expected)
                    identity = self.expected[self.next_translation]
                    expected_leaves = [member.identity for member in self.store.leaves(identity)]
                    deltas = [(-1.0, 0.0), (2.0, 0.0), (4.0, 0.0)]
                    translation = FakeTranslation(
                        self.store,
                        expected_leaves,
                        deltas[self.next_translation],
                    )
                    self.next_translation += 1
                    return translation

                def finish(self):
                    assert self.next_translation == len(self.expected)
                    self.store.arrange_finishes += 1


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate(self)
                    self.members = []

                def layoutSession(self):
                    return FakeLayoutSession(self)

                def arrangeSession(self, direction_x, direction_y, buff, center):
                    return FakeArrange(self, direction_x, direction_y, buff, center)

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
                    self.entities = {}
                    self.arrange_calls = []
                    self.arrange_includes = []
                    self.arrange_finishes = 0
                    self.applied = []

                def allocate(self, entity):
                    value = self.next_identity
                    self.next_identity += 1
                    self.entities[value] = entity
                    return value

                def leaves(self, identity):
                    entity = self.entities[identity]
                    if isinstance(entity, FakeObjectHandle):
                        return [entity]
                    result = []
                    for child in entity.members:
                        result.extend(self.leaves(child))
                    return result

                def createMobject(self, snapshot_json):
                    return FakeObjectHandle(self, snapshot_json)

                def createFamily(self):
                    return FakeFamilyHandle(self)


            store = FakeStore()
            handles._create_handle = store.createMobject
            handles._create_family_handle = store.createFamily
            handles.install()

            def forbidden_fallback(*args, **kwargs):
                raise AssertionError(\"Python arrange fallback must not run on shared path\")

            handles._ORIGINAL_GROUP_ARRANGE = forbidden_fallback

            from noon import Circle, RIGHT, Square, VGroup

            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            nested = VGroup(second)
            family = VGroup(first, nested)

            family.arrange(direction=2.0 * RIGHT, buff=0.25, center=True)

            assert store.arrange_calls == [
                (family._semantic_family_handle.identity, 2.0, 0.0, 0.25, True)
            ]
            assert store.arrange_includes == [
                (\"mobject\", first._semantic_handle.identity),
                (\"family\", nested._semantic_family_handle.identity),
            ]
            assert store.applied == [
                first._semantic_handle.identity,
                second._semantic_handle.identity,
            ]
            assert first._semantic_handle.shift_calls[-1] == (-1.0, 0.0)
            assert second._semantic_handle.shift_calls[-1] == (2.0, 0.0)
            assert store.arrange_finishes == 1
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
