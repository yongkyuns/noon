"""Real CPython extension and native GPU proof; run with PYTHONPATH=build/python."""
import asyncio
import os
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
                     "showcase_code_blocks",
                     "glow_pulse", "ordered_effects", "composed_effects", "effect_restore",
                     "noise_dissolve",
                     "linear_gradient", "radial_gradient", "noise_paint", "hatch_paint",
                     "image_fill", "showcase_layered_effects", "showcase_painted_surfaces",
                     "ordinary_path_construction", "ordinary_path_editing", "ordinary_path_smoothing",
                     "ordinary_path_refinement", "ordinary_path_selection", "ordinary_path_alignment",
                     "showcase_bezier_paths", "manim_parity_family_slices",
                     "manim_parity_object_members", "ordinary_family_membership_order", "manim_parity_family_member_queries",
                     "manim_parity_point_promotion", "ordinary_subset_display",
                     "coordinate_plotting", "animated_number_line", "live_coordinate_plotting", "area_helpers",
                     "number_plane", "polar_plane", "manim_example_moving_camera_center",
                     "camera_bloom", "showcase_bloom_emission", "pixelate", "scope_opacity",
                     "showcase_glass_lens", "showcase_camera_follows_path",
                     "ordinary_family_arrangement", "top_level_family_arrangement", "ordinary_family_grid",
                     "ordinary_planar_affine", "ordinary_dimension_fitting", "ordinary_arrow_geometry", "manim_compatible_arrow_vector_field",
                     "ordinary_live_primitive_construction", "ordinary_paint_queries_gradients", "ordinary_style_operations", "ordinary_family_paint",
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
                     "manim_parity_cyclic_replace",
                     "shader_time", "showcase_wave_interference", "showcase_shader_material",
                     "noon_spatial_surface_lighting", "showcase_spatial_scene", "noon_spatial_three_d_axes",
                     "manim_example_three_d_camera_rotation", "manim_example_three_d_camera_illusion_rotation",
                     "manim_example_three_d_surface_plot", "live_line_match_callback",
                     "manim_gallery_moving_dots", "showcase_reactive_relationships",
                     "ordinary_graph",
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

    def test_area_and_riemann_rectangles_follow_effective_axes(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        axes = Axes([-1, 1, 1], [-1, 1, 1], x_length=2, y_length=2, tips=False)
        calls = []
        def top(x):
            calls.append(("top", x))
            return x * x
        def lower(x):
            calls.append(("lower", x))
            return -0.25
        graph = axes.plot(top, [-1, 1, 1], use_smoothing=False)
        bound = axes.plot(lower, [-1, 1, 1], use_smoothing=False)
        def rectangles():
            calls.clear()
            family = axes.get_riemann_rectangles(
                graph, dx=0.5, input_sample_type="center",
                width_scale_factor=1, bounded_graph=bound)
            assert calls == [
                (kind, x) for start in (-1, -0.5, 0, 0.5)
                for kind, x in (("top", start + 0.25), ("lower", start))
            ]
            assert len(family) == 4
            actual, expected = family[0].get_start(), axes.c2p(-0.5, 0.5625)
            assert abs(actual.x - expected.x) < 1e-5 and abs(actual.y - expected.y) < 1e-5
            return family
        subject = VGroup(axes, graph, bound)
        self.add(subject)
        before = rectangles()
        self.add(before)
        self.play(subject.animate.shift(RIGHT), run_time=0.125)
        after = rectangles()
        assert after[0].get_start().x > before[0].get_start().x
        area = axes.get_area(graph, [-1, 0], bounded_graph=bound, color=GREEN, opacity=0.4)
        actual, expected = area.get_start(), axes.c2p(-1, 1)
        assert abs(actual.x - expected.x) < 1e-5 and abs(actual.y - expected.y) < 1e-5
        assert abs(area.get_fill_opacity() - 0.4) < 1e-6
        self.add(area)
        self.add(after)
        self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_native_coordinate_text_labels_publish_during_playback(self):
        source = """from noon import *
class Demo(Scene):
    async def construct(self):
        axes = Axes((-2, 2, 1), (-2, 2, 1), tips=False)
        self.add(axes)
        axes.add_coordinates((-1, 1), (-1, 1), font="DejaVu Sans Mono")
        assert len(axes.x_axis.numbers) == len(axes.y_axis.numbers) == 2
        before = axes.x_axis.numbers[1].get_center()
        await self.play(axes.animate.shift(RIGHT), run_time=0.125)
        axes.x_axis.add_labels({1: "X"})
        assert axes.x_axis.labels[0].text == "X"
        numbers = axes.x_axis.get_number_mobjects(1, font="DejaVu Sans Mono")
        assert abs(numbers[0].get_center()[0] - before[0] - 1) < 1e-5
        axes.add_coordinates((1,), (1,), font="DejaVu Sans Mono")
        assert len(axes.x_axis.numbers) == len(axes.y_axis.numbers) == 1
        await self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_decimal_coordinate_labels_publish_during_playback(self):
        source = """from noon import *
class Demo(Scene):
    async def construct(self):
        await prepare_latex()
        axes = Axes((-2, 2, 1), (-2, 2, 1), tips=False)
        self.add(axes)
        axes.add_coordinates((1,), (1,))
        before = axes.x_axis.numbers[0].get_center()
        await self.play(axes.animate.shift(RIGHT), run_time=0.125)
        axes.add_coordinates((1,), (-1, 1))
        assert len(axes.y_axis.numbers) == 2
        x_label = axes.x_axis.numbers[0]
        assert x_label.get_value() == 1
        assert abs(x_label.get_center()[0] - before[0] - 1) < 1e-5
        axes.x_axis.add_numbers((2,))
        assert axes.x_axis.numbers[0].get_value() == 2
        await self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_vector_field_samples_and_copies_during_playback(self):
        source = """from noon import *
class Demo(Scene):
    async def construct(self):
        anchor = Circle(0.25).move_to(2 * LEFT)
        self.add(anchor)
        await self.wait(0.125)
        points, lengths, values = [], [], []
        def field(point):
            points.append(point)
            return point
        def length(norm):
            lengths.append(norm)
            return 0.5
        def color(vector):
            values.append(vector)
            return abs(vector[0])
        vectors = ArrowVectorField(
            field, x_range=(0, 1, 1), y_range=(0, 0, 1),
            length_func=length, color_scheme=color, colors=(BLUE, RED),
        )
        assert points == [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0)]
        assert values == points and lengths == [1.0]
        assert len(vectors) == 2 and abs(vectors[1].get_length() - 0.5) < 1e-6
        copied = vectors.copy()
        copied[1].scale(2)
        assert abs(copied[1].get_length() - 1.0) < 1e-6
        assert abs(vectors[1].get_length() - 0.5) < 1e-6
        copied.shift(UP)
        self.add(vectors, copied)
        await self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_tangent_line_uses_the_published_path_after_a_play(self):
        source = """from noon import *
class Demo(Scene):
    async def construct(self):
        path = Circle()
        self.add(path)
        before = TangentLine(path, 0.2, length=0.75, d_alpha=0.01)
        self.add(before)
        await self.play(path.animate.shift(2 * RIGHT), run_time=0.125)
        after = TangentLine(path, 0.2, length=0.75, d_alpha=0.01)
        assert abs(after.get_start()[0] - before.get_start()[0] - 2) < 1e-5
        assert abs(after.get_end()[1] - before.get_end()[1]) < 1e-5
        self.add(after)
        await self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_svg_import_after_wait_keeps_playback_coherent(self):
        source = """from noon import *
class Demo(Scene):
    async def construct(self):
        svg = '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>'
        before = SVGMobject.from_string(svg, height=2, fill_color=BLUE)
        self.add(before)
        await self.wait(0.125)
        after = SVGMobject.from_string(svg, height=2, fill_color=BLUE).shift(RIGHT)
        assert len(after) == 1
        assert abs(after.get_center()[0] - before.get_center()[0] - 1) < 1e-6
        self.add(after)
        await self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_three_d_axis_labels_publish_after_playback_starts(self):
        source = """from noon import *
class Demo(ThreeDScene):
    async def construct(self):
        axes = ThreeDAxes()
        self.add_world_mobjects(axes)
        before = axes.get_x_axis_label(Text("X"))
        self.add_world_mobjects(before)
        await self.wait(0.125)
        self.shift_world(axes, (1, 0, 0))
        after = axes.get_x_axis_label(Text("X"))
        self.add_world_mobjects(after)
        context = self._canonical_authoring_context
        def center(label):
            return context.effectiveWorldFamilyCenter(label._semantic_family_handle)[0]
        assert abs(center(after) - center(before) - 1) < 1e-5
        await self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_code_listing_constructed_during_playback_can_be_copied(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        self.add(Dot())
        self.wait(0.125)
        listing = Code(code_string="value = 1", language="python", add_line_numbers=False)
        duplicate = listing.copy()
        assert duplicate.code_string == listing.code_string == "value = 1"
        assert duplicate.code_lines[0] is not listing.code_lines[0]
        self.add(duplicate)
        self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_native_shape_matching_gallery_replays_the_membership_swap(self):
        path = ROOT / "web/python/examples/showcase_shape_matching.py"
        scene = asyncio.run(run_source(path.read_text(), filename=str(path), retain_history=True))
        view = prepare_view(scene)
        try:
            self.assertAlmostEqual(view.duration, 8.3)
            source = view.capture(time=2.0, width=320, height=180)
            self.assertNotEqual(source, view.capture(time=4.2, width=320, height=180))
            self.assertNotEqual(source, view.capture(time=view.duration, width=320, height=180))
            self.assertEqual(source, view.capture(time=2.0, width=320, height=180))
        finally:
            view.close()

    def test_translation_drag_gallery_prepares_its_authored_endpoint(self):
        path = ROOT / "web/python/examples/showcase_translation_drag.py"
        scene = asyncio.run(run_source(path.read_text(), filename=str(path), retain_history=True))
        view = prepare_view(scene)
        try:
            self.assertAlmostEqual(view.duration, 2.6)
            with self.assertRaisesRegex(RuntimeError, "authored endpoint"):
                view.capture(time=0.0, width=320, height=180)
            pixels = view.capture(time=2.6, width=320, height=180)
            self.assertGreater(len(set(pixels)), 16)
        finally:
            view.close()

    def test_brace_text_rebuilds_against_the_current_scene(self):
        source = """from noon import *
class BraceEdits(Scene):
    async def construct(self):
        left, right = Square().shift(LEFT * 2), Square().shift(RIGHT * 2)
        label = BraceText(left, "Left")
        self.add(left, right, label)
        await self.wait(0.1)
        first_label = label.label
        label.shift_brace(right)
        assert label.label is first_label
        assert abs(label.brace.get_center()[0] - right.get_center()[0]) < 1e-6
        label.change_label("Right")
        assert label.label is not first_label
        label.change_brace_label(left, "Back")
        assert abs(label.brace.get_center()[0] - left.get_center()[0]) < 1e-6
        clone = label.copy()
        assert clone.brace is not label.brace
        self.add(clone.shift(UP * 2), BraceBetweenPoints(LEFT, RIGHT).shift(UP * 3))
        await self.wait(0.1)
"""
        scene = asyncio.run(run_source(source))
        try:
            self.assertEqual(len(scene.mobjects), 5)
        finally:
            close_scene(scene)

    def test_sample_space_partitions_during_playback(self):
        path = ROOT / "web/python/examples/sample_space.py"
        source = path.read_text() + """
        self.wait(0.1)
        assert len(horizontal.get_vertical_division([0.5])) == 2
        clone = horizontal.copy()
        clone.divide_vertically([0.4])
        assert clone.vertical_parts is not None and horizontal.vertical_parts is None
        self.add(clone.shift((0, -2)))
        self.wait(0.1)
"""
        scene = asyncio.run(run_source(source, filename=str(path)))
        try:
            self.assertEqual(len(scene.mobjects), 3)
        finally:
            close_scene(scene)

    def test_always_redraw_gallery_uses_sampled_callback_content(self):
        path = ROOT / "web/python/examples/showcase_always_redraw.py"
        scene = asyncio.run(run_source(path.read_text(), filename=str(path), sample_hz=12))
        try:
            from _manim_scene import _context
            frames, segments, regions = _context(scene).metrics()
            self.assertEqual(segments, 7)
            self.assertGreater(frames, 7)
            self.assertGreater(regions, 7)
        finally:
            close_scene(scene)

    @unittest.skipUnless(
        os.environ.get("NOON_LATEX_FONT_DIR") and os.environ.get("NOON_LATEX_RESOURCE_IDENTITY"),
        "requires an explicit native TeX compiler and BaKoMa font profile",
    )
    def test_latex_gallery_and_retained_part_edits(self):
        matrix = ROOT / "web/python/examples/matrix.py"
        scene = asyncio.run(run_source(matrix.read_text(), filename=str(matrix)))
        try:
            self.assertEqual(len(scene.mobjects), 5)
        finally:
            close_scene(scene)

        for name, time in (("showcase_latex_create", 0.5), ("showcase_linear_algebra", 2.5)):
            with self.subTest(example=name):
                path = ROOT / "web/python/examples" / (name + ".py")
                scene = asyncio.run(run_source(path.read_text(), filename=str(path), retain_history=True))
                view = prepare_view(scene)
                try:
                    first = view.capture(time=0.0, width=320, height=180)
                    self.assertNotEqual(first, view.capture(time=time, width=320, height=180))
                    self.assertEqual(first, view.capture(time=0.0, width=320, height=180))
                finally:
                    view.close()

        source = """from noon import *
class Demo(Scene):
    def construct(self):
        equation = MathTex("x", "+", "1", font_size=40)
        equation.set_color_by_tex("x", BLUE)
        assert abs(equation.font_size - 40) < 1e-6
        self.add(equation)
        self.wait(0.125)
        equation.scale(1.5)
        assert abs(equation.font_size - 60) < 1e-5
        clone = equation.copy()
        clone.set_color_by_tex("1", YELLOW)
        assert [part.tex_string for part in clone] == ["x", "+", "1"]
        assert clone[0] is not equation[0]
        self.add(clone.shift(RIGHT))
        self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    @unittest.skipUnless(
        os.environ.get("NOON_LATEX_FONT_DIR") and os.environ.get("NOON_LATEX_RESOURCE_IDENTITY"),
        "requires an explicit native TeX compiler and BaKoMa font profile",
    )
    def test_table_examples_and_playback_cell_edits(self):
        for name in ("table", "table_labels", "mobject_table_family_labels", "decimal_table_format"):
            with self.subTest(example=name):
                path = ROOT / "web/python/examples" / (name + ".py")
                scene = asyncio.run(run_source(path.read_text(), filename=str(path)))
                close_scene(scene)

        source = """from noon import *
class TableCellEdits(Scene):
    async def construct(self):
        table = Table([["a", "b"], ["c", "d"]], include_outer_lines=True)
        assert len(table.get_horizontal_lines()) == len(table.get_vertical_lines()) == 3
        self.add(table)
        await self.wait(0.1)
        table.shift(RIGHT)
        assert table.get_cell((1, 1)).width > 0
        clone = table.copy()
        assert len(clone.get_entries()) == 4
        assert len(clone.get_rows()) == len(clone.get_columns()) == 2
        assert clone.get_entries((1, 1)) is not table.get_entries((1, 1))
        clone.add_highlighted_cell((2, 2), color=YELLOW)
        assert len(clone.submobjects[0]) == 1 and len(table.submobjects[0]) == 0
        self.add(clone.shift(LEFT * 3), table.get_highlighted_cell((1, 2), color=BLUE))
        await self.wait(0.1)
"""
        scene = asyncio.run(run_source(source))
        try:
            self.assertEqual(len(scene.mobjects), 3)
        finally:
            close_scene(scene)

    @unittest.skipUnless(
        os.environ.get("NOON_LATEX_FONT_DIR") and os.environ.get("NOON_LATEX_RESOURCE_IDENTITY"),
        "requires an explicit native TeX compiler and BaKoMa font profile",
    )
    def test_numeric_scene_edits_preserve_text_size_and_rewind(self):
        chart = ROOT / "web/python/examples/bar_chart.py"
        source = chart.read_text() + """
        await self.wait(0.1)
        chart.change_bar_values([2, 1, -3, 1, -2])
        assert chart.values == [2, 1, -3, 1, -2]
        self.add(chart.get_bar_labels())
        await self.wait(0.1)
"""
        scene = asyncio.run(run_source(source, filename=str(chart)))
        close_scene(scene)

        variable = ROOT / "web/python/examples/variable.py"
        scene = asyncio.run(run_source(variable.read_text(), filename=str(variable)))
        try:
            self.assertEqual(scene.mobjects[0].value.get_value(), 12_345.6)
        finally:
            close_scene(scene)
        source = """from noon import *
class Demo(Scene):
    async def construct(self):
        await prepare_latex()
        number = DecimalNumber(12.5, font_size=32, include_sign=True)
        number.set_value(7).increment_value(0.5)
        assert number.get_value() == 7.5
        self.add(number)
        await self.wait(0.1)
        number.scale(1.5)
        assert number.font_size == 48
        number.set_value(1234).increment_value(0.75)
        assert number.get_value() == 1234.75 and number._source == "+1,234.75"
        assert number.font_size == 48
        await self.wait(0.1)
"""
        view = prepare_view(asyncio.run(run_source(source, retain_history=True)))
        try:
            first = view.capture(time=0.05, width=320, height=180)
            self.assertNotEqual(first, view.capture(time=0.15, width=320, height=180))
            self.assertEqual(first, view.capture(time=0.05, width=320, height=180))
        finally:
            view.close()

    def test_detached_suspension_preserves_running_scene_publication(self):
        source = """from noon import *
class Demo(Scene):
    async def construct(self):
        child = Square()
        group = VGroup(child)
        group.suspend_updating()
        assert group.updating_suspended and child.updating_suspended
        dot = Dot()
        self.add(dot)
        await self.wait(0.1)
        group.resume_updating()
        assert not group.updating_suspended and not child.updating_suspended
        alias = VGroup(child, dot)
        try:
            alias.suspend_updating()
        except NotImplementedError:
            pass
        else:
            raise AssertionError("attached descendant was suspended")
        assert not child.updating_suspended
        dot.shift(RIGHT)
        assert abs(dot.get_center()[0] - 1) < 1e-6
        await self.wait(0.1)
"""
        scene = asyncio.run(run_source(source, retain_history=True))
        close_scene(scene)

    def test_native_pointer_selection_gallery_prepares_retained_preview(self):
        path = ROOT / "web/python/examples/showcase_pointer_selection.py"
        scene = asyncio.run(run_source(path.read_text(), filename=str(path), retain_history=True))
        view = prepare_view(scene)
        try:
            self.assertAlmostEqual(view.duration, 2.6)
            start = view.capture(time=0.0, width=320, height=180)
            endpoint = view.capture(time=view.duration, width=320, height=180)
            self.assertNotEqual(start, endpoint)
            self.assertEqual(start, view.capture(time=0.0, width=320, height=180))
        finally:
            view.close()

    def test_background_music_lesson_prepares_its_full_authored_timeline(self):
        path = ROOT / "web/python/examples/background_music_lesson.py"
        scene = asyncio.run(run_source(path.read_text(), filename=str(path), retain_history=True))
        view = prepare_view(scene)
        try:
            self.assertEqual(view.duration, 30)
            first = view.capture(time=0, width=320, height=180)
            self.assertNotEqual(first, view.capture(time=9, width=320, height=180))
            self.assertEqual(first, view.capture(time=0, width=320, height=180))
        finally:
            view.close()

    def test_scene_path_edits_match_during_setup_and_playback(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        path = VMobject()
        target = VMobject().set_points_as_corners([(1, 2, 0), (3, 2, 0)])
        family = VGroup(VMobject().set_points_as_corners([(0, 0, 0), (0, 1, 0)]))
        self.add(path, target, family)
        for _ in range(2):
            assert family.get_num_points() == 0
            family.set_points([(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)])
            assert family.get_num_points() == 4
            family.clear_points()
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

    def test_scene_point_reducers_follow_path_and_family_placement(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        path = VMobject().set_points_as_corners([(0, 1, 0), (2, 1, 0)])
        family = VGroup(path)

        def check(shift):
            for target in (path, family):
                center = target.get_center_of_mass()
                assert abs(center.x - (1 + shift)) < 1e-9 and center.y == 1
                assert tuple(target.get_boundary_point(RIGHT)) == (2 + shift, 1)
                assert target.length_over_dim(0) == 2
                assert target.get_extremum_along_dim(dim=0, key=1) == 2 + shift
            assert path.get_right()[0] == 2 + shift

        check(0)
        self.add(family)
        check(0)
        self.wait(0.125)
        family.shift(RIGHT)
        check(1)
        self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_native_linear_transform_grows_and_rebuilds_vectors(self):
        source = """from noon import *
class Demo(LinearTransformationScene):
    def __init__(self):
        super().__init__(leave_ghost_vectors=True)

    def construct(self):
        vector = self.add_vector((2, 1), color=YELLOW)
        assert tuple(vector.get_vector()) == (2, 1)
        self.apply_matrix([[0, 1], [1, 0]], run_time=0.25)
        assert tuple(vector.get_vector()) == (1, 2)
        self.apply_matrix([[0, 1], [1, 0]], run_time=0.25)
        assert tuple(vector.get_vector()) == (2, 1)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_scene_family_shift_uses_all_three_coordinates_in_both_phases(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        child = Line(LEFT, RIGHT)
        family = VGroup(child)
        self.add(family)
        for depth in (0.25, 0.5):
            family.shift((0., 0., 0.25))
            assert abs(child.get_center()[2] - depth) < 1e-9
            self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_family_root_style_uses_the_same_scene_before_and_after_wait(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        leaf = Square().set_fill(WHITE, opacity=1)
        family = VGroup(leaf)
        detached = VGroup()
        detached.set_stroke(RED, width=3, family=False)
        assert abs(detached.get_stroke_width() - 3) < 1e-5
        self.add(family)
        for width in (4, 8):
            family.set_fill(RED, 0.3, family=False)
            family.set_stroke(BLUE, width=width, opacity=0.4, family=False)
            family.set_background_stroke(color=GREEN, width=2, opacity=0.5, family=False)
            family.set_cap_style(CapStyleType.ROUND)
            assert abs(family.get_fill_opacity() - 0.3) < 1e-6
            assert abs(family.get_stroke_width() - width) < 1e-5
            assert abs(family.get_stroke_width(background=True) - 2) < 1e-5
            assert family.cap_style is CapStyleType.ROUND
            assert leaf.get_fill_opacity() == 1
            self.wait(0.125)
"""
        scene = asyncio.run(run_source(source))
        close_scene(scene)

    def test_interpolation_preserves_root_scope_and_publishes_points_in_both_phases(self):
        source = """from noon import *
class Demo(Scene):
    def construct(self):
        start = Line(LEFT, RIGHT).set_stroke(RED, width=2)
        end = Line(2 * LEFT + UP, 2 * RIGHT + UP).set_stroke(BLUE, width=6)
        leaf = Line(LEFT, RIGHT)
        group = VGroup(leaf)
        start_group = VGroup().set_stroke(RED, width=2, family=False)
        end_group = VGroup().set_stroke(BLUE, width=6, family=False)
        original = leaf.get_stroke_width()
        self.add(group)
        for amount in (0.25, 0.75):
            leaf.interpolate(start, end, amount)
            assert abs(leaf.get_start()[1] - amount) < 1e-5
            assert abs(leaf.get_stroke_width() - (2 + 4 * amount)) < 1e-5
            child_width = leaf.get_stroke_width()
            group.interpolate_color(start_group, end_group, amount)
            assert abs(group.get_stroke_width() - (2 + 4 * amount)) < 1e-5
            assert leaf.get_stroke_width() == child_width
            self.wait(0.125)
        assert leaf.get_stroke_width() != original
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
        title = Typst("*Native*", color=YELLOW).shift(LEFT)
        family = Group(title)
        self.add(family)
        expected = tuple(title.get_center())
        assert tuple(family.get_center()) == expected
        self.wait(1)
        assert tuple(title.get_center()) == tuple(family.get_center()) == expected
        self.next_section("Math")
        label = Text("Fraction", color=YELLOW, t2c={"Frac": BLUE}).shift(UP)
        markup = MarkupText("<b>Numerator</b>").shift(DOWN)
        assert label.width > 0 and markup.width > 0
        self.add(label, markup, MathTypst("frac(x, 2)", color=BLUE).shift(RIGHT))
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
