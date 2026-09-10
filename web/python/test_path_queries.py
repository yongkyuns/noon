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
