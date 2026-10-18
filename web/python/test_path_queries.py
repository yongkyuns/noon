import gc
import inspect
import unittest
from unittest.mock import Mock, patch
import _manim_compat as compat
import _manim_path_queries as queries
import _manim_semantic_handles as semantic_handles
from _typed_geometry_test_support import identity_only_wrapper


class PathQueryTests(unittest.TestCase):
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

    def test_vmobject_constructor_accepts_query_stride_and_tolerance(self):
        import numpy as np

        with patch.object(semantic_handles, "_empty_path_init") as initialize:
            value = compat.VMobject(
                n_points_per_cubic_curve=3,
                tolerance_for_point_equality=2e-5,
                marker="forwarded",
            )

        self.assertEqual(value.n_points_per_cubic_curve, 3)
        self.assertEqual(value.n_points_per_curve, 3)
        self.assertEqual(value.tolerance_for_point_equality, 2e-5)
        np.testing.assert_array_equal(value._bezier_t_values, [0., .5, 1.])
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
            "_path_boundary_extremum_along_dim_observation",
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
        query.pointCount = 4
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
        self.assertEqual(query.free.call_count, 6)

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
        with patch.object(queries, "_typed_manim_observation", return_value=query) as observe:
            self.assertEqual(
                value.get_start_and_end(),
                (noon.Vec2(1.0, 2.0), noon.Vec2(3.0, 4.0)),
            )
            self.assertEqual(observe.call_count, 4)
            self.assertEqual(query.start.call_count, 1)
            self.assertEqual(query.end.call_count, 1)
            self.assertEqual(query.free.call_count, 4)

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

        root = Mock()
        root.has_points.return_value = False
        leaf = Mock(get_num_points=Mock(return_value=4), submobjects=[])
        second_leaf = Mock(get_num_points=Mock(return_value=2), submobjects=[])
        empty = Mock(get_num_points=Mock(return_value=0), submobjects=[])
        nested = Mock(get_num_points=Mock(return_value=0), submobjects=[empty])
        root.submobjects = [nested, empty, leaf, second_leaf]

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

    def test_base_point_count_queries_share_the_path_snapshot_contract(self):
        import noon

        value = identity_only_wrapper(noon.Mobject)
        query = Mock()
        query.pointCount = 4
        with patch.object(queries, "_typed_manim_observation", return_value=query) as observe:
            self.assertEqual(value.get_num_points(), 4)
            self.assertTrue(value.has_points())
            self.assertFalse(value.has_no_points())
            query.pointCount = 0
            self.assertFalse(value.has_points())
            self.assertTrue(value.has_no_points())
        self.assertEqual(observe.call_count, 5)
        self.assertEqual(query.free.call_count, 5)

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
