import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyArrangeTests(unittest.TestCase):
    def test_group_arrange_uses_one_shared_call_and_preserves_live_dispatch(self) -> None:
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
                    self.identity = store.allocate(self)
                    self.snapshot = json.loads(snapshot_json)
                    self.shift_calls = []

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

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


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate(self)
                    self.members = []

                def arrangeOptions(self, direction_x, direction_y, buff, center, *alignment):
                    return (direction_x, direction_y, buff, center, *alignment)

                def arrange(self, options):
                    direction_x, direction_y, buff, center = options[:4]
                    self.store.arrange_calls.append(
                        (self.identity, direction_x, direction_y, buff, center)
                    )
                    if self.store.reject_arrange:
                        raise RuntimeError("invalid shared arrangement")

                @property
                def memberCount(self):
                    return len(self.members)

            class FakeStore:
                def __init__(self):
                    self.next_identity = 0
                    self.entities = {}
                    self.arrange_calls = []
                    self.reject_arrange = False

                def allocate(self, entity):
                    value = self.next_identity
                    self.next_identity += 1
                    self.entities[value] = entity
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
            assert first._semantic_handle.shift_calls == []
            assert second._semantic_handle.shift_calls == []

            store.reject_arrange = True
            try:
                family.arrange()
                raise AssertionError("shared rejection was swallowed")
            except ValueError as error:
                assert str(error) == "invalid shared arrangement"
            store.reject_arrange = False

            class FakeLiveContext:
                def __init__(self):
                    self.calls = []

                def liveArrangeFamily(self, family, options):
                    direction_x, direction_y, buff, center = options[:4]
                    self.calls.append(
                        (family.identity, float(direction_x), float(direction_y), float(buff), bool(center))
                    )

            live = FakeLiveContext()
            first._canonical_live_target_context = live
            second._canonical_live_target_context = live
            prior_direct_calls = list(store.arrange_calls)
            family.arrange(direction=RIGHT, buff=0.15, center=False)
            assert live.calls == [
                (family._semantic_family_handle.identity, 1.0, 0.0, 0.15, False)
            ]
            assert store.arrange_calls == prior_direct_calls
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
