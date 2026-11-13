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
                     "glow_pulse", "ordered_effects", "composed_effects", "effect_restore",
                     "noise_dissolve",
                     "linear_gradient", "radial_gradient", "noise_paint", "hatch_paint",
                     "image_fill", "showcase_layered_effects", "showcase_painted_surfaces",
                     "ordinary_path_construction", "ordinary_path_editing", "ordinary_path_smoothing",
                     "ordinary_path_refinement", "ordinary_path_selection", "ordinary_path_alignment",
                     "showcase_bezier_paths", "manim_parity_family_slices",
                     "manim_parity_object_members", "manim_parity_family_member_queries",
                     "manim_parity_point_promotion", "ordinary_subset_display",
                     "coordinate_plotting", "animated_number_line", "live_coordinate_plotting",
                     "number_plane", "polar_plane", "manim_example_moving_camera_center",
                     "camera_bloom", "showcase_bloom_emission", "pixelate", "scope_opacity",
                     "showcase_glass_lens", "showcase_camera_follows_path",
                     "ordinary_family_arrangement", "top_level_family_arrangement", "ordinary_family_grid",
                     "ordinary_planar_affine", "ordinary_dimension_fitting",
                     "ordinary_circle_surround", "ordinary_family_replacement",
                     "tutorial_positioning", "tutorial_composition", "ordinary_value_tracker_continuation",
                     "ordinary_mixed_scalar_composition", "tutorial_value_tracker",
                     "manim_example_grow_from_point", "manim_example_grow_from_edge",
                     "manim_example_spin_in_from_nothing", "manim_parity_shrink_to_center",
                     "manim_compatible_family_indicate", "ordinary_focus_on", "line_passing_flash",
                     "ordinary_draw_border_then_fill", "ordinary_text_family_fade",
                     "ordinary_text_family_reveal", "ordinary_text_family_write", "ordinary_arc_geometry",
                     "manim_example_typst_text", "manim_example_math_typst", "ordinary_automatic_wait_text",
                     "showcase_editorial_cover", "live_affine_animation", "live_affine_completion",
                     "live_masked_placement", "ordinary_family_placement",
                     "live_semantic_scene", "live_content_switch", "ordinary_become_semantics",
                     "shader_time", "showcase_wave_interference", "showcase_shader_material",
                     "noon_spatial_surface_lighting", "showcase_spatial_scene", "noon_spatial_three_d_axes",
                     "manim_example_three_d_camera_rotation", "manim_example_three_d_camera_illusion_rotation",
                     "manim_example_three_d_surface_plot", "live_line_match_callback",
                     "manim_gallery_moving_dots", "showcase_reactive_relationships",
                     "ordinary_animated_stroke_width", "ordinary_z_index", "ordinary_animated_priority"):
            with self.subTest(example=name):
                path = ROOT / "web/python/examples" / (name + ".py")
                scene = asyncio.run(run_source(path.read_text(), filename=str(path)))
                if name == "live_line_match_callback":
                    line = scene.mobjects[-1]
                    self.assertAlmostEqual(line.get_start().x, 2.0, delta=1e-5)
                    self.assertAlmostEqual(line.get_end().x, 0.5, delta=1e-5)
                close_scene(scene)
        for name, selection in (
            ("noon_spatial_surface", "SpatialSurface"), ("noon_spatial_surface", "CairoSpatialSurface"),
            ("noon_spatial_surface", "CairoSphereTorus"), ("noon_spatial_surface", "CairoConeBodies"),
            ("noon_spatial_primitives", "SpatialPrimitives"), ("noon_spatial_primitives", "SpatialCairoCubePrism"),
            ("noon_spatial_primitives", "SpatialCairoCylinderConeCaps"),
        ):
            with self.subTest(example=name, scene=selection):
                path = ROOT / "web/python/examples" / (name + ".py")
                scene = asyncio.run(run_source(path.read_text(), filename=str(path), scene_name=selection))
                close_scene(scene)
        self.assertNotIn("js", sys.modules)
        self.assertNotIn("pyodide", sys.modules)

    def test_scene_path_edits_match_during_setup_and_playback(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        path = VMobject()
        target = VMobject().set_points_as_corners([(1, 2, 0), (3, 2, 0)])
        family = VGroup(VMobject().set_points_as_corners([(0, 0, 0), (0, 1, 0)]))
        self.add(path, target, family)
        for _ in range(2):
            path.match_points(target)
            assert list(path.get_start()) == [1, 2]
            assert list(path.get_subcurve(0.25, 0.75).get_start()) == [1.5, 2]
            child = family[0]
            family.reverse_points()
            assert list(child.get_start()) == [0, 1]
            family.reverse_points()
            path.clear_points()
            assert path.get_num_points() == 0
            self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_native_style_edits_preserve_replay(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        line = Line(LEFT, RIGHT, color=WHITE, stroke_width=2)
        self.add(line)
        self.wait(1)
        line.set_stroke(RED, width=20)
        assert abs(line.get_stroke_width() - 20) < 1e-5
        self.wait(1)
"""
        view = prepare_view(asyncio.run(run_source(source, retain_history=True)))
        try:
            initial = view.capture(time=0.5, width=160, height=90)
            self.assertNotEqual(initial, view.capture(time=1.5, width=160, height=90))
            self.assertEqual(initial, view.capture(time=0.5, width=160, height=90))
        finally:
            view.close()

    def test_live_coordinate_construction_stays_detached_until_add(self):
        source = """from noon import *

class Demo(ThreeDScene):
    def construct(self):
        marker = Dot(4 * LEFT)
        self.add(marker)
        self.wait(0.1)
        try:
            NumberPlane(y_length=float("nan"))
        except ValueError:
            pass
        else:
            raise AssertionError("invalid live plane was admitted")
        coordinates = (
            NumberLine((-1, 1, 1)),
            Axes((-1, 1, 1), (-1, 1, 1), tips=False),
            NumberPlane((-1, 1, 1), (-1, 1, 1)),
            PolarPlane(radius_max=1, size=2, azimuth_step=4),
            ThreeDAxes((-1, 1, 1), (-1, 1, 1), (-1, 1, 1)),
        )
        assert self.mobjects == [marker]
        self.add(*coordinates[:-1])
        self.add_world_mobjects(coordinates[-1])
        assert self.mobjects == [marker, *coordinates]
        axes = coordinates[-1]
        origin = axes.c2p(0, 0, 0)
        delta = (1, -2, 0.5)
        self.shift_world(axes, delta)
        assert all(abs(actual - expected - shift) < 1e-6
                   for actual, expected, shift in zip(axes.c2p(0, 0, 0), origin, delta))
        self.wait(0.1)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_live_planar_coordinate_queries_observe_detached_and_animating_axes(self):
        source = """from noon import *
from math import isclose

def assert_point(actual, expected):
    assert len(actual) == len(expected), (actual, expected)
    assert all(isclose(a, b, rel_tol=0, abs_tol=1e-6)
               for a, b in zip(actual, expected)), (actual, expected)

class Demo(Scene):
    def construct(self):
        plane = PolarPlane(radius_max=2, size=4, azimuth_step=4)
        axes = Axes((-2, 2, 1), (-2, 2, 1), x_length=4, y_length=4, tips=False)
        self.add(plane, axes)
        shaft = plane.x_axis.shaft
        target = shaft.copy().stretch(2, 0, about_point=ORIGIN)
        animation = self.declare_live_transform_to(shaft, target, run_time=1, rate_func=linear)
        axis_shaft = axes.x_axis.shaft
        axis_target = axis_shaft.copy().stretch(2, 0, about_point=ORIGIN)
        axis_animation = self.declare_live_transform_to(axis_shaft, axis_target, run_time=1, rate_func=linear)
        live = self.live_execution()
        end = live.play(animation)
        live.advance_to(0.5)
        assert_point(plane.polar_to_point(1, 0), [1.5, 0])
        assert_point(plane.point_to_polar([1.5, 0]), [1, 0])
        context = self._canonical_authoring_context
        family = plane._semantic_family_handle
        cartesian = context.queryNumberPlaneFrame(family)
        assert_point(cartesian.coordsToPoint(1, 0.5), [1.5, 0.5])
        assert_point(cartesian.pointToCoords(1.5, 0.5), [1, 0.5])
        live.advance_to(end)
        live.complete()
        self.remove(plane)
        polar = context.queryPolarPlaneFrame(family)
        assert_point(polar.polarToPoint(1, 0), [2, 0])
        assert_point(polar.pointToPolar(2, 0), [1, 0])
        cartesian = context.queryNumberPlaneFrame(family)
        assert_point(cartesian.coordsToPoint(1, 0.5), [2, 0.5])
        assert_point(cartesian.pointToCoords(2, 0.5), [1, 0.5])

        end = live.play(axis_animation)
        live.advance_to(end - 0.5)
        assert_point(axes.c2p(1, 0.5), [1.5, 0.5])
        assert_point(axes.x_axis.n2p(1), [1.5, 0])
        live.advance_to(end)
        live.complete()
        self.remove(axes)
        assert_point(context.queryAxesFrame(axes._semantic_family_handle).coordsToPoint(1, 0.5), [2, 0.5])
        assert_point(context.queryNumberLineFrame(axes.x_axis._semantic_family_handle).numberToPoint(1), [2, 0])
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_callback_line_targets_reject_invalid_coordinates_and_foreign_owners(self):
        first, second = _noon_native.Store(), _noon_native.Store()
        context, other = first.context(), second.context()
        try:
            target = other.callbackLineTarget(0, 0, 1, 1)
            line = first.geometry(_noon_native.GeometryOptions.line(0, 0, 1, 0))
            with self.assertRaises(RuntimeError):
                context.callbackMatchLineTransform(line, target)
            for invalid in (float("nan"), float("inf"), 1e100):
                with self.assertRaises(RuntimeError):
                    context.callbackLineTarget(invalid, 0, 1, 1)
            context.retire()
            with self.assertRaises(RuntimeError):
                context.callbackLineTarget(0, 0, 1, 1)
        finally:
            context.retire()
            other.retire()

    def test_nested_introducers_publish_observable_native_handles(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        line = Line(LEFT, RIGHT)
        self.play(AnimationGroup(Create(line)), run_time=1)
        assert tuple(line.get_start()) == (-1.0, 0.0)
        self.play(line.animate.shift(UP), run_time=1)
        assert tuple(line.get_end()) == (1.0, 1.0)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

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

    def test_manual_live_segments_preserve_rejection_and_replay(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        c = Circle(0.4, fill=RED, fill_opacity=1, stroke=None).shift(LEFT)
        self.add(c)
        start_x = c.get_start()[0]
        animation = self.declare_live_transform_to(c, c.copy().shift(2*RIGHT), run_time=1, rate_func=linear)
        live = self.live_execution(duration=2)
        end = live.play(animation)
        assert not live.advance_to(0.5)
        assert abs(c.get_start()[0] - start_x - 1) < 1e-6
        for invalid in (live.complete, lambda: live.wait(1), lambda: live.evaluate(3),
                        lambda: self.live_execution(duration=0.25)):
            try:
                invalid()
            except (ValueError, RuntimeError):
                pass
            else:
                raise AssertionError("invalid live operation was admitted")
            assert tuple(c.get_center()) == (0.0, 0.0, 0.0)
        assert not live.advance_to(end)
        live.complete()
        live.evaluate(1.5)
        assert tuple(c.get_center()) == (1.0, 0.0, 0.0)
"""
        view = prepare_view(asyncio.run(run_source(source, retain_history=True)))
        try:
            self.assertEqual(view.duration, 1.5)
            first = view.capture(time=0.25, width=160, height=90)
            self.assertNotEqual(first, view.capture(time=0.75, width=160, height=90))
            self.assertEqual(first, view.capture(time=0.25, width=160, height=90))
        finally:
            view.close()

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
        assert path.reduce_across_dimension(min, 0) == precise
        group = Group(path, Line((-3, 0, 0), (-2, 0, 0)))
        assert group.reduce_across_dimension(min, 0) == -3
        self.add(group)
        self.wait(0.1)
        path.add_line_to((3, 0, 0.75))
        before = [tuple(point) for point in path.get_all_points()]
        assert before[0] == (precise, 0, 0.25)
        assert before[-1] == (3, 0, 0.75)
        assert path.reduce_across_dimension(max, 2) == 0.75
        calls = []
        def reducer(values):
            calls.append(len(values))
            return max(values)
        assert group.reduce_across_dimension(reducer, 2) == 0.75
        assert calls == [len(before), 4, 2]
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

    def test_native_typst_resources_and_sections_survive_live_admission_and_rewind(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        self.next_section("Title")
        self.add(Typst("*Native*", color=YELLOW).shift(LEFT))
        self.wait(1)
        self.next_section("Math")
        self.add(MathTypst("frac(x, 2)", color=BLUE).shift(RIGHT))
        self.wait(1)
"""
        scene = asyncio.run(run_source(source, retain_history=True))
        self.assertEqual(scene.sections, [
            {"name": "Title", "type": "normal", "time": 0.0},
            {"name": "Math", "type": "normal", "time": 1.0},
        ])
        view = prepare_view(scene)
        try:
            first = view.capture(time=0.5, width=160, height=90)
            second = view.capture(time=1.5, width=160, height=90)
            self.assertNotEqual(first, second)
            self.assertEqual(first, view.capture(time=0.5, width=160, height=90))
        finally:
            view.close()

    def test_native_surface_samples_once_and_replays_world_light_motion(self):
        source = """from noon import *
def sample(u, v):
    context["samples"] += 1
    return u, v, 0.25*u*v
class Demo(SpatialScene):
    def construct(self):
        surface = Mesh3D.parametric(sample, u_range=(-1.5, 1.5), v_range=(-1.5, 1.5),
                                    resolution=(2, 2), color=Color(0.2, 0.3, 0.4), point_lit=True)
        light = self.point_light(position=(4, -3, 6), intensity=0.3)
        self.add(surface, light)
        self.play(WorldTransformTo(light, translation=(-3, 3, 4)), run_time=1, rate_func=linear)
        assert tuple(light.get_center()) == (-3.0, 3.0, 4.0)
"""
        context = {"samples": 0}
        view = prepare_view(asyncio.run(run_source(source, context, retain_history=True)))
        try:
            first = view.capture(time=0.1, width=160, height=90)
            self.assertGreater(sum(first[0::4]), 1000)
            self.assertNotEqual(first, view.capture(time=0.8, width=160, height=90))
            self.assertEqual(first, view.capture(time=0.1, width=160, height=90))
            self.assertEqual(context["samples"], 9)
        finally:
            view.close()

    def test_named_scene_selection_executes_one_construct(self):
        source = """from noon import *
context["module"] += 1
class First(Scene):
    def construct(self):
        raise AssertionError("unselected construct ran")
class Second(Scene):
    def construct(self):
        context["construct"] += 1
        self.wait(0.25)
"""
        context = {"module": 0, "construct": 0}
        scene = asyncio.run(run_source(source, context, scene_name="Second"))
        close_scene(scene)
        self.assertEqual(context, {"module": 1, "construct": 1})
        with self.assertRaises(ValueError):
            asyncio.run(run_source(source, context, scene_name="Second()"))
        self.assertEqual(context, {"module": 1, "construct": 1})

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

    def test_native_shader_preparation_gates_and_replay_use_real_device_resources(self):
        source = '''from noon import *
import math
schema = [ShaderParameter.scalar("gain", default=-0.0, minimum=0, maximum=1),
          ShaderParameter.vector2("offset", default=(0, 1), minimum=(0, 0), maximum=(1, 1)),
          ShaderParameter.color("tint", default=RED), ShaderParameter.boolean("enabled", default=True),
          ShaderParameter.unsigned("steps", default=3)]
wgsl = "fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> { return vec4<f32>(p.tint.rgb * p.gain, 1.0); }"
program = WgslEffectProgram(wgsl, parameters=schema)
key = program.parameter("gain")
definition = ShaderEffect(program)
assert math.copysign(1, definition.value(key)) == -1
assert definition.value(program.parameter("offset")) == (0.0, 1.0)
assert definition.value(program.parameter("enabled")) is True
assert definition.value(program.parameter("steps")) == 3
other = WgslEffectProgram(wgsl, parameters=schema, coordinates="scene")
try:
    ShaderEffect(program, parameters={other.parameter("gain"): 1})
except ValueError:
    pass
else:
    raise AssertionError("foreign shader key was admitted")
entering = Rectangle(width=2, height=2, fill=WHITE, fill_opacity=1, stroke=None).shift(3*LEFT)
entering.add_effect(ShaderEffect(other, parameters={other.parameter("gain"): 1}), name="material")
third = WgslEffectProgram(wgsl, parameters=schema, coordinates="pixels")
appended = Rectangle(width=2, height=2, fill=WHITE, fill_opacity=1, stroke=None).shift(3*RIGHT)
appended.add_effect(ShaderEffect(third, parameters={third.parameter("gain"): 1}), name="material")
class Demo(Scene):
    def construct(self):
        panel = Rectangle(width=2, height=2, fill=WHITE, fill_opacity=1, stroke=None)
        self.add(panel)
        self.wait(0.25)
        panel.add_effect(definition, name="material")
        self.wait(0.25)
        self.play(panel.animate.set_effect("material", parameters={key: 1}), Create(entering),
                  run_time=1, rate_func=linear)
        assert tuple(entering.get_center()) == (-3.0, 0.0, 0.0)
        self.add(appended)
        self.wait(0.25)
'''
        with self.assertRaises(NotImplementedError) as rejected:
            asyncio.run(run_source(source, prepare_shaders=False))
        self.assertEqual(rejected.exception.code, "shader.device_preparation_required")
        view = prepare_view(asyncio.run(run_source(source, retain_history=True)))
        try:
            first = view.capture(time=0.1, width=160, height=90)
            before = first[(45*160+80)*4:(45*160+80)*4+3]
            dark = view.capture(time=0.3, width=160, height=90)
            after = view.capture(time=1.6, width=160, height=90)
            self.assertTrue(all(channel > 240 for channel in before))
            self.assertEqual(dark[(45*160+80)*4:(45*160+80)*4+3], bytes(3))
            self.assertGreater(after[(45*160+80)*4], 200)
            self.assertEqual(first, view.capture(time=0.1, width=160, height=90))
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
