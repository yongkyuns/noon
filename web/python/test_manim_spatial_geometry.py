import unittest
from unittest.mock import Mock, patch

import noon
import _manim_spatial_geometry as spatial
from _noon_spatial import Mesh3D


class SpatialGeometryAdapterTests(unittest.TestCase):
    def setUp(self):
        self.mesh = Mock()
        self.mesh._semantic_handle = object()

    def test_exports_are_bounded_mesh_wrappers(self):
        for name in ("Surface", "Sphere", "Dot3D", "Cube", "Cylinder", "Prism", "Line3D", "Torus", "Cone"):
            self.assertIs(getattr(noon, name), getattr(spatial, name))

    def test_sphere_uses_native_mesh_and_rust_world_center(self):
        with patch.object(Mesh3D, "sphere", return_value=self.mesh) as factory:
            sphere = spatial.Sphere(center=(1, 2, 3), radius=2, resolution=(12, 8),
                                    checkerboard_colors=False, stroke_width=0,
                                    shade_in_3d=False)
        factory.assert_called_once()
        self.assertEqual(factory.call_args.args, (2.0,))
        self.assertEqual(factory.call_args.kwargs["resolution"], (12, 8))
        self.mesh.move_to.assert_called_once_with((1, 2, 3))
        self.assertIs(sphere._semantic_handle, self.mesh._semantic_handle)

    def test_surface_defaults_use_one_rust_cell_family_and_sample_callback_once(self):
        import _noon_spatial as native

        parameters = (0, 0, 0, 1, 1, 0, 1, 1)
        points = (0, 0, 0, 0, 1, 0, 1, 0, 0, 1, 1, 1)
        candidate = Mock()
        handle = Mock()

        class Plan:
            def parameters(self):
                return parameters

            def finishCells(self, sampled, normals):
                self.sampled = sampled
                self.normals = normals
                return candidate

            def free(self):
                self.freed = True

        plan = Plan()
        sampled = []
        callback = lambda u, v: sampled.append((u, v)) or (u, v, u * v)
        with patch.object(native, "_surface_plan", return_value=plan), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(native, "_create_mesh_family", return_value=handle), \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            surface = spatial.Surface(callback, u_range=(-2, 2), v_range=(0, 1),
                                      resolution=(1, 1), shade_in_3d=False,
                                      point_lit=True)

        self.assertEqual(len(sampled), 4)
        self.assertEqual(plan.sampled, points)
        self.assertEqual(plan.normals, ())
        self.assertTrue(plan.freed)
        candidate.setCheckerboard.assert_called_once_with(
            (noon.BLUE_D.red, noon.BLUE_D.green, noon.BLUE_D.blue, noon.BLUE_D.alpha),
            (noon.BLUE_E.red, noon.BLUE_E.green, noon.BLUE_E.blue, noon.BLUE_E.alpha),
            1.0,
        )
        stroke = noon.color_from_hex(0xBBBBBB)
        candidate.setStroke.assert_called_once_with(
            stroke.red, stroke.green, stroke.blue, stroke.alpha, 0.005, 1.0,
        )
        candidate.setPointLit.assert_called_once_with(True)
        self.assertIs(surface._semantic_handle, handle)

    def test_surface_checkerboard_and_world_scale_use_shared_family_handle(self):
        import _noon_spatial as native
        handle = Mock()
        surface = object.__new__(spatial.Surface)
        from _manim_semantic_handles import _attach_shared_handle
        _attach_shared_handle(surface, handle)
        with patch.object(native, "_bulk", side_effect=lambda values: tuple(values)):
            surface.set_fill_by_checkerboard((noon.RED, noon.BLUE), opacity=0.25)
        handle.setFillByCheckerboard.assert_called_once()
        args = handle.setFillByCheckerboard.call_args.args
        self.assertEqual(args[0], (noon.RED.red, noon.RED.green, noon.RED.blue, noon.RED.alpha))
        self.assertEqual(args[1], (noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha))
        self.assertEqual(args[2], 0.25)
        with patch.object(native, "_bulk", side_effect=lambda values: tuple(values)):
            surface.scale(2, about_point=noon.ORIGIN)
        handle.scaleWorld.assert_called_once_with(2.0, (0.0, 0.0, 0.0))

    def test_surface_checkerboard_uses_canonical_owner_when_live(self):
        import _noon_spatial as native
        from _manim_semantic_handles import _attach_shared_handle
        context = Mock()
        handle = Mock()
        surface = object.__new__(spatial.Surface)
        _attach_shared_handle(surface, handle)
        with patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(spatial, "_live_mutation_context", return_value=context):
            surface.set_fill_by_checkerboard((noon.RED, noon.BLUE), opacity=0.25)
        context.setSurfaceCheckerboard.assert_called_once()
        self.assertIs(context.setSurfaceCheckerboard.call_args.args[0], handle)
        handle.setFillByCheckerboard.assert_not_called()

    def test_explicit_unshaded_tipless_surface_keeps_the_single_mesh_path(self):
        callback = lambda u, v: (u, v, 0)
        with patch.object(Mesh3D, "parametric", return_value=self.mesh) as factory:
            surface = spatial.Surface(callback, resolution=(4, 3),
                                      checkerboard_colors=False, stroke_width=0,
                                      shade_in_3d=False)
        factory.assert_called_once_with(
            callback, u_range=(0.0, 1.0), v_range=(0.0, 1.0),
            resolution=(4, 3), normal=None,
            color=spatial._color(noon.BLUE_D), point_lit=False,
        )
        self.assertIs(surface._semantic_handle, self.mesh._semantic_handle)

    def test_renderer_light_proxy_reuses_the_scene_point_light(self):
        from types import SimpleNamespace
        from _noon_spatial import _RendererCameraProxy, _WorldMobject
        light = object.__new__(_WorldMobject)
        from _manim_semantic_handles import _attach_shared_handle
        _attach_shared_handle(light, Mock())
        light._semantic_point_light = True
        scene = SimpleNamespace(
            _manim_camera_light_source=None,
            add_world_mobjects=Mock(),
            _edit_membership=Mock(),
        )
        def point_light(**kwargs):
            scene.point_light_calls.append(kwargs)
            scene._manim_camera_light_source = light
            return light
        scene.point_light_calls = []
        scene.point_light = point_light
        camera = _RendererCameraProxy(scene)
        self.assertIs(camera.light_source, light)
        self.assertEqual(scene.point_light_calls, [{"position": (-7, -9, 10)}])
        self.assertEqual(scene.add_world_mobjects.call_count, 1)
        self.assertIs(camera.light_source, light)
        self.assertEqual(len(scene.point_light_calls), 1)
        self.assertEqual(scene.add_world_mobjects.call_count, 2)

        replacement = object.__new__(_WorldMobject)
        _attach_shared_handle(replacement, Mock())
        replacement._semantic_point_light = True
        camera.light_source = replacement
        scene.add_world_mobjects.assert_called_with(replacement)
        scene._edit_membership.assert_called_once_with("remove", (light,))

    def test_cylinder_requires_closed_z_profile_and_centers_native_mesh(self):
        with patch.object(Mesh3D, "cylinder", return_value=self.mesh) as factory:
            cylinder = spatial.Cylinder(height=4, resolution=(16, 16),
                                        checkerboard_colors=False, stroke_width=0,
                                        shade_in_3d=False)
        factory.assert_called_once()
        self.mesh.shift.assert_called_once_with((0, 0, -2.0))
        self.assertIs(cylinder._semantic_handle, self.mesh._semantic_handle)

    def test_cairo_decorations_and_non_native_options_fail_explicitly(self):
        with self.assertRaises(TypeError):
            spatial.Surface(lambda u, v: (u, v, 0), surface_piece_config={})
        with self.assertRaises(TypeError):
            spatial.Surface(lambda u, v: (u, v, 0), shade_in_3d=1)
        with self.assertRaisesRegex(NotImplementedError, "Manim Cairo Surface shading"):
            spatial.Surface(lambda u, v: (u, v, 0))
        with self.assertRaises(TypeError):
            spatial.Surface(lambda u, v: (u, v, 0), shade_in_3d=False, point_lit=1)
        with self.assertRaises(NotImplementedError):
            spatial.Sphere(u_range=(0, 1))

    def test_axial_direction_caps_opacity_and_strokes_are_not_silently_approximated(self):
        with patch.object(Mesh3D, "cylinder") as factory:
            with self.assertRaises(NotImplementedError):
                spatial.Cylinder(direction=(0, 1, 0))
            with self.assertRaises(NotImplementedError):
                spatial.Cylinder(show_ends=False)
            with self.assertRaises(NotImplementedError):
                spatial.Cylinder(fill_opacity=0.5)
            with self.assertRaises(NotImplementedError):
                spatial.Cube(fill_opacity=1, stroke_width=1)
            with self.assertRaises(NotImplementedError):
                spatial.Cube(fill_opacity=1, stroke_width=0)
            with self.assertRaises(NotImplementedError):
                spatial.Prism(fill_opacity=1, stroke_width=0)
            with self.assertRaises(NotImplementedError):
                spatial.Cube()
            with self.assertRaises(NotImplementedError):
                spatial.Cone()
            factory.assert_not_called()

    def test_cube_and_prism_require_explicit_cairo_shading_opt_out(self):
        with patch.object(Mesh3D, "cube", return_value=self.mesh) as cube_factory:
            cube = spatial.Cube(fill_opacity=1, shade_in_3d=False)
        cube_factory.assert_called_once_with(2.0, color=spatial._color(noon.BLUE))
        self.assertIs(cube._semantic_handle, self.mesh._semantic_handle)

        with patch.object(Mesh3D, "prism", return_value=self.mesh) as prism_factory:
            prism = spatial.Prism(fill_opacity=1, shade_in_3d=False)
        prism_factory.assert_called_once_with((3, 2, 1), color=spatial._color(noon.BLUE))
        self.assertIs(prism._semantic_handle, self.mesh._semantic_handle)

    def test_capped_cone_profile_is_explicit_and_uses_native_mesh(self):
        with patch.object(Mesh3D, "cone", return_value=self.mesh) as factory:
            cone = spatial.Cone(show_base=True, height=2, resolution=12,
                                stroke_width=0, shade_in_3d=False)
        factory.assert_called_once()
        self.assertEqual(factory.call_args.args[:3], (1.0, 2.0, 12))
        self.mesh.shift.assert_called_once_with((0, 0, -2.0))
        self.assertIs(cone._semantic_handle, self.mesh._semantic_handle)


if __name__ == "__main__":
    unittest.main()
