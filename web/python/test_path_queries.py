import unittest
from unittest.mock import Mock, patch
import _manim_compat as compat
import _manim_path_queries as queries
from _typed_geometry_test_support import identity_only_wrapper


class PathQueryTests(unittest.TestCase):
    def test_shared_observation_dispatch_and_result_conversion(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.pointFromProportion.return_value = [2.5, -1.0]
        query.arcLength.return_value = 7.25
        query.start.return_value = [1., 2.]
        query.end.return_value = [3., 4.]
        with patch.object(queries, '_typed_manim_observation', return_value=query) as observe:
            self.assertEqual(value.point_from_proportion(0.25), (2.5, -1.0))
            observe.assert_called_with(value, 'pathQuery', 'queryMobjectPath')
            query.pointFromProportion.assert_called_with(0.25)
            self.assertEqual(value.get_start(), (1., 2.))
            query.start.assert_called_once_with()
            self.assertEqual(value.get_end(), (3., 4.))
            query.end.assert_called_once_with()
            self.assertEqual(value.get_arc_length(20), 7.25)
            query.arcLength.assert_called_with(20)
            self.assertEqual(query.free.call_count, 4)

    def test_input_errors_precede_query_allocation_and_failed_reads_free_snapshot(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(queries, '_typed_manim_observation') as observe:
            for samples in (0, 1, -1, 2**32):
                with self.assertRaises(ValueError):
                    value.get_arc_length(samples)
            with self.assertRaises(TypeError):
                value.get_arc_length(2.5)
            with self.assertRaises(ValueError):
                value.point_from_proportion(float('nan'))
            observe.assert_not_called()
        query = Mock()
        query.pointFromProportion.side_effect = ValueError('invalid proportion')
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            with self.assertRaises(ValueError):
                value.point_from_proportion(-1)
            query.free.assert_called_once()

    def test_curve_queries_share_one_snapshot_and_release_it(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.curvePoints.return_value = [0., 0., 1., 2., 2., 2., 3., 0.]
        query.startAnchors.return_value = [0., 0., 3., 0.]
        query.endAnchors.return_value = [3., 0., 4., 0.]
        query.firstHandles.return_value = [1., 2., 3.3, 0.]
        query.secondHandles.return_value = [2., 2., 3.6, 0.]
        query.anchors.return_value = [0., 0., 3., 0., 3., 0., 4., 0.]
        with patch.object(queries, '_typed_manim_observation', return_value=query) as observe:
            self.assertEqual(value.get_num_curves(), 2)
            self.assertEqual(value.get_nth_curve_points(0), [(0., 0.), (1., 2.), (2., 2.), (3., 0.)])
            self.assertEqual(value.get_start_anchors(), [(0., 0.), (3., 0.)])
            self.assertEqual(value.get_end_anchors(), [(3., 0.), (4., 0.)])
            self.assertEqual(len(value.get_anchors()), 4)
            columns = value.get_anchors_and_handles()
            self.assertEqual(columns[1], [(1., 2.), (3.3, 0.)])
            self.assertEqual(observe.call_count, 6)
            self.assertEqual(query.free.call_count, 6)
            query.curvePoints.side_effect = ValueError('curve index out of bounds')
            with self.assertRaises(ValueError): value.get_nth_curve_points(4)
            self.assertEqual(query.free.call_count, 7)


class SubpathQueryTests(unittest.TestCase):
    def test_subpaths_and_closure_free_the_snapshot_on_success_and_error(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.subpaths.return_value = [[0., 0., 1., 0., 2., 0., 3., 0.], [5., 1., 6., 1., 7., 1., 8., 1.]]
        query.isClosed.return_value = False
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            self.assertEqual(value.get_subpaths(), [[(0., 0.), (1., 0.), (2., 0.), (3., 0.)], [(5., 1.), (6., 1.), (7., 1.), (8., 1.)]])
            self.assertFalse(value.is_closed())
            self.assertEqual(query.free.call_count, 2)
            query.subpaths.side_effect = ValueError('bad content')
            with self.assertRaises(ValueError): value.get_subpaths()
            self.assertEqual(query.free.call_count, 3)

    def test_subcurve_dispatch_preserves_wrapper_type_and_python_metadata(self):
        import _manim_semantic_handles as handles
        value = identity_only_wrapper(compat.VMobject)
        value.label = {'kind': ['source']}
        source = Mock()
        for context in (None, Mock()):
            with patch.object(handles, '_handle_for', return_value=source), patch.object(handles, '_live_mutation_context', return_value=context):
                result = value.get_subcurve(0.8, 0.2)
            expected = source.subcurve.return_value if context is None else context.liveSubcurve.return_value
            self.assertIs(result._semantic_handle, expected)
            self.assertIsInstance(result, type(value))
            self.assertEqual(result.label, value.label)
            self.assertIsNot(result.label, value.label)
            if context is None: source.subcurve.assert_called_once_with(0.8, 0.2)
            else: context.liveSubcurve.assert_called_once_with(source, 0.8, 0.2)
        source.cloneHandle.assert_not_called()
