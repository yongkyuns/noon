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

    def test_surface_callback_is_passed_to_existing_native_sampler(self):
        callback = lambda u, v: (u, v, u * v)
        with patch.object(Mesh3D, "parametric", return_value=self.mesh) as factory:
            surface = spatial.Surface(callback, u_range=(-2, 2), v_range=(0, 1),
                                     resolution=(8, 4), checkerboard_colors=False,
                                     stroke_width=0, shade_in_3d=False)
        self.assertIs(surface._semantic_handle, self.mesh._semantic_handle)
        self.assertIs(factory.call_args.args[0], callback)
        self.assertEqual(factory.call_args.kwargs["resolution"], (8, 4))
        self.assertEqual(factory.call_args.kwargs["u_range"], (-2, 2))

    def test_cylinder_requires_closed_z_profile_and_centers_native_mesh(self):
        with patch.object(Mesh3D, "cylinder", return_value=self.mesh) as factory:
            cylinder = spatial.Cylinder(height=4, resolution=(16, 16),
                                        checkerboard_colors=False, stroke_width=0,
                                        shade_in_3d=False)
        factory.assert_called_once()
        self.mesh.shift.assert_called_once_with((0, 0, -2.0))
        self.assertIs(cylinder._semantic_handle, self.mesh._semantic_handle)

    def test_cairo_decorations_and_non_native_options_fail_explicitly(self):
        with patch.object(Mesh3D, "parametric") as factory:
            with self.assertRaises(NotImplementedError):
                spatial.Surface(lambda u, v: (u, v, 0), checkerboard_colors=(noon.BLUE, noon.RED))
            with self.assertRaises(NotImplementedError):
                spatial.Surface(lambda u, v: (u, v, 0), stroke_width=0.5)
            with self.assertRaises(NotImplementedError):
                spatial.Surface(lambda u, v: (u, v, 0), shade_in_3d=True)
            with self.assertRaises(TypeError):
                spatial.Surface(lambda u, v: (u, v, 0), surface_piece_config={})
            with self.assertRaises(NotImplementedError):
                spatial.Sphere(u_range=(0, 1))
            factory.assert_not_called()

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
