import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyMemberSelectionTests(unittest.TestCase):
    def test_indexed_and_explicit_group_aligners_stay_in_shared_semantics(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        env["PYTHONPATH"] = str(python_dir)
        source = textwrap.dedent(
            """
            import json
            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_semantic_handles as handles

            class Bounds:
                def __init__(self, store, identity):
                    self.store = store
                    self.identity = identity

            class Translation:
                def __init__(self, store, leaves, delta):
                    self.store = store
                    self.leaves = list(leaves)
                    self.delta = delta
                    self.next = 0
                def applyMobject(self, member):
                    assert member.identity == self.leaves[self.next]
                    member.shift(*self.delta)
                    self.next += 1
                def finish(self):
                    assert self.next == len(self.leaves)

            class Obj:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.alloc(self)
                    self.snapshot = json.loads(snapshot_json)
                    self.shifts = []
                def snapshotJson(self): return json.dumps(self.snapshot, separators=(\",\", \":\"))
                def replaceSnapshotJson(self, value): self.snapshot = json.loads(value)
                def cloneHandle(self): return Obj(self.store, self.snapshotJson())
                def targetEditor(self): return self.cloneHandle()
                def shift(self, x, y): self.shifts.append((float(x), float(y)))
                def layoutBoundsHandle(self):
                    self.store.bounds_calls.append((\"mobject\", self.identity))
                    return Bounds(self.store, self.identity)
                def setFillOpacity(self, value): pass
                def setStrokeOpacity(self, value): pass

            class Layout:
                def __init__(self, family):
                    self.family = family
                    self.leaves = []
                def includeMobject(self, member): self.leaves.append(member.identity)
                def boundsHandle(self): return Bounds(self.family.store, self.family.identity)
                def nextToBoundsWithAligner(self, source, target, *args):
                    self.family.store.selected_calls.append((\"bounds\", source.identity, target.identity, args))
                    return Translation(self.family.store, self.leaves, (3.0, 0.0))
                def nextToPointWithAligner(self, source, x, y, *args):
                    self.family.store.selected_calls.append((\"point\", source.identity, (x, y), args))
                    return Translation(self.family.store, self.leaves, (-2.0, 1.0))

            class MemberLayout:
                def __init__(self, family, index):
                    self.family = family
                    self.index = index
                    normalized = index if index >= 0 else len(family.members) + index
                    if normalized < 0 or normalized >= len(family.members):
                        raise IndexError(index)
                    self.expected = family.members[normalized]
                    self.accepted = None
                    family.store.member_selections.append((family.identity, index, self.expected))
                def includeMobject(self, member):
                    assert member.identity == self.expected
                    self.accepted = member.identity
                def includeFamily(self, layout):
                    assert layout.family.identity == self.expected
                    self.accepted = layout.family.identity
                def boundsHandle(self):
                    assert self.accepted == self.expected
                    return Bounds(self.family.store, self.expected)

            class Family:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.alloc(self)
                    self.members = []
                def addMobject(self, member):
                    if member.identity not in self.members: self.members.append(member.identity)
                    return True
                def addFamily(self, member):
                    if member.identity not in self.members: self.members.append(member.identity)
                    return True
                def removeMobject(self, member): return False
                def removeFamily(self, member): return False
                @property
                def memberCount(self): return len(self.members)
                def layoutSession(self): return Layout(self)
                def memberLayoutSession(self, index): return MemberLayout(self, int(index))

            class Store:
                def __init__(self):
                    self.next = 0; self.entities = {}; self.member_selections=[]; self.bounds_calls=[]; self.selected_calls=[]
                def alloc(self, entity):
                    value=self.next; self.next+=1; self.entities[value]=entity; return value
                def createMobject(self, snapshot): return Obj(self, snapshot)
                def createFamily(self): return Family(self)

            store = Store()
            handles._create_handle = store.createMobject
            handles._create_family_handle = store.createFamily
            handles.install()
            handles._ORIGINAL_GROUP_NEXT_TO = lambda *a, **k: (_ for _ in ()).throw(AssertionError(\"fallback used\"))

            from noon import Circle, RIGHT, Square, VGroup
            first = Circle(radius=0.2)
            second = Square(side_length=0.4)
            source_nested = VGroup(second)
            source = VGroup(first, source_nested)

            target_first = Circle(radius=0.3)
            target_second = Square(side_length=0.5)
            target_nested = VGroup(target_second)
            target = VGroup(target_first, target_nested)

            source.next_to(target, RIGHT, index_of_submobject_to_align=-1)
            assert store.member_selections[-2:] == [
                (source._semantic_family_handle.identity, -1, source_nested._semantic_family_handle.identity),
                (target._semantic_family_handle.identity, -1, target_nested._semantic_family_handle.identity),
            ]
            assert store.selected_calls[-1][0] == \"bounds\"
            assert second._semantic_handle.shifts[-1] == (3.0, 0.0)

            external = Circle(radius=0.1)
            source.next_to(
                target,
                RIGHT,
                submobject_to_align=external,
                index_of_submobject_to_align=-1,
            )
            assert store.bounds_calls[-1] == ("mobject", external._semantic_handle.identity)
            assert store.member_selections[-1] == (
                target._semantic_family_handle.identity,
                -1,
                target_nested._semantic_family_handle.identity,
            )
            assert store.selected_calls[-1][0] == "bounds"
            assert store.selected_calls[-1][1] == external._semantic_handle.identity
            assert store.selected_calls[-1][2] == target_nested._semantic_family_handle.identity

            source.next_to((5.0, 2.0), RIGHT, submobject_to_align=external)
            assert store.bounds_calls[-1] == (\"mobject\", external._semantic_handle.identity)
            assert store.selected_calls[-1][0] == \"point\"
            assert first._semantic_handle.shifts[-1] == (-2.0, 1.0)
            assert second._semantic_handle.shifts[-1] == (-2.0, 1.0)
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source], cwd=python_dir, env=env,
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(completed.returncode, 0, msg=f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}")

if __name__ == "__main__":
    unittest.main()
