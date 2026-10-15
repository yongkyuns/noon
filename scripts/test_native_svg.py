"""Real native static SVG import through the existing shared Rust family API."""
import asyncio
import gc
from pathlib import Path
import tempfile
import unittest

import _noon_native as native
from noon import SVGMobject, RIGHT
from noon_native import run_source, close_scene
from _noon_errors import engine_call, NoonForeignHandleError, NoonOwnershipError
import _manim_updaters

SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>'


class NativeSvgTests(unittest.TestCase):
    @staticmethod
    def rgb(color):
        return (color.red, color.green, color.blue)

    def test_real_import_dimensions_members_and_copy_are_authoritative(self):
        svg = SVGMobject.from_string(SVG)
        self.assertEqual(len(svg), 1)
        self.assertAlmostEqual(svg.width, 4)
        self.assertAlmostEqual(svg.height, 2)
        self.assertAlmostEqual(svg.get_center()[0], 0)
        self.assertAlmostEqual(svg.get_center()[1], 0)
        self.assertEqual(self.rgb(svg[0].get_fill_color()), (1, 0, 0))
        copied = svg.copy().shift(RIGHT)
        self.assertAlmostEqual(copied.get_center()[0], 1)
        self.assertAlmostEqual(svg.get_center()[0], 0)
        self.assertAlmostEqual(svg.get_center()[1], 0)
        self.assertNotEqual(svg[0]._semantic_handle.semanticSlot,
                            copied[0]._semantic_handle.semanticSlot)

    def test_file_loading_extension_and_noncentered_bounds_use_same_importer(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'shape.svg'
            path.write_text(SVG)
            by_path = SVGMobject(path, height=1)
            by_extension = SVGMobject(path.with_suffix(''), height=1)
            self.assertEqual((by_path.width, by_path.height), (2, 1))
            self.assertEqual((by_extension.width, by_extension.height), (2, 1))
        uncentered = SVGMobject.from_string(SVG, should_center=False, height=None, width=None)
        self.assertEqual((uncentered.width, uncentered.height), (20, 10))
        self.assertEqual(tuple(uncentered.get_center())[:2], (10, 5))

    def test_typed_defaults_preserve_inheritance_and_explicit_paint_override(self):
        defaults = dict(color='#0000ff', opacity=.25, fill_color=None,
                        fill_opacity=None, stroke_color=None,
                        stroke_opacity=None, stroke_width=0)
        source = SVG.replace(' fill="red"', '')
        svg = SVGMobject.from_string(source, svg_default=defaults)
        self.assertEqual(self.rgb(svg[0].get_fill_color()), (0, 0, 1))
        self.assertEqual(svg[0].get_fill_opacity(), 64 / 255)
        explicit = SVGMobject.from_string(SVG, svg_default=defaults)
        self.assertEqual(self.rgb(explicit[0].get_fill_color()), (1, 0, 0))
        override = SVGMobject.from_string(SVG, svg_default=defaults, fill_color='#00ff00')
        self.assertEqual(self.rgb(override[0].get_fill_color()), (0, 1, 0))

    def test_parse_errors_and_invalid_defaults_allocate_no_semantic_identity(self):
        store = native.Store()
        first = store.geometry(native.GeometryOptions.circle(.5))
        for source in ('<svg', '<svg xmlns="http://www.w3.org/2000/svg"><text>not supported</text></svg>'):
            with self.assertRaises(RuntimeError):
                store.svg(source, True, 2, None)
        defaults = native.GeometryOptions.svgDefaultTransport(None, None, 2., None, None, 0.)
        with self.assertRaises(RuntimeError):
            store.svg(SVG, True, 2, defaults)
        next_handle = store.geometry(native.GeometryOptions.circle(.5))
        self.assertEqual(next_handle.semanticSlot, first.semanticSlot + 1)

    def test_foreign_member_queries_and_out_of_bounds_indices_are_rejected(self):
        family = native.Store().svg(SVG, True, 2, None)
        self.assertEqual(family.memberCount, 1)
        with self.assertRaises(RuntimeError): family.memberMobject(1)
        foreign = native.Store().context()
        with self.assertRaises(NoonForeignHandleError):
            engine_call(foreign.containsMobject, family.memberMobject(0))

    def test_static_svg_uses_existing_live_family_operations_on_both_routes(self):
        source = f'''from noon import *
class SvgScene(Scene):
 def construct(self):
  self.svg=SVGMobject.from_string({SVG!r})
  self.add(self.svg)
  self.wait(.1)
  self.svg.shift(RIGHT)
  self.wait(.1)
'''
        async def check():
            for portable in (False, True):
                scene = await run_source(source, portable=portable, sample_hz=10)
                try:
                    self.assertAlmostEqual(scene.time, .2)
                    self.assertAlmostEqual(scene.svg.get_center()[0], 1)
                    self.assertAlmostEqual(scene.svg[0].get_center()[0], 1)
                    self.assertEqual(self.rgb(scene.svg[0].get_fill_color()), (1, 0, 0))
                finally:
                    close_scene(scene)
                with self.assertRaises(NoonOwnershipError): scene.svg.get_center()
                with self.assertRaises(NoonOwnershipError): scene.svg[0].get_fill_color()
        asyncio.run(check())

    def test_source_failure_retires_the_existing_svg_family_context(self):
        before = (len(_manim_updaters._CANONICAL_SESSIONS), len(_manim_updaters._TRACKED_MOBJECTS))
        source = f'''from noon import *
class BrokenSvg(Scene):
 def construct(self):
  self.svg=SVGMobject.from_string({SVG!r})
  self.add(self.svg)
  self.wait(.1)
  raise ValueError('intentional SVG failure')
'''
        for portable in (False, True):
            with self.assertRaisesRegex(ValueError, 'intentional SVG failure'):
                asyncio.run(run_source(source, portable=portable, sample_hz=10))
        gc.collect()
        self.assertEqual(before, (len(_manim_updaters._CANONICAL_SESSIONS), len(_manim_updaters._TRACKED_MOBJECTS)))


if __name__ == '__main__':
    unittest.main()
