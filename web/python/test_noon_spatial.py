import unittest
import sys
from unittest.mock import patch, Mock

import noon
import _noon_spatial as spatial


class Candidate:
    def __init__(self):
        self.calls = []
        self.freed = 0

    def setColor(self, *values): self.calls.append(("color", values))
    def setPointLit(self, enabled): self.calls.append(("lit", enabled))
    def free(self): self.freed += 1


class SpatialFacadeTests(unittest.TestCase):
    def test_indexed_mesh_exports_are_explicit_native_profile(self):
        self.assertIs(noon.Mesh3D, spatial.Mesh3D)
        self.assertIs(noon.SpatialScene, spatial.SpatialScene)
        self.assertIn("WorldTransformTo", noon.__all__)

    def test_generated_solid_configures_inert_options_before_admission(self):
        candidate = Candidate()
        handle = object()
        class Factory:
            @staticmethod
            def sphere(*values):
                self.assertEqual(values, (2.0, 8, 4))
                return candidate
        with patch.object(spatial, "_mesh_options", Factory), \
             patch.object(spatial, "_live_constructor_context", return_value=None), \
             patch.object(spatial, "_create_mesh", return_value=handle) as admit:
            mesh = spatial.Mesh3D.sphere(2, resolution=(8, 4), color=noon.RED, point_lit=True)
        self.assertIs(mesh._semantic_handle, handle)
        self.assertIsNone(mesh._raw)
        self.assertEqual(candidate.calls, [("color", (noon.RED.red, noon.RED.green, noon.RED.blue, noon.RED.alpha)), ("lit", True)])
        self.assertEqual(candidate.freed, 0)
        admit.assert_called_once_with(candidate)

    def test_failed_inert_configuration_frees_candidate_without_admission(self):
        candidate = Candidate()
        with patch.object(spatial, "_create_mesh") as admit:
            with self.assertRaises(TypeError):
                spatial.Mesh3D(candidate, color=object())
        self.assertEqual(candidate.freed, 1)
        admit.assert_not_called()

    def test_surface_callback_uses_rust_parameter_order_once_at_construction(self):
        candidate = Candidate()
        class Plan:
            freed = False
            def parameters(self): return [2, 7, 2, 9, 3, 7, 3, 9]
            def finishMesh(self, points, normals):
                self.points, self.normals = tuple(points), tuple(normals)
                return candidate
            def free(self): self.freed = True
        plan = Plan()
        calls = []
        def sample(u, v):
            calls.append((u, v)); return (u, v, 0)
        with patch.object(spatial, "_surface_plan", return_value=plan) as prepare, \
             patch.object(spatial, "_bulk", side_effect=tuple), \
             patch.object(spatial, "_live_constructor_context", return_value=None), \
             patch.object(spatial, "_create_mesh", return_value=object()):
            spatial.Mesh3D.parametric(sample, u_range=(2, 3), v_range=(7, 9), resolution=(1, 1))
        prepare.assert_called_once_with(2, 3, 7, 9, 1, 1)
        self.assertEqual(calls, [(2, 7), (2, 9), (3, 7), (3, 9)])
        self.assertEqual(plan.points, (2, 7, 0, 2, 9, 0, 3, 7, 0, 3, 9, 0))
        self.assertEqual(plan.normals, ())
        self.assertTrue(plan.freed)

    def test_callback_exception_frees_sampling_plan_and_never_admits(self):
        class Plan:
            freed = False
            def parameters(self): return [0, 0, 0, 1]
            def free(self): self.freed = True
        plan = Plan()
        def sample(*args): raise RuntimeError("author callback failed")
        with patch.object(spatial, "_surface_plan", return_value=plan), \
             patch.object(spatial, "_create_mesh") as admit:
            with self.assertRaisesRegex(RuntimeError, "author callback failed"):
                spatial.Mesh3D.parametric(sample)
        self.assertTrue(plan.freed)
        admit.assert_not_called()

    def test_world_endpoint_is_an_inert_request_not_a_python_animation(self):
        mesh = object.__new__(spatial.Mesh3D)
        request = spatial.WorldTransformTo(mesh, translation=(1, 2, 3), run_time=2)
        self.assertIs(request.mobject, mesh)
        self.assertEqual(request.endpoint, (1, 2, 3, 1, 0, 0, 0, 1, 1, 1))
        self.assertEqual(request.anim_args, {"run_time": 2})

    def test_invalid_options_and_fractional_counts_never_allocate(self):
        with patch.object(spatial, "_mesh_options") as factory, \
             patch.object(spatial, "_surface_plan") as plan:
            for options in ({"unknown": 1}, {"point_lit": "false"}):
                with self.assertRaises(TypeError):
                    spatial.Mesh3D.sphere(**options)
                with self.assertRaises(TypeError):
                    spatial.Mesh3D.parametric(lambda u, v: (u, v, 0), **options)
            for count in (1.5, True, -1, 2**32):
                with self.assertRaises((TypeError, ValueError)):
                    spatial.Mesh3D.sphere(resolution=(count, 8))
            factory.sphere.assert_not_called()
            plan.assert_not_called()

    def test_spatial_line_delegates_endpoint_geometry_to_rust(self):
        candidate = Candidate()
        factory = Mock()
        factory.line3D.return_value = candidate
        with patch.object(spatial, "_mesh_options", factory), \
             patch.object(spatial, "_bulk", side_effect=tuple), \
             patch.object(spatial, "_live_constructor_context", return_value=None), \
             patch.object(spatial, "_create_mesh", return_value=object()):
            spatial.Mesh3D.line3d((1, 2, 3), (4, 5, 6), thickness=0.1, segments=8)
        factory.line3D.assert_called_once_with((1, 2, 3), (4, 5, 6), 0.1, 8)

    def test_spatial_animate_rejects_planar_capture_before_mutation(self):
        mesh = object.__new__(spatial.Mesh3D)
        with self.assertRaisesRegex(NotImplementedError, "WorldTransformTo"):
            mesh.animate

    def test_world_center_uses_typed_authored_and_effective_center_queries(self):
        mesh = object.__new__(spatial.Mesh3D)
        handle = Mock()
        handle.worldCenter.return_value = [1.25, -2.5, 3.75]
        context = Mock()
        context.effectiveWorldCenter.return_value = [4.0, 5.0, 6.0]
        with patch.object(spatial, "_handle_for", return_value=handle), \
             patch.object(spatial, "_live_mutation_context", return_value=None):
            self.assertEqual(mesh.get_center(), (1.25, -2.5, 3.75))
        handle.worldCenter.assert_called_once_with()
        with patch.object(spatial, "_handle_for", return_value=handle), \
             patch.object(spatial, "_live_mutation_context", return_value=context):
            self.assertEqual(mesh.get_center(), (4.0, 5.0, 6.0))
        context.effectiveWorldCenter.assert_called_once_with(handle)

    def test_world_move_to_offsets_from_geometry_center(self):
        mesh = object.__new__(spatial.Mesh3D)
        handle = Mock()
        handle.worldCenter.return_value = [10.0, -3.0, 2.0]
        with patch.object(spatial, "_handle_for", return_value=handle), \
             patch.object(spatial, "_live_mutation_context", return_value=None):
            self.assertIs(mesh.move_to((1, 2, 3)), mesh)
        handle.shiftWorld.assert_called_once_with(-9.0, 5.0, 1.0)

    def test_world_rotation_uses_the_same_scalar_axis_signature_for_cold_handles(self):
        mesh = object.__new__(spatial.Mesh3D)
        handle = Mock()
        with patch.object(spatial, "_handle_for", return_value=handle), \
             patch.object(spatial, "_live_mutation_context", return_value=None), \
             patch.object(spatial, "_bulk", side_effect=tuple):
            self.assertIs(mesh.rotate(0.75, axis=(1, 0, 0)), mesh)
        handle.rotateWorld.assert_called_once_with(1.0, 0.0, 0.0, 0.75, ())



