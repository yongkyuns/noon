"""Executable native extension checks for shared path queries (no test doubles)."""
import asyncio
import math
import unittest

from _noon_native import GeometryOptions, Store
from _noon_errors import engine_call, NoonForeignHandleError, NoonOwnershipError
from noon import RIGHT
from noon_native import run_source, close_scene


class NativePathQueries(unittest.TestCase):
    def test_detached_snapshot_controls_measure_and_release(self):
        store = Store()
        handle = store.geometry(GeometryOptions.rectangle(2, 1))
        query = handle.pathQuery()
        self.assertEqual(query.curveCount, 4)
        self.assertEqual(len(query.curvePoints(0)), 8)
        self.assertEqual(len(query.startAnchors()), 8)
        self.assertEqual(len(query.endAnchors()), 8)
        self.assertEqual(len(query.firstHandles()), 8)
        self.assertEqual(len(query.secondHandles()), 8)
        self.assertEqual(len(query.anchors()), 16)
        self.assertEqual(len(query.subpaths()), 1)
        self.assertEqual(len(query.vertexGroups()), 1)
        self.assertTrue(query.isClosed())
        self.assertAlmostEqual(query.arcLength(16), 6)
        self.assertEqual(query.pointFromProportion(0), query.start())
        self.assertEqual(query.pointFromProportion(1), query.end())
        prior = query.start()
        handle.shift(2, 0)
        self.assertEqual(query.start(), prior)
        next_query = handle.pathQuery()
        self.assertEqual(next_query.start(), (prior[0] + 2, prior[1]))
        next_query.free()
        with self.assertRaises(RuntimeError):
            next_query.start()
        next_query.free()
        query.free()

    def test_bound_effective_queries_use_current_execution_and_guard_ownership(self):
        observations = []
        source = '''from noon import *
class QueryScene(Scene):
    def construct(self):
        self.marker = Square()
        self.add(self.marker)
        self.before = self.marker.get_start()
        assert self.marker.get_num_curves() == 4
        assert len(self.marker.get_anchors_and_handles()) == 4
        self.play(self.marker.animate.shift(RIGHT), run_time=1)
        self.after = self.marker.get_start()
        assert abs(self.after[0] - self.before[0] - 1) < 1e-6
        assert abs(self.after[1] - self.before[1]) < 1e-6
'''
        for portable in (False, True):
            scene = asyncio.run(run_source(source, portable=portable, sample_hz=4))
            try:
                observations.append((scene.before, scene.after))
                retained = scene._canonical_authoring_context.queryMobjectPath(scene.marker._semantic_handle)
                prior = retained.start()
            finally:
                close_scene(scene)
            self.assertEqual(retained.start(), prior)
            retained.free()
            with self.assertRaises(NoonOwnershipError):
                scene.marker.get_start()
        self.assertEqual(observations[0], observations[1])

    def test_foreign_store_and_nonfinite_parameter_rejected(self):
        first, second = Store(), Store()
        handle = first.geometry(GeometryOptions.line(0, 0, 3, 4))
        with self.assertRaises(NoonForeignHandleError):
            engine_call(second.context().queryMobjectPath, handle)
        query = handle.pathQuery()
        self.assertAlmostEqual(query.arcLength(), 5)
        with self.assertRaises(RuntimeError):
            query.pointFromProportion(math.nan)
        with self.assertRaises(RuntimeError):
            query.curvePoints(1)
        query.free()


if __name__ == '__main__':
    unittest.main()
