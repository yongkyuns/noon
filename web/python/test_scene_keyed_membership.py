"""Keyed Scene.add identity reservations against a mocked Rust membership boundary.

Foreground ordering is Rust-owned. These tests cover only Python wrapper identity,
key validation, speculative reservations, and the typed batch shape.
"""

import ast
import __future__
from dataclasses import dataclass
from pathlib import Path
import sys
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

import _noon_ir as ir


class Group:
    def __init__(self, slot, *members):
        self._semantic_family_handle = SimpleNamespace(
            semanticSlot=slot, semanticGeneration=0
        )
        self.submobjects = members


class Batch:
    def __init__(self, kind):
        self.kind = kind
        self.bindings = []
        self.members = []

    def reserveMobjectBinding(self, wrapper_id, handle):
        self.bindings.append((wrapper_id, handle.semanticSlot))

    def appendMobject(self, wrapper_id, handle):
        self.members.append((wrapper_id, handle.semanticSlot))

    def appendFamily(self, handle):
        self.members.append((None, handle.semanticSlot))


class Context:
    def __init__(self):
        self.edits = []
        self.reject = False

    def beginMembershipBatch(self, kind):
        return Batch(kind)

    def editMembership(self, batch):
        if self.reject:
            raise RuntimeError("injected Rust publication rejection")
        self.edits.append((batch.kind, tuple(batch.bindings), tuple(batch.members)))


