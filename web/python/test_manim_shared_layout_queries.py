"""Facade delegation; geometry correctness is tested against shared Rust bounds."""
import unittest
from types import SimpleNamespace

import noon
import _manim_compat

import _manim_semantic_handles as shared


class Observation:
    centerX, centerY, width, height = 7.0, -3.0, 10.0, 6.0

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
        value._scene = SimpleNamespace(_canonical_authoring_context=SimpleNamespace(queryMobjectLayout=query))
        value._object = object()
        self.assertEqual(shared._get_center(value), (7, -3))
        self.assertEqual(shared._width(value), 10)
        self.assertEqual(shared._get_critical_point(value, noon.UP), (7, 0))
        self.assertEqual(calls, [value._semantic_handle] * 3)

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
