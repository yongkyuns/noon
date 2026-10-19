import json
import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimSharedFamilyIdentityTests(unittest.TestCase):
    def test_group_add_to_back_uses_one_shared_family_order_operation(self):
        from types import SimpleNamespace
        from unittest.mock import Mock, patch
        import _manim_compat as compat
        import _manim_semantic_handles as handles
        from _typed_geometry_test_support import identity_only_wrapper

        root = identity_only_wrapper(compat.Group)
        first = identity_only_wrapper(compat.Square)
        second = identity_only_wrapper(compat.Circle)
        root._semantic_family_handle = SimpleNamespace(
            prependMembers=Mock(),
        )
        root._semantic_member_wrappers = {}
        first._semantic_handle = SimpleNamespace(semanticSlot=1, semanticGeneration=2)
        second._semantic_handle = SimpleNamespace(semanticSlot=3, semanticGeneration=4)
        batch = object()

        with (
            patch.object(handles, "_live_constructor_context", return_value=None),
            patch.object(handles, "_family_membership_batch", return_value=batch) as make_batch,
        ):
            self.assertIs(handles._group_add_to_back(root, first, second, first), root)

        make_batch.assert_called_once_with(None, "add", (first, second, first))
        root._semantic_family_handle.prependMembers.assert_called_once_with(batch)
        self.assertIs(root._semantic_member_wrappers["1:2"], first)
        self.assertIs(root._semantic_member_wrappers["3:4"], second)

    def test_family_updaters_flatten_member_callbacks_in_shared_order(self):
        from unittest.mock import Mock, call
        import _manim_compat as compat
        from _typed_geometry_test_support import identity_only_wrapper

        root = identity_only_wrapper(compat.Group)
        child = identity_only_wrapper(compat.Circle)
        root_updaters = [Mock(), Mock()]
        child_updaters = [Mock()]
        root.get_family = Mock(return_value=[root, child])
        root.get_updaters = Mock(return_value=root_updaters)
        child.get_updaters = Mock(return_value=child_updaters)

        self.assertEqual(root.get_family_updaters(), root_updaters + child_updaters)
        root.get_family.assert_called_once_with()
        self.assertEqual(root.get_updaters.call_args_list, [call()])
        self.assertEqual(child.get_updaters.call_args_list, [call()])

    def test_apply_to_family_invokes_callbacks_in_shared_pointful_order(self):
        from unittest.mock import Mock, call
        import _manim_compat as compat
        from _typed_geometry_test_support import identity_only_wrapper

        root = identity_only_wrapper(compat.Group)
        members = [object(), object()]
        root.family_members_with_points = Mock(return_value=members)
        callback = Mock()

        self.assertIs(root.apply_to_family(callback), root)
        root.family_members_with_points.assert_called_once_with()
        self.assertEqual(callback.call_args_list, [call(members[0]), call(members[1])])

    def test_family_query_uses_shared_order_and_existing_wrapper_identity(self):
        from unittest.mock import Mock, patch
        import _manim_compat as compat
        import _manim_semantic_handles as handles
        from _typed_geometry_test_support import identity_only_wrapper

        a = identity_only_wrapper(compat.Square)
        b = identity_only_wrapper(compat.Circle)
        a._semantic_handle = Mock()
        a._semantic_handle.familyKeys.return_value = ["a"]
        nested = identity_only_wrapper(compat.VGroup)
        root = identity_only_wrapper(compat.Group)
        nested._semantic_family_handle = Mock()
        root._semantic_family_handle = Mock()
        root._semantic_family_handle.familyKeys.return_value = ["root", "nested", "b", "a"]
        wrappers = {id(a): "a", id(b): "b", id(nested): "nested", id(root): "root"}
        children = {id(root): [nested, a], id(nested): [a, b]}
        with patch.object(handles, "_family_wrapper_key", side_effect=lambda value: wrappers[id(value)]), \
             patch.object(handles, "_group_members", side_effect=lambda value: children[id(value)]):
            self.assertEqual(root.get_family(), [root, nested, b, a])
            self.assertEqual(root.get_family(recurse=False), [root, nested, b, a])
            self.assertEqual(a.get_family(), [a])
            root._semantic_family_handle.familyKeys.return_value = ["root", "unknown"]
            with self.assertRaisesRegex(RuntimeError, "no Python wrapper"):
                root.get_family()
        self.assertEqual(root._semantic_family_handle.familyKeys.call_count, 3)

    def test_staged_family_query_is_explicitly_rejected_before_host_read(self):
        from unittest.mock import Mock
        import _manim_compat as compat
        import _manim_updaters as updaters
        from _typed_geometry_test_support import identity_only_wrapper
        root = identity_only_wrapper(compat.Group)
        root._semantic_family_handle = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(Mock())
        try:
            with self.assertRaisesRegex(NotImplementedError, "staged callback membership"):
                root.get_family()
            callback = Mock()
            with self.assertRaisesRegex(NotImplementedError, "staged callback membership"):
                root.apply_to_family(callback)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        root._semantic_family_handle.familyKeys.assert_not_called()
        callback.assert_not_called()

    def test_live_observations_and_coordinates_delegate_without_python_bounds_math(self):
        from types import SimpleNamespace
        from unittest.mock import Mock, patch
        import _manim_compat as compat
        import _manim_semantic_handles as handles
        from _typed_geometry_test_support import identity_only_wrapper

        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = object()
        layout = SimpleNamespace(centerX=7, centerY=-2, width=99, height=13,
                                 criticalX=Mock(return_value=42),
                                 criticalY=Mock(return_value=-17))
        context = SimpleNamespace(queryFamilyLayout=Mock(return_value=layout))
        with patch.object(handles, "_group_live_layout_context", return_value=context):
            self.assertEqual(family.get_center(), compat._base.Vec2(7, -2))
            self.assertEqual((family.width, family.height), (99, 13))
            self.assertEqual(family.get_critical_point(compat._base.RIGHT),
                             compat._base.Vec2(42, -17))
        layout.criticalX.assert_called_once_with(1.0, 0.0)
        layout.criticalY.assert_called_once_with(1.0, 0.0)
        family.move_to = Mock(return_value=family)
        family.get_center = Mock(side_effect=AssertionError("Python coordinate delta"))
        self.assertIs(family.set_x(5), family)
        self.assertIs(family.set_y(-3), family)
        self.assertEqual(family.move_to.call_args_list[0].args, (compat._base.Vec2(5, 0),))
        self.assertEqual(family.move_to.call_args_list[0].kwargs, {"aligned_edge": compat._base.ORIGIN, "coor_mask": (1.0, 0.0, 0.0)})
        self.assertEqual(family.move_to.call_args_list[1].args, (compat._base.Vec2(0, -3),))
        self.assertEqual(family.move_to.call_args_list[1].kwargs, {"aligned_edge": compat._base.ORIGIN, "coor_mask": (0.0, 1.0, 0.0)})

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

            import _manim_semantic_handles as handles


            class FakeObjectHandle:
                def __init__(self, store, snapshot_json):
                    self.store = store
                    self.identity = store.allocate()
                    self.snapshot = json.loads(snapshot_json)

                def snapshotJson(self):
                    return json.dumps(self.snapshot, separators=(\",\", \":\"))

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


            class FakeLayoutObservation:
                centerX = 1.0
                centerY = 1.0
                width = 8.0
                height = 6.0

                def __init__(self, store):
                    self.store = store
                    store.layout_queries += 1

                def criticalX(self, direction_x, direction_y):
                    del direction_y
                    return -3.0 if direction_x < 0 else (5.0 if direction_x > 0 else 1.0)

                def criticalY(self, direction_x, direction_y):
                    del direction_x
                    return -2.0 if direction_y < 0 else (4.0 if direction_y > 0 else 1.0)


            class FakeFamilyHandle:
                def __init__(self, store):
                    self.store = store
                    self.identity = store.allocate()
                    self.members = []

                def layout(self):
                    return FakeLayoutObservation(self.store)

                @property
                def memberCount(self):
                    return len(self.members)

            class FakeStore:
                def __init__(self):
                    self.next_identity = 0
                    self.layout_queries = 0

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

            family._semantic_family_handle.reject_membership = True
            try:
                family.add(first, second)
                raise AssertionError("expected shared rejection")
            except RuntimeError:
                pass
            assert list(family) == [second]
            family._semantic_family_handle.reject_membership = False

            nested = VGroup(first)
            outer = VGroup(nested, second)
            assert outer._semantic_family_handle.memberCount == 2
            assert nested._semantic_family_handle.memberCount == 1

            center = outer.get_center()
            assert center.x == 1.0 and center.y == 1.0
            assert outer.width == 8.0
            assert outer.height == 6.0
            assert store.layout_queries == 3

            clone = outer.copy()
            assert clone is not outer
            assert clone._semantic_family_handle is not outer._semantic_family_handle
            assert clone._semantic_family_handle.memberCount == 2
            assert clone[0] is not nested
            assert clone[1] is not second

            aliased = VGroup(first, nested)
            copied_alias = aliased.copy()
            assert copied_alias[0] is copied_alias[1][0]

            class Named(VGroup):
                constructions = 0
                def __init__(self, child, label):
                    Named.constructions += 1
                    super().__init__(child)
                    self.child = child
                    self.label = label

            named = Named(first, "label")
            named_copy = named.copy()
            assert type(named_copy) is Named
            assert Named.constructions == 1
            assert named_copy.child is named_copy[0]
            assert named_copy.child is not first
            assert named_copy.label == "label"

            first.save_state()
            saved_copy = aliased.copy()
            assert saved_copy[0].saved_state is not first.saved_state
            named.related = {"state": first.saved_state, "self": named}
            related_copy = named.copy()
            assert related_copy.related["state"] is related_copy[0].saved_state
            assert related_copy.related["self"] is related_copy

            class Uncopyable:
                def __deepcopy__(self, memo):
                    raise ValueError("metadata copy refused")
            named.bad = Uncopyable()
            before = store.next_identity
            try:
                named.copy()
                raise AssertionError("expected metadata failure")
            except ValueError:
                pass
            assert store.next_identity == before
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