class CameraFacadeTests(unittest.TestCase):
    def test_ordinary_spatial_membership_delegates_default_domain_policy_to_rust(self):
        from types import SimpleNamespace
        scene = object.__new__(spatial.ThreeDScene)
        value = object()
        edit = Mock()
        with patch.dict(sys.modules, {"_manim_scene": SimpleNamespace(_canonical_edit_membership=edit)}):
            scene._edit_membership("add", (value,), key="shape")
        edit.assert_called_once_with(scene, "add", (value,), key="shape", spatial_domain="world_default")

    def test_camera_endpoint_retains_rust_values_and_only_replaces_supplied_arguments(self):
        scene = object.__new__(spatial.ThreeDScene)
        scene.camera = object()
        from types import SimpleNamespace
        profile = (0.6, -1.2, 0.1, 20.0, 1.0, 8.0, 0.0, 0.0, 0.0)
        context = SimpleNamespace(effectiveCameraProfile=lambda _: profile)
        with patch.dict(sys.modules, {"_manim_scene": SimpleNamespace(_context=lambda _: context)}), \
             patch.object(spatial, "_handle_for", return_value=object()):
            endpoint = scene._camera_endpoint(theta=5.0, zoom=1.5, frame_center=(2, 3, 4))
        self.assertEqual(endpoint, (0.6, 5.0, 0.1, 20.0, 1.5, 8.0, 2.0, 3.0, 4.0))

    def test_camera_move_delegates_to_ordinary_play_with_parallel_animations(self):
        scene = object.__new__(spatial.ThreeDScene)
        scene.camera = object()
        endpoint = (0.6, 7.0, 0.1, 20, 1, 8, 0, 0, 0)
        extra = object()
        with patch.object(scene, "_camera_endpoint", return_value=endpoint), \
             patch.object(scene, "play", return_value="continuation") as play:
            result = scene.move_camera(theta=7, added_anims=(extra,), run_time=2)
        self.assertEqual(result, "continuation")
        camera_move, other = play.call_args.args
        self.assertIsInstance(camera_move, spatial.CameraProfileTo)
        self.assertEqual(camera_move.endpoint, endpoint)
        self.assertIs(other, extra)
        self.assertEqual(play.call_args.kwargs, {"run_time": 2})

    def test_fixed_labels_use_atomic_existing_membership_batch(self):
        scene = object.__new__(spatial.SpatialScene)
        first, second = object(), object()
        from types import SimpleNamespace
        edit = Mock()
        with patch.dict(sys.modules, {"_manim_scene": SimpleNamespace(_canonical_edit_membership=edit)}):
            self.assertIs(scene.add_fixed_orientation_mobjects(first, second), scene)
        edit.assert_called_once_with(scene, "add", (first, second), spatial_domain="fixed_orientation")

if __name__ == "__main__":
    unittest.main()
