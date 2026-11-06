"""Real CPython extension and native GPU proof; run with PYTHONPATH=build/python."""
import asyncio
from pathlib import Path
import sys
import tempfile
import unittest

import _noon_native
from noon_native import close_scene, prepare_view, run_scene, run_source

ROOT = Path(__file__).resolve().parents[1]


class NativePythonTests(unittest.TestCase):
    def test_published_paired_examples_execute_without_a_browser(self):
        self.assertIn(Path(_noon_native.__file__).suffix, {".so", ".pyd"})
        for name in ("ordinary_affine_play", "ordinary_composition_play",
                     "ordinary_text_write", "ordinary_fade_play", "shared_text", "raster_image",
                     "glow_pulse", "ordered_effects", "composed_effects", "noise_dissolve",
                     "linear_gradient", "radial_gradient", "noise_paint", "hatch_paint",
                     "image_fill", "showcase_layered_effects", "showcase_painted_surfaces",
                     "ordinary_path_construction", "ordinary_path_editing", "ordinary_path_smoothing",
                     "ordinary_path_refinement", "ordinary_path_selection", "ordinary_path_alignment",
                     "showcase_bezier_paths", "manim_parity_family_slices",
                     "coordinate_plotting", "animated_number_line", "live_coordinate_plotting",
                     "number_plane", "polar_plane", "manim_example_moving_camera_center",
                     "camera_bloom", "showcase_bloom_emission", "pixelate", "scope_opacity",
                     "showcase_glass_lens", "showcase_camera_follows_path",
                     "ordinary_family_arrangement", "top_level_family_arrangement", "ordinary_family_grid",
                     "tutorial_positioning", "tutorial_composition", "ordinary_value_tracker_continuation",
                     "ordinary_mixed_scalar_composition", "tutorial_value_tracker"):
            with self.subTest(example=name):
                path = ROOT / "web/python/examples" / (name + ".py")
                scene = asyncio.run(run_source(path.read_text(), filename=str(path)))
                close_scene(scene)
        self.assertNotIn("js", sys.modules)
        self.assertNotIn("pyodide", sys.modules)

    def test_deterministic_segment_has_no_per_frame_python_continuations(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        self.circle = Circle(0.4)
        self.add(self.circle)
        assert abs(self.circle.width-0.8)<1e-6
        self.play(self.circle.animate.shift(RIGHT), run_time=100, rate_func=linear)
        self.circle.scale(2)
        assert abs(self.circle.width-1.6)<1e-6
"""
        scene = asyncio.run(run_source(source))
        try:
            self.assertEqual(scene._canonical_authoring_context.metrics(), (1, 1, 0))
            self.assertEqual(tuple(scene.circle.get_center()), (1.0, 0.0, 0.0))
        finally:
            close_scene(scene)

    def test_prepared_native_pixels_rewind_membership_and_preserve_output(self):
        source = """from noon import *
context["runs"] += 1
class Demo(Scene):
    def construct(self):
        c = Circle(0.4, color="#ff0000").set_fill("#ff0000", opacity=1).set_stroke(width=0)
        c.shift(LEFT)
        self.add(c)
        self.play(c.animate.shift(2*RIGHT), run_time=1, rate_func=linear)
        self.wait(0.5)
        self.remove(c)
        self.wait(0.5)
"""
        context = {"runs": 0}
        view = prepare_view(asyncio.run(run_source(source, context, retain_history=True)))
        try:
            self.assertEqual(view.duration, 2.0)
            first = None
            for time, expected_x in ((0.25, -0.5), (0.75, 0.5), (1.75, None), (0.25, -0.5)):
                pixels = view.capture(time=time, width=320, height=180)
                self.assertEqual(len(pixels), 320 * 180 * 4)
                red = [index // 4 % 320 for index in range(0, len(pixels), 4)
                       if pixels[index] > 180 and pixels[index + 1] < 60 and pixels[index + 2] < 60]
                if expected_x is None:
                    self.assertEqual(red, [])
                else:
                    self.assertGreater(len(red), 100)
                    # Default Rust camera: eight world units over 180 pixels.
                    self.assertAlmostEqual(sum(red) / len(red), 159.5 + 22.5 * expected_x, delta=1)
                if time == 0.25:
                    if first is None:
                        first = pixels
                    else:
                        self.assertEqual(first, pixels)
            for invalid in (-1, 2.1, float("nan")):
                with self.assertRaises(RuntimeError):
                    view.capture(time=invalid, width=320, height=180)
            self.assertEqual(context["runs"], 1)
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "native.png"
                view.snapshot(path, time=0.25, width=320, height=180)
                original = path.read_bytes()
                self.assertTrue(original.startswith(b"\x89PNG\r\n\x1a\n"))
                with self.assertRaises(RuntimeError):
                    view.snapshot(path, width=320, height=180)
                self.assertEqual(path.read_bytes(), original)
        finally:
            view.close()
        view.close()
        with self.assertRaises(RuntimeError):
            view.capture()

    def test_path_edits_keep_full_coordinates_and_reject_partial_writes(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        precise = 1 + 2**-40
        path = VMobject().start_new_path((precise, 0, 0.25))
        path.add_line_to((2, 1, 0.5))
        assert tuple(path.get_all_points()[0]) == (precise, 0, 0.25)
        self.add(path)
        self.wait(0.1)
        path.add_line_to((3, 0, 0.75))
        before = [tuple(point) for point in path.get_all_points()]
        assert before[0] == (precise, 0, 0.25)
        assert before[-1] == (3, 0, 0.75)
        try:
            path.set_points([(float("nan"), 0, 0)] * 4)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid coordinates were admitted")
        assert before == [tuple(point) for point in path.get_all_points()]
        self.wait(0.1)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_plot_callable_samples_once_against_the_effective_axes(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        axes = Axes((-1, 1, 0.5), (-1, 1, 0.5), tips=False)
        self.add(axes)
        self.wait(0.1)
        axes.shift(RIGHT)
        samples = []
        def sample(x):
            samples.append(x)
            return x
        graph = axes.plot(sample, (-1, 1, 0.5), use_smoothing=False)
        start, expected = graph.get_start(), axes.c2p(-1, -1)
        assert abs(start[0] - expected[0]) < 1e-6
        assert abs(start[1] - expected[1]) < 1e-6
        assert len(samples) == 5
        sampled = tuple(samples)
        self.add(graph)
        self.play(graph.animate.shift(UP), run_time=2, rate_func=linear)
        assert tuple(samples) == sampled
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_native_camera_motion_and_rewind_use_the_same_publication(self):
        source = """from noon import *
class Demo(MovingCameraScene):
    def construct(self):
        dot = Circle(0.4, fill="#ff0000", fill_opacity=1, stroke=None).shift(2 * RIGHT)
        self.add(dot)
        self.play(self.camera.frame.animate.shift(2 * RIGHT), run_time=1, rate_func=linear)
"""
        view = prepare_view(asyncio.run(run_source(source, retain_history=True)))
        try:
            first = None
            for time in (0.25, 0.75, 0.25):
                pixels = view.capture(time=time, width=320, height=180)
                red = [index // 4 % 320 for index in range(0, len(pixels), 4)
                       if pixels[index] > 180 and pixels[index + 1] < 60 and pixels[index + 2] < 60]
                self.assertGreater(len(red), 100)
                self.assertAlmostEqual(sum(red) / len(red), 159.5 + 22.5 * (2 - 2 * time), delta=1)
                if time == 0.25:
                    if first is None:
                        first = pixels
                    else:
                        self.assertEqual(first, pixels)
        finally:
            view.close()

    def test_native_scalar_binding_rewinds_across_a_persistent_write(self):
        path = ROOT / "web/python/examples/ordinary_value_tracker_continuation.py"
        view = prepare_view(asyncio.run(run_source(path.read_text(), filename=str(path),
                                                  retain_history=True)))
        try:
            first = None
            for time, expected_x in ((0.5, -1.5), (1.5, -0.5), (2.5, 1), (3.5, 2), (0.5, -1.5)):
                pixels = view.capture(time=time, width=320, height=180)
                white = [index // 4 % 320 for index in range(0, len(pixels), 4)
                         if min(pixels[index:index + 3]) > 180]
                self.assertGreater(len(white), 100)
                self.assertAlmostEqual(sum(white) / len(white), 159.5 + 22.5 * expected_x, delta=1)
                if time == 0.5:
                    if first is None:
                        first = pixels
                    else:
                        self.assertEqual(first, pixels)
        finally:
            view.close()

    def test_native_gradient_pixels_animate_and_rewind(self):
        path = ROOT / "web/python/examples/linear_gradient.py"
        view = prepare_view(asyncio.run(run_source(path.read_text(), filename=str(path),
                                                  retain_history=True)))
        try:
            first = view.capture(time=0.25, width=320, height=180)
            left, right = ((90 * 320 + x) * 4 for x in (135, 185))
            self.assertGreater(first[left], first[left + 2])
            self.assertGreater(first[right + 2], first[right])
            solid = view.capture(time=2.875, width=320, height=180)
            for offset in (left, right):
                self.assertEqual(tuple(solid[offset:offset + 3]), (255, 0, 0))
            self.assertEqual(first, view.capture(time=0.25, width=320, height=180))
        finally:
            view.close()

    def test_composed_dissolve_replay_preserves_effects_and_membership(self):
        path = ROOT / "web/python/examples/dissolve.py"
        view = prepare_view(asyncio.run(run_source(path.read_text(), filename=str(path),
                                                  retain_history=True)))
        try:
            first = None
            for time, visible in ((0.25, True), (1.75, False), (3.25, True),
                                  (4.75, False), (6.25, True), (0.25, True)):
                pixels = view.capture(time=time, width=320, height=180)
                right = bytes(channel for y in range(180) for x in range(160, 320)
                              for channel in pixels[(y * 320 + x) * 4:(y * 320 + x) * 4 + 3])
                self.assertEqual(any(right), visible)
                if visible:
                    if first is None:
                        first = pixels
                    else:
                        self.assertEqual(first, pixels)
        finally:
            view.close()

    def test_interleaved_failure_retires_only_its_acquisitions(self):
        from noon import Circle, RIGHT, Scene
        import _manim_updaters

        async def execute():
            ready, release = asyncio.Event(), asyncio.Event()
            class Failing(Scene):
                async def construct(self):
                    circle = Circle(0.5)
                    self.add(circle)
                    circle.add_updater(lambda m, dt: None)
                    ready.set()
                    await release.wait()
                    raise ValueError("source aborted")
            class Survivor(Scene):
                async def construct(self):
                    self.circle = Circle(0.5)
                    self.add(self.circle)
                    self.circle.add_updater(lambda m, dt: m.shift(dt * RIGHT))
                    await self.wait(0.25)
            task = asyncio.create_task(run_scene(Failing))
            await ready.wait()
            survivor = await run_scene(Survivor, sample_hz=4)
            try:
                release.set()
                with self.assertRaisesRegex(ValueError, "source aborted"):
                    await task
                self.assertEqual(tuple(survivor.circle.get_center()), (0.25, 0, 0))
            finally:
                close_scene(survivor)
        before = len(_manim_updaters._CANONICAL_SESSIONS)
        asyncio.run(execute())
        self.assertEqual(len(_manim_updaters._CANONICAL_SESSIONS), before)


if __name__ == "__main__":
    unittest.main()
