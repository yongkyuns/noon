import gc
import inspect
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
        query.pointCount = 9
        query.curvePoints.return_value = [0., 0., 1., 2., 2., 2., 3., 0.]
        query.startAnchors.return_value = [0., 0., 3., 0.]
        query.endAnchors.return_value = [3., 0., 4., 0.]
        query.firstHandles.return_value = [1., 2., 3.3, 0.]
        query.secondHandles.return_value = [2., 2., 3.6, 0.]
        query.anchors.return_value = [0., 0., 3., 0., 3., 0., 4., 0.]
        with patch.object(queries, '_typed_manim_observation', return_value=query) as observe:
            self.assertEqual(value.get_num_curves(), 2)
            self.assertEqual(value.get_num_points(), 9)
            self.assertEqual(value.get_nth_curve_points(0), [(0., 0.), (1., 2.), (2., 2.), (3., 0.)])
            self.assertEqual(value.get_start_anchors(), [(0., 0.), (3., 0.)])
            self.assertEqual(value.get_end_anchors(), [(3., 0.), (4., 0.)])
            self.assertEqual(len(value.get_anchors()), 4)
            columns = value.get_anchors_and_handles()
            self.assertEqual(columns[1], [(1., 2.), (3.3, 0.)])
            self.assertEqual(observe.call_count, 7)
            self.assertEqual(query.free.call_count, 7)
            query.curvePoints.side_effect = ValueError('curve index out of bounds')
            with self.assertRaises(ValueError): value.get_nth_curve_points(4)
            self.assertEqual(query.free.call_count, 8)

    def test_curve_length_queries_forward_sample_count_and_free_snapshots(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy contracts run in the pinned utility lane")

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.curveLengthPieces.return_value = [0.25, 0.5, 0.75]
        query.curveLength.return_value = 1.5
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            pieces = value.get_nth_curve_length_pieces(0)
            np.testing.assert_array_equal(pieces, [0.25, 0.5, 0.75])
            self.assertEqual(pieces.dtype, np.float64)
            query.curveLengthPieces.assert_called_once_with(0, None)
            np.testing.assert_array_equal(value.get_nth_curve_length_pieces(1, sample_points=4), [0.25, 0.5, 0.75])
            query.curveLengthPieces.assert_called_with(1, 4)
            length = value.get_nth_curve_length(0)
            self.assertEqual(length, np.float64(1.5))
            self.assertIsInstance(length, np.float64)
            query.curveLength.assert_called_once_with(0, None)
            self.assertEqual(query.free.call_count, 3)
            with self.assertRaises(AssertionError):
                value.get_nth_curve_length_pieces(2)
            self.assertEqual(query.free.call_count, 4)

        with patch.object(queries, '_typed_manim_observation') as observe:
            for samples in (-1, 2**32):
                with self.assertRaises(ValueError):
                    value.get_nth_curve_length(0, sample_points=samples)
            with self.assertRaises(TypeError):
                value.get_nth_curve_length_pieces(0, sample_points=2.5)
            with self.assertRaises(IndexError):
                value.get_nth_curve_length(-1)
            observe.assert_not_called()

    def test_curve_functions_keep_snapshot_and_vectorize_numpy_parameters(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy contracts run in the pinned utility lane")
        self.assertTrue(inspect.isgeneratorfunction(compat.VMobject.get_curve_functions))
        self.assertTrue(inspect.isgeneratorfunction(compat.VMobject.get_curve_functions_with_lengths))
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.curvePointsAt.side_effect = lambda index, values: [
            component
            for parameter in values
            for component in (parameter + index, 2 * parameter, 0.0)
        ]
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            functions = list(value.get_curve_functions())
            self.assertEqual(len(functions), 2)
            self.assertTrue(inspect.isfunction(functions[0]))
            self.assertEqual(functions[0].__name__, "cubic_bezier")
            np.testing.assert_array_equal(functions[0](t=0.5), [0.5, 1., 0.])
            np.testing.assert_array_equal(
                functions[1](np.array([[0.25], [0.75]])),
                [[1.25, 0.5, 0.], [1.75, 1.5, 0.]],
            )
            self.assertEqual(query.free.call_count, 0)
            query.curveLength.side_effect = [2.5, 3.5]
            pairs = list(value.get_curve_functions_with_lengths(sample_points=6))
            self.assertEqual([length for _, length in pairs], [np.float64(2.5), np.float64(3.5)])
            pairs[0][0](1.0)
            self.assertEqual(query.curveLength.call_args_list[-1].args, (1, 6))
            self.assertEqual(query.free.call_count, 0)
            del functions, pairs
            gc.collect()
            self.assertEqual(query.free.call_count, 2)
            query.curveLength.side_effect = None
            query.curveLength.return_value = 4.0
            function, length = value.get_nth_curve_function_with_length(0, sample_points=6)
            self.assertTrue(inspect.isfunction(function))
            self.assertIsInstance(length, np.float64)
            del function
            gc.collect()
            self.assertEqual(query.free.call_count, 3)

    def test_yielded_curve_survives_generator_error_until_last_owner_releases_it(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy curve callable results")
        value = identity_only_wrapper(compat.VMobject)
        for with_lengths in (False, True):
            with self.subTest(with_lengths=with_lengths):
                query = Mock(curveCount=2, pointCount=8)
                query.curveLength.return_value = 2.5
                query.curvePointsAt.return_value = [0.5, 1., 0.]
                with patch.object(queries, "_typed_manim_observation", return_value=query):
                    stream = (
                        value.get_curve_functions_with_lengths()
                        if with_lengths else value.get_curve_functions()
                    )
                    yielded = next(stream)
                    function = yielded[0] if with_lengths else yielded
                    with self.assertRaisesRegex(ValueError, "stop querying"):
                        stream.throw(ValueError("stop querying"))
                query.free.assert_not_called()
                np.testing.assert_array_equal(function(0.5), [0.5, 1., 0.])
                del stream, yielded, function
                gc.collect()
                query.free.assert_called_once_with()

    def test_yielded_curve_survives_later_length_query_failure(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy curve callable results")
        value = identity_only_wrapper(compat.VMobject)
        query = Mock(curveCount=2, pointCount=8)
        query.curvePointsAt.return_value = [0.5, 1., 0.]

        def length(index, _samples):
            if index == 1:
                raise ValueError("later length failed")
            return 2.5

        query.curveLength.side_effect = length
        with patch.object(queries, "_typed_manim_observation", return_value=query):
            stream = value.get_curve_functions_with_lengths()
            function, curve_length = next(stream)
            self.assertEqual(curve_length, 2.5)
            with self.assertRaisesRegex(ValueError, "later length failed"):
                next(stream)
        query.free.assert_not_called()
        np.testing.assert_array_equal(function(0.5), [0.5, 1., 0.])
        del stream, function
        gc.collect()
        query.free.assert_called_once_with()

    def test_curve_function_index_and_length_generator_arguments(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 1
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            with self.assertRaises(AssertionError):
                value.get_nth_curve_function(1)
            query.free.assert_called_once()
            with self.assertRaises(TypeError):
                next(value.get_curve_functions_with_lengths(unsupported=True))
            with self.assertRaises(IndexError):
                value.get_nth_curve_function(-1)

    def test_force_direction_uses_shared_query_and_preserves_manim_errors(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.direction.return_value = "CCW"
        value.reverse_direction = Mock(return_value=value)
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            self.assertEqual(value.get_direction(), "CCW")
            self.assertIs(value.force_direction("CW"), value)
            value.reverse_direction.assert_called_once_with()
            self.assertIs(value.force_direction("CCW"), value)
            self.assertEqual(query.free.call_count, 3)
            with self.assertRaisesRegex(
                ValueError,
                'Invalid input for force_direction. Use "CW" or "CCW"',
            ):
                value.force_direction("cw")
            self.assertEqual(query.free.call_count, 3)


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
