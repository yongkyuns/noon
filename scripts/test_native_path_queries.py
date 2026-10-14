"""Executable native extension checks for shared path queries (no test doubles)."""
import asyncio
import math
import unittest

from _noon_native import AuthoringVectorPath, GeometryOptions, Store
from _noon_errors import engine_call, NoonForeignHandleError, NoonOwnershipError
from noon import RIGHT
from noon_native import run_source, close_scene


class NativePathQueries(unittest.TestCase):
    def test_native_path_input_rolls_back_invalid_controls_and_is_consumed_once(self):
        store = Store()
        path = AuthoringVectorPath()
        path.moveTo(0, 0)
        path.lineTo(1, 0)
        with self.assertRaises(RuntimeError):
            path.cubicTo(1, 2, math.nan, 3, 4, 5)
        handle = store.geometry(GeometryOptions.path(path))
        query = handle.pathQuery()
        self.assertEqual(query.pointCount, 4)
        self.assertEqual(query.end(), (1, 0))
        with self.assertRaisesRegex(RuntimeError, "consumed"):
            path.lineTo(2, 0)
        with self.assertRaisesRegex(RuntimeError, "consumed"):
            GeometryOptions.path(path)
        query.free()

    def test_cubic_and_pending_anchor_counts_survive_live_edit_and_both_execution_routes(self):
        source = """from noon import *
class CubicPointCounts(Scene):
    def construct(self):
        curve = CubicBezier([-2, -1, 0], [-1, 2, 0], [1, -2, 0], [2, 1, 0])
        path = VMobject()
        assert curve.get_num_curves() == 1 and curve.get_num_points() == 4
        assert path.get_num_points() == 0
        path.start_new_path([0, 0, 0])
        assert path.get_num_points() == 1
        path.add_line_to([1, 0, 0])
        assert path.get_num_points() == 4
        self.add(curve, path)
        self.play(curve.animate.shift(RIGHT), run_time=.25, rate_func=linear)
        retained = self._canonical_authoring_context.queryMobjectPath(path._semantic_handle)
        path.add_line_to([2, 0, 0])
        assert path.get_num_points() == 8
        assert retained.pointCount == 4
        retained.free()
        try:
            self._canonical_authoring_context.liveAddLineTo(path._semantic_handle, float('nan'), 0)
        except RuntimeError:
            pass
        else:
            raise AssertionError('nonfinite path edit must fail')
        assert path.get_num_points() == 8
        self.counts = [curve.get_num_points(), path.get_num_points()]
"""
        for portable in (False, True):
            scene = asyncio.run(run_source(source, portable=portable, sample_hz=4))
            try:
                self.assertEqual(scene.counts, [4, 8])
            finally:
                close_scene(scene)

    def test_detached_snapshot_controls_measure_and_release(self):
        store = Store()
        handle = store.geometry(GeometryOptions.rectangle(2, 1))
        query = handle.pathQuery()
        self.assertEqual(query.curveCount, 4)
        self.assertEqual(query.pointCount, 16)
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
            _ = next_query.pointCount
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
        assert self.marker.get_num_points() == 16
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

    def test_binding_provenance_rejects_unbound_and_other_same_store_contexts(self):
        store = Store()
        owner, other = store.context(), store.context()
        handle = store.geometry(GeometryOptions.square(1))
        with self.assertRaisesRegex(RuntimeError, "not bound"):
            owner.queryMobjectPath(handle)
        owner.bindMobject("0", handle)
        query = owner.queryMobjectPath(handle)
        self.assertEqual(query.curveCount, 4)
        query.free()
        with self.assertRaisesRegex(RuntimeError, "not bound"):
            other.queryMobjectPath(handle)

    def test_foreign_binding_reservation_cannot_partially_publish(self):
        store, foreign = Store(), Store()
        context = store.context()
        own = store.geometry(GeometryOptions.square(1))
        batch = context.beginMembershipBatch("add")
        batch.appendMobject("0", own)
        batch.reserveMobjectBinding("1", foreign.geometry(GeometryOptions.square(1)))
        with self.assertRaises(NoonForeignHandleError):
            engine_call(context.editMembership, batch)
        self.assertFalse(context.containsMobject(own))
        with self.assertRaisesRegex(RuntimeError, "not bound"):
            context.queryMobjectPath(own)

    def test_foreign_store_and_nonfinite_parameter_rejected(self):
        first, second = Store(), Store()
        handle = first.geometry(GeometryOptions.line(0, 0, 3, 4))
        with self.assertRaises(NoonForeignHandleError):
            engine_call(second.context().queryMobjectPath, handle)
        with self.assertRaises(NoonForeignHandleError):
            engine_call(second.context().liveAddLineTo, handle, 1, 0)
        query = handle.pathQuery()
        self.assertAlmostEqual(query.arcLength(), 5)
        self.assertEqual(query.pointCount, 4)
        with self.assertRaises(RuntimeError):
            query.pointFromProportion(math.nan)
        with self.assertRaises(RuntimeError):
            query.curvePoints(1)
        query.free()


if __name__ == '__main__':
    unittest.main()
