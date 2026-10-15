"""Real optional native shaping, resource ownership, and source continuation guards."""
import asyncio
import copy
import gc
import unittest
from pathlib import Path

import _noon_native as native
from noon import Text, NativeFontFace
from noon_native import run_source, close_scene
from _noon_errors import NoonOwnershipError, NoonForeignHandleError, engine_call
import _manim_updaters


class NativeTextTests(unittest.TestCase):
    def test_source_part_queries_reuse_utf8_resource_spans_and_immutable_values(self):
        label = Text("écho écho", font="DejaVu Sans Mono")
        before = label.source_parts_for("écho")
        self.assertEqual([(p.source_start, p.source_end) for p in before], [(0, 5), (6, 11)])
        self.assertTrue(all(p.cluster_count > 0 for p in before))
        self.assertTrue(all(p.semantic_key for p in before))
        self.assertEqual(label.source_parts_for("missing"), ())
        self.assertEqual(label.source_parts_for(""), ())
        self.assertIsInstance(label.text_parts(), tuple)
        label.shift((1, 0, 0))
        self.assertEqual(label.source_parts_for("écho"), before)
        self.assertEqual(label._semantic_handle.textSource(), "écho écho")
        with self.assertRaises(TypeError):
            label.source_parts_for(None)

    def test_raw_source_part_values_have_independent_release_and_checked_bounds(self):
        store = native.Store()
        handle = store.text("Noon Noon", "DejaVu Sans Mono", 30, -1)
        raw = handle.textSourcePartsFor("Noon")
        self.assertEqual(raw.length, 2)
        first = raw.item(0)
        with self.assertRaises(RuntimeError):
            raw.item(2)
        raw.free()
        raw.free()
        with self.assertRaises(RuntimeError):
            _ = raw.length
        self.assertEqual((first.sourceStart, first.sourceEnd), (0, 4))
        first.free()
        with self.assertRaises(RuntimeError):
            _ = first.sourceStart
        circle = store.geometry(native.GeometryOptions.circle(.5))
        with self.assertRaises(RuntimeError) as caught:
            circle.textParts()
        self.assertEqual(caught.exception.code, "text_parts.not_text")
        self.assertEqual(circle.centerCoordinates(), (0., 0.))

    def test_part_queries_after_wait_match_before_and_reject_retired_owner(self):
        source = '''from noon import *
class TextParts(Scene):
 def construct(self):
  self.label=Text("écho écho",font="DejaVu Sans Mono")
  self.add(self.label)
  self.before=self.label.source_parts_for("écho")
  self.wait(.1)
  self.label.shift(RIGHT)
  self.after=self.label.source_parts_for("écho")
  self.late=Text("écho écho",font="DejaVu Sans Mono")
  self.add(self.late)
  self.late_parts=self.late.source_parts_for("écho")
  self.wait(.1)
'''
        async def check():
            for portable in (False, True):
                scene=await run_source(source,portable=portable,sample_hz=10)
                try:
                    self.assertEqual(scene.before,scene.after)
                    self.assertEqual(scene.after,scene.late_parts)
                finally:
                    close_scene(scene)
                with self.assertRaises(NoonOwnershipError): scene.label.source_parts_for("écho")
                with self.assertRaises(NoonOwnershipError): scene.late.text_parts()
        asyncio.run(check())

    def test_plain_and_markup_sources_create_after_a_completed_barrier_on_both_routes(self):
        source = (Path(__file__).resolve().parents[1] / "parity/python-host/native_text.py").read_text()
        async def check():
            for portable in (False, True):
                scene = await run_source(source, portable=portable, sample_hz=10)
                try:
                    self.assertAlmostEqual(scene.time, .2)
                    self.assertGreater(scene.label.width, 0)
                    self.assertGreater(scene.other.height, 0)
                    self.assertAlmostEqual(scene.label.get_center()[0], 1., places=6)
                    self.assertAlmostEqual(scene.label.get_center()[1], 0., places=6)
                    self.assertAlmostEqual(scene.other.get_center()[0], 2., places=6)
                    self.assertAlmostEqual(scene.other.get_center()[1], 0., places=6)
                finally:
                    close_scene(scene)
                with self.assertRaises(NoonOwnershipError):
                    scene.label.get_center()
        asyncio.run(check())

    def test_immutable_bundled_font_clones_keep_resource_input(self):
        face = NativeFontFace.bundled("DejaVu Sans Mono")
        self.assertIs(copy.copy(face), face)
        self.assertIs(copy.deepcopy(face), face)
        self.assertEqual(face.family, "DejaVu Sans Mono")
        self.assertEqual(face.face_index, 0)
        self.assertEqual(face.with_metrics_profile("cairo-freetype").metrics_profile, "cairo-freetype")
        one = Text("same", font=face)
        two = Text("same", font=face)
        self.assertEqual((one.width, one.height), (two.width, two.height))
        self.assertNotEqual(one._semantic_handle.semanticSlot, two._semantic_handle.semanticSlot)
        with self.assertRaises((ValueError, RuntimeError)):
            NativeFontFace("bad", b"not a font")

    def test_bad_constructor_is_atomic_and_cannot_poison_the_next_shared_handle(self):
        store = native.Store()
        first = store.geometry(native.GeometryOptions.circle(.5))
        for size, spacing in [(float("nan"), -1), (float("inf"), -1), (1e300, -1), (48, -2)]:
            with self.assertRaises(RuntimeError):
                store.text("bad", "DejaVu Sans Mono", size, spacing)
        second = store.geometry(native.GeometryOptions.circle(.5))
        self.assertEqual(second.semanticSlot, first.semanticSlot + 1)
        self.assertEqual(first.centerCoordinates(), (0., 0.))

    def test_range_color_batch_is_atomic_consumed_once_and_uses_real_shaping(self):
        store = native.Store()
        batch = native.TextColorBatch()
        batch.setBaseColor(1,1,1,1)
        batch.push("red", 1,0,0,1)
        with self.assertRaises(RuntimeError):
            batch.push("bad", float("nan"), 0,0,1)
        handle = store.text("red white", "DejaVu Sans Mono", 30, -1, batch)
        self.assertGreater(handle.width, 0)
        with self.assertRaises(RuntimeError):
            store.text("again", "DejaVu Sans Mono", 30, -1, batch)
        batch.free()
        with self.assertRaises(RuntimeError):
            batch.setBaseColor(1,1,1,1)

    def test_foreign_text_binding_is_rejected_by_existing_context_guard(self):
        store = native.Store()
        handle = store.text("foreign", "DejaVu Sans Mono", 30, -1)
        foreign = native.Store().context()
        with self.assertRaises(NoonForeignHandleError):
            engine_call(foreign.containsMobject, handle)

    def test_failed_live_markup_creation_keeps_the_existing_scene_coherent(self):
        source = '''from noon import *
class RecoverText(Scene):
 def construct(self):
  self.label = Text("before", font="DejaVu Sans Mono")
  self.add(self.label)
  self.wait(.1)
  try:
   MarkupText("<unsupported>bad</unsupported>", font="DejaVu Sans Mono")
  except Exception:
   pass
  else:
   raise AssertionError("unsupported markup was accepted")
  self.other = Text("after", font="DejaVu Sans Mono")
  self.add(self.other)
  self.label.shift(RIGHT)
  self.wait(.1)
'''
        async def check():
            for portable in (False,True):
                scene = await run_source(source, portable=portable, sample_hz=10)
                try:
                    self.assertAlmostEqual(scene.time, .2)
                    self.assertGreater(scene.other.width, 0)
                    self.assertAlmostEqual(scene.label.get_center()[0], 1., places=6)
                    context = scene._canonical_authoring_context
                finally:
                    close_scene(scene)
                with self.assertRaises(NoonOwnershipError):
                    engine_call(context.liveCreateManimText, "retired", "DejaVu Sans Mono", 30., -1., 1., 1., 1., 1., 1.)
        asyncio.run(check())

    def test_text_source_failure_releases_callback_sessions(self):
        before = (len(_manim_updaters._CANONICAL_SESSIONS),len(_manim_updaters._TRACKED_MOBJECTS))
        source = '''from noon import *
class BrokenText(Scene):
 def construct(self):
  label = Text("cleanup", font="DejaVu Sans Mono")
  self.add(label)
  label.add_updater(lambda m, dt: m.shift(dt * RIGHT))
  self.wait(.1)
  raise ValueError("intentional text failure")
'''
        for portable in (False,True):
            with self.assertRaisesRegex(ValueError, "intentional text failure"):
                asyncio.run(run_source(source, portable=portable, sample_hz=10))
        gc.collect()
        self.assertEqual(before, (len(_manim_updaters._CANONICAL_SESSIONS),len(_manim_updaters._TRACKED_MOBJECTS)))


if __name__ == "__main__":
    unittest.main()
