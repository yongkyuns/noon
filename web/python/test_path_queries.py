import gc
import inspect
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch
import _manim_compat as compat
import _manim_path_queries as queries
import _manim_semantic_handles as semantic_handles
from _typed_geometry_test_support import identity_only_wrapper, empty_path_query


def _path_observation(query):
    """Return scalar counts or owned snapshots at the typed test boundary."""
    def observe(_value, method, context_method, *, handle_property=False):
        if method == "pointCount":
            assert context_method == "queryMobjectPointCount" and handle_property
            return query.pointCount
        assert method == "pathQuery" and context_method == "queryMobjectPath"
        assert not handle_property
        return query
    return observe


class PathQueryTests(unittest.TestCase):
    def test_single_anchor_array_precedes_mutable_stride_validation(self):
        try:
            import numpy as np
        except ImportError:
            np = None
        value = identity_only_wrapper(compat.VMobject)
        for stride in (4, 0, -2, 3.5, None):
            value.n_points_per_cubic_curve = stride
            query = Mock(pointCount=1)
            query.anchors.return_value = [1.0 + 2.0 ** -40, -2.0]
            with patch.object(queries, "_query", return_value=query):
                result = value.get_anchors()
            self.assertEqual(float(result[0][0]), 1.0 + 2.0 ** -40)
            if np is not None:
                self.assertIsInstance(result, np.ndarray)
            else:
                self.assertIsInstance(result, list)
            query.points.assert_not_called()
            query.anchors.assert_called_once_with()
            query.free.assert_called_once_with()

    def test_default_anchor_pairing_error_is_engine_owned_and_frees_snapshot(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock(pointCount=2)
        error = ValueError("zip() argument 2 is shorter than argument 1")
        query.anchors.side_effect = error
        with patch.object(queries, "_query", return_value=query):
            with self.assertRaises(ValueError) as raised:
                value.get_anchors()
        self.assertIs(raised.exception, error)
        query.points.assert_not_called()
        query.free.assert_called_once_with()

    def test_vmobject_point_equality_uses_rust_with_broadcast_arrays_and_tolerance(self):
        value = identity_only_wrapper(compat.VMobject)
        value.tolerance_for_point_equality = 2e-6
        handle = Mock()
        handle.pointsAllClose.return_value = True
        handle.points2dClose.return_value = False
        with patch.object(queries, "_handle_for", return_value=handle):
            self.assertTrue(value.consider_points_equals([1, 2, 3], [1, 2, 3]))
            self.assertFalse(value.consider_points_equals_2d([1, 2, 8], [1, 2, -8]))
        handle.pointsAllClose.assert_called_once_with([1.0, 2.0, 3.0], [1.0, 2.0, 3.0], 2e-6)
        handle.points2dClose.assert_called_once_with([1.0, 2.0], [1.0, 2.0], 2e-6)

    def test_vmobject_point_equality_preserves_numpy_broadcasting_and_shape_errors(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy broadcasting behavior")
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        with patch.object(queries, "_handle_for", return_value=handle):
            value.consider_points_equals(np.array([[1.0], [2.0]]), np.array([1.0, 2.0]))
            with self.assertRaises(ValueError):
                value.consider_points_equals([1, 2], [1, 2, 3])
        handle.pointsAllClose.assert_called_once_with(
            [1.0, 1.0, 2.0, 2.0], [1.0, 2.0, 1.0, 2.0], 1e-6
        )

    def test_vmobject_exposes_mutable_default_point_equality_tolerance(self):
        value = object.__new__(compat.VMobject)
        self.assertEqual(value.tolerance_for_point_equality, 1e-6)
        value.tolerance_for_point_equality = 3e-5
        self.assertEqual(value.tolerance_for_point_equality, 3e-5)

    def test_vmobject_exposes_canonical_cubic_stride_name(self):
        value = identity_only_wrapper(compat.VMobject)
        self.assertEqual(value.n_points_per_curve, 4)
        self.assertEqual(value.n_points_per_cubic_curve, 4)
        self.assertEqual(compat.VMobject.n_points_per_cubic_curve, 4)

    def test_proportion_from_point_uses_shared_curve_lengths_and_maximum_parameter(self):
        value = identity_only_wrapper(compat.VMobject)
        value.throw_error_if_no_points = Mock()
        query = Mock()
        query.proportionFromPoint.return_value = 0.8125
        with patch.object(queries, "_typed_manim_observation", return_value=query), patch.dict(
            "sys.modules", {"numpy": None}
        ):
            self.assertEqual(value.proportion_from_point(compat._base.Vec2(1, 1)), 0.8125)

        value.throw_error_if_no_points.assert_called_once_with()
        query.proportionFromPoint.assert_called_once_with(1.0, 1.0, 0.0)
        query.free.assert_called_once_with()

    def test_proportion_from_point_uses_mutable_stride_ranges_in_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value.throw_error_if_no_points = Mock()
        query = Mock()
        query.pointCount = 4
        query.proportionFromPointForRanges.return_value = 0.625
        query.bezierLengthPiecesRange.side_effect = AssertionError("Rust owns length normalization")
        query.bezierParametersForPointRange.side_effect = AssertionError("Rust owns curve matching")

        with patch.object(queries, "_typed_manim_observation", return_value=query):
            self.assertEqual(value.proportion_from_point((1.0, 2.0, 0.0)), 0.625)

        query.proportionFromPointForRanges.assert_called_once_with([0, 2, 2, 4], 1.0, 2.0, 0.0)
        query.bezierLengthPiecesRange.assert_not_called()
        query.bezierParametersForPointRange.assert_not_called()
        query.proportionFromPoint.assert_not_called()
        query.free.assert_called_once_with()

    def test_proportion_from_point_preserves_off_curve_and_empty_path_errors(self):
        value = identity_only_wrapper(compat.VMobject)
        value.throw_error_if_no_points = Mock()
        query = Mock()
        query.proportionFromPoint.return_value = None
        with patch.object(queries, "_typed_manim_observation", return_value=query):
            with self.assertRaisesRegex(ValueError, "Point .* does not lie on this curve"):
                value.proportion_from_point((1, 1, 0))
        query.free.assert_called_once_with()

        value = identity_only_wrapper(compat.VMobject)
        value.throw_error_if_no_points = Mock(side_effect=Exception("no points"))
        with patch.object(queries, "_typed_manim_observation") as observe:
            with self.assertRaisesRegex(Exception, "no points"):
                value.proportion_from_point((0, 0, 0))
            observe.assert_not_called()

    def test_custom_stride_inverse_preserves_unmatched_point_error_and_releases_query(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 3
        value.throw_error_if_no_points = Mock()
        query = Mock(pointCount=8)
        query.proportionFromPointForRanges.return_value = None
        with patch.object(queries, "_typed_manim_observation", return_value=query):
            with self.assertRaisesRegex(ValueError, "Point .* does not lie on this curve"):
                value.proportion_from_point((99.0, 0.0, 0.0))
        query.proportionFromPointForRanges.assert_called_once_with([0, 3, 3, 6], 99.0, 0.0, 0.0)
        value.throw_error_if_no_points.assert_called_once_with()
        query.free.assert_called_once_with()

    def test_vmobject_constructor_accepts_query_stride_and_tolerance(self):
        with patch.object(semantic_handles, "_empty_path_init") as initialize:
            value = compat.VMobject(
                n_points_per_cubic_curve=3,
                tolerance_for_point_equality=2e-5,
                marker="forwarded",
            )

        self.assertEqual(value.n_points_per_cubic_curve, 3)
        self.assertEqual(value.n_points_per_curve, 3)
        self.assertEqual(value.tolerance_for_point_equality, 2e-5)
        self.assertEqual(list(value._bezier_t_values), [0., .5, 1.])
        initialize.assert_called_once_with(
            value, color=compat._base.WHITE, kind="VMobject", marker="forwarded"
        )

    def test_vmobject_constructor_bezier_samples_without_numpy(self):
        with patch.object(semantic_handles, "_empty_path_init"), patch.dict(
            "sys.modules", {"numpy": None}
        ):
            value = compat.VMobject(n_points_per_cubic_curve=3)
            singleton = compat.VMobject(n_points_per_cubic_curve=1)
            empty = compat.VMobject(n_points_per_cubic_curve=0)
        self.assertEqual(value._bezier_t_values, [0., .5, 1.])
        self.assertEqual(singleton._bezier_t_values, [0.0])
        self.assertEqual(empty._bezier_t_values, [])

    def test_vmobject_constructor_bezier_stride_matches_numpy_validation(self):
        with patch.object(semantic_handles, "_empty_path_init"):
            with self.assertRaisesRegex(TypeError, "cannot be interpreted as an integer"):
                compat.VMobject(n_points_per_cubic_curve=3.5)
            with self.assertRaisesRegex(ValueError, "Number of samples, -1"):
                compat.VMobject(n_points_per_cubic_curve=-1)

    def test_subpath_split_query_uses_rust_and_returns_numpy_index_ranges(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy array result shape")
        value = identity_only_wrapper(compat.VMobject)
        value.tolerance_for_point_equality = 2e-5
        handle = Mock()
        handle.subpathRanges.return_value = [0, 4, 4, 8]
        points = np.arange(24, dtype=np.float64).reshape((8, 3))
        with patch.object(queries, "_handle_for", return_value=handle):
            ranges = value.get_subpath_split_indices_from_points(points, n_dims=-1)
        self.assertIsInstance(ranges, np.ndarray)
        self.assertEqual(ranges.shape, (2, 2))
        np.testing.assert_array_equal(ranges, [[0, 4], [4, 8]])
        handle.subpathRanges.assert_called_once_with(
            points.reshape(-1).tolist(), 8, 3, 2, 4.0, 2e-5, False
        )

    def test_subpath_split_query_without_numpy_returns_range_lists(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        handle.subpathRanges.return_value = [0, 4, 4, 8]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_handle_for", return_value=handle
        ):
            ranges = value.get_subpath_split_indices_from_points(
                [[0, 0], [1, 0], [2, 0], [3, 0], [3, 0], [4, 0], [5, 0], [6, 0]],
                n_dims=3,
            )
        self.assertEqual(ranges, [[0, 4], [4, 8]])
        handle.subpathRanges.assert_called_once_with(
            [0., 0., 1., 0., 2., 0., 3., 0., 3., 0., 4., 0., 5., 0., 6., 0.],
            8, 2, 2, 4.0, 1e-6, False,
        )

    def test_subpath_split_query_rejects_ragged_or_nonmatrix_points_before_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(queries, "_handle_for") as handle:
            with self.assertRaises(ValueError):
                value.get_subpath_split_indices_from_points([[0, 0], [1, 0, 0]])
            with self.assertRaises(ValueError):
                value.get_subpath_split_indices_from_points([0, 1, 2, 3])
        handle.assert_not_called()

    def test_subpath_getters_use_rust_ranges_and_preserve_point_slice_views(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy subpath slice behavior")
        value = identity_only_wrapper(compat.VMobject)
        points = np.arange(39, dtype=np.float64).reshape((13, 3))
        handle = Mock()
        handle.subpathRanges.side_effect = [[0, 8, 8, 12], [0, 12]]
        with patch.object(queries, "_handle_for", return_value=handle):
            subpaths = value.get_subpaths_from_points(points)
            self.assertEqual(len(subpaths), 2)
            np.testing.assert_array_equal(subpaths[0], points[:8])
            self.assertTrue(np.shares_memory(subpaths[0], points))

            generated = value.gen_subpaths_from_points_2d(points)
            self.assertTrue(hasattr(generated, "__next__"))
            planar_subpaths = list(generated)
            self.assertEqual(len(planar_subpaths), 1)
            np.testing.assert_array_equal(planar_subpaths[0], points[:12])
            self.assertTrue(np.shares_memory(planar_subpaths[0], points))
        expected_points = points.reshape(-1).tolist()
        self.assertEqual(
            handle.subpathRanges.call_args_list[0].args,
            (expected_points, 13, 3, 3, 4.0, 1e-6, True),
        )
        self.assertEqual(
            handle.subpathRanges.call_args_list[1].args,
            (expected_points, 13, 3, 2, 4.0, 1e-6, True),
        )

    def test_subpath_getters_forward_custom_stride_and_zero_stride_raises(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        handle.subpathRanges.return_value = [0, 6]
        points = [[index, 0, 0] for index in range(6)]
        with patch.object(queries, "_handle_for", return_value=handle):
            value.n_points_per_cubic_curve = 3
            self.assertEqual(value.get_subpaths_from_points(points), [points[:6]])
            value.n_points_per_cubic_curve = 0
            with self.assertRaises(ValueError):
                value.get_subpaths_from_points(points)
        handle.subpathRanges.assert_called_once_with(
            [float(c) for row in points for c in row], 6, 3, 3, 3.0, 1e-6, True
        )

    def test_subpath_getters_slice_python_lists_when_numpy_is_unavailable(self):
        value = identity_only_wrapper(compat.VMobject)
        points = [[index, 0, 0] for index in range(8)]
        handle = Mock()
        handle.subpathRanges.side_effect = [[0, 4], [0, 4, 4, 8]]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_handle_for", return_value=handle
        ):
            self.assertEqual(value.get_subpaths_from_points(points), [points[:4]])
            self.assertEqual(
                list(value.gen_subpaths_from_points_2d(points)),
                [points[:4], points[4:]],
            )

    def test_cubic_tuple_helpers_use_rust_stride_ranges_and_keep_generator_views(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy tuple grouping behavior")
        value = identity_only_wrapper(compat.VMobject)
        points = np.arange(30, dtype=np.float64).reshape((10, 3))
        handle = Mock()

        def ranges(point_count, stride):
            stride = int(stride)
            complete = point_count // stride * stride if stride > 0 else 0
            return [index for start in range(0, complete, stride)
                    for index in (start, start + stride)]

        handle.cubicBezierPointRanges.side_effect = ranges
        with patch.object(queries, "_handle_for", return_value=handle):
            generated = value.gen_cubic_bezier_tuples_from_points(points)
            self.assertIsInstance(generated, tuple)
            self.assertEqual(len(generated), 2)
            np.testing.assert_array_equal(generated[0], points[:4])
            self.assertTrue(np.shares_memory(generated[0], points))

            grouped = value.get_cubic_bezier_tuples_from_points(points)
            self.assertIsInstance(grouped, np.ndarray)
            self.assertEqual(grouped.shape, (2, 4, 3))
            np.testing.assert_array_equal(grouped, points[:8].reshape((2, 4, 3)))
            self.assertFalse(np.shares_memory(grouped, points))

            value.n_points_per_cubic_curve = 3
            custom = value.get_cubic_bezier_tuples_from_points(points)
            self.assertEqual(custom.shape, (3, 3, 3))
            np.testing.assert_array_equal(custom, points[:9].reshape((3, 3, 3)))
        self.assertEqual(handle.cubicBezierPointRanges.call_args_list[0].args, (10, 4.0))
        self.assertEqual(handle.cubicBezierPointRanges.call_args_list[-1].args, (10, 3.0))

    def test_cubic_tuple_getter_groups_the_current_rust_path_snapshot(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy grouped-array result shape")

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.points.return_value = [
            0., 0., 1., 0., 1., 1., 2., 1.,
            2., 1., 3., 1., 3., 2., 4., 2.,
        ]
        handle = Mock()

        def ranges(point_count, stride):
            stride = int(stride)
            complete = point_count // stride * stride
            return [
                index
                for start in range(0, complete, stride)
                for index in (start, start + stride)
            ]

        handle.cubicBezierPointRanges.side_effect = ranges
        with patch.object(queries, "_typed_manim_observation", return_value=query), patch.object(
            queries, "_handle_for", return_value=handle
        ):
            default = value.get_cubic_bezier_tuples()
            value.n_points_per_cubic_curve = 3
            custom = value.get_cubic_bezier_tuples()

        self.assertIsInstance(default, np.ndarray)
        self.assertEqual(default.shape, (2, 4, 2))
        np.testing.assert_array_equal(default, np.asarray(query.points.return_value).reshape((2, 4, 2)))
        self.assertEqual(custom.shape, (2, 3, 2))
        np.testing.assert_array_equal(custom, np.asarray(query.points.return_value[:-4]).reshape((2, 3, 2)))
        self.assertEqual(query.free.call_count, 2)
        self.assertEqual(handle.cubicBezierPointRanges.call_args_list[0].args, (8, 4.0))
        self.assertEqual(handle.cubicBezierPointRanges.call_args_list[1].args, (8, 3.0))

    def test_cubic_tuple_getter_current_snapshot_without_numpy_returns_nested_vec2(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.points.return_value = [
            0., 0., 1., 0., 1., 1., 2., 1.,
            2., 1., 3., 1., 3., 2., 4., 2.,
        ]
        handle = Mock()
        handle.cubicBezierPointRanges.side_effect = [[0, 4, 4, 8], [0, 3, 3, 6]]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_typed_manim_observation", return_value=query
        ), patch.object(queries, "_handle_for", return_value=handle):
            default = value.get_cubic_bezier_tuples()
            value.n_points_per_cubic_curve = 3
            custom = value.get_cubic_bezier_tuples()

        def observe(groups):
            return [[tuple(point) for point in group] for group in groups]

        self.assertEqual(
            observe(default),
            [
                [(0., 0.), (1., 0.), (1., 1.), (2., 1.)],
                [(2., 1.), (3., 1.), (3., 2.), (4., 2.)],
            ],
        )
        self.assertEqual(
            observe(custom),
            [
                [(0., 0.), (1., 0.), (1., 1.)],
                [(2., 1.), (2., 1.), (3., 1.)],
            ],
        )
        self.assertEqual(query.free.call_count, 2)

    def test_cubic_tuple_helpers_preserve_negative_and_zero_stride_behavior(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = []
        with patch.object(queries, "_handle_for", return_value=handle):
            value.n_points_per_cubic_curve = -4
            self.assertEqual(value.gen_cubic_bezier_tuples_from_points([[0], [1]]), ())
            value.n_points_per_cubic_curve = 0
            with self.assertRaises(ZeroDivisionError):
                value.gen_cubic_bezier_tuples_from_points([[0], [1]])
        handle.cubicBezierPointRanges.assert_called_once_with(2, -4.0)

    def test_cubic_tuple_getter_without_numpy_returns_nested_lists(self):
        value = identity_only_wrapper(compat.VMobject)
        points = [[index, index + 1] for index in range(10)]
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = [0, 4, 4, 8]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_handle_for", return_value=handle
        ):
            self.assertEqual(
                value.get_cubic_bezier_tuples_from_points(points),
                [points[:4], points[4:8]],
            )

    def test_n_points_per_curve_tracks_the_mutable_cubic_stride(self):
        value = identity_only_wrapper(compat.VMobject)
        self.assertEqual(value.n_points_per_curve, 4)
        value.n_points_per_cubic_curve = 3
        self.assertEqual(value.n_points_per_curve, 3)

    def test_all_points_returns_a_two_column_array_and_boundary_aliases_it(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy-only array shape check")
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(
            queries,
            "_all_points_observation",
            return_value=[0., 0., 1., 0., 2., 0., 3., 0.],
        ) as observe, patch.object(
            queries,
            "_path_boundary_points_observation",
            return_value=[0., 0., 3., 0.],
        ) as boundary_observe:
            points = value.get_all_points()
            self.assertIsInstance(points, np.ndarray)
            self.assertEqual(points.shape, (4, 2))
            np.testing.assert_array_equal(points, [[0., 0.], [1., 0.], [2., 0.], [3., 0.]])
            boundary = value.get_points_defining_boundary()
            self.assertEqual(boundary.shape, (2, 2))
            np.testing.assert_array_equal(boundary, [[0., 0.], [3., 0.]])
        self.assertEqual(observe.call_count, 1)
        boundary_observe.assert_called_once_with(value)

    def test_vector_boundary_points_follow_mutable_stride_anchors(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy-only array shape check")
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value.get_family = Mock(return_value=[value])
        query = Mock()
        query.points.return_value = [0., 0., 1., 0., 2., 0., 3., 0.]

        with patch.object(queries, "_typed_manim_observation", return_value=query):
            boundary = value.get_points_defining_boundary()

        np.testing.assert_array_equal(
            boundary,
            [[0., 0.], [1., 0.], [2., 0.], [3., 0.]],
        )
        value.get_family.assert_called_once_with()
        self.assertEqual(query.free.call_count, 1)

    def test_vgroup_boundary_points_aggregate_family_anchors_in_order(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy-only array shape check")
        import noon

        group = identity_only_wrapper(compat.VGroup)
        group._semantic_family_handle = SimpleNamespace(pathQuery=empty_path_query)
        first = identity_only_wrapper(compat.VMobject)
        second = identity_only_wrapper(compat.VMobject)
        first.get_anchors = Mock(return_value=[noon.Vec2(0, 0), noon.Vec2(2, 0)])
        second.get_anchors = Mock(return_value=[noon.Vec2(3, 1), noon.Vec2(6, 1)])
        group.get_family = Mock(return_value=[group, first, second])

        boundary = group.get_points_defining_boundary()

        np.testing.assert_array_equal(
            boundary,
            [[0, 0], [2, 0], [3, 1], [6, 1]],
        )
        group.get_family.assert_called_once_with()
        first.get_anchors.assert_called_once_with()
        second.get_anchors.assert_called_once_with()

    def test_vgroup_boundary_points_preserve_nested_alias_order(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy-only family ordering check")
        import noon

        root = identity_only_wrapper(compat.VGroup)
        root._semantic_family_handle = SimpleNamespace(pathQuery=empty_path_query)
        nested = identity_only_wrapper(compat.VGroup)
        nested._semantic_family_handle = SimpleNamespace(pathQuery=empty_path_query)
        first = identity_only_wrapper(compat.VMobject)
        second = identity_only_wrapper(compat.VMobject)
        first.get_anchors = Mock(return_value=[noon.Vec2(0, 0), noon.Vec2(2, 0)])
        second.get_anchors = Mock(return_value=[noon.Vec2(3, 1), noon.Vec2(6, 1)])
        # Rust supplies Manim's last-occurrence order for the aliased family.
        root.get_family = Mock(return_value=[root, nested, second, first])

        boundary = root.get_points_defining_boundary()

        np.testing.assert_array_equal(
            boundary,
            [[3, 1], [6, 1], [0, 0], [2, 0]],
        )
        root.get_family.assert_called_once_with()

    def test_empty_vgroup_boundary_points_keep_pinned_numpy_shape(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy-only empty array shape check")

        group = identity_only_wrapper(compat.VGroup)
        group._semantic_family_handle = SimpleNamespace(pathQuery=empty_path_query)
        group.get_family = Mock(return_value=[group])

        boundary = group.get_points_defining_boundary()

        self.assertIsInstance(boundary, np.ndarray)
        self.assertEqual(boundary.shape, (0,))
        group.get_family.assert_called_once_with()

    def test_vgroup_boundary_points_without_numpy_keep_vec2_values(self):
        import noon

        group = identity_only_wrapper(compat.VGroup)
        group._semantic_family_handle = SimpleNamespace(pathQuery=empty_path_query)
        member = identity_only_wrapper(compat.VMobject)
        member.get_anchors = Mock(return_value=[noon.Vec2(1, 2), noon.Vec2(3, 4)])
        group.get_family = Mock(return_value=[group, member])

        with patch.dict("sys.modules", {"numpy": None}):
            boundary = group.get_points_defining_boundary()

        self.assertEqual(boundary, [noon.Vec2(1, 2), noon.Vec2(3, 4)])

    def test_vector_boundary_points_without_numpy_keep_vec2_values(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value.get_family = Mock(return_value=[value])
        query = Mock()
        query.points.return_value = [0., 0., 1., 0., 2., 0., 3., 0.]
        with (
            patch.object(queries, "_typed_manim_observation", return_value=query),
            patch.dict("sys.modules", {"numpy": None}),
        ):
            self.assertEqual(
                value.get_points_defining_boundary(),
                [
                    noon.Vec2(0., 0.), noon.Vec2(1., 0.),
                    noon.Vec2(2., 0.), noon.Vec2(3., 0.),
                ],
            )
        self.assertEqual(query.free.call_count, 1)

    def test_all_points_uses_vec2_list_when_numpy_is_unavailable(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        with (
            patch.object(
                queries,
                "_all_points_observation",
                return_value=[0., 0., 1., 2.],
            ),
            patch.dict("sys.modules", {"numpy": None}),
        ):
            self.assertEqual(value.get_all_points(), [noon.Vec2(0., 0.), noon.Vec2(1., 2.)])

    def test_shared_family_all_points_uses_live_or_authored_rust_handle(self):
        value = identity_only_wrapper(compat.Group)
        family = Mock()
        value._semantic_family_handle = family
        coordinates = [0., 0., 1., 2.]

        context = Mock()
        context.queryFamilyAllPoints.return_value = coordinates
        with patch.object(
            semantic_handles,
            "_group_live_layout_context",
            return_value=context,
        ):
            self.assertIs(semantic_handles._all_points_observation(value), coordinates)
        context.queryFamilyAllPoints.assert_called_once_with(family)
        family.allPoints.assert_not_called()

        family.allPoints.return_value = coordinates
        with patch.object(
            semantic_handles,
            "_group_live_layout_context",
            return_value=None,
        ):
            self.assertIs(semantic_handles._all_points_observation(value), coordinates)
        family.allPoints.assert_called_once_with()

    def test_group_point_count_excludes_descendant_points(self):
        for cls in (compat.Group, compat.VGroup):
            with self.subTest(cls=cls.__name__):
                value = identity_only_wrapper(cls)
                family = Mock()
                family.pointCount.return_value = 0
                value._semantic_family_handle = family

                self.assertEqual(value.get_num_points(), 0)
                self.assertFalse(value.has_points())
                self.assertTrue(value.has_no_points())
                self.assertEqual(family.pointCount.call_count, 3)

    def test_shared_family_extremum_uses_live_query_or_authored_handle(self):
        value = identity_only_wrapper(compat.Group)
        family = Mock()
        value._semantic_family_handle = family

        context = Mock()
        context.queryFamilyExtremumAlongDim.return_value = 4.0
        with patch.object(
            semantic_handles,
            "_group_live_layout_context",
            return_value=context,
        ):
            self.assertEqual(semantic_handles._extremum_along_dim_observation(value, 1, 1.0), 4.0)
        context.queryFamilyExtremumAlongDim.assert_called_once_with(family, 1, 1.0)
        family.extremumAlongDim.assert_not_called()

        family.extremumAlongDim.return_value = 2.0
        with patch.object(
            semantic_handles,
            "_group_live_layout_context",
            return_value=None,
        ):
            self.assertEqual(semantic_handles._extremum_along_dim_observation(value, 0, -1.0), 2.0)
        family.extremumAlongDim.assert_called_once_with(0, -1.0)

    def test_shared_family_point_queries_use_live_query_or_authored_handle(self):
        value = identity_only_wrapper(compat.Group)
        family = Mock()
        value._semantic_family_handle = family
        context = Mock()
        context.queryFamilyCenterOfMass.return_value = [1.0, 2.0]
        context.queryFamilyBoundaryPoint.return_value = [3.0, 4.0]
        context.queryFamilyLengthOverDim.return_value = 5.0
        with patch.object(
            semantic_handles,
            "_group_live_layout_context",
            return_value=context,
        ):
            self.assertEqual(
                semantic_handles._center_of_mass_observation(value),
                [1.0, 2.0],
            )
            self.assertEqual(
                semantic_handles._boundary_point_observation(value, 0.0, 1.0),
                [3.0, 4.0],
            )
            self.assertEqual(
                semantic_handles._length_over_dim_observation(value, 1),
                5.0,
            )
        context.queryFamilyCenterOfMass.assert_called_once_with(family)
        context.queryFamilyBoundaryPoint.assert_called_once_with(family, 0.0, 1.0)
        context.queryFamilyLengthOverDim.assert_called_once_with(family, 1)
        family.centerOfMass.assert_not_called()
        family.boundaryPoint.assert_not_called()
        family.lengthOverDim.assert_not_called()

    def test_mobject_point_queries_use_live_context_or_authored_handle(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock()
        handle.centerOfMass.return_value = [1.0, 2.0]
        handle.pathBoundaryPoint.return_value = [3.0, 4.0]
        handle.lengthOverDim.return_value = 2.5
        value._semantic_handle = handle
        value._semantic_handle_fresh = True
        self.assertEqual(semantic_handles._center_of_mass_observation(value), [1.0, 2.0])
        self.assertEqual(
            semantic_handles._boundary_point_observation(value, 0.0, 1.0),
            [3.0, 4.0],
        )
        self.assertEqual(semantic_handles._length_over_dim_observation(value, 0), 2.5)
        handle.centerOfMass.assert_called_once_with()
        handle.pathBoundaryPoint.assert_called_once_with(0.0, 1.0)
        handle.lengthOverDim.assert_called_once_with(0)
        context = Mock()
        context.queryMobjectCenterOfMass.return_value = [5.0, 6.0]
        context.queryMobjectPathBoundaryPoint.return_value = [7.0, 8.0]
        context.queryMobjectPathBoundaryExtremumAlongDim.return_value = 9.0
        context.queryMobjectPathBoundaryPoints.return_value = [0.0, 0.0, 1.0, 1.0]
        context.queryMobjectLengthOverDim.return_value = 4.5
        scene = Mock()
        scene._canonical_authoring_context = context
        value._scene = scene
        value._object = Mock()
        self.assertEqual(semantic_handles._center_of_mass_observation(value), [5.0, 6.0])
        self.assertEqual(
            semantic_handles._boundary_point_observation(value, 0.0, 1.0),
            [7.0, 8.0],
        )
        self.assertEqual(
            semantic_handles._extremum_along_dim_observation(value, 1, 1.0),
            9.0,
        )
        self.assertEqual(
            semantic_handles._path_boundary_points_observation(value),
            [0.0, 0.0, 1.0, 1.0],
        )
        self.assertEqual(semantic_handles._length_over_dim_observation(value, 2), 4.5)
        context.queryMobjectCenterOfMass.assert_called_once_with(handle)
        context.queryMobjectPathBoundaryPoint.assert_called_once_with(handle, 0.0, 1.0)
        context.queryMobjectPathBoundaryExtremumAlongDim.assert_called_once_with(handle, 1, 1.0)
        context.queryMobjectPathBoundaryPoints.assert_called_once_with(handle)
        context.queryMobjectLengthOverDim.assert_called_once_with(handle, 2)

    def test_custom_stride_boundary_point_uses_rust_anchor_reducer(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value.get_anchors = Mock(return_value=[
            noon.Vec2(0.0, 0.0),
            noon.Vec2(30.0, 0.0),
            noon.Vec2(1.0, 0.0),
            noon.Vec2(2.0, 0.0),
        ])
        handle = Mock()
        handle.boundaryPointFromPoints.return_value = [30.0, 0.0]
        value._semantic_handle = handle
        value._semantic_handle_fresh = True

        self.assertEqual(value.get_boundary_point(noon.RIGHT), noon.Vec2(30.0, 0.0))
        handle.boundaryPointFromPoints.assert_called_once_with(
            [0.0, 0.0, 30.0, 0.0, 1.0, 0.0, 2.0, 0.0],
            2,
            1.0,
            0.0,
        )

    def test_custom_stride_live_extrema_and_boundary_use_effective_path_query(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value._object = object()
        value._semantic_handle = Mock()
        value._semantic_handle_fresh = True
        value._semantic_handle.extremumFromPoints.return_value = 30.0
        value._semantic_handle.boundaryPointFromPoints.return_value = [30.0, 0.0]

        path_query = Mock()
        path_query.points.return_value = [
            0.0, 0.0,
            30.0, 0.0,
            1.0, 0.0,
            2.0, 0.0,
        ]
        context = Mock()
        context.queryMobjectPath.return_value = path_query
        scene = Mock()
        scene._canonical_authoring_context = context
        value._scene = scene

        self.assertEqual(
            semantic_handles._extremum_along_dim_observation(value, 0, 1.0),
            30.0,
        )
        self.assertEqual(
            semantic_handles._boundary_point_observation(value, 1.0, 0.0),
            [30.0, 0.0],
        )

        expected_anchor_coordinates = [
            0.0, 0.0,
            30.0, 0.0,
            1.0, 0.0,
            2.0, 0.0,
        ]
        self.assertEqual(
            value._semantic_handle.extremumFromPoints.call_args.args,
            (expected_anchor_coordinates, 2, 0, 1.0),
        )
        self.assertEqual(
            value._semantic_handle.boundaryPointFromPoints.call_args.args,
            (expected_anchor_coordinates, 2, 1.0, 0.0),
        )
        self.assertEqual(context.queryMobjectPath.call_count, 2)
        self.assertEqual(path_query.points.call_count, 2)
        self.assertEqual(path_query.free.call_count, 2)

    def test_custom_stride_extrema_and_boundary_reject_staged_callbacks(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        with patch(
            "_manim_updaters.canonical_callback_phase_active",
            return_value=True,
        ):
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                semantic_handles._extremum_along_dim_observation(value, 0, 1.0)
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                semantic_handles._boundary_point_observation(value, 1.0, 0.0)

    def test_length_over_dim_uses_rust_and_accepts_negative_dimensions(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(
            queries,
            "_length_over_dim_observation",
            return_value=3.25,
        ) as observe:
            self.assertEqual(value.length_over_dim(-3), 3.25)
        observe.assert_called_once_with(value, 0)
        with self.assertRaises(IndexError):
            value.length_over_dim(3)

    def test_planar_z_helpers_match_the_3d_compatibility_surface(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        with patch.object(value, "get_extremum_along_dim", return_value=0.0) as extremum:
            self.assertEqual(value.get_coord(2), 0.0)
            self.assertEqual(value.get_z(noon.OUT), 0.0)
            self.assertEqual(value.get_coord(-1), 0.0)
        self.assertEqual(extremum.call_args_list[0].kwargs, {"dim": 2, "key": 0})
        self.assertEqual(extremum.call_args_list[1].kwargs, {"dim": 2, "key": 0})
        self.assertEqual(extremum.call_args_list[2].kwargs, {"dim": 2, "key": 0})
        with self.assertRaises(IndexError):
            value.get_coord(3)

        value.get_critical_point = Mock(return_value=noon.Vec2(1.0, 2.0))
        self.assertEqual(value.get_zenith(), noon.Vec2(1.0, 2.0))
        self.assertEqual(value.get_nadir(), noon.Vec2(1.0, 2.0))
        self.assertEqual(value.get_critical_point.call_count, 2)
        value.get_critical_point.assert_called_with(noon.ORIGIN)

    def test_get_coord_selects_each_point_extremum_from_its_direction_axis(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        with patch.object(value, "get_extremum_along_dim", side_effect=[-2.0, 3.0]) as extremum:
            self.assertEqual(value.get_coord(0, (-1.0, 1.0, 1.0)), -2.0)
            self.assertEqual(value.get_y(noon.UP), 3.0)
        self.assertEqual(
            [call.kwargs for call in extremum.call_args_list],
            [{"dim": 0, "key": -1.0}, {"dim": 1, "key": 1.0}],
        )

    def test_vmobject_critical_point_uses_anchor_extrema(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        value.get_num_points = Mock(return_value=4)
        with patch.object(
            semantic_handles,
            "_extremum_along_dim_observation",
            side_effect=[-2.0, 3.0],
        ) as observe:
            self.assertEqual(
                semantic_handles._get_critical_point(value, (-1.0, 1.0)),
                noon.Vec2(-2.0, 3.0),
            )
        self.assertEqual(
            [call.args for call in observe.call_args_list],
            [(value, 0, -1.0), (value, 1, 1.0)],
        )

    def test_custom_stride_vmobject_critical_point_reduces_anchor_snapshot_in_rust(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value.get_num_points = Mock(return_value=4)
        value.get_anchors = Mock(return_value=[
            noon.Vec2(0.0, 0.0),
            noon.Vec2(30.0, 0.0),
            noon.Vec2(1.0, 0.0),
            noon.Vec2(2.0, 0.0),
        ])
        with patch.object(
            semantic_handles,
            "_extremum_from_points_observation",
            side_effect=[30.0, 0.0],
        ) as reduce:
            self.assertEqual(
                semantic_handles._get_critical_point(value, (1.0, 1.0)),
                noon.Vec2(30.0, 0.0),
            )
        value.get_anchors.assert_called_with()
        self.assertEqual(value.get_anchors.call_count, 2)
        self.assertEqual(
            [call.args for call in reduce.call_args_list],
            [
                (value, [0.0, 0.0, 30.0, 0.0, 1.0, 0.0, 2.0, 0.0], 2, 0, 1.0),
                (value, [0.0, 0.0, 30.0, 0.0, 1.0, 0.0, 2.0, 0.0], 2, 1, 1.0),
            ],
        )

    def test_vmobject_center_uses_anchor_critical_point(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        with patch.object(
            semantic_handles, "_vmobject_path_boundary_queries_available", return_value=True
        ), patch.object(
            semantic_handles,
            "_get_critical_point",
            return_value=noon.Vec2(0.5, 1.0),
        ) as critical_point:
            self.assertEqual(semantic_handles._get_center(value), noon.Vec2(0.5, 1.0))
        critical_point.assert_called_once_with(value, noon.ORIGIN)

    def test_extremum_along_dim_routes_default_points_to_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value.get_points_defining_boundary = Mock(side_effect=AssertionError("Python bounds query"))
        with patch.object(
            queries,
            "_extremum_along_dim_observation",
            return_value=3.5,
        ) as observe:
            self.assertEqual(value.get_extremum_along_dim(dim=1, key=1), 3.5)
        observe.assert_called_once_with(value, 1, 1.0)

        with patch.object(
            queries,
            "_extremum_along_dim_observation",
            return_value=0.0,
        ) as observe:
            self.assertEqual(value.get_extremum_along_dim(dim=-1, key=1), 0.0)
        observe.assert_called_once_with(value, 2, 1.0)
        value.get_points_defining_boundary.assert_not_called()
        with self.assertRaises(IndexError):
            value.get_extremum_along_dim(dim=3)

    def test_center_of_mass_and_boundary_point_route_to_rust(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        with (
            patch.object(queries, "_center_of_mass_observation", return_value=[1.5, -0.5]),
            patch.object(queries, "_boundary_point_observation", return_value=[2.0, 3.0]) as point,
        ):
            self.assertEqual(value.get_center_of_mass(), noon.Vec2(1.5, -0.5))
            self.assertEqual(
                value.get_boundary_point([0.0, 0.0, 1.0]),
                noon.Vec2(2.0, 3.0),
            )
        point.assert_called_once_with(value, 0.0, 0.0)

    def test_extremum_along_dim_flattens_custom_points_for_rust_reducer(self):
        value = identity_only_wrapper(compat.VMobject)
        points = [(-2., 3.), (4., -1.)]
        try:
            import numpy as np
        except ImportError:
            np = None
        with patch.object(queries, "_extremum_from_points_observation", return_value=3.0) as observe:
            self.assertEqual(value.get_extremum_along_dim(points, dim=1, key=1), 3.0)
        if np is not None:
            observe.assert_called_once_with(value, [3., -1.], 1, 0, 1.0)
        else:
            observe.assert_called_once_with(value, [-2., 3., 4., -1.], 2, 1, 1.0)

        with (
            patch.dict("sys.modules", {"numpy": None}),
            patch.object(queries, "_extremum_from_points_observation", return_value=-2.0) as observe,
        ):
            self.assertEqual(value.get_extremum_along_dim(points, dim=-2, key=-1), -2.0)
        observe.assert_called_once_with(value, [-2., 3., 4., -1.], 2, 0, -1.0)
        with self.assertRaises(IndexError):
            value.get_extremum_along_dim(points, dim=3)

    def test_shared_observation_dispatch_and_result_conversion(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 1
        query.pointCount = 4
        query.pointFromProportion.return_value = [2.5, -1.0]
        query.curveLength.return_value = 7.25
        query.start.return_value = [1., 2.]
        query.end.return_value = [3., 4.]
        with patch.object(queries, '_typed_manim_observation', side_effect=_path_observation(query)) as observe:
            self.assertEqual(value.point_from_proportion(0.25), (2.5, -1.0))
            observe.assert_called_with(value, 'pathQuery', 'queryMobjectPath')
            query.pointFromProportion.assert_called_with(0.25)
            start = value.get_start()
            query.start.assert_called_once_with()
            end = value.get_end()
            query.end.assert_called_once_with()
            try:
                import numpy as np
            except ImportError:
                self.assertEqual(start, (1., 2.))
                self.assertEqual(end, (3., 4.))
            else:
                self.assertIsInstance(start, np.ndarray)
                self.assertIsInstance(end, np.ndarray)
                self.assertEqual(start.shape, (2,))
                self.assertEqual(end.shape, (2,))
                self.assertEqual(start.dtype, np.dtype(np.float64))
                self.assertEqual(end.dtype, np.dtype(np.float64))
                np.testing.assert_array_equal(start, [1., 2.])
                np.testing.assert_array_equal(end, [3., 4.])
            self.assertEqual(value.get_arc_length(20), 7.25)
            query.curveLength.assert_called_with(0, 20)
        self.assertEqual(query.free.call_count, 4)

    def test_endpoint_queries_fall_back_to_vec2_without_numpy(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.pointCount = 2
        query.start.return_value = [1.25, -2.5]
        query.end.return_value = [3.75, 4.5]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_typed_manim_observation", side_effect=_path_observation(query)
        ):
            self.assertEqual(value.get_start(), noon.Vec2(1.25, -2.5))
            self.assertEqual(value.get_end(), noon.Vec2(3.75, 4.5))
        self.assertEqual(query.free.call_count, 2)

    def test_get_pieces_builds_independent_equal_proportion_path_copies(self):
        value = identity_only_wrapper(compat.VMobject)
        template = Mock()
        pieces = [Mock(), Mock()]
        template.copy.side_effect = pieces
        value.copy = Mock(return_value=template)
        result = object()

        with patch.object(compat, "_is_shared_family", return_value=False), patch.object(
            compat, "Group", return_value=result
        ) as group_type:
            self.assertIs(value.get_pieces(2), result)

        value.copy.assert_called_once_with()
        self.assertEqual(template.submobjects, [])
        self.assertEqual(template.copy.call_count, 2)
        group_type.assert_called_once_with(*(
            piece.pointwise_become_partial.return_value for piece in pieces
        ))
        pieces[0].pointwise_become_partial.assert_called_once_with(
            value, 0.0, 0.5
        )
        pieces[1].pointwise_become_partial.assert_called_once_with(
            value, 0.5, 1.0
        )

    def test_get_pieces_keeps_no_numpy_and_empty_piece_fallbacks(self):
        value = identity_only_wrapper(compat.VMobject)
        template = Mock()
        value.copy = Mock(return_value=template)
        result = object()
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            compat, "_is_shared_family", return_value=False
        ), patch.object(compat, "Group", return_value=result) as group_type:
            self.assertIs(value.get_pieces(-1), result)
        group_type.assert_called_once_with()
        template.copy.assert_not_called()
        with patch.dict("sys.modules", {"numpy": None}):
            with self.assertRaisesRegex(
                ValueError, "Number of samples, -1, must be non-negative"
            ):
                value.get_pieces(-2)

    def test_vectorized_point_is_exported_from_the_compatibility_layer(self):
        import noon

        self.assertIn("VectorizedPoint", noon.__all__)
        self.assertIs(noon.VectorizedPoint, compat.VectorizedPoint)
        self.assertTrue(issubclass(noon.VectorizedPoint, compat.VMobject))

    def test_point_from_proportion_follows_mutable_stride_in_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        query = Mock()
        query.pointCount = 4
        query.bezierLengthPiecesRange.side_effect = ([2.0], [6.0])
        query.bezierPointsAtRange.return_value = [2.5, -1.0, 0.0]

        with patch.object(queries, "_typed_manim_observation", return_value=query):
            self.assertEqual(value.point_from_proportion(0.5), (2.5, -1.0))

        self.assertEqual(
            [call.args for call in query.bezierLengthPiecesRange.call_args_list],
            [(0, 2, None), (2, 4, None)],
        )
        query.bezierPointsAtRange.assert_called_once_with(2, 4, [1.0 / 3.0])
        query.pointFromProportion.assert_not_called()
        query.free.assert_called_once()

    def test_custom_stride_proportions_use_bound_live_path_queries(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value._object = object()
        value.throw_error_if_no_points = Mock()
        value._semantic_handle = Mock()
        value._semantic_handle_fresh = True

        path_query = Mock()
        path_query.pointCount = 4
        path_query.bezierLengthPiecesRange.side_effect = ([30.0], [1.0])
        path_query.bezierPointsAtRange.return_value = [15.5, 0.0, 0.0]
        path_query.proportionFromPointForRanges.return_value = 0.5

        context = Mock()
        context.queryMobjectPath.return_value = path_query
        value._scene = Mock(_canonical_authoring_context=context)

        self.assertEqual(value.point_from_proportion(0.5), (15.5, 0.0))
        self.assertAlmostEqual(value.proportion_from_point((15.5, 0.0, 0.0)), 0.5)

        self.assertEqual(context.queryMobjectPath.call_count, 2)
        self.assertEqual(path_query.free.call_count, 2)
        self.assertEqual(
            [call.args for call in path_query.bezierLengthPiecesRange.call_args_list],
            [(0, 2, None), (2, 4, None)],
        )
        path_query.bezierParametersForPointRange.assert_not_called()
        path_query.proportionFromPointForRanges.assert_called_once_with(
            [0, 2, 2, 4], 15.5, 0.0, 0.0
        )

    def test_custom_stride_proportions_reject_staged_callbacks_before_host_reads(self):
        import _manim_updaters as updaters

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        value._object = object()
        value.throw_error_if_no_points = Mock()
        value._semantic_handle = Mock()
        value._semantic_handle_fresh = True
        context = Mock()
        value._scene = Mock(_canonical_authoring_context=context)

        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(Mock())
        try:
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                value.point_from_proportion(0.5)
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                value.proportion_from_point((15.5, 0.0, 0.0))
            for query in (
                value.get_anchors,
                value.get_start_anchors,
                value.get_end_anchors,
                value.get_anchors_and_handles,
                lambda: value.get_num_curves(),
                lambda: value.get_arc_length(5),
            ):
                with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                    query()
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)

        context.queryMobjectPath.assert_not_called()
        value.throw_error_if_no_points.assert_not_called()

    def test_mutable_stride_proportion_one_returns_the_last_stored_point(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        query = Mock()
        query.pointCount = 4
        query.points.return_value = [0.0, 0.0, 1.0, 0.0, 2.0, 3.0, 4.0, 5.0]

        with patch.object(queries, "_typed_manim_observation", return_value=query):
            self.assertEqual(value.point_from_proportion(1), (4.0, 5.0))

        query.points.assert_called_once_with()
        query.bezierLengthPiecesRange.assert_not_called()
        query.free.assert_called_once()

    def test_anchor_groups_follow_mutable_stride_over_rust_point_snapshot(self):
        try:
            import numpy as np
        except ImportError:
            np = None

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        query = Mock()
        query.points.return_value = [0., 0., 1., 0., 2., 0., 3., 0.]

        with patch.object(queries, "_typed_manim_observation", return_value=query):
            starts = value.get_start_anchors()
            ends = value.get_end_anchors()
            anchors = value.get_anchors()
            handles = value.get_anchors_and_handles()
            if np is None:
                self.assertEqual(starts, [(0., 0.), (2., 0.)])
                self.assertEqual(ends, [(1., 0.), (3., 0.)])
                self.assertEqual(anchors, [(0., 0.), (1., 0.), (2., 0.), (3., 0.)])
                self.assertEqual(
                    handles,
                    [[(0., 0.), (2., 0.)], [(1., 0.), (3., 0.)]],
                )
            else:
                self.assertIsInstance(starts, np.ndarray)
                self.assertIsInstance(ends, np.ndarray)
                np.testing.assert_array_equal(starts, [[0., 0.], [2., 0.]])
                np.testing.assert_array_equal(ends, [[1., 0.], [3., 0.]])
                self.assertTrue(all(isinstance(point, np.ndarray) for point in anchors))
                np.testing.assert_array_equal(
                    anchors,
                    [[0., 0.], [1., 0.], [2., 0.], [3., 0.]],
                )
                self.assertTrue(all(isinstance(group, np.ndarray) for group in handles))
                np.testing.assert_array_equal(handles[0], [[0., 0.], [2., 0.]])
                np.testing.assert_array_equal(handles[1], [[1., 0.], [3., 0.]])
        self.assertEqual(query.free.call_count, 4)

    def test_anchor_queries_fall_back_to_point_lists_without_numpy(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        query = Mock()
        query.points.return_value = [0., 0., 1., 0., 2., 0., 3., 0.]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_typed_manim_observation", return_value=query
        ):
            self.assertEqual(value.get_start_anchors(), [(0., 0.), (2., 0.)])
            self.assertEqual(value.get_end_anchors(), [(1., 0.), (3., 0.)])
            self.assertEqual(
                value.get_anchors(),
                [(0., 0.), (1., 0.), (2., 0.), (3., 0.)],
            )
            self.assertEqual(
                value.get_anchors_and_handles(),
                [[(0., 0.), (2., 0.)], [(1., 0.), (3., 0.)]],
            )

    def test_anchor_handle_groups_keep_empty_and_fractional_stride_behavior(self):
        value = identity_only_wrapper(compat.VMobject)
        with patch.object(queries, "_typed_manim_observation") as observe:
            for stride in (0, -2):
                value.n_points_per_cubic_curve = stride
                self.assertEqual(value.get_anchors_and_handles(), [])
            value.n_points_per_cubic_curve = 2.5
            with self.assertRaises(TypeError):
                value.get_anchors_and_handles()
        observe.assert_not_called()

    def test_start_and_end_are_base_methods_and_keep_dynamic_getter_dispatch(self):
        import noon

        self.assertIn("get_start", noon.Mobject.__dict__)
        self.assertIn("get_end", noon.Mobject.__dict__)
        self.assertIn("get_start_and_end", noon.Mobject.__dict__)
        self.assertIn("get_num_points", noon.Mobject.__dict__)
        self.assertIn("has_points", noon.Mobject.__dict__)
        self.assertIn("has_no_points", noon.Mobject.__dict__)
        self.assertIn("throw_error_if_no_points", noon.Mobject.__dict__)
        self.assertIn("get_extremum_along_dim", noon.Mobject.__dict__)
        self.assertIn("length_over_dim", noon.Mobject.__dict__)
        self.assertIn("get_z", noon.Mobject.__dict__)
        self.assertIn("get_zenith", noon.Mobject.__dict__)
        self.assertIn("get_nadir", noon.Mobject.__dict__)
        self.assertIn("split", noon.Mobject.__dict__)
        self.assertIn("nonempty_submobjects", noon.Mobject.__dict__)
        self.assertIn("family_members_with_points", noon.Mobject.__dict__)
        self.assertIn("get_center_of_mass", noon.Mobject.__dict__)
        self.assertIn("get_boundary_point", noon.Mobject.__dict__)
        self.assertIn("get_midpoint", noon.Mobject.__dict__)
        self.assertIn("get_z_index_reference_point", noon.Mobject.__dict__)
        self.assertIs(compat.VMobject.get_num_points, noon.Mobject.get_num_points)

        value = identity_only_wrapper(compat.VMobject)
        value.point_from_proportion = Mock(return_value=noon.Vec2(2.0, -1.0))
        self.assertEqual(value.get_midpoint(), noon.Vec2(2.0, -1.0))
        value.point_from_proportion.assert_called_once_with(0.5)

        own_center = Mock(return_value=noon.Vec2(1.0, 2.0))
        value.get_center = own_center
        self.assertEqual(value.get_z_index_reference_point(), noon.Vec2(1.0, 2.0))
        own_center.assert_called_once_with()

        group_center = Mock(return_value=noon.Vec2(-3.0, 4.0))
        value.z_index_group = Mock(get_center=group_center)
        self.assertEqual(value.get_z_index_reference_point(), noon.Vec2(-3.0, 4.0))
        group_center.assert_called_once_with()

        query = Mock()
        query.pointCount = 4
        query.start.return_value = [1.0, 2.0]
        query.end.return_value = [3.0, 4.0]
        with patch.object(queries, "_typed_manim_observation", side_effect=_path_observation(query)) as observe:
            points = value.get_start_and_end()
            try:
                import numpy as np
            except ImportError:
                self.assertEqual(
                    points,
                    (noon.Vec2(1.0, 2.0), noon.Vec2(3.0, 4.0)),
                )
            else:
                self.assertEqual(len(points), 2)
                for point, expected in zip(points, ([1.0, 2.0], [3.0, 4.0])):
                    self.assertIsInstance(point, np.ndarray)
                    self.assertEqual(point.shape, (2,))
                    np.testing.assert_array_equal(point, expected)
            self.assertEqual(observe.call_count, 4)
            self.assertEqual(query.start.call_count, 1)
            self.assertEqual(query.end.call_count, 1)
            self.assertEqual(query.free.call_count, 2)

        line = identity_only_wrapper(compat.Line)
        line.get_start = Mock(return_value=noon.Vec2(-1.0, 0.0))
        line.get_end = Mock(return_value=noon.Vec2(2.0, 5.0))
        self.assertEqual(
            line.get_start_and_end(),
            (noon.Vec2(-1.0, 0.0), noon.Vec2(2.0, 5.0)),
        )
        line.get_start.assert_called_once_with()
        line.get_end.assert_called_once_with()

    def test_family_split_filters_point_owners_in_pinned_order(self):
        from unittest.mock import Mock
        import noon

        root = identity_only_wrapper(noon.Mobject)
        root.has_points = Mock(return_value=False)
        root.get_num_points = Mock(return_value=0)
        root.get_family = Mock()
        leaf = Mock(get_num_points=Mock(return_value=4), submobjects=[])
        second_leaf = Mock(get_num_points=Mock(return_value=2), submobjects=[])
        empty = Mock(get_num_points=Mock(return_value=0), submobjects=[])
        nested = Mock(get_num_points=Mock(return_value=0), submobjects=[empty])
        root.submobjects = [nested, empty, leaf, second_leaf]
        from types import SimpleNamespace
        root._semantic_member_wrappers = dict(zip(("1:1", "2:1", "3:1", "4:1"), root.submobjects))
        root._semantic_handle = SimpleNamespace(
            semanticSlot=99, semanticGeneration=1,
            splitKeys=Mock(side_effect=lambda include: (["99:1"] if include else []) + list(root._semantic_member_wrappers)),
        )
        root._semantic_handle_fresh = True

        self.assertEqual(noon.Mobject.split(root), [nested, empty, leaf, second_leaf])
        self.assertEqual(noon.Mobject.nonempty_submobjects(root), [nested, leaf, second_leaf])
        empty.get_num_points.assert_called_once_with()
        nested.get_num_points.assert_called_once_with()
        leaf.get_num_points.assert_called_once_with()
        second_leaf.get_num_points.assert_called_once_with()

        root.has_points.return_value = True
        self.assertEqual(noon.Mobject.split(root), [root, nested, empty, leaf, second_leaf])

        root.get_family.return_value = [root, nested, empty, leaf, second_leaf]
        root.get_num_points.return_value = 0
        self.assertEqual(noon.Mobject.family_members_with_points(root), [leaf, second_leaf])

    def test_empty_path_guards_keep_manim_caller_names(self):
        value = identity_only_wrapper(compat.VMobject)
        value.get_num_points = Mock(return_value=0)

        for method, caller in ((value.get_start, "get_start"), (value.get_end, "get_end")):
            with self.subTest(method=caller), self.assertRaisesRegex(
                Exception,
                f"Cannot call Mobject.{caller} for a Mobject with no points",
            ):
                method()

    def test_base_point_count_queries_use_scalar_rust_observation(self):
        import noon

        value = identity_only_wrapper(noon.Mobject)
        query = Mock()
        query.pointCount = 4
        with patch.object(queries, "_typed_manim_observation", side_effect=_path_observation(query)) as observe:
            self.assertEqual(value.get_num_points(), 4)
            self.assertTrue(value.has_points())
            self.assertFalse(value.has_no_points())
            query.pointCount = 0
            self.assertFalse(value.has_points())
            self.assertTrue(value.has_no_points())
        self.assertEqual(observe.call_count, 5)
        query.free.assert_not_called()
        observe.assert_called_with(value, "pointCount", "queryMobjectPointCount", handle_property=True)

    def test_image_and_text_point_counts_use_the_same_typed_scalar_operation(self):
        from _manim_image import ImageMobject
        from _manim_typst import Text

        for cls, count in ((ImageMobject, 4), (Text, 0)):
            with self.subTest(cls=cls.__name__):
                value = identity_only_wrapper(cls)
                with patch.object(queries, "_typed_manim_observation", return_value=count) as observe:
                    self.assertEqual(value.get_num_points(), count)
                    self.assertEqual(value.has_points(), count > 0)
                    self.assertEqual(value.has_no_points(), count == 0)
                self.assertEqual(observe.call_count, 3)
                observe.assert_called_with(
                    value, "pointCount", "queryMobjectPointCount", handle_property=True
                )

    def test_bound_point_count_reads_effective_owner_and_keeps_its_failure(self):
        value = identity_only_wrapper(compat.VMobject)
        handle = Mock(pointCount=32)
        value._semantic_handle = handle
        value._semantic_handle_fresh = True
        value._object = object()
        context = Mock()
        context.queryMobjectPointCount.return_value = 12
        value._scene = Mock(_canonical_authoring_context=context)
        self.assertEqual(value.get_num_points(), 12)
        context.queryMobjectPointCount.assert_called_once_with(handle)
        context.queryMobjectPointCount.side_effect = RuntimeError("stale publication")
        with self.assertRaisesRegex(Exception, "stale publication"):
            value.get_num_points()
        self.assertEqual(context.queryMobjectPointCount.call_count, 2)
        handle.pathQuery.assert_not_called()

    def test_custom_stride_point_count_keeps_staged_query_guard(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        with patch("_manim_updaters.canonical_callback_phase_active", return_value=True), \
             patch.object(queries, "_typed_manim_observation") as observe:
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                value.get_num_points()
        observe.assert_not_called()

    def test_get_last_point_returns_the_final_anchor_and_matches_empty_index_error(self):
        import noon

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.pointCount = 4
        query.end.return_value = [7.0, -2.0]
        with patch.object(queries, "_typed_manim_observation", return_value=query) as observe:
            self.assertEqual(value.get_last_point(), noon.Vec2(7.0, -2.0))
            self.assertEqual(observe.call_count, 1)
            query.end.assert_called_once_with()
            self.assertEqual(query.free.call_count, 1)

            query.pointCount = 0
            with self.assertRaisesRegex(
                IndexError, "index -1 is out of bounds for axis 0 with size 0"
            ):
                value.get_last_point()
            self.assertEqual(query.end.call_count, 1)
            self.assertEqual(query.free.call_count, 2)

    def test_has_new_path_started_delegates_to_rust_and_releases_snapshots(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.hasNewPathStarted.side_effect = [False, True]
        with patch.object(queries, "_typed_manim_observation", return_value=query) as observe:
            self.assertFalse(value.has_new_path_started())
            self.assertTrue(value.has_new_path_started())
        self.assertEqual(observe.call_count, 2)
        self.assertEqual(query.hasNewPathStarted.call_count, 2)
        self.assertEqual(query.free.call_count, 2)

    def test_has_new_path_started_uses_mutable_stride_and_rejects_zero_before_query(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 5
        query = Mock()
        query.hasNewPathStarted.return_value = True
        with patch.object(queries, "_typed_manim_observation", return_value=query) as observe:
            self.assertTrue(value.has_new_path_started())
        observe.assert_called_once_with(value, "pathQuery", "queryMobjectPath")
        query.hasNewPathStarted.assert_called_once_with(5)
        query.free.assert_called_once_with()

        value.n_points_per_cubic_curve = 0
        with patch.object(queries, "_typed_manim_observation") as observe:
            with self.assertRaisesRegex(ZeroDivisionError, "integer modulo by zero"):
                value.has_new_path_started()
        observe.assert_not_called()

    def test_curve_sample_validation_matches_curve_iteration_and_frees_snapshots(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy error contract")

        axis_error = getattr(np, "AxisError", None) or np.exceptions.AxisError
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.pointCount = 8
        query.curveLength.return_value = 0.0
        with patch.object(queries, '_typed_manim_observation', return_value=query) as observe:
            with self.assertRaisesRegex(axis_error, "axis 1 is out of bounds"):
                value.get_arc_length(0)
            self.assertEqual(value.get_arc_length(1), 0.0)
            self.assertEqual(
                [call.args for call in query.curveLength.call_args_list],
                [(0, 1), (1, 1)],
            )
            for samples, error in ((-1, ValueError), (2**32, ValueError), (2.5, TypeError)):
                with self.subTest(samples=samples), self.assertRaises(error):
                    value.get_arc_length(samples)
            self.assertEqual(query.curveLength.call_count, 2)
            self.assertEqual(query.free.call_count, 5)
        self.assertEqual(observe.call_count, 5)

        empty_query = Mock()
        empty_query.curveCount = 0
        empty_query.pointCount = 0
        with patch.object(queries, '_typed_manim_observation', return_value=empty_query):
            for samples in (-1, 2.5, 2**32, object()):
                self.assertEqual(value.get_arc_length(samples), 0.0)
        self.assertEqual(empty_query.curveLength.call_count, 0)
        self.assertEqual(empty_query.free.call_count, 4)

        with patch.object(queries, '_typed_manim_observation') as observe:
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
        try:
            import numpy as np
        except ImportError:
            np = None

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.pointCount = 9
        query.points.return_value = [
            0., 0., 1., 2., 2., 2., 3., 0.,
            3., 0., 3.3, 0., 3.6, 0., 4., 0.,
            5., 0.,
        ]
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = [0, 4, 4, 8]
        query.startAnchors.return_value = [0., 0., 3., 0.]
        query.endAnchors.return_value = [3., 0., 4., 0.]
        query.firstHandles.return_value = [1., 2., 3.3, 0.]
        query.secondHandles.return_value = [2., 2., 3.6, 0.]
        query.anchors.return_value = [0., 0., 3., 0., 3., 0., 4., 0.]
        with patch.object(queries, '_typed_manim_observation', side_effect=_path_observation(query)) as observe, patch.object(
            queries, "_handle_for", return_value=handle
        ):
            self.assertEqual(value.get_num_curves(), 2)
            self.assertEqual(value.get_num_points(), 9)
            selected = value.get_nth_curve_points(0)
            if np is None:
                self.assertEqual(selected, [(0., 0.), (1., 2.), (2., 2.), (3., 0.)])
            else:
                np.testing.assert_array_equal(
                    selected,
                    [(0., 0.), (1., 2.), (2., 2.), (3., 0.)],
                )
            starts = value.get_start_anchors()
            ends = value.get_end_anchors()
            anchors = value.get_anchors()
            if np is None:
                self.assertEqual(starts, [(0., 0.), (3., 0.)])
                self.assertEqual(ends, [(3., 0.), (4., 0.)])
            else:
                np.testing.assert_array_equal(starts, [[0., 0.], [3., 0.]])
                np.testing.assert_array_equal(ends, [[3., 0.], [4., 0.]])
                self.assertTrue(all(isinstance(point, np.ndarray) for point in anchors))
            self.assertEqual(len(anchors), 4)
            columns = value.get_anchors_and_handles()
            if np is None:
                self.assertEqual(columns[1], [(1., 2.), (3.3, 0.)])
            else:
                np.testing.assert_array_equal(columns[1], [[1., 2.], [3.3, 0.]])
            self.assertEqual(observe.call_count, 8)
            self.assertEqual(query.free.call_count, 7)
            with self.assertRaises(AssertionError):
                value.get_nth_curve_points(4)
            self.assertEqual(query.free.call_count, 8)
            self.assertEqual(handle.cubicBezierPointRanges.call_count, 3)

    def test_nth_curve_points_match_negative_slice_and_assertion_semantics(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy curve point arrays")

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.pointCount = 9
        points = [(float(index), float(-index)) for index in range(9)]
        query.points.return_value = [coordinate for point in points for coordinate in point]
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = [0, 4, 4, 8]
        with patch.object(queries, "_typed_manim_observation", return_value=query), patch.object(
            queries, "_handle_for", return_value=handle
        ):
            empty = value.get_nth_curve_points(-1)
            self.assertIsInstance(empty, np.ndarray)
            self.assertEqual(empty.shape, (0, 2))
            selected = value.get_nth_curve_points(-2)
            self.assertIsInstance(selected, np.ndarray)
            np.testing.assert_array_equal(selected, points[1:5])
            with self.assertRaises(AssertionError):
                value.get_nth_curve_points(2)
            with self.assertRaisesRegex(TypeError, "slice indices must be integers"):
                value.get_nth_curve_points(0.5)

        self.assertEqual(query.points.call_count, 3)
        self.assertEqual(query.free.call_count, 7)
        self.assertEqual(handle.cubicBezierPointRanges.call_count, 4)

    def test_nth_curve_points_fall_back_to_point_list_without_numpy(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.pointCount = 4
        query.points.return_value = [0., 0., 1., 2., 2., 2., 3., 0.]
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = [0, 4]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_typed_manim_observation", return_value=query
        ), patch.object(queries, "_handle_for", return_value=handle):
            selected = value.get_nth_curve_points(0)

        self.assertEqual(selected, [(0., 0.), (1., 2.), (2., 2.), (3., 0.)])

    def test_curve_count_and_nth_points_follow_custom_stride(self):
        try:
            import numpy as np
        except ImportError:
            np = None

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 3
        query = Mock()
        query.curveCount = 2
        query.pointCount = 10
        points = [(float(index), float(-index)) for index in range(10)]
        query.points.return_value = [coordinate for point in points for coordinate in point]
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = [0, 3, 3, 6, 6, 9]

        with patch.object(queries, "_typed_manim_observation", return_value=query), patch.object(
            queries, "_handle_for", return_value=handle
        ):
            self.assertEqual(value.get_num_curves(), 3)
            selected = value.get_nth_curve_points(2)
            if np is None:
                self.assertEqual(selected, points[6:9])
            else:
                np.testing.assert_array_equal(selected, points[6:9])

        handle.cubicBezierPointRanges.assert_any_call(10, 3.0)
        self.assertEqual(query.free.call_count, 3)

    def test_curve_count_preserves_mutable_noninteger_stride_behavior(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 3.5
        query = Mock()
        query.pointCount = 10
        query.points.return_value = [float(index) for index in range(20)]
        with patch.object(queries, "_typed_manim_observation", return_value=query):
            self.assertEqual(value.get_num_curves(), 2.0)
            with self.assertRaisesRegex(TypeError, "slice indices must be integers"):
                value.get_nth_curve_points(0)
            value.n_points_per_cubic_curve = 0.0
            with self.assertRaises(ZeroDivisionError):
                value.get_num_curves()

        self.assertEqual(query.free.call_count, 4)

    def test_negative_nth_curve_functions_and_lengths_follow_point_slice(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy curve callable and error contracts")

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.pointCount = 8
        query.curvePointsAt.return_value = [1.0, 0.5, 0.0]
        query.curveLengthPieces.return_value = [2.25]
        query.curveLength.return_value = 2.25
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = [0, 4, 4, 8]
        axis_error = getattr(np, "AxisError", None) or np.exceptions.AxisError

        with patch.object(queries, "_typed_manim_observation", return_value=query), patch.object(
            queries, "_handle_for", return_value=handle
        ):
            function = value.get_nth_curve_function(-2)
            np.testing.assert_array_equal(function(0.5), [1.0, 0.5, 0.0])
            np.testing.assert_array_equal(
                value.get_nth_curve_length_pieces(-2, sample_points=2), [2.25]
            )
            self.assertEqual(value.get_nth_curve_length(-2, sample_points=2), 2.25)
            with_length, length = value.get_nth_curve_function_with_length(
                -2, sample_points=2
            )
            self.assertEqual(length, 2.25)
            np.testing.assert_array_equal(with_length(0.5), [1.0, 0.5, 0.0])

            empty = value.get_nth_curve_function(-1)
            with self.assertRaisesRegex(
                IndexError, "index 0 is out of bounds for axis 1 with size 0"
            ):
                empty(0.5)
            with self.assertRaisesRegex(
                axis_error, "axis 1 is out of bounds"
            ):
                value.get_nth_curve_length_pieces(-1, sample_points=0)
            with self.assertRaisesRegex(
                IndexError, "index 0 is out of bounds for axis 1 with size 0"
            ):
                value.get_nth_curve_length_pieces(-1, sample_points=2)

    def test_curve_length_queries_forward_sample_count_and_free_snapshots(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy contracts run in the pinned utility lane")

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.pointCount = 8
        query.curveLengthPieces.side_effect = lambda index, samples: (
            [] if samples == 0 else [0.25, 0.5, 0.75]
        )
        query.curveLength.return_value = 1.5
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            pieces = value.get_nth_curve_length_pieces(0)
            np.testing.assert_array_equal(pieces, [0.25, 0.5, 0.75])
            self.assertEqual(pieces.dtype, np.float64)
            query.curveLengthPieces.assert_called_once_with(0, None)
            axis_error = getattr(np, "AxisError", None) or np.exceptions.AxisError
            with self.assertRaisesRegex(axis_error, "axis 1 is out of bounds"):
                value.get_nth_curve_length_pieces(0, sample_points=0)
            query.curveLengthPieces.assert_called_once_with(0, None)
            np.testing.assert_array_equal(value.get_nth_curve_length_pieces(1, sample_points=4), [0.25, 0.5, 0.75])
            query.curveLengthPieces.assert_called_with(1, 4)
            length = value.get_nth_curve_length(0)
            self.assertEqual(length, np.float64(1.5))
            self.assertIsInstance(length, np.float64)
            query.curveLength.assert_called_once_with(0, None)
            with self.assertRaisesRegex(axis_error, "axis 1 is out of bounds"):
                value.get_nth_curve_length(0, sample_points=0)
            query.curveLength.assert_called_once_with(0, None)
            with self.assertRaisesRegex(axis_error, "axis 1 is out of bounds"):
                value.get_arc_length(0)
            query.curveLength.assert_called_once_with(0, None)
            self.assertEqual(query.free.call_count, 6)
            with self.assertRaises(AssertionError):
                value.get_nth_curve_length_pieces(2)
            self.assertEqual(query.free.call_count, 7)

        empty_query = Mock()
        empty_query.curveCount = 1
        empty_query.pointCount = 4
        with patch.object(
            queries,
            '_typed_manim_observation',
            return_value=empty_query,
        ) as observe:
            for samples in (-1, 2**32):
                with self.assertRaises(ValueError):
                    value.get_nth_curve_length(0, sample_points=samples)
            with self.assertRaises(TypeError):
                value.get_nth_curve_length_pieces(0, sample_points=2.5)
            with self.assertRaises(IndexError):
                value.get_nth_curve_length(-1)
        self.assertEqual(observe.call_count, 4)

    def test_custom_stride_curve_functions_and_lengths_use_point_slices(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy curve callable and length contracts")

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        query = Mock()
        query.curveCount = 1
        query.pointCount = 4
        query.bezierPointsAtRange.side_effect = lambda start, stop, parameters: [
            coordinate
            for parameter in parameters
            for coordinate in (
                start / 3 + parameter * ((stop - 1) / 3 - start / 3),
                0.0,
                0.0,
            )
        ]
        query.bezierLengthPiecesRange.side_effect = lambda start, stop, samples: [
            1.0 / 3.0
        ]
        handle = Mock()
        handle.cubicBezierPointRanges.return_value = [0, 2, 2, 4]

        with patch.object(
            queries,
            "_typed_manim_observation",
            return_value=query,
        ), patch.object(queries, "_handle_for", return_value=handle):
            self.assertEqual(value.get_num_curves(), 2)
            functions = list(value.get_curve_functions())
            self.assertEqual(
                [function.__name__ for function in functions],
                ["linear_bezier", "linear_bezier"],
            )
            np.testing.assert_allclose(functions[0](0.5), [1 / 6, 0, 0])
            np.testing.assert_allclose(functions[1](0.5), [5 / 6, 0, 0])
            function, length = value.get_nth_curve_function_with_length(
                1, sample_points=2
            )
            np.testing.assert_allclose(function(0.5), [5 / 6, 0, 0])
            self.assertAlmostEqual(float(length), 1 / 3)
            lengths = list(
                value.get_curve_functions_with_lengths(sample_points=2)
            )
            self.assertEqual(len(lengths), 2)
            self.assertAlmostEqual(float(lengths[0][1]), 1 / 3)
            self.assertAlmostEqual(float(lengths[1][1]), 1 / 3)
            self.assertAlmostEqual(
                float(value.get_arc_length(sample_points_per_curve=2)),
                2 / 3,
            )

        high_grade = identity_only_wrapper(compat.VMobject)
        high_grade.n_points_per_cubic_curve = 5
        high_grade_query = Mock()
        high_grade_query.pointCount = 5
        high_grade_query.bezierPointsAtRange.side_effect = (
            lambda start, stop, parameters: [
                coordinate
                for parameter in parameters
                for coordinate in (parameter, parameter, parameter)
            ]
        )
        with patch.object(
            queries,
            "_typed_manim_observation",
            return_value=high_grade_query,
        ):
            function = next(high_grade.get_curve_functions())
            self.assertEqual(function.__name__, "nth_grade_bezier")
            self.assertEqual(
                function(np.array([[0., .25], [.5, 1.]])).shape,
                (4, 3),
            )

        negative_stride = identity_only_wrapper(compat.VMobject)
        negative_stride.n_points_per_cubic_curve = -1
        negative_stride_query = Mock()
        negative_stride_query.pointCount = 4
        with patch.object(
            queries,
            "_typed_manim_observation",
            return_value=negative_stride_query,
        ):
            self.assertEqual(negative_stride.get_arc_length(-1), 0.0)

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
        query.pointCount = 8
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
        query.pointCount = 4
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            with self.assertRaises(AssertionError):
                value.get_nth_curve_function(1)
            query.free.assert_called_once()
            with self.assertRaisesRegex(
                TypeError,
                r"VMobject\.get_nth_curve_function_with_length\(\) got an unexpected keyword argument 'unsupported'",
            ):
                next(value.get_curve_functions_with_lengths(unsupported=True))
            empty = value.get_nth_curve_function(-1)
            with self.assertRaisesRegex(
                IndexError, "index 0 is out of bounds for axis 1 with size 0"
            ):
                empty(0.5)

    def test_zero_sample_length_matches_numpy_axis_error_only_for_existing_curves(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest("NumPy error contract")

        axis_error = getattr(np, "AxisError", None) or np.exceptions.AxisError
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.curveCount = 2
        query.pointCount = 8
        with patch.object(queries, "_typed_manim_observation", return_value=query):
            for probe in (
                lambda: value.get_nth_curve_function_with_length(0, sample_points=0),
                lambda: list(value.get_curve_functions_with_lengths(sample_points=0)),
                lambda: value.get_arc_length(0),
            ):
                with self.assertRaisesRegex(axis_error, "axis 1 is out of bounds"):
                    probe()
            self.assertEqual(query.free.call_count, 3)

            query.curveCount = 0
            query.pointCount = 0
            self.assertEqual(list(value.get_curve_functions_with_lengths(sample_points=-1)), [])
            self.assertEqual(list(value.get_curve_functions_with_lengths(sample_points=2.5)), [])
            self.assertEqual(list(value.get_curve_functions_with_lengths(unsupported=True)), [])
            self.assertEqual(value.get_arc_length(2.5), 0.0)
            self.assertEqual(query.free.call_count, 7)

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

    def test_direction_uses_current_point_stride_through_rust(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        query = Mock()
        query.directionWithStride.return_value = "CW"
        with patch.object(queries, "_typed_manim_observation", return_value=query):
            self.assertEqual(value.get_direction(), "CW")
        query.directionWithStride.assert_called_once_with(2)
        query.direction.assert_not_called()
        query.free.assert_called_once_with()

    def test_direction_rejects_zero_stride_before_reading_path(self):
        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 0
        with patch.object(queries, "_typed_manim_observation") as observe:
            with self.assertRaisesRegex(ValueError, "slice step cannot be zero"):
                value.get_direction()
        observe.assert_not_called()


class SubpathQueryTests(unittest.TestCase):
    def test_object_subpaths_follow_mutable_stride_over_rust_snapshot(self):
        try:
            import numpy as np
        except ImportError:
            np = None

        value = identity_only_wrapper(compat.VMobject)
        value.n_points_per_cubic_curve = 2
        query = Mock()
        query.points.return_value = [0., 0., 1., 0., 2., 0., 3., 0.]
        handle = Mock()
        handle.subpathRanges.return_value = [0, 2, 2, 4]

        with patch.object(
            queries, "_typed_manim_observation", return_value=query
        ), patch.object(queries, "_handle_for", return_value=handle):
            subpaths = value.get_subpaths()
            if np is None:
                self.assertEqual(
                    subpaths,
                    [[(0., 0.), (1., 0.)], [(2., 0.), (3., 0.)]],
                )
            else:
                self.assertTrue(all(isinstance(path, np.ndarray) for path in subpaths))
                np.testing.assert_array_equal(subpaths[0], [[0., 0.], [1., 0.]])
                np.testing.assert_array_equal(subpaths[1], [[2., 0.], [3., 0.]])

        query.free.assert_called_once_with()
        handle.subpathRanges.assert_called_once_with(
            [0., 0., 1., 0., 2., 0., 3., 0.],
            4,
            2,
            2,
            2.0,
            1e-6,
            True,
        )

    def test_subpaths_and_closure_free_the_snapshot_on_success_and_error(self):
        try:
            import numpy as np
        except ImportError:
            np = None

        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.subpaths.return_value = [[0., 0., 1., 0., 2., 0., 3., 0.], [5., 1., 6., 1., 7., 1., 8., 1.]]
        query.pointCount = 8
        query.isClosed.return_value = False
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            subpaths = value.get_subpaths()
            if np is None:
                self.assertEqual(
                    subpaths,
                    [
                        [(0., 0.), (1., 0.), (2., 0.), (3., 0.)],
                        [(5., 1.), (6., 1.), (7., 1.), (8., 1.)],
                    ],
                )
            else:
                self.assertTrue(
                    all(isinstance(path, np.ndarray) for path in subpaths)
                )
                np.testing.assert_array_equal(
                    subpaths[0], [[0., 0.], [1., 0.], [2., 0.], [3., 0.]]
                )
                np.testing.assert_array_equal(
                    subpaths[1], [[5., 1.], [6., 1.], [7., 1.], [8., 1.]]
                )
            self.assertFalse(value.is_closed())
            query.isClosed.assert_called_once_with(1e-6)
            self.assertEqual(query.free.call_count, 2)
            query.subpaths.side_effect = ValueError('bad content')
            with self.assertRaises(ValueError): value.get_subpaths()
            self.assertEqual(query.free.call_count, 3)
            query.pointCount = 0
            with self.assertRaisesRegex(
                IndexError, "index 0 is out of bounds for axis 0 with size 0"
            ):
                value.is_closed()
            self.assertEqual(query.isClosed.call_count, 1)
            self.assertEqual(query.free.call_count, 4)

    def test_object_subpaths_fall_back_to_point_lists_without_numpy(self):
        value = identity_only_wrapper(compat.VMobject)
        query = Mock()
        query.subpaths.return_value = [[0., 0., 1., 0., 2., 0., 3., 0.]]
        with patch.dict("sys.modules", {"numpy": None}), patch.object(
            queries, "_typed_manim_observation", return_value=query
        ):
            self.assertEqual(
                value.get_subpaths(),
                [[(0., 0.), (1., 0.), (2., 0.), (3., 0.)]],
            )

    def test_is_closed_passes_mutable_tolerance_to_the_shared_path_query(self):
        value = identity_only_wrapper(compat.VMobject)
        value.tolerance_for_point_equality = 2e-5
        query = Mock()
        query.pointCount = 4
        query.isClosed.return_value = True
        with patch.object(queries, '_typed_manim_observation', return_value=query):
            self.assertTrue(value.is_closed())
        query.isClosed.assert_called_once_with(2e-5)
        query.free.assert_called_once_with()

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
