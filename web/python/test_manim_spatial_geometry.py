import math
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch, call

import noon
import _manim_spatial_geometry as spatial
from _noon_spatial import Mesh3D
import _manim_compat as compat
import _manim_semantic_handles as semantic


def _family_handle(members=()):
    handle = Mock()
    members = tuple(members)
    handle.memberKeys.return_value = [
        f"{member.semanticSlot}:{member.semanticGeneration}" for member in members
    ]
    handle.memberIsFamily.return_value = False
    handle.memberMobject.side_effect = members.__getitem__
    return handle


def _family_surface(handle):
    surface = object.__new__(spatial.Surface)
    from _manim_semantic_handles import _attach_shared_family, _initialize_shared_wrapper
    if not handle.memberKeys():
        member = SimpleNamespace(semanticSlot=17, semanticGeneration=2)
        handle.memberKeys.return_value = ["17:2"]
        handle.memberIsFamily.return_value = False
        handle.memberMobject.side_effect = None
        handle.memberMobject.return_value = member
    _initialize_shared_wrapper(surface)
    _attach_shared_family(surface, handle, leaf_type=compat.VMobject)
    return surface


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
        self.assertEqual(factory.call_args.kwargs["u_range"], (0.0, 2 * math.pi))
        self.assertEqual(factory.call_args.kwargs["v_range"], (0.0, math.pi))
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
        member = SimpleNamespace(semanticSlot=17, semanticGeneration=2)
        family = _family_handle((member,))
        with patch.object(native, "_surface_plan", return_value=plan), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(native, "_create_mesh_family", return_value=family), \
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
        candidate.setCairoSurface.assert_called_once_with(False)
        self.assertIsInstance(surface, compat.Group)
        self.assertIs(surface._semantic_family_handle, family)
        self.assertIs(surface.submobjects[0]._semantic_handle, member)

    def test_surface_checkerboard_and_world_scale_use_shared_family_handle(self):
        import _noon_spatial as native
        handle = _family_handle()
        surface = _family_surface(handle)
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

    def test_surface_family_membership_and_cold_world_affines(self):
        import _noon_spatial as native
        handle = _family_handle()
        surface = _family_surface(handle)
        axes_handle = Mock()
        axes = object.__new__(noon.Mobject)
        from _manim_semantic_handles import _attach_shared_handle
        _attach_shared_handle(axes, axes_handle)

        batch = Mock()
        with patch.object(semantic, "_new_membership_batch", return_value=batch):
            semantic._family_membership_batch(None, "add", (axes, surface))
        batch.appendMobject.assert_called_once_with("", axes_handle)
        batch.appendFamily.assert_called_once_with(handle)

        with patch.object(native, "_bulk", side_effect=lambda values: tuple(values)):
            surface.shift((1, 2, 3))
            surface.rotate(0.5, axis=(0, 0, 1), about_point=(1, 2, 3))
            surface.scale(2, about_point=(1, 2, 3))
        handle.shiftWorld.assert_called_once_with(1.0, 2.0, 3.0)
        handle.rotateWorld.assert_called_once_with(0.5, (0.0, 0.0, 1.0), (1.0, 2.0, 3.0))
        handle.scaleWorld.assert_called_once_with(2.0, (1.0, 2.0, 3.0))
        self.assertEqual(semantic._family_member_handle(surface), ("family", handle))

    def test_surface_family_world_affines_and_style_use_live_owner(self):
        import _noon_spatial as native
        handle = _family_handle()
        surface = _family_surface(handle)
        context = Mock()
        context.liveExecutionOwnership.return_value = "active"
        surface.submobjects[0]._scene = SimpleNamespace(
            _canonical_authoring_context=context,
        )
        with patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(semantic, "_live_constructor_context", return_value=None):
            surface.shift((1, 2, 3))
            surface.rotate(0.5, axis=(0, 0, 1), about_point=(1, 2, 3))
            surface.scale(2, about_point=(1, 2, 3))
            surface.set_style(fill_color=noon.RED, stroke_color=noon.BLUE)
            surface.set_color(noon.PURPLE)
            surface.set_opacity(0.5)
            surface.set_fill_by_checkerboard((noon.RED, noon.BLUE), opacity=0.25)
        context.shiftFamilyWorld.assert_called_once_with(handle, 1.0, 2.0, 3.0)
        context.rotateFamilyWorld.assert_called_once_with(
            handle, 0.0, 0.0, 1.0, 0.5, (1.0, 2.0, 3.0),
        )
        context.scaleFamilyWorld.assert_called_once_with(handle, 2.0, (1.0, 2.0, 3.0))
        context.liveSetFamilyStyle.assert_called_once()
        context.liveSetFamilyColor.assert_called_once()
        context.liveSetFamilyOpacity.assert_called_once()
        context.setSurfaceCheckerboard.assert_called_once_with(
            handle,
            (noon.RED.red, noon.RED.green, noon.RED.blue, noon.RED.alpha),
            (noon.BLUE.red, noon.BLUE.green, noon.BLUE.blue, noon.BLUE.alpha),
            0.25,
        )
        self.assertEqual(semantic._family_member_handle(surface), ("family", handle))
        handle.worldFamilyCenter.return_value = [1.0, 2.0, 3.0]
        with patch.object(native, "_group_target_context", return_value=None):
            self.assertEqual(surface.get_center(), (1.0, 2.0, 3.0))
        handle.worldFamilyCenter.assert_called_once_with()
        with self.assertRaisesRegex(NotImplementedError, "world transforms"):
            _ = surface.world_transform
        for name in ("id", "geometry", "transform", "style"):
            with self.subTest(property=name), self.assertRaises(AttributeError):
                getattr(surface, name)

    def test_surface_checkerboard_uses_canonical_owner_when_live(self):
        import _noon_spatial as native
        context = Mock()
        context.liveExecutionOwnership.return_value = "active"
        handle = _family_handle()
        surface = _family_surface(handle)
        surface.submobjects[0]._scene = SimpleNamespace(
            _canonical_authoring_context=context,
        )
        with patch.object(native, "_bulk", side_effect=lambda values: tuple(values)):
            surface.set_fill_by_checkerboard((noon.RED, noon.BLUE), opacity=0.25)
        context.setSurfaceCheckerboard.assert_called_once()
        self.assertIs(context.setSurfaceCheckerboard.call_args.args[0], handle)
        handle.setFillByCheckerboard.assert_not_called()

    def test_explicit_unshaded_tipless_surface_keeps_the_single_mesh_path(self):
        callback = lambda u, v: (u, v, 0)
        mesh_handle = Mock()
        mesh_handle.worldCenter.return_value = (1.0, 2.0, 3.0)
        self.mesh._semantic_handle = mesh_handle
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
        self.assertEqual(semantic._family_member_handle(surface),
                         ("mobject", self.mesh._semantic_handle))
        from _noon_spatial import _WorldMobject
        self.assertIs(spatial.Surface.__mro__[1], Mesh3D)
        self.assertIs(spatial.Surface.get_center, _WorldMobject.get_center)
        self.assertEqual(surface.get_center(), (1.0, 2.0, 3.0))
        mesh_handle.worldCenter.assert_called_once()

        context = Mock()
        context.liveExecutionOwnership.return_value = "active"
        surface._canonical_live_target_context = None
        surface._scene = SimpleNamespace(_canonical_authoring_context=context)
        surface._object = SimpleNamespace(id=42)
        surface.set_style(fill_color=noon.RED)
        context.liveSetStyle.assert_called_once()
        context.liveSetFamilyStyle.assert_not_called()

        mesh_handle.width.return_value = 2.5
        mesh_handle.height.return_value = 1.25
        mesh_handle.criticalX.return_value = 0.75
        mesh_handle.criticalY.return_value = -0.5
        live_layout = context.queryMobjectLayout.return_value
        live_layout.width = 2.5
        live_layout.height = 1.25
        live_layout.criticalX.return_value = 0.75
        live_layout.criticalY.return_value = -0.5
        self.assertEqual(surface.width, 2.5)
        self.assertEqual(surface.height, 1.25)
        self.assertEqual(surface.get_critical_point(noon.RIGHT), noon.Vec2(0.75, -0.5))
        self.assertEqual(surface.id, 42)

        raw = SimpleNamespace(geometry={"mesh": True}, transform={"scale": 2},
                              style={"opacity": 0.5})
        operations = SimpleNamespace(_current_raw=Mock(return_value=raw))
        with patch.object(noon, "_semantic_operations", return_value=operations):
            self.assertEqual(surface.geometry, raw.geometry)
            self.assertEqual(surface.transform, raw.transform)
            self.assertEqual(surface.style, raw.style)

        target_handle = Mock()
        target = object.__new__(spatial.Surface)
        semantic._attach_shared_handle(target, target_handle)
        target._scene = surface._scene
        target._object = SimpleNamespace(id=43)
        target._canonical_live_target_context = None
        surface.set_color(noon.BLUE)
        surface.set_opacity(0.4)
        surface.match_style(target)
        context.liveSetColor.assert_called_once()
        context.liveSetObjectOpacity.assert_called_once_with(mesh_handle, 0.4)
        context.liveMatchStyle.assert_called_once_with(mesh_handle, target_handle)

        with patch("_manim_geometry._mobject_get_color", return_value=noon.BLUE) as get_color:
            self.assertIs(surface.get_color(), noon.BLUE)
        get_color.assert_called_once_with(surface)

        target_clone = object()
        operations = SimpleNamespace(_target_mobject=Mock(return_value=target_clone))
        with patch.object(noon, "_semantic_operations", return_value=operations):
            self.assertIs(surface._copy_for_animate_target(), target_clone)
        operations._target_mobject.assert_called_once_with(surface)

        clone = object()
        with patch.object(semantic, "_clone_mobject", return_value=clone) as clone_mobject:
            self.assertIs(semantic._copy_nested_mobject(surface, None, {}), clone)
        clone_mobject.assert_called_once_with(surface, context_override=None, memo={})

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

    def test_cylinder_centers_and_orients_inert_native_mesh_before_admission(self):
        with patch.object(Mesh3D, "cylinder", return_value=self.mesh) as factory:
            cylinder = spatial.Cylinder(height=4, direction=(1, -2, 3), resolution=(16, 16),
                                        v_range=(math.pi / 4, math.pi),
                                        checkerboard_colors=False, stroke_width=0,
                                        shade_in_3d=False)
        factory.assert_called_once()
        self.assertIs(factory.call_args.kwargs["show_ends"], True)
        self.assertEqual(factory.call_args.kwargs["direction"], (1, -2, 3))
        self.assertEqual(factory.call_args.kwargs["axial_offset"], -2.0)
        self.assertEqual(factory.call_args.kwargs["v_range"], (math.pi / 4, math.pi))
        self.mesh.shift.assert_not_called()
        self.assertIs(cylinder._semantic_handle, self.mesh._semantic_handle)

    def test_cairo_decorations_and_non_native_options_fail_explicitly(self):
        with self.assertRaises(TypeError):
            spatial.Surface(lambda u, v: (u, v, 0), surface_piece_config={})
        with self.assertRaises(TypeError):
            spatial.Surface(lambda u, v: (u, v, 0), shade_in_3d=1)
        with self.assertRaises(TypeError):
            spatial.Surface(lambda u, v: (u, v, 0), shade_in_3d=False, point_lit=1)
        with self.assertRaisesRegex(ValueError, "distinct Surface materials"):
            spatial.Surface(lambda u, v: (u, v, 0), shade_in_3d=True, point_lit=True)
        with self.assertRaisesRegex(NotImplementedError, "custom normal callbacks"):
            spatial.Surface(lambda u, v: (u, v, 0), normal=lambda u, v: (0, 0, 1))

    def test_cairo_shaded_surface_samples_closed_path_controls_and_selects_its_material(self):
        import _noon_spatial as native

        parameters = tuple(float(value) for _ in range(16) for value in (0, 0))
        candidate = Mock()
        handle = Mock()

        class Plan:
            def cairoParameters(self):
                return parameters

            def finishCairoCells(self, sampled):
                self.sampled = sampled
                return candidate

            def free(self):
                self.freed = True

        plan = Plan()
        sampled = []
        callback = lambda u, v: sampled.append((u, v)) or (u, v, u * u + v * v)
        member = SimpleNamespace(semanticSlot=17, semanticGeneration=2)
        family = _family_handle((member,))
        with patch.object(native, "_surface_plan", return_value=plan), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(native, "_create_mesh_family", return_value=family), \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            surface = spatial.Surface(callback, resolution=(1, 1), shade_in_3d=True)

        self.assertEqual(len(sampled), 16)
        self.assertEqual(len(plan.sampled), 48)
        self.assertTrue(plan.freed)
        candidate.setPointLit.assert_called_once_with(False)
        candidate.setCairoSurface.assert_called_once_with(True)
        self.assertIs(surface._semantic_family_handle, family)
        with patch.object(Mesh3D, "sphere", return_value=self.mesh) as factory:
            spatial.Sphere(u_range=(math.pi / 4, 3 * math.pi / 4),
                           v_range=(math.pi / 6, 5 * math.pi / 6),
                           checkerboard_colors=False, stroke_width=0, shade_in_3d=False)
        self.assertEqual(factory.call_args.kwargs["u_range"], (math.pi / 4, 3 * math.pi / 4))
        self.assertEqual(factory.call_args.kwargs["v_range"], (math.pi / 6, 5 * math.pi / 6))
        for kwargs in ({"u_range": (-0.1, 1)}, {"u_range": (2, 1)},
                       {"v_range": (0, math.pi + 0.1)}):
            with self.subTest(kwargs=kwargs), patch.object(Mesh3D, "sphere") as factory:
                with self.assertRaises(ValueError):
                    spatial.Sphere(**kwargs)
                factory.assert_not_called()

    def test_axial_direction_caps_opacity_and_strokes_are_not_silently_approximated(self):
        import _noon_spatial as native

        with patch.object(Mesh3D, "cylinder") as factory:
            open_cylinder = spatial.Cylinder(show_ends=False, checkerboard_colors=False,
                                             stroke_width=0, shade_in_3d=False)
            self.assertIsNotNone(open_cylinder)
            self.assertIs(factory.call_args.kwargs["show_ends"], False)
            with self.assertRaises(TypeError):
                spatial.Cylinder(show_ends=0)
            with self.assertRaises(NotImplementedError):
                spatial.Cylinder(fill_opacity=0.5)
        for shape in (spatial.Cube, spatial.Prism):
            with self.assertRaises(NotImplementedError):
                shape(shade_in_3d=True)
            with self.assertRaises(NotImplementedError):
                shape(stroke_width=1, shade_in_3d=False)

        with patch.object(native, "_prism_face_family_options") as options:
            with self.assertRaises(ValueError):
                spatial.Cube(fill_opacity=1.1, shade_in_3d=False)
            options.assert_not_called()

    def test_cube_and_prism_use_one_shared_translucent_face_family(self):
        import _noon_spatial as native

        members = tuple(
            SimpleNamespace(semanticSlot=index + 1, semanticGeneration=1)
            for index in range(6)
        )
        family = _family_handle(members)
        candidate = Mock()
        translucent_red = noon.Color(0.8, 0.1, 0.2, 0.5)
        with patch.object(native, "_prism_face_family_options", return_value=candidate) as faces, \
             patch.object(native, "_create_mesh_family", return_value=family) as admit, \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            cube = spatial.Cube(shade_in_3d=False)
            prism = spatial.Prism(dimensions=(3, 2, 1), fill_opacity=0.4,
                                  fill_color=translucent_red, shade_in_3d=False)

        faces.assert_has_calls([
            call((2.0, 2.0, 2.0)),
            call((3.0, 2.0, 1.0)),
        ])
        self.assertEqual(faces.call_count, 2)
        default = noon.BLUE
        self.assertEqual(candidate.setFill.call_args_list, [
            call(default.red, default.green, default.blue, default.alpha, 0.75),
            call(translucent_red.red, translucent_red.green, translucent_red.blue,
                 translucent_red.alpha, 0.4),
        ])
        self.assertEqual(admit.call_count, 2)
        for shape in (cube, prism):
            self.assertIsInstance(shape, compat.Group)
            self.assertIs(shape._semantic_family_handle, family)
            self.assertEqual(len(shape.submobjects), 6)
        cube.set_fill(opacity=0.25)
        family.setFill.assert_called_once()
        with patch.object(native, "_bulk", side_effect=lambda values: tuple(values)):
            cube.shift((1, 2, 3))
        family.shiftWorld.assert_called_once_with(1.0, 2.0, 3.0)
        family.worldFamilyCenter.return_value = [10.0, -3.0, 2.0]
        with patch.object(native, "_group_target_context", return_value=None), \
             patch.object(native, "_bulk", side_effect=lambda values: tuple(values)):
            self.assertIs(cube.move_to((1, 2, 3)), cube)
        family.worldFamilyCenter.assert_called_once_with()
        family.shiftWorld.assert_called_with(-9.0, 5.0, 1.0)

    def test_capped_cone_profile_is_explicit_and_uses_native_mesh(self):
        with patch.object(Mesh3D, "cone", return_value=self.mesh) as factory:
            cone = spatial.Cone(show_base=True, height=2, direction=(-1, 2, -3), resolution=12,
                                v_range=(math.pi / 3, math.pi),
                                stroke_width=0, shade_in_3d=False)
        factory.assert_called_once()
        self.assertEqual(factory.call_args.args[:3], (1.0, 2.0, 12))
        self.assertEqual(factory.call_args.kwargs["direction"], (-1, 2, -3))
        self.assertEqual(factory.call_args.kwargs["axial_offset"], -2.0)
        self.assertEqual(factory.call_args.kwargs["v_range"], (math.pi / 3, math.pi))
        self.mesh.shift.assert_not_called()
        self.assertIs(cone._semantic_handle, self.mesh._semantic_handle)

    def test_cone_uses_the_pinned_open_base_default_and_accepts_only_boolean_options(self):
        with patch.object(Mesh3D, "cone", return_value=self.mesh) as factory:
            cone = spatial.Cone(stroke_width=0, shade_in_3d=False)
        factory.assert_called_once()
        self.assertIs(factory.call_args.kwargs["show_base"], False)
        self.assertIs(cone._semantic_handle, self.mesh._semantic_handle)
        with self.assertRaises(TypeError):
            spatial.Cone(show_base=1, stroke_width=0, shade_in_3d=False)

    def test_angular_sweeps_are_bounded_and_cone_radial_profile_remains_rejected(self):
        for shape in (spatial.Cylinder, spatial.Cone):
            for v_range in ((-0.1, 1), (1, 1), (2, 1),
                            (0, 2 * math.pi + 0.1), (0, float("inf"))):
                with self.subTest(shape=shape.__name__, v_range=v_range), \
                     patch.object(Mesh3D, "cylinder" if shape is spatial.Cylinder else "cone") as factory:
                    with self.assertRaises((TypeError, ValueError)):
                        shape(v_range=v_range, stroke_width=0, shade_in_3d=False)
                    factory.assert_not_called()
        with patch.object(Mesh3D, "cone") as factory:
            with self.assertRaisesRegex(NotImplementedError, "u_min"):
                spatial.Cone(u_min=0.2, stroke_width=0, shade_in_3d=False)
            factory.assert_not_called()


if __name__ == "__main__":
    unittest.main()
