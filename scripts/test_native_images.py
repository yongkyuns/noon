"""Real native raster resource inputs, live admission and ownership guards."""
import asyncio
import gc
import io
from pathlib import Path
import tempfile
import unittest

import _noon_native as native
from noon import ImageMobject, Scene, RIGHT
from noon_native import run_source, close_scene
from _noon_errors import engine_call, NoonOwnershipError, NoonForeignHandleError
import _manim_updaters


class NativeImageTests(unittest.TestCase):
    def test_existing_raster_source_runs_on_both_routes_with_late_creation(self):
        source = (Path(__file__).resolve().parents[1] / "web/python/examples/raster_image.py").read_text()
        async def check():
            for portable in (False, True):
                scene = await run_source(source, portable=portable, sample_hz=4)
                self.assertEqual(scene.time, 4)
                close_scene(scene)
        asyncio.run(check())

    def test_array_dimensions_copy_and_sampling_keep_independent_identity(self):
        image = ImageMobject([[[255,0,0,255],[0,255,0,128]]], height=2)
        self.assertEqual((image.pixel_width,image.pixel_height,image.width,image.height),(2,1,4,2))
        duplicate = image.copy().shift(RIGHT).scale(.5)
        duplicate.set_resampling_algorithm("nearest")
        self.assertEqual((image.width,duplicate.width),(4,2))
        self.assertNotEqual(image._semantic_handle.semanticSlot,duplicate._semantic_handle.semanticSlot)
        for pixels in ([[0,255]], [[[0],[255]]], [[[0,0,0],[255,255,255]]]):
            self.assertEqual(ImageMobject(pixels).pixel_width,2)

    def test_prepared_inputs_are_consumed_on_success_rejection_and_free(self):
        store = native.Store()
        make = lambda: native.ImageOptions.rgba8(1,1,b"\xff\x00\x00\xff")
        candidate = make(); store.image(candidate)
        with self.assertRaises(RuntimeError):store.image(candidate)
        candidate.free()
        with self.assertRaises(RuntimeError):candidate.setHeight(2)
        retired = store.context();retired.retire();candidate=make()
        with self.assertRaises(NoonOwnershipError):engine_call(retired.liveCreateImage,candidate)
        with self.assertRaises(RuntimeError):store.image(candidate)

    def test_bad_inputs_do_not_allocate_a_semantic_slot_or_poison_valid_input(self):
        store=native.Store();first=store.geometry(native.GeometryOptions.circle(.5))
        for width,height,pixels in [(0,1,b""),(2,2,b"x"),(1,1,b"12345")]:
            with self.assertRaises(RuntimeError):native.ImageOptions.rgba8(width,height,pixels)
        for data in (b"GIF89a",b"not png",b"\x89PNG\r\n\x1a\n"):
            with self.assertRaises(RuntimeError):native.ImageOptions.encoded(data)
        next=store.image(native.ImageOptions.rgba8(1,1,b"\x01\x02\x03\xff"))
        self.assertEqual(next.semanticSlot,first.semanticSlot+1)

    def test_encoded_file_stream_and_bytes_use_existing_rust_decoder(self):
        # Valid fixed PNG fixture, including its CRC, encoded by the stdlib only
        # to provide an input. Production decoding stays in the Rust image crate.
        import struct,zlib
        def chunk(name,data):return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data))
        png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',2,1,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(b'\x00\xff\x00\x00\xff\x00\xff\x00\x80'))+chunk(b'IEND',b'')
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'input.png';path.write_bytes(png)
            for value in (png,bytearray(png),memoryview(png),io.BytesIO(png),path,str(path)):
                image=ImageMobject(value,height=2)
                self.assertEqual((image.pixel_width,image.pixel_height,image.width,image.height),(2,1,4,2))

    def test_foreign_image_and_retired_reads_fail_existing_guards(self):
        handle=native.Store().image(native.ImageOptions.rgba8(1,1,b"\x00\x00\x00\xff"))
        foreign=native.Store().context()
        with self.assertRaises(NoonForeignHandleError):engine_call(foreign.containsMobject,handle)
        with self.assertRaises(NoonForeignHandleError):engine_call(foreign.liveSetImageSampling,handle,'nearest')
        async def check():
            scene=await run_source('''from noon import *
class ImageScene(Scene):
 def construct(self):
  self.image=ImageMobject([[255]],height=1)
  self.add(self.image)
  self.wait(.1)
  self.image.set_resampling_algorithm('nearest')
  self.wait(.1)
''')
            close_scene(scene)
            with self.assertRaises(NoonOwnershipError):scene.image.get_center()
        asyncio.run(check())

    def test_numpy_uint8_and_negative_stride_inputs_use_logical_row_order(self):
        try:
            import numpy as np
        except ImportError:
            self.skipTest('NumPy is optional in the minimal interpreter')
        gray=np.array([[32,96,160],[64,128,224]],dtype=np.uint8)
        rgba=np.repeat(gray[:,:,None],4,axis=2);rgba[:,:,3]=255
        view=rgba[:,::-1].copy()[:,::-1]
        self.assertFalse(view.flags.c_contiguous)
        for value in (gray,gray[:,:,None],rgba[:,:,:3],rgba,view):
            image=ImageMobject(value,height=2)
            self.assertEqual((image.pixel_width,image.pixel_height,image.width,image.height),(3,2,3,2))
        with self.assertRaises(TypeError):ImageMobject(gray.astype(np.float32))

    def test_failed_late_constructor_keeps_existing_scene_and_next_image_valid(self):
        source='''from noon import *
class RecoverImage(Scene):
 def construct(self):
  self.image=ImageMobject([[255]],height=1)
  self.add(self.image)
  self.wait(.1)
  try:
   ImageMobject([[255]],height=-1)
  except ValueError:
   pass
  else:raise AssertionError('invalid image height accepted')
  self.other=ImageMobject([[0,255]],height=2).shift(RIGHT)
  self.add(self.other)
  self.wait(.1)
'''
        async def check():
            for portable in (False,True):
                scene=await run_source(source,portable=portable,sample_hz=10)
                try:
                    self.assertEqual((scene.image.width,scene.other.width),(1,4))
                    self.assertAlmostEqual(scene.time,.2)
                finally:close_scene(scene)
        asyncio.run(check())

    def test_source_failure_releases_image_context_and_callback_tables(self):
        before=(len(_manim_updaters._CANONICAL_SESSIONS),len(_manim_updaters._TRACKED_MOBJECTS))
        source='''from noon import *
class BrokenImage(Scene):
 def construct(self):
  image=ImageMobject([[255]],height=1)
  self.add(image)
  image.add_updater(lambda m,dt:m.shift(dt*RIGHT))
  self.wait(.1)
  raise ValueError('intentional image failure')
'''
        for portable in (False,True):
            with self.assertRaisesRegex(ValueError,'intentional image failure'):
                asyncio.run(run_source(source,portable=portable,sample_hz=10))
        gc.collect()
        self.assertEqual(before,(len(_manim_updaters._CANONICAL_SESSIONS),len(_manim_updaters._TRACKED_MOBJECTS)))


if __name__ == '__main__':unittest.main()
