import unittest
from unittest.mock import Mock, patch

import noon


class ManimMobjectBasePathMethodTests(unittest.TestCase):
    def test_default_critical_query_reads_only_three_rust_result_coordinates(self):
        from types import SimpleNamespace
        from _manim_path_queries import critical_point
        from _typed_geometry_test_support import identity_only_wrapper
        value = identity_only_wrapper(noon.Mobject)
        value.dim = 3
        value._semantic_handle = SimpleNamespace(
            semanticSlot=1, semanticGeneration=0,
            familyKeys=Mock(return_value=['1:0']),
            allPoints3D=Mock(side_effect=AssertionError("geometry must stay in Rust")),
            criticalPoint3D=Mock(return_value=(3., 2., 11.125)))
        value._semantic_handle_fresh = True
        result = critical_point(value, (1., -1., 1.))
        self.assertEqual(list(result), [3., 2., 11.125])
        value._semantic_handle.criticalPoint3D.assert_called_once_with(1., -1., 1., False)
        value._semantic_handle.allPoints3D.assert_not_called()

    def test_caller_direction_indexing_keeps_the_snapshot_and_exact_access_order(self):
        from types import SimpleNamespace
        from _manim_path_queries import critical_point
        from _typed_geometry_test_support import identity_only_wrapper
        value = identity_only_wrapper(noon.Mobject)
        value.dim = 3
        calls = []
        class Direction:
            def __getitem__(self, index):
                calls.append(index)
                return (1., -1., 1.)[index]
        value._semantic_handle = SimpleNamespace(
            pointArray=Mock(return_value=((1., 2., -7., 3., 4., 11.125), (2, 3))),
            criticalPoint3D=Mock(side_effect=AssertionError("caller direction hook")),
            extremumFromPoints=Mock(side_effect=[3., 2., 11.125]))
        value._semantic_handle_fresh = True
        self.assertEqual(list(critical_point(value, Direction())), [3., 2., 11.125])
        self.assertEqual(calls, [0, 1, 2])
        value._semantic_handle.criticalPoint3D.assert_not_called()
        value._semantic_handle.pointArray.assert_called_once_with()

    def test_point_result_formatter_keeps_xyz_and_explicit_planar_inputs(self):
        from _manim_compat import _manim_point3
        self.assertEqual(list(_manim_point3((1., 2., 7.125))), [1., 2., 7.125])
        self.assertEqual(list(_manim_point3(noon.Vec2(1., 2.))), [1., 2., 0.])

    def test_base_point_setter_forwards_to_the_shared_rust_adapter(self):
        mobject = object.__new__(noon.Mobject)
        points = [[1., 2., 3.]]
        with patch("_manim_path_editing.set_points", return_value=mobject) as edit:
            self.assertIs(mobject.set_points(points), mobject)
        edit.assert_called_once_with(mobject, points)
        self.assertNotIn("points", mobject.__dict__)

    def test_base_drawing_intent_is_set_on_inert_constructor_options(self):
        import _manim_semantic_handles as handles
        for kind in ("Mobject", "VMobject"):
            options = Mock()
            factory = Mock()
            factory.emptyPath.return_value = options
            mobject = object.__new__(noon.Mobject)
            with patch.object(handles, "_geometry_options", factory), \
                 patch.object(handles, "_apply_shared_constructor_options"), \
                 patch.object(handles, "_attach_geometry_options") as attach:
                handles._empty_path_init(mobject, color=noon.WHITE, kind=kind)
            attach.assert_called_once_with(mobject, options, kind)
            if kind == "Mobject":
                options.setDrawEnabled.assert_called_once_with(False)
            else:
                options.setDrawEnabled.assert_not_called()

    def test_xyz_critical_point_preserves_one_snapshot_and_python_extremum_hooks(self):
        try:
            import numpy as np
        except ImportError:
            np = None
        from _manim_path_queries import critical_point
        mobject = object.__new__(noon.Mobject)
        mobject.dim = 3
        points = [[1., 2., -7.25], [3., 4., 11.125]]
        if np is not None:
            points = np.array(points)
        mobject.get_points_defining_boundary = Mock(return_value=points)
        mobject.get_extremum_along_dim = Mock(side_effect=[3., 2., 11.125])
        result = critical_point(mobject, [1., -1., 1.])
        self.assertEqual(list(result), [3., 2., 11.125])
        if np is not None:
            np.testing.assert_array_equal(result, [3., 2., 11.125])
            self.assertEqual(result.dtype, np.dtype("float64"))
        else:
            self.assertIs(type(result), list)
        mobject.get_points_defining_boundary.assert_called_once_with()
        calls = mobject.get_extremum_along_dim.call_args_list
        self.assertEqual([(call.kwargs["dim"], call.kwargs["key"]) for call in calls],
                         [(0, 1.), (1, -1.), (2, 1.)])
        self.assertTrue(all(call.args[0] is points for call in calls))

    def test_empty_xyz_critical_point_does_not_index_a_direction_or_reduce(self):
        try:
            import numpy as np
        except ImportError:
            np = None
        from _manim_path_queries import critical_point
        mobject = object.__new__(noon.Mobject)
        mobject.dim = 3
        mobject.get_points_defining_boundary = Mock(return_value=[] if np is None else np.empty((0,)))
        mobject.get_extremum_along_dim = Mock(side_effect=AssertionError("empty reduction"))
        result = critical_point(mobject, object())
        self.assertEqual(list(result), [0., 0., 0.])
        if np is not None:
            np.testing.assert_array_equal(result, [0., 0., 0.])
            self.assertEqual(result.dtype, np.dtype("float64"))
        else:
            self.assertIs(type(result), list)
        mobject.get_extremum_along_dim.assert_not_called()

    def test_z_coordinate_and_zenith_nadir_keep_the_requested_direction(self):
        mobject = object.__new__(noon.Mobject)
        mobject.get_extremum_along_dim = Mock(return_value=11.125)
        self.assertEqual(mobject.get_z([0., 0., 1.]), 11.125)
        mobject.get_extremum_along_dim.assert_called_once_with(dim=2, key=1.)
        mobject.get_critical_point = Mock()
        mobject.get_zenith()
        mobject.get_nadir()
        self.assertEqual([call.args[0] for call in mobject.get_critical_point.call_args_list],
                         [(0., 0., 1.), (0., 0., -1.)])

    def test_repeat_submobject_returns_the_member_copy(self):
        parent = object.__new__(noon.Mobject)
        member = Mock()
        copied = object()
        member.copy.return_value = copied

        self.assertIs(parent.repeat_submobject(member), copied)
        member.copy.assert_called_once_with()

    def test_base_path_queries_raise_the_pinned_override_error(self):
        mobject = object.__new__(noon.Mobject)

        with self.assertRaisesRegex(
            NotImplementedError, "^Please override in a child class\\.$"
        ):
            mobject.point_from_proportion(0.5)
        with self.assertRaisesRegex(
            NotImplementedError, "^Please override in a child class\\.$"
        ):
            mobject.proportion_from_point((0.0, 0.0, 0.0))


if __name__ == "__main__":
    unittest.main()
