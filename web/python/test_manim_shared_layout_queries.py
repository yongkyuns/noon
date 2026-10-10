"""Facade delegation; geometry correctness is tested against shared Rust bounds."""
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import noon
import _manim_compat

import _manim_semantic_handles as shared


class Observation:
    centerX, centerY, width, height = 7.0, -3.0, 10.0, 6.0

    def centerCoordinates(self):
        return (self.centerX, self.centerY)

    def criticalX(self, x, y):
        return 2.0 if x < 0 else 12.0 if x > 0 else self.centerX

    def criticalY(self, x, y):
        return -6.0 if y < 0 else 0.0 if y > 0 else self.centerY

    def snapshotJson(self):
        raise AssertionError("layout must not request a geometry snapshot")


class ManimSharedLayoutQueryTests(unittest.TestCase):
    def wrapper(self, handle):
        value = object.__new__(noon.Mobject)
        shared._attach_shared_handle(value, handle)
        return value

    def test_detached_layout_delegates_without_materializing_geometry(self):
        value = self.wrapper(Observation())
        self.assertEqual(shared._get_center(value), (7, -3))
        self.assertEqual(shared._width(value), 10)
        self.assertEqual(shared._height(value), 6)
        self.assertEqual(shared._layout_bounds(value), ((2, -6), (12, 0)))
        self.assertEqual(shared._get_critical_point(value, noon.RIGHT), (12, -3))

    def test_bound_layout_uses_the_owning_live_context(self):
        value = self.wrapper(object())
        observed = Observation()
        calls = []
        def query(handle):
            calls.append(handle)
            return observed
        value._scene = SimpleNamespace(_canonical_authoring_context=SimpleNamespace(queryMobjectLayout=query,
            queryMobjectCenter=lambda handle: (query(handle).centerX, observed.centerY)))
        value._object = object()
        self.assertEqual(shared._get_center(value), (7, -3))
        self.assertEqual(shared._width(value), 10)
        self.assertEqual(shared._get_critical_point(value, noon.UP), (7, 0))
        self.assertEqual(calls, [value._semantic_handle] * 3)

    def test_center_reads_one_value_projection_without_layout_or_component_getters(self):
        calls = []
        coordinates = [7.125, -3.75]
        class Handle:
            def centerCoordinates(self):
                calls.append("authored")
                return coordinates[:]
            @property
            def centerX(self):
                raise AssertionError("center must not make a component getter call")
            centerY = centerX
        value = self.wrapper(Handle())
        first = value.get_center()
        self.assertEqual(first, (7.125, -3.75))
        self.assertEqual(calls, ["authored"])
        def query(handle):
            self.assertIs(handle, value._semantic_handle)
            calls.append("effective")
            return coordinates[:]
        context = SimpleNamespace(queryMobjectCenter=query,
            queryMobjectLayout=lambda _: self.fail("center allocated a full layout observation"))
        value._scene = SimpleNamespace(_canonical_authoring_context=context)
        value._object = object()
        coordinates[:] = [2.25, 4.5]
        self.assertEqual(value.get_center(), (2.25, 4.5))
        self.assertEqual(calls, ["authored", "effective"])
        self.assertEqual(first, (7.125, -3.75))

    def test_center_queries_remain_live_and_propagate_owner_failure_without_fallback(self):
        value = self.wrapper(SimpleNamespace(centerCoordinates=lambda: self.fail("authored fallback")))
        context = SimpleNamespace(queryMobjectCenter=lambda _: (1.0, 2.0))
        value._scene = SimpleNamespace(_canonical_authoring_context=context)
        value._object = object()
        self.assertEqual(value.get_center(), (1.0, 2.0))
        context.queryMobjectCenter = lambda _: (3.0, 4.0)
        self.assertEqual(value.get_center(), (3.0, 4.0))
        failure = RuntimeError("transferred or retired owner")
        def reject(_):
            raise failure
        context.queryMobjectCenter = reject
        with self.assertRaises(RuntimeError) as caught:
            value.get_center()
        self.assertIs(caught.exception, failure)
        value._semantic_handle_fresh = False
        with self.assertRaises(RuntimeError):
            value.get_center()

    def test_active_callback_center_stays_on_the_staged_row(self):
        import _manim_updaters as updaters
        value = self.wrapper(SimpleNamespace(centerCoordinates=lambda: self.fail("authored read")))
        value._scene = SimpleNamespace(_canonical_authoring_context=SimpleNamespace(
            queryMobjectCenter=lambda _: self.fail("published read during callback")))
        value._object = object()
        row = SimpleNamespace(center=lambda: noon.Vec2(12.5, -2.25))
        with patch.object(updaters, "_canonical_provisional_context", return_value=None), \
                patch.object(updaters, "_canonical_row", return_value=(object(), (0, 1), row)):
            self.assertEqual(value.get_center(), (12.5, -2.25))

    def test_missing_shared_layout_never_falls_back_to_snapshot_math(self):
        for handle in (None, SimpleNamespace(snapshotJson=lambda: self.fail("raw fallback"))):
            value = self.wrapper(handle)
            for query in (shared._get_center, shared._width, shared._height, shared._layout_bounds):
                with self.subTest(handle=handle, query=query.__name__):
                    with self.assertRaises((RuntimeError, AttributeError)):
                        query(value)

    def test_copy_and_target_capture_require_a_shared_handle(self):
        value = self.wrapper(None)
        value._current_raw = lambda: self.fail("copy must not materialize raw state")
        for target_state in (False, True):
            with self.assertRaisesRegex(RuntimeError, "shared Rust"):
                shared._clone_mobject(value, target_state=target_state)
        with self.assertRaisesRegex(NotImplementedError, "raw replacement"):
            shared._apply(value, object())


if __name__ == "__main__":
    unittest.main()
