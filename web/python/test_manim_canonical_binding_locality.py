import sys
import types
import unittest


class _FakeJs(types.ModuleType):
    def __getattr__(self, _name: str):
        return lambda *_args, **_kwargs: None


sys.modules.setdefault("js", _FakeJs("js"))

# This focused binding test needs only the canonical adapter's type checks, not
# retained-text scheduling. Stubbing those unrelated modules keeps it host-runnable.
_fake_typst = types.ModuleType("_manim_typst")
_fake_typst._RetainedTextMobject = type("_RetainedTextMobject", (), {})
sys.modules.setdefault("_manim_typst", _fake_typst)
sys.modules.setdefault("_manim_retained_state", types.ModuleType("_manim_retained_state"))

import noon as base
import _manim_canonical_scene as canonical


class _NoValuesDict(dict):
    def values(self):  # pragma: no cover - failure path proves the local reservation path
        raise AssertionError("typed binding must not scan every existing object key")


class _Handle:
    pass


class _Context:
    def __init__(self, *, fail_bind: bool = False, fail_live_add: bool = False) -> None:
        self.fail_bind = fail_bind
        self.fail_live_add = fail_live_add
        self.bound: list[tuple[str, object]] = []
        self.live_added: list[tuple[str, object]] = []
        self.started: list[float] = []

    def bindMobject(self, object_id: str, handle: object) -> None:
        if self.fail_bind:
            raise RuntimeError("shared Rust bind rejected")
        self.bound.append((object_id, handle))

    def beginLiveExecution(self, duration: float) -> None:
        self.started.append(duration)

    def liveAdd(self, object_id: str, handle: object) -> None:
        if self.fail_live_add:
            raise RuntimeError("shared Rust live add rejected")
        self.live_added.append((object_id, handle))


class CanonicalBindingLocalityTests(unittest.TestCase):
    @staticmethod
    def _typed_circle():
        circle = base.Circle(1.0)
        circle._semantic_handle = _Handle()
        return circle

    @staticmethod
    def _scene(context: _Context):
        scene = base.Scene()
        scene._canonical_authoring_context = context
        return scene

    def test_typed_binding_reserves_the_next_wrapper_identity_without_scanning_keys(self) -> None:
        context = _Context()
        scene = self._scene(context)
        scene._object_keys = _NoValuesDict()
        circle = self._typed_circle()

        obj = canonical._bind_mobject(circle, scene, key="anchor")

        expected_id = 0
        self.assertEqual(obj.id, expected_id)
        self.assertEqual(circle.id, expected_id)
        self.assertEqual(context.bound, [(str(expected_id), circle._semantic_handle)])
        self.assertEqual(scene._object_keys, {expected_id: "anchor"})
        self.assertEqual(scene._object_key_ids, {"anchor": expected_id})
        self.assertEqual(scene._object_positions, {expected_id: 0})
        self.assertEqual(scene._objects, [{"id": expected_id}])

    def test_failed_typed_bind_has_no_visible_python_bookkeeping_change(self) -> None:
        context = _Context(fail_bind=True)
        scene = self._scene(context)
        circle = self._typed_circle()
        before = (
            list(scene._objects),
            dict(scene._object_keys),
            dict(scene._object_key_ids),
            dict(scene._object_positions),
            scene._next_object_id,
            scene._next_painter_order,
        )
        scene._authoring_checkpoint = lambda: self.fail("typed bind must not checkpoint")

        with self.assertRaisesRegex(RuntimeError, "shared Rust bind rejected"):
            canonical._bind_mobject(circle, scene, key="failing")

        self.assertIsNone(circle._scene)
        self.assertEqual(context.bound, [])
        self.assertEqual(
            (
                scene._objects,
                scene._object_keys,
                scene._object_key_ids,
                scene._object_positions,
                scene._next_object_id,
                scene._next_painter_order,
            ),
            before,
        )

    def test_failed_bind_does_not_consume_the_next_wrapper_identity(self) -> None:
        context = _Context(fail_bind=True)
        scene = self._scene(context)
        rejected = self._typed_circle()

        with self.assertRaisesRegex(RuntimeError, "shared Rust bind rejected"):
            canonical._bind_mobject(rejected, scene)

        context.fail_bind = False
        accepted = self._typed_circle()
        obj = canonical._bind_mobject(accepted, scene)
        self.assertEqual(obj.id, 0)
        self.assertEqual(scene._next_object_id, 1)

    def test_duplicate_explicit_key_is_rejected_before_shared_bind_without_visible_changes(self) -> None:
        context = _Context()
        scene = self._scene(context)
        first = self._typed_circle()
        second = self._typed_circle()
        canonical._bind_mobject(first, scene, key="duplicate")
        before = (
            list(scene._objects),
            dict(scene._object_keys),
            dict(scene._object_key_ids),
            dict(scene._object_positions),
            scene._next_object_id,
            scene._next_painter_order,
        )

        with self.assertRaisesRegex(ValueError, "duplicate object key: duplicate"):
            canonical._bind_mobject(second, scene, key="duplicate")

        self.assertIsNone(second._scene)
        self.assertEqual(context.bound, [("0", first._semantic_handle)])
        self.assertEqual(
            (
                scene._objects,
                scene._object_keys,
                scene._object_key_ids,
                scene._object_positions,
                scene._next_object_id,
                scene._next_painter_order,
            ),
            before,
        )

    def test_live_add_reuses_the_shared_binding_commit(self) -> None:
        context = _Context()
        scene = self._scene(context)
        live = canonical.LiveExecution(scene)
        detached = self._typed_circle()

        live.add(detached)

        expected_id = 0
        self.assertEqual(detached.id, expected_id)
        self.assertEqual(context.live_added, [(str(expected_id), detached._semantic_handle)])
        self.assertEqual(scene._object_keys, {expected_id: f"@object:{expected_id}"})
        self.assertEqual(scene._object_key_ids, {f"@object:{expected_id}": expected_id})

    def test_failed_live_add_does_not_checkpoint_or_restore_python_or_context_state(self) -> None:
        context = _Context(fail_live_add=True)
        scene = self._scene(context)
        live = canonical.LiveExecution(scene)
        detached = self._typed_circle()
        before = (
            list(scene._objects),
            dict(scene._object_keys),
            dict(scene._object_key_ids),
            dict(scene._object_positions),
            scene._next_object_id,
            scene._next_painter_order,
        )
        scene._authoring_checkpoint = lambda: self.fail("live add must not checkpoint")
        context.restore = lambda *_args: self.fail("live add must not restore the context")

        with self.assertRaisesRegex(RuntimeError, "shared Rust live add rejected"):
            live.add(detached)

        self.assertIsNone(detached._scene)
        self.assertEqual(context.live_added, [])
        self.assertEqual(
            (
                scene._objects,
                scene._object_keys,
                scene._object_key_ids,
                scene._object_positions,
                scene._next_object_id,
                scene._next_painter_order,
            ),
            before,
        )


if __name__ == "__main__":
    unittest.main()