class KeyedSceneMembershipTests(unittest.TestCase):
    def compile_definitions(self, filename, names, namespace):
        path = Path(__file__).with_name(filename)
        tree = ast.parse(path.read_text())
        selected = [node for node in tree.body if getattr(node, "name", None) in names]
        self.assertEqual({node.name for node in selected}, names)
        exec(
            compile(
                ast.Module(body=selected, type_ignores=[]),
                str(path),
                "exec",
                flags=__future__.annotations.compiler_flag,
            ),
            namespace,
        )

    def setUp(self):
        public_source = Path(__file__).with_name("noon.py")
        isolated = {"__name__": __name__, "__file__": str(public_source)}
        exec(compile(public_source.read_text(), str(public_source), "exec"), isolated)
        self.Mobject = isolated["Mobject"]
        self.Scene = isolated["Scene"]
        compat = ModuleType("_manim_compat")
        compat.Group = Group
        compat.Mobject = self.Mobject
        self.compile_definitions("_manim_compat.py", {"_leaf_mobjects"}, compat.__dict__)
        modules = patch.dict(sys.modules, {"_manim_compat": compat})
        modules.start()
        self.addCleanup(modules.stop)
        self.ns = {
            "__name__": __name__,
            "dataclass": dataclass,
            "_base": SimpleNamespace(Scene=self.Scene, Mobject=self.Mobject),
            "_compat": compat,
            "_ir": ir,
            "engine_call": lambda method, *args, operation=None: method(*args),
        }
        self.compile_definitions(
            "_manim_scene.py",
            {
                "_context",
                "_TypedBindingReservation",
                "_reserve_typed_binding",
                "_commit_typed_binding",
                "_record_mobject_binding",
                "_semantic_wrapper_key",
                "_membership_registry",
                "_register_membership_wrappers",
                "_membership_wrapper_leaves",
                "_membership_leaf_bindings",
                "_append_membership_value",
                "_sync_membership_wrapper_attachments",
                "_canonical_edit_membership",
            },
            self.ns,
        )
        isolated["_scene_operations"] = lambda: SimpleNamespace(**self.ns)
        self.scene = self.Scene()
        self.context = Context()
        self.scene._canonical_authoring_context = self.context
        self.wrappers = []

    def mobject(self, slot):
        value = object.__new__(self.Mobject)
        value._semantic_handle = SimpleNamespace(
            semanticSlot=slot, semanticGeneration=0
        )
        value._scene = None
        value._object = None
        self.wrappers.append(value)
        return value

    def snapshot(self):
        return (
            self.scene._next_object_id,
            dict(self.scene._object_keys),
            dict(self.scene._object_key_ids),
            dict(self.scene._binding_handles),
            dict(getattr(self.scene, "_canonical_membership_wrappers", {})),
            tuple(self.context.edits),
            tuple((value._scene, value._object) for value in self.wrappers),
        )

    def assert_rejected_without_changes(self, operation, message):
        before = self.snapshot()
        with self.assertRaisesRegex(ValueError, message):
            operation()
        self.assertEqual(self.snapshot(), before)

    def test_add_foreground_reserves_group_leaves_once_and_uses_foreground_batch(self):
        child, target = self.mobject(10), self.mobject(11)
        group = Group(100, child, target)
        self.scene.add_foreground_mobjects(group, target)
        kind, bindings, members = self.context.edits[-1]
        self.assertEqual(kind, "add_foreground")
        self.assertEqual([slot for _, slot in bindings], [10, 11])
        self.assertEqual(members, ((None, 100), (str(target.id), 11)))
        self.assertIs(child._scene, self.scene)
        self.assertIs(target._scene, self.scene)

    def test_keyed_readd_after_foreground_binding_stays_single_object(self):
        child, target = self.mobject(10), self.mobject(11)
        group = Group(100, child)
        self.scene.add_foreground_mobjects(group, target)
        stable_key = self.scene._object_keys[target.id]
        original_id = target.id
        original_keys = dict(self.scene._object_keys)
        before_edits = len(self.context.edits)
        self.assertIs(self.scene.add(target, key=stable_key), target)
        self.assertEqual(target.id, original_id)
        self.assertEqual(self.scene._object_keys, original_keys)
        self.assertEqual(len(self.context.edits), before_edits + 1)
        self.assertEqual(self.context.edits[-1][0], "add")
        self.assertEqual([slot for _, slot in self.context.edits[-1][2]], [11])

    def test_keyed_group_is_rejected_by_public_facade(self):
        child = self.mobject(11)
        group = Group(100, child)
        self.assert_rejected_without_changes(
            lambda: self.scene.add(group, key="not-a-leaf"),
            "one ordinary Mobject",
        )

    def test_existing_key_cannot_be_redirected_to_another_wrapper(self):
        first, target = self.mobject(10), self.mobject(11)
        self.scene.add(first, key="first")
        self.scene.add(target, key="target")
        self.scene.add_foreground_mobjects(first, target)
        self.assert_rejected_without_changes(
            lambda: self.scene.add(target, key="first"), "existing key"
        )

    def test_existing_key_cannot_be_silently_changed(self):
        target = self.mobject(10)
        self.scene.add(target, key="stable")
        self.assert_rejected_without_changes(
            lambda: self.scene.add(target, key="replacement"), "existing key"
        )

    def test_internal_multi_value_keyed_batch_is_rejected(self):
        first, second = self.mobject(10), self.mobject(11)
        self.assert_rejected_without_changes(
            lambda: self.ns["_canonical_edit_membership"](
                self.scene, "add", (first, second), key="ambiguous"
            ),
            "one ordinary Mobject",
        )

    def test_new_key_with_foreground_present_uses_only_the_new_object_batch(self):
        front, target = self.mobject(10), self.mobject(11)
        self.scene.add(front, key="front")
        self.scene.add_foreground_mobject(front)
        self.assertIs(self.scene.add(target, key="target"), target)
        self.assertEqual(self.scene._object_keys[front.id], "front")
        self.assertEqual(self.scene._object_keys[target.id], "target")
        self.assertIs(self.scene._binding_handles[target.id], target._semantic_handle)
        kind, bindings, members = self.context.edits[-1]
        self.assertEqual(kind, "add")
        self.assertEqual([slot for _, slot in bindings], [11])
        self.assertEqual([slot for _, slot in members], [11])

    def test_duplicate_new_key_is_rejected_without_publishing(self):
        front, target = self.mobject(10), self.mobject(11)
        self.scene.add(front, key="front")
        self.scene.add_foreground_mobject(front)
        self.assert_rejected_without_changes(
            lambda: self.scene.add(target, key="front"), "duplicate object key"
        )

    def test_rejected_rust_batch_does_not_commit_reserved_identities(self):
        front, target = self.mobject(10), self.mobject(11)
        self.scene.add_foreground_mobject(front)
        self.context.reject = True
        before = self.snapshot()
        with self.assertRaisesRegex(RuntimeError, "publication rejection"):
            self.scene.add(target, key="target")
        self.assertEqual(self.snapshot(), before)

    def test_matching_existing_key_keeps_identity(self):
        target = self.mobject(10)
        self.scene.add(target, key="stable")
        original = target._object
        self.assertIs(self.scene.add(target, key="stable"), target)
        self.assertIs(target._object, original)
        self.assertEqual(self.scene._object_keys, {original.id: "stable"})

    def test_existing_key_cannot_change_when_foreground_group_contains_target(self):
        target = self.mobject(10)
        self.scene.add(target, key="stable")
        group = Group(100, target)
        self.scene.add_foreground_mobjects(group, target)
        self.assert_rejected_without_changes(
            lambda: self.scene.add(target, key="replacement"), "existing key"
        )

    def test_matching_key_after_overlapping_foreground_uses_one_binding(self):
        first, target = self.mobject(10), self.mobject(11)
        self.scene.add(target, key="stable")
        group = Group(100, first, target)
        self.scene.add_foreground_mobjects(group, target)
        original = target._object
        before_keys = dict(self.scene._object_keys)
        self.assertIs(self.scene.add(target, key="stable"), target)
        self.assertIs(target._object, original)
        self.assertEqual(self.scene._object_keys, before_keys)
        self.assertEqual([slot for _, slot in self.context.edits[-1][1]], [11])
        self.assertEqual([slot for _, slot in self.context.edits[-1][2]], [11])


if __name__ == "__main__":
    unittest.main()
