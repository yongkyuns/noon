"""Exercise real pinned-Manim source resolution with relocated manifests."""

import copy
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from types import SimpleNamespace
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "parity/manim-v0.21/manifest.json"
ORACLE = ROOT / "scripts/manim-raster-semantic-reference.py"


class RasterManifestTests(unittest.TestCase):
    def test_polynomial_easing_checkpoints_have_exact_original_frames(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        fixture = next(fixture for fixture in manifest["fixtures"] if fixture["id"] == "polynomial-easings")
        self.assertEqual(fixture["reference_frame_rate"], 120)
        self.assertEqual(fixture["sample_times"], [0, 0.125, 0.25, 0.375, 0.5, 0.75, 29 / 30])
        for time in fixture["sample_times"]:
            self.assertAlmostEqual(time * fixture["reference_frame_rate"], round(time * fixture["reference_frame_rate"]), places=9)
        self.assertEqual(manifest["reference"]["frame_rate"], 30)

    def test_polynomial_interiors_use_real_reference_frame_times(self):
        manifest = json.loads((ROOT / "parity/manim-v0.21/manifest.json").read_text())
        fixture = next(fixture for fixture in manifest["fixtures"] if fixture["id"] == "polynomial-rates")
        fps = manifest["reference"]["frame_rate"]
        self.assertIn(fixture["expected_duration"] / 2, fixture["sample_times"])
        for time in fixture["sample_times"]:
            self.assertAlmostEqual(time * fps, round(time * fps), places=9)
            self.assertLess(time, fixture["expected_duration"])

    def test_vector_space_lts_has_direct_and_worker_pairs(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        fixtures = {
            fixture["id"]: fixture
            for fixture in manifest["fixtures"]
            if fixture["id"] in {"vector-space-lts-direct", "vector-space-lts-worker"}
        }
        self.assertEqual(set(fixtures), {"vector-space-lts-direct", "vector-space-lts-worker"})
        direct = fixtures["vector-space-lts-direct"]
        worker = fixtures["vector-space-lts-worker"]
        self.assertEqual(direct["scene"], "VectorSpaceLTS")
        self.assertEqual(worker["scene"], direct["scene"])
        self.assertEqual(direct["source"], worker["source"])
        self.assertEqual(direct["direct_factory"], "createDirectVectorSpaceSmokeRenderer")
        self.assertEqual(direct["expected_duration"], 4.0)
        self.assertEqual(worker["expected_duration"], 4.0)
        self.assertEqual(direct["sample_times"], [0.0, 0.5, 1.0, 2.5, 3.966666666666667])
        self.assertEqual(worker["sample_times"], direct["sample_times"])
        self.assertNotIn("tolerance", direct)
        self.assertNotIn("tolerance", worker)
        source = (ROOT / direct["source"]).read_text(encoding="utf-8")
        self.assertIn("from manim import *", source)
        self.assertIn("class VectorSpaceLTS(LinearTransformationScene)", source)
        self.assertIn("self.apply_matrix([[0.0, 1.0], [1.0, 0.0]])", source)

    def test_focused_lts_feature_pair_uses_tracked_vector_for_ghost_history(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        fixtures = {
            fixture["id"]: fixture
            for fixture in manifest["fixtures"]
            if fixture["id"] in {"lts-feature-slice-direct", "lts-feature-slice-worker"}
        }
        self.assertEqual(set(fixtures), {"lts-feature-slice-direct", "lts-feature-slice-worker"})
        direct = fixtures["lts-feature-slice-direct"]
        worker = fixtures["lts-feature-slice-worker"]
        self.assertEqual(direct["scene"], "VectorSpaceLTSFeatures")
        self.assertEqual(worker["scene"], direct["scene"])
        oracle = (ROOT / direct["source"]).read_text(encoding="utf-8")
        noon = (ROOT / worker["noon_source"]).read_text(encoding="utf-8")
        for source in (oracle, noon):
            self.assertIn("self.add_transformable_mobject(square)", source)
            self.assertIn("Vector((0.5, 0.25), color=YELLOW)", source)
            self.assertIn("self.add_vector(vector, animate=False)", source)
            self.assertIn("self.apply_matrix([[0.0, 1.0], [1.0, 0.0]]", source)
            self.assertIn("self.apply_matrix([[2.0, 0.0], [0.0, 1.0]]", source)

    def test_spatial_mesh_fixture_is_a_direct_typed_rust_wasm_pair(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        fixtures = [
            fixture for fixture in manifest["fixtures"]
            if fixture["id"] == "spatial-mesh-depth"
        ]
        self.assertEqual(len(fixtures), 1)
        fixture = fixtures[0]
        self.assertEqual(fixture["scene"], "SpatialMeshDepthOracle")
        self.assertEqual(fixture["direct_factory"], "createDirectSpatialMeshSmokeRenderer")
        self.assertEqual(fixture["expected_duration"], 2.0)
        self.assertEqual(fixture["sample_times"], [0.0, 0.5, 1.0, 1.5, 1.9666666666666666])
        self.assertIn(fixture["expected_duration"] / 2, fixture["sample_times"])
        source = (ROOT / fixture["source"]).read_text(encoding="utf-8")
        raster_driver = (ROOT / "scripts/manim-raster-differential.mjs").read_text(
            encoding="utf-8"
        )
        for required in (
            "class SpatialMeshDepthOracle(ThreeDScene)",
            "focal_distance=5",
            "zoom=4 / (5 * np.tan(0.5))",
            "frame_center=RIGHT * 0.25",
            "self.red_mesh.animate.shift(2.0 * IN)",
            "def noon_oracle_state(self)",
        ):
            self.assertIn(required, source)
        self.assertIn("if (fixture.direct_factory)", raster_driver)
        self.assertIn("direct_typed_execution", raster_driver)

    def test_three_d_semantics_capture_effective_pinned_camera_profile(self):
        # This test runs in the pinned-Manim workflow and deliberately reads the
        # real ThreeDCamera getters used by the independent semantic oracle.
        from manim.camera.three_d_camera import ThreeDCamera
        from manim.scene.moving_camera_scene import MovingCameraScene
        from manim.scene.three_d_scene import ThreeDScene

        spec = importlib.util.spec_from_file_location("manim_semantic_reference_test", ORACLE)
        self.assertIsNotNone(spec)
        self.assertIsNotNone(spec.loader)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        camera = ThreeDCamera()
        camera.set_phi(0.7)
        camera.set_theta(-1.1)
        camera.set_gamma(0.2)
        camera.set_zoom(1.3)
        camera.set_focal_distance(4.5)
        camera.frame_height = 8.0
        camera.frame_center = [0.25, -0.5, 1.25]
        camera.light_source.move_to([-6.0, -8.0, 9.0])

        scene = object.__new__(ThreeDScene)
        scene.renderer = SimpleNamespace(camera=camera)
        scene.mobjects = []
        observed = module._scene_state(scene, 2, 1.0, 0.25)
        self.assertEqual(observed["camera_3d"], {
            "phi": 0.7, "theta": -1.1, "gamma": 0.2, "zoom": 1.3,
            "focal_distance": 4.5, "frame_height": 8.0,
            "frame_center": [0.25, -0.5, 1.25],
            "light_source": [-6.0, -8.0, 9.0],
        })
        self.assertNotIn("camera", observed)

        # Non-3D and existing 2D camera states retain their existing schemas.
        ordinary = module._scene_state(SimpleNamespace(mobjects=[]), 0, 0.0, 0.0)
        self.assertNotIn("camera_3d", ordinary)
        camera_2d = SimpleNamespace(
            frame=SimpleNamespace(get_center=lambda: [0.5, -0.25, 0.0], height=4.0)
        )
        moving_scene = object.__new__(MovingCameraScene)
        moving_scene.renderer = SimpleNamespace(camera=camera_2d)
        moving_scene.mobjects = []
        self.assertEqual(
            module._scene_state(moving_scene, 1, 0.5, 0.5)["camera"],
            {"center": [0.5, -0.25], "height": 4.0},
        )

    def test_zoomed_semantics_observe_activation_motion_and_display_removal(self):
        from manim import RIGHT, tempconfig
        from manim.scene.zoomed_scene import ZoomedScene

        spec = importlib.util.spec_from_file_location("manim_inset_reference_test", ORACLE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempconfig({"pixel_width": 960, "pixel_height": 540}):
            scene = ZoomedScene()
            scene.setup()
            self.assertEqual(module._scene_state(scene, 0, 0.0, 0.0)["inset_2d_views"], [])
            scene.activate_zooming()
            scene.zoomed_camera.frame.shift(RIGHT * 0.5)
            scene.zoomed_display.scale(0.5)
            expected_center = scene.zoomed_display.get_center()
            before = scene.zoomed_camera.frame.copy()
            observed = module._scene_state(scene, 1, 0.5, 0.5)
            self.assertAlmostEqual(observed["inset_2d_views"][0]["camera"]["height"], 0.45)
            expected_height = observed["inset_2d_views"][0]["camera"]["height"]
            self.assertEqual(observed["inset_2d_views"], [{
                "camera": {"center": {"x": 0.5, "y": 0.0}, "height": expected_height},
                "display_center": {"x": expected_center[0], "y": expected_center[1]},
                "display_size": {"x": 1.5, "y": 1.5},
                "capture_own_display": False,
            }])
            self.assertTrue((scene.zoomed_camera.frame.points == before.points).all())
            # Pinned Scene.remove restructures the image family when its border
            # is removed. The camera registration alone is not visible membership.
            scene.remove(scene.zoomed_display.display_frame)
            self.assertEqual(len(scene.camera.image_mobjects_from_cameras), 1)
            self.assertEqual(module._scene_state(scene, 2, 0.75, 0.75)["inset_2d_views"], [])
            scene.add(scene.zoomed_display)
            self.assertEqual(len(module._scene_state(scene, 3, 0.8, 0.8)["inset_2d_views"]), 1)
            scene.remove(scene.zoomed_display)
            self.assertEqual(module._scene_state(scene, 4, 1.0, 1.0)["inset_2d_views"], [])

    def test_authored_camera_projection_keeps_original_frames_clock_and_endpoint(self):
        from manim import Circle, ThreeDScene, tempconfig

        spec = importlib.util.spec_from_file_location("authored_camera_reference_test", ORACLE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        class CheckpointCamera(ThreeDScene):
            def construct(self):
                self.set_camera_orientation(phi=0.7, theta=0.2)
                self.add(Circle())
                self.begin_ambient_camera_rotation(rate=0.1)
                self.wait(0.1)
                self.stop_ambient_camera_rotation()
                self.wait(0.1)

        fixture = {"id": "checkpoint-camera", "scene": "CheckpointCamera",
                   "expected_duration": 0.2, "sample_times": [0, 1 / 30, 0.1, 0.15, 0.2],
                   "camera_profile_observation": True}
        with tempfile.TemporaryDirectory() as directory, tempconfig({
            "pixel_width": 160, "pixel_height": 90, "frame_rate": 30,
            "media_dir": directory, "progress_bar": "none", "disable_caching": True,
        }):
            root = Path(directory)
            scenes = SimpleNamespace(CheckpointCamera=CheckpointCamera)
            original = module._render_fixture(scenes, fixture, 30, root / "original")
            controlled_fixture = {**fixture, "authored_camera_checkpoints": [
                {"time": time, "phi": 0.7, "theta": 0.4} for time in fixture["sample_times"]
            ]}
            controlled = module._render_fixture(scenes, controlled_fixture, 30, root / "controlled")
            for field in ("frames", "frozen_intervals", "terminal_state", "logical_duration"):
                self.assertEqual(controlled[field], original[field], field)
            self.assertEqual(Path(controlled["terminal_png"]["path"]).read_bytes(),
                             Path(original["terminal_png"]["path"]).read_bytes())
            samples = controlled["authored_camera_samples"]
            self.assertEqual([row["time"] for row in samples], fixture["sample_times"])
            self.assertEqual(samples[3]["sampled_time"], 0.1)
            for sample in samples:
                self.assertEqual(sample["camera_3d"]["theta"], 0.4)
                self.assertNotEqual(sample["sampled_manim_camera_3d"]["theta"], 0.4)
                self.assertTrue(Path(sample["path"]).is_file())
            self.assertNotEqual(samples[-1]["sha256"], controlled["terminal_png"]["sha256"])

    def test_authored_camera_configuration_fails_closed(self):
        spec = importlib.util.spec_from_file_location("authored_camera_validation_test", ORACLE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        fixture = {"sample_times": [0, 1], "expected_duration": 1,
                   "camera_profile_observation": True, "authored_camera_checkpoints": [
                       {"time": 0, "phi": 0.7, "theta": 0.2},
                       {"time": 1, "phi": 0.7, "theta": 0.3}]}
        self.assertEqual(module._camera_checkpoints(fixture), fixture["authored_camera_checkpoints"])
        for change in [[], [{"time": 0, "phi": 0.7, "theta": 0.2}],
                       [{"time": 0, "phi": float("nan"), "theta": 0.2}],
                       [{"time": True, "phi": 0.7, "theta": 0.2}],
                       [{"time": 0, "phi": 0.7, "theta": 0.2, "zoom": 2}]]:
            with self.subTest(checkpoints=change), self.assertRaises(ValueError):
                module._camera_checkpoints({**fixture, "authored_camera_checkpoints": change})

    def test_failed_authored_capture_restores_the_original_camera(self):
        from manim import Circle, ThreeDScene, tempconfig

        spec = importlib.util.spec_from_file_location("authored_camera_failure_test", ORACLE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory, tempconfig({
            "pixel_width": 160, "pixel_height": 90, "media_dir": directory,
        }):
            renderer = module.SemanticRenderer(camera_class=module.ThreeDCamera,
                camera_checkpoints=[{"time": 0, "phi": 0.7, "theta": 0.4}],
                checkpoint_root=Path(directory))
            scene = ThreeDScene(renderer=renderer)
            scene.add(Circle())
            before = module._three_d_camera_state(scene.camera)
            with mock.patch.object(scene.camera, "get_image", side_effect=RuntimeError("capture failed")):
                with self.assertRaisesRegex(RuntimeError, "capture failed"):
                    renderer.capture_camera_checkpoints(scene, 0)
            self.assertEqual(module._three_d_camera_state(scene.camera), before)
            self.assertEqual(renderer.authored_camera_samples, [])

    def test_real_reference_font_default_preserves_explicit_object_fonts(self):
        import manimpango

        fonts = sorted(manimpango.list_fonts())
        self.assertGreaterEqual(len(fonts), 2)
        selected = "DejaVu Sans Mono" if "DejaVu Sans Mono" in fonts else fonts[0]
        explicit = next(font for font in fonts if font != selected)
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "font-default.py"
            script.write_text(
                "import json\nfrom manim import Text, MarkupText, tempconfig\n"
                f"with tempconfig({{'media_dir': {directory!r}}}):\n"
                "    text = Text('abc')\n    markup = MarkupText('abc')\n"
                f"    explicit = Text('abc', font={explicit!r})\n"
                "    print(json.dumps([text.font, markup.font, explicit.font]))\n"
            )
            result = subprocess.run(
                [sys.executable, str(ORACLE.with_name("manim-reference-run.py")), str(script)],
                env={**os.environ, "NOON_MANIM_REFERENCE_TEXT_FONT": selected},
                text=True, capture_output=True, timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(json.loads(result.stdout.strip().splitlines()[-1]),
                             [selected, selected, explicit])

    def test_manifest_location_and_cwd_do_not_change_fixture_sources(self):
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        matching = [
            fixture for fixture in manifest["fixtures"]
            if fixture["scene"] == "MatchingShapesReordered"
        ]
        self.assertEqual(len(matching), 1)
        manifest["fixtures"] = matching
        original_manifest = MANIFEST.read_bytes()

        with tempfile.TemporaryDirectory(prefix="noon-raster-manifest-") as directory:
            outside = Path(directory)

            def capture(config, manifest_path, cwd, name):
                manifest_path.write_text(json.dumps(config), encoding="utf-8")
                output = outside / f"{name}-frames.json"
                result = subprocess.run(
                    [sys.executable, str(ORACLE), "--manifest", str(manifest_path),
                     "--output", str(output)],
                    cwd=cwd, text=True, capture_output=True, timeout=60,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return json.loads(output.read_text(encoding="utf-8"))

            # Keep the original manifest depth for the backward-compatible case.
            with tempfile.NamedTemporaryFile(
                dir=MANIFEST.parent, prefix=".manifest-test-", suffix=".json"
            ) as canonical:
                expected = capture(manifest, Path(canonical.name), ROOT, "canonical")

            relocated = capture(manifest, outside / "selected.json", outside, "relocated")
            self.assertEqual(relocated, expected)

            fallback = copy.deepcopy(manifest)
            fallback["reference"]["source"] = fallback["fixtures"][0].pop("source")
            self.assertEqual(
                capture(fallback, outside / "fallback.json", outside, "fallback"),
                expected,
            )

            fixture = expected["fixtures"][0]
            self.assertEqual(expected["manim_version"], "0.21.0")
            self.assertAlmostEqual(fixture["logical_duration"], 2.2)
            times = [frame["time"] for frame in fixture["frames"]]
            for time in (0.0, 0.5, 1.0, 1.5, 2.0, 2.1):
                self.assertTrue(any(abs(actual - time) < 1e-9 for actual in times), time)

        self.assertEqual(MANIFEST.read_bytes(), original_manifest)


if __name__ == "__main__":
    unittest.main()
