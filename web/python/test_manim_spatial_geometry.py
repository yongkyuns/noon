import math
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch, call

import noon
import _manim_spatial_geometry as spatial
from _noon_spatial import Mesh3D
import _manim_compat as compat
import _manim_semantic_handles as semantic
from _noon_errors import NoonErrorCause, NoonOwnershipError


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

    def test_sphere_delegates_exact_mapping_and_center_to_shared_surface(self):
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize, \
             patch.object(spatial.Surface, "shift", autospec=True) as shift:
            sphere = spatial.Sphere(center=(1, 2, 3), radius=2, resolution=(12, 8))

        initialize.assert_called_once()
        self.assertIsInstance(sphere, spatial.Surface)
        self.assertEqual(initialize.call_args.kwargs["resolution"], (12, 8))
        self.assertEqual(initialize.call_args.kwargs["u_range"], (0, 2 * math.pi))
        self.assertEqual(initialize.call_args.kwargs["v_range"], (0, math.pi))
        func = initialize.call_args.args[1]
        u, v = 0.4, 0.7
        self.assertEqual(func(u, v), (
            2 * math.cos(u) * math.sin(v),
            2 * math.sin(u) * math.sin(v),
            -2 * math.cos(v),
        ))
        shift.assert_called_once_with(sphere, (1.0, 2.0, 3.0))

    def test_torus_delegates_pinned_surface_mapping_and_partial_uv_ranges(self):
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize:
            torus = spatial.Torus(major_radius=4, minor_radius=1.5,
                                  u_range=(0.2, 3.0), v_range=(0.4, 5.2),
                                  resolution=(10, 7))

        initialize.assert_called_once()
        self.assertIsInstance(torus, spatial.Surface)
        self.assertEqual(initialize.call_args.kwargs["u_range"], (0.2, 3.0))
        self.assertEqual(initialize.call_args.kwargs["v_range"], (0.4, 5.2))
        self.assertEqual(initialize.call_args.kwargs["resolution"], (10, 7))
        func = initialize.call_args.args[1]
        u, v = 0.4, 0.7
        radial = 4 - 1.5 * math.cos(v)
        self.assertEqual(func(u, v), (
            radial * math.cos(u),
            radial * math.sin(u),
            -1.5 * math.sin(v),
        ))
        for kwargs in ({"major_radius": float("nan")}, {"minor_radius": 0},
                       {"major_radius": 1, "minor_radius": 1},
                       {"u_range": (1, 1)}):
            with self.subTest(kwargs=kwargs), patch.object(
                spatial.Surface, "__init__", autospec=True, return_value=None,
            ) as factory:
                with self.assertRaises(ValueError):
                    spatial.Torus(**kwargs)
                factory.assert_not_called()

    def test_dot3d_uses_uniform_surface_fill_without_checkerboard(self):
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize, \
             patch.object(spatial.Surface, "shift", autospec=True):
            dot = spatial.Dot3D(point=(1, 2, 3), color=noon.RED)

        self.assertIsInstance(dot, spatial.Sphere)
        self.assertIs(initialize.call_args.kwargs["checkerboard_colors"], False)
        self.assertIs(initialize.call_args.kwargs["fill_color"], noon.RED)
        self.assertIs(initialize.call_args.kwargs["shade_in_3d"], True)
        self.assertEqual(initialize.call_args.kwargs["resolution"], (8, 8))

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

    def test_surface_setter_failure_frees_unconsumed_family_options_once(self):
        import _noon_spatial as native

        failure = ValueError("invalid family style")
        candidate = Mock()
        candidate.setFill.side_effect = failure
        plan = SimpleNamespace(
            parameters=Mock(return_value=(0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0)),
            finishCells=Mock(return_value=candidate),
            free=Mock(),
        )
        with patch.object(native, "_surface_plan", return_value=plan), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(native, "_create_mesh_family") as detached_create, \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            with self.assertRaises(ValueError) as raised:
                spatial.Surface(lambda u, v: (u, v, 0), resolution=(1, 1),
                                shade_in_3d=False, checkerboard_colors=False)

        self.assertIs(raised.exception, failure)
        plan.free.assert_called_once()
        candidate.free.assert_called_once()
        detached_create.assert_not_called()

    def test_surface_live_binding_error_preserves_typed_error_without_freeing_consumed_options(self):
        import _noon_spatial as native

        typed = NoonOwnershipError(
            NoonErrorCause("ownership", "family_rejected", "family admission failed"),
            object(),
            "createMeshFamily",
        )
        candidate = Mock()
        plan = SimpleNamespace(
            parameters=Mock(return_value=(0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0)),
            finishCells=Mock(return_value=candidate),
            free=Mock(),
        )
        context = Mock()
        context.createMeshFamily.side_effect = typed
        context.liveExecutionOwnership.return_value = "active"
        with patch.object(native, "_surface_plan", return_value=plan), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(native, "_create_mesh_family") as detached_create, \
             patch.object(spatial, "_live_constructor_context", return_value=context):
            with self.assertRaises(NoonOwnershipError) as raised:
                spatial.Surface(lambda u, v: (u, v, 0), resolution=(1, 1),
                                shade_in_3d=False, checkerboard_colors=False)

        self.assertIs(raised.exception, typed)
        plan.free.assert_called_once()
        candidate.free.assert_not_called()
        context.createMeshFamily.assert_called_once_with(candidate)
        detached_create.assert_not_called()

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
        with self.assertRaisesRegex(NotImplementedError, "aggregate family world pose"):
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

    def test_cylinder_uses_shared_cairo_surface_and_atomic_circle_caps(self):
        fill = noon.RED
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize:
            cylinder = spatial.Cylinder(
                height=4, direction=(1, -2, 3), resolution=(16, 16),
                v_range=(math.pi / 4, math.pi), fill_color=fill,
                checkerboard_colors=(noon.GREEN, noon.YELLOW),
            )
        kwargs = initialize.call_args.kwargs
        self.assertIsInstance(cylinder, spatial.Surface)
        self.assertEqual(kwargs["u_range"], (-2.0, 2.0))
        self.assertEqual(kwargs["v_range"], (math.pi / 4, math.pi))
        self.assertEqual(kwargs["resolution"], (16, 16))
        self.assertEqual(kwargs["_axial_pose"], ((1.0, -2.0, 3.0), 0.0))
        self.assertEqual(kwargs["_cairo_circle_caps"], (
            (1.0, -2.0, fill, True), (1.0, 2.0, fill, True),
        ))
        point = initialize.call_args.args[1]
        self.assertEqual(point(1.0, 0.0), (1.0, 0.0, 1.0))

    def test_cylinder_defaults_match_pinned_surface_and_shaded_end_contract(self):
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize:
            cylinder = spatial.Cylinder()

        kwargs = initialize.call_args.kwargs
        self.assertIsInstance(cylinder, spatial.Surface)
        self.assertEqual(kwargs["resolution"], (24, 24))
        self.assertEqual(kwargs["u_range"], (-1.0, 1.0))
        self.assertEqual(kwargs["v_range"], (0.0, 2.0 * math.pi))
        self.assertIs(kwargs["fill_color"], noon.BLUE_D)
        self.assertEqual(kwargs["stroke_width"], 0.5)
        self.assertTrue(kwargs["shade_in_3d"])
        self.assertEqual(kwargs["_cairo_circle_caps"], (
            (1.0, -1.0, noon.BLUE_D, True),
            (1.0, 1.0, noon.BLUE_D, True),
        ))

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

    def test_surface_cap_request_reaches_shared_candidate_before_atomic_publication(self):
        import _noon_spatial as native

        candidate = Mock()
        events = []
        candidate.addCircleCap.side_effect = lambda *args: events.append("cap")
        family = _family_handle((SimpleNamespace(semanticSlot=17, semanticGeneration=2),))
        candidate.setAxialPose.side_effect = lambda *args: events.append("pose")

        class Plan:
            def cairoParameters(self):
                return tuple(value for _ in range(16) for value in (0.25, 0.5))

            def finishCairoCells(self, points):
                return candidate

            def free(self):
                pass

        color = noon.RED
        with patch.object(native, "_surface_plan", return_value=Plan()), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=tuple), \
             patch.object(native, "_create_mesh_family",
                          side_effect=lambda value: events.append("publish") or family), \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            spatial.Surface(
                lambda u, v: (u, v, 0), resolution=(1, 1),
                _axial_pose=((0, 0, 1), 0),
                _cairo_circle_caps=((1.0, -1.0, color, True),),
            )

        self.assertEqual(events, ["pose", "cap", "publish"])
        cap_args = candidate.addCircleCap.call_args.args
        self.assertEqual(cap_args[:7], (1.0, -1.0, color.red, color.green,
                                         color.blue, color.alpha, True))
        self.assertEqual(len(cap_args), 7)

    def test_default_cone_uses_manim_surface_cells_and_rust_axial_family_pose(self):
        import _noon_spatial as native

        base_radius, height = 2.0, 3.0
        u_max = math.hypot(base_radius, height)
        u, v = 0.5 * u_max, math.pi / 3
        parameters = tuple(value for _ in range(16) for value in (u, v))
        candidate = Mock()
        member = SimpleNamespace(semanticSlot=17, semanticGeneration=2)
        family = _family_handle((member,))
        events = []
        candidate.setAxialPose.side_effect = lambda *args: events.append("pose")

        class Plan:
            def __init__(self):
                self.freed = False

            def cairoParameters(self):
                return parameters

            def finishCairoCells(self, points):
                self.points = tuple(points)
                return candidate

            def free(self):
                self.freed = True

        plan = Plan()
        plan_calls = []

        def make_plan(*args):
            plan_calls.append(args)
            return plan

        def publish(value):
            events.append("publish")
            self.assertIs(value, candidate)
            return family

        with patch.object(native, "_surface_plan", side_effect=make_plan), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=tuple), \
             patch.object(native, "_create_mesh_family", side_effect=publish) as create_family, \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            cone = spatial.Cone(
                base_radius=base_radius, height=height, direction=(-1, 2, -3),
            )

        self.assertEqual(plan_calls, [(0.0, u_max, 0.0, 2 * math.pi, 32, 32)])
        create_family.assert_called_once_with(candidate)
        self.assertTrue(plan.freed)
        theta = math.pi - math.atan(base_radius / height)
        expected = (
            u * math.sin(theta) * math.cos(v),
            u * math.sin(theta) * math.sin(v),
            u * math.cos(theta),
        )
        for actual, expected_component in zip(plan.points[:3], expected):
            self.assertAlmostEqual(actual, expected_component)
        candidate.setFill.assert_called_once_with(
            noon.BLUE_D.red, noon.BLUE_D.green, noon.BLUE_D.blue,
            noon.BLUE_D.alpha, 1.0,
        )
        candidate.setStroke.assert_called_once_with(
            spatial._DEFAULT_SURFACE_STROKE.red,
            spatial._DEFAULT_SURFACE_STROKE.green,
            spatial._DEFAULT_SURFACE_STROKE.blue,
            spatial._DEFAULT_SURFACE_STROKE.alpha, 0.005, 1.0,
        )
        candidate.setCairoSurface.assert_called_once_with(True)
        candidate.setAxialPose.assert_called_once_with((-1.0, 2.0, -3.0), 0.0)
        self.assertEqual(events, ["pose", "publish"])
        self.assertIs(cone._semantic_family_handle, family)
        self.assertIsInstance(cone, spatial.Surface)

    def test_cone_tilted_opt_out_fallbacks_use_posed_surface_family(self):
        import _noon_spatial as native

        direction = (-2.0, 3.0, 4.0)
        for u_min, resolution in ((0.25, (4, 4)), (0.0, (3, 4))):
            with self.subTest(u_min=u_min, resolution=resolution):
                candidate = Mock()
                family = _family_handle((SimpleNamespace(
                    semanticSlot=17, semanticGeneration=2,
                ),))
                events = []
                candidate.setAxialPose.side_effect = lambda *args: events.append("pose")

                class Plan:
                    freed = False

                    def cairoParameters(self):
                        return tuple(value for _ in range(16) for value in (0.5, 0.75))

                    def parameters(self):
                        return tuple(value for _ in range(16) for value in (0.5, 0.75))

                    def finishCairoCells(self, points):
                        self.points = tuple(points)
                        return candidate

                    def finishCells(self, points, normals):
                        self.points = tuple(points)
                        return candidate

                    def free(self):
                        self.freed = True

                plan = Plan()

                def publish(value):
                    events.append("publish")
                    self.assertIs(value, candidate)
                    return family

                with patch.object(native, "_surface_plan", return_value=plan), \
                     patch.object(native, "_mesh_family_options", object()), \
                     patch.object(native, "_bulk", side_effect=tuple), \
                     patch.object(native, "_create_mesh_family", side_effect=publish), \
                     patch.object(spatial, "_live_constructor_context", return_value=None), \
                     patch.object(Mesh3D, "parametric") as parametric:
                    cone = spatial.Cone(
                        base_radius=3, height=4, direction=direction,
                        u_min=u_min, resolution=resolution, shade_in_3d=False,
                        checkerboard_colors=False, stroke_width=0,
                    )

                parametric.assert_not_called()
                self.assertTrue(plan.freed)
                candidate.setAxialPose.assert_called_once_with(direction, 0.0)
                self.assertEqual(events, ["pose", "publish"])
                self.assertIs(cone._semantic_family_handle, family)

    def test_sphere_and_torus_defaults_use_shared_cairo_cells_with_pole_and_live_semantics(self):
        import _noon_spatial as native

        sampled = []
        plans = []
        candidate = Mock()
        members = (SimpleNamespace(semanticSlot=17, semanticGeneration=2),)
        family = _family_handle(members)
        sample_parameters = tuple(
            value for pair in ((0.0, 0.0), (0.0, math.pi), (0.4, 0.7), (0.4, 0.7))
            for value in pair
        ) * 4

        class Plan:
            def __init__(self):
                self.freed = False

            def cairoParameters(self):
                return sample_parameters

            def finishCairoCells(self, points):
                sampled.append(tuple(points))
                return candidate

            def free(self):
                self.freed = True

        def make_plan(*args):
            plans.append((args, Plan()))
            return plans[-1][1]

        context = Mock()
        context.liveExecutionOwnership.return_value = "active"
        context.createMeshFamily.return_value = family
        with patch.object(native, "_surface_plan", side_effect=make_plan), \
             patch.object(native, "_mesh_family_options", object()), \
             patch.object(native, "_bulk", side_effect=lambda values: tuple(values)), \
             patch.object(native, "_create_mesh_family") as detached_create, \
             patch.object(spatial, "_live_constructor_context", return_value=context):
            sphere = spatial.Sphere()
            torus = spatial.Torus()

        self.assertEqual([args for args, _ in plans], [
            (0, 2 * math.pi, 0, math.pi, 24, 12),
            (0, 2 * math.pi, 0, 2 * math.pi, 24, 24),
        ])
        self.assertTrue(all(plan.freed for _, plan in plans))
        self.assertEqual(len(sampled), 2)
        self.assertEqual(sampled[0][:3], (0.0, 0.0, -1.0))
        self.assertAlmostEqual(sampled[0][3 + 2], 1.0)
        u, v = 0.4, 0.7
        self.assertAlmostEqual(sampled[0][6], math.cos(u) * math.sin(v))
        self.assertAlmostEqual(sampled[0][7], math.sin(u) * math.sin(v))
        self.assertAlmostEqual(sampled[0][8], -math.cos(v))
        radial = 3 - math.cos(v)
        self.assertAlmostEqual(sampled[1][6], radial * math.cos(u))
        self.assertAlmostEqual(sampled[1][7], radial * math.sin(u))
        self.assertAlmostEqual(sampled[1][8], -math.sin(v))
        self.assertIs(sphere._canonical_live_target_context, context)
        self.assertIs(torus._canonical_live_target_context, context)
        self.assertEqual(context.createMeshFamily.call_count, 2)
        context.shiftFamilyWorld.assert_called_once_with(family, 0.0, 0.0, 0.0)
        detached_create.assert_not_called()
        candidate.setCairoSurface.assert_has_calls([call(True), call(True)])

        copied = Mock()
        with patch.object(compat.Group, "copy", return_value=copied) as copy_group:
            self.assertIs(sphere.copy(), copied)
        copy_group.assert_called_once_with(sphere)
        with patch.object(semantic, "_set_style", return_value=sphere) as set_style:
            sphere.set_style(fill_color=noon.RED, stroke_color=noon.GREEN)
        set_style.assert_called_once()

    def test_sphere_and_torus_keep_the_opaque_single_mesh_opt_out(self):
        sphere_mesh, torus_mesh = Mock(), Mock()
        for mesh in (sphere_mesh, torus_mesh):
            mesh._semantic_handle = Mock()
        with patch.object(Mesh3D, "sphere", return_value=sphere_mesh) as sphere_factory, \
             patch.object(Mesh3D, "torus", return_value=torus_mesh) as torus_factory, \
             patch.object(Mesh3D, "parametric") as parametric:
            sphere = spatial.Sphere(checkerboard_colors=False, stroke_width=0,
                                    shade_in_3d=False)
            torus = spatial.Torus(checkerboard_colors=False, stroke_width=0,
                                  shade_in_3d=False)

        self.assertIsNotNone(sphere._semantic_handle)
        self.assertIsNotNone(torus._semantic_handle)
        sphere_factory.assert_called_once_with(
            1.0, resolution=(24, 12), u_range=(0.0, 2 * math.pi),
            v_range=(0.0, math.pi), color=spatial._color(noon.BLUE_D), point_lit=False,
        )
        torus_factory.assert_called_once_with(
            3.0, 1.0, resolution=(24, 24), color=spatial._color(noon.BLUE_D),
            point_lit=False,
        )
        parametric.assert_not_called()

        partial_mesh = Mock()
        partial_mesh._semantic_handle = Mock()
        with patch.object(Mesh3D, "parametric", return_value=partial_mesh) as parametric:
            partial_torus = spatial.Torus(
                u_range=(0.2, 3.0), checkerboard_colors=False,
                stroke_width=0, shade_in_3d=False,
            )
        self.assertIsNotNone(partial_torus._semantic_handle)
        parametric.assert_called_once()
        self.assertEqual(parametric.call_args.kwargs["u_range"], (0.2, 3.0))

        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize, \
             patch.object(spatial.Surface, "shift", autospec=True):
            spatial.Sphere(center=(1, 2, 3), u_range=(math.pi / 4, 3 * math.pi / 4),
                           v_range=(math.pi / 6, 5 * math.pi / 6),
                           checkerboard_colors=False, stroke_width=0, shade_in_3d=False)
        self.assertEqual(initialize.call_args.kwargs["u_range"], (math.pi / 4, 3 * math.pi / 4))
        self.assertEqual(initialize.call_args.kwargs["v_range"], (math.pi / 6, 5 * math.pi / 6))
        partial_sphere_mesh = Mock()
        partial_sphere_mesh._semantic_handle = Mock()
        with patch.object(Mesh3D, "sphere", return_value=partial_sphere_mesh) as sphere_factory, \
             patch.object(Mesh3D, "parametric") as parametric:
            spatial.Sphere(
                u_range=(math.pi / 4, 3 * math.pi / 4),
                v_range=(math.pi / 6, 5 * math.pi / 6),
                checkerboard_colors=False, stroke_width=0, shade_in_3d=False,
            )
        sphere_factory.assert_called_once_with(
            1.0, resolution=(24, 12),
            u_range=(math.pi / 4, 3 * math.pi / 4),
            v_range=(math.pi / 6, 5 * math.pi / 6),
            color=spatial._color(noon.BLUE_D), point_lit=False,
        )
        parametric.assert_not_called()
        for kwargs in ({"u_range": (-0.1, 1)}, {"u_range": (2, 1)},
                       {"v_range": (0, math.pi + 0.1)}, {"radius": float("nan")},
                       {"radius": 0}):
            with self.subTest(kwargs=kwargs), patch.object(
                spatial.Surface, "__init__", autospec=True, return_value=None,
            ) as factory:
                with self.assertRaises(ValueError):
                    spatial.Sphere(**kwargs)
                factory.assert_not_called()

    def test_axial_direction_caps_opacity_and_strokes_are_not_silently_approximated(self):
        import _noon_spatial as native

        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as factory:
            open_cylinder = spatial.Cylinder(show_ends=False, checkerboard_colors=False,
                                             stroke_width=0, shade_in_3d=False)
            self.assertIsNotNone(open_cylinder)
            self.assertEqual(factory.call_args.kwargs["_cairo_circle_caps"], ())
            with self.assertRaises(TypeError):
                spatial.Cylinder(show_ends=0)
            with self.assertRaises(NotImplementedError):
                spatial.Cylinder(fill_opacity=0.5)
        for shape in (spatial.Cube, spatial.Prism):
            with self.assertRaises(TypeError):
                shape(shade_in_3d=1)
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
            cube = spatial.Cube()
            prism = spatial.Prism(dimensions=(3, 2, 1), fill_opacity=0.4,
                                  fill_color=translucent_red, shade_in_3d=False)

        faces.assert_has_calls([
            call((2.0, 2.0, 2.0), True),
            call((3.0, 2.0, 1.0), False),
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

    def test_prism_family_setter_failure_frees_unconsumed_options_once(self):
        import _noon_spatial as native

        failure = ValueError("invalid family fill")
        candidate = Mock()
        candidate.setFill.side_effect = failure
        with patch.object(native, "_prism_face_family_options", return_value=candidate), \
             patch.object(native, "_create_mesh_family") as detached_create, \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            with self.assertRaises(ValueError) as raised:
                spatial.Cube()

        self.assertIs(raised.exception, failure)
        candidate.free.assert_called_once()
        detached_create.assert_not_called()

    def test_prism_detached_binding_error_preserves_typed_error_without_freeing_consumed_options(self):
        import _noon_spatial as native

        typed = NoonOwnershipError(
            NoonErrorCause("ownership", "family_rejected", "family admission failed"),
            object(),
            "createMeshFamily",
        )
        candidate = Mock()
        with patch.object(native, "_prism_face_family_options", return_value=candidate), \
             patch.object(native, "_create_mesh_family", side_effect=typed) as detached_create, \
             patch.object(spatial, "_live_constructor_context", return_value=None):
            with self.assertRaises(NoonOwnershipError) as raised:
                spatial.Cube()

        self.assertIs(raised.exception, typed)
        candidate.free.assert_not_called()
        detached_create.assert_called_once_with(candidate)

    def test_capped_cone_adds_ordinary_base_to_shared_surface_family(self):
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize:
            cone = spatial.Cone(show_base=True, base_radius=2, height=3,
                                direction=(-1, 2, -3), resolution=(12, 12),
                                v_range=(math.pi / 3, math.pi))
        kwargs = initialize.call_args.kwargs
        self.assertIsInstance(cone, spatial.Surface)
        self.assertEqual(kwargs["_cairo_circle_caps"], ((2.0, -3.0, noon.BLUE_D, False),))
        self.assertTrue(kwargs["_force_surface_family"])
        with self.assertRaisesRegex(NotImplementedError, "truncated radial range"):
            spatial.Cone(show_base=True, u_min=0.1)

    def test_cone_uses_the_pinned_open_base_default_and_accepts_only_boolean_options(self):
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as factory:
            cone = spatial.Cone(stroke_width=0, shade_in_3d=False)
        self.assertEqual(factory.call_args.kwargs["_cairo_circle_caps"], ())
        self.assertIsInstance(cone, spatial.Surface)
        with self.assertRaises(TypeError):
            spatial.Cone(show_base=1, stroke_width=0, shade_in_3d=False)

    def test_angular_sweeps_are_bounded_and_cone_radial_profiles_are_validated(self):
        for shape in (spatial.Cylinder, spatial.Cone):
            for v_range in ((-0.1, 1), (1, 1), (2, 1),
                            (0, 2 * math.pi + 0.1), (0, float("inf"))):
                with self.subTest(shape=shape.__name__, v_range=v_range), \
                     patch.object(spatial.Surface, "__init__", autospec=True,
                                  return_value=None) as factory:
                    with self.assertRaises((TypeError, ValueError)):
                        shape(v_range=v_range, stroke_width=0, shade_in_3d=False)
                    factory.assert_not_called()
        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize:
            spatial.Cone(base_radius=3, height=4, u_min=1,
                         v_range=(math.pi / 4, math.pi), resolution=(4, 6))
        self.assertEqual(initialize.call_args.kwargs["u_range"], (1.0, 5.0))
        self.assertEqual(initialize.call_args.kwargs["v_range"], (math.pi / 4, math.pi))
        self.assertEqual(initialize.call_args.kwargs["resolution"], (4, 6))
        self.assertEqual(initialize.call_args.kwargs["_axial_pose"], ((0.0, 0.0, 1.0), 0.0))
        function = initialize.call_args.args[1]
        u, v = 3.0, 0.7
        theta = math.pi - math.atan(3 / 4)
        self.assertEqual(function(u, v), (
            u * math.sin(theta) * math.cos(v),
            u * math.sin(theta) * math.sin(v),
            u * math.cos(theta),
        ))

        for u_min in (-0.1, 5.0, float("nan"), float("inf")):
            with self.subTest(u_min=u_min), patch.object(
                spatial.Surface, "__init__", autospec=True, return_value=None,
            ) as factory:
                with self.assertRaises(ValueError):
                    spatial.Cone(base_radius=3, height=4, u_min=u_min)
                factory.assert_not_called()

        with patch.object(spatial.Surface, "__init__", autospec=True,
                          return_value=None) as initialize:
            with self.assertRaisesRegex(ValueError, "u_min"):
                spatial.Cone(base_radius=1.7e308, height=1.7e308)
            initialize.assert_not_called()

        with self.assertRaisesRegex(NotImplementedError, "opaque fills"):
            spatial.Cone(show_base=True, fill_opacity=0.5)


if __name__ == "__main__":
    unittest.main()
