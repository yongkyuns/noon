import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import noon
import _manim_image as images


class _Options:
    def __init__(self):
        self.calls = []
        self.freed = 0
    def setScaleToResolution(self, value): self.calls.append(("resolution", value))
    def setFrameHeight(self, value): self.calls.append(("frame_height", value))
    def setSampling(self, value): self.calls.append(("sampling", value))
    def setOpacity(self, value): self.calls.append(("opacity", value))
    def setZIndex(self, value): self.calls.append(("z_index", value))
    def setHeight(self, value):
        if value <= 0: raise ValueError("invalid height")
        self.calls.append(("height", value))
    def free(self): self.freed += 1


class _Handle:
    def imagePixelWidth(self): return 2
    def imagePixelHeight(self): return 1


class ImageFacadeTests(unittest.TestCase):
    def test_public_export_is_shared_image_facade(self):
        self.assertIs(noon.ImageMobject, images.ImageMobject)
        self.assertIn("ImageMobject", noon.__all__)
        self.assertIs(noon.RESAMPLING_ALGORITHMS, images.RESAMPLING_ALGORITHMS)

    def test_normalizes_gray_rgb_and_rgba_without_numpy(self):
        self.assertEqual(images._rgba8_array([[0, 127]]), (2, 1, bytes([0,0,0,255, 127,127,127,255])))
        self.assertEqual(images._rgba8_array([[[1,2,3], [4,5,6]]]), (2, 1, bytes([1,2,3,255, 4,5,6,255])))
        self.assertEqual(images._rgba8_array([[[1,2,3,4]]]), (1, 1, bytes([1,2,3,4])))
        for invalid in [[], [[]], [[1], [1,2]], [[[1,2]]], [[-1]], [[256]], [[1.5]]]:
            with self.subTest(invalid=invalid), self.assertRaises((ValueError, TypeError)):
                images._rgba8_array(invalid)

    def test_numpy_style_buffer_path_is_one_bulk_read_in_row_order(self):
        calls = []
        class Array:
            dtype = "uint8"
            shape = (1, 2, 4)
            def tobytes(self, order):
                calls.append(order)
                return bytes(range(8))
        self.assertEqual(images._rgba8_array(Array()), (2, 1, bytes(range(8))))
        self.assertEqual(calls, ["C"])
        Array.dtype = "float64"
        with self.assertRaises(TypeError): images._rgba8_array(Array())

    def test_file_and_file_object_adaptation_is_bounded(self):
        data = b"\x89PNG\r\n\x1a\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.png"
            path.write_bytes(data)
            self.assertEqual(images._encoded_bytes(path), data)
            self.assertEqual(images._encoded_bytes(io.BytesIO(data)), data)
            with patch.object(images, "_MAX_ENCODED_BYTES", 3):
                for source in [path, io.BytesIO(data), data]:
                    with self.assertRaises(ValueError): images._encoded_bytes(source)

    def test_options_are_configured_before_single_semantic_admission(self):
        candidate = _Options(); handle = _Handle(); events = []
        class Factory:
            @staticmethod
            def rgba8(width, height, data):
                events.append((width, height, data)); return candidate
        def publish(options):
            self.assertIs(options, candidate)
            self.assertIn(("height", 3.0), options.calls)
            return handle
        with patch.object(images, "_image_options", Factory), patch.object(images, "_js_bytes", bytes), \
             patch.object(images, "_create_image", publish), patch.object(images, "_live_constructor_context", return_value=None):
            value = images.ImageMobject([[[1,2,3], [4,5,6]]], height=3, opacity=.5)
        self.assertIs(value._semantic_handle, handle)
        self.assertEqual((value.pixel_width, value.pixel_height), (2,1))
        self.assertEqual(candidate.freed, 0, "Rust consumes the handed-off options")
        self.assertEqual(events, [(2,1,bytes([1,2,3,255,4,5,6,255]))])
        self.assertIsNone(value._raw, "no Python-owned geometry/pixel scene")

    def test_invalid_inert_options_are_freed_without_admission(self):
        candidate = _Options()
        class Factory:
            @staticmethod
            def rgba8(*args): return candidate
        with patch.object(images, "_image_options", Factory), patch.object(images, "_js_bytes", bytes), \
             patch.object(images, "_create_image") as publish:
            with self.assertRaises(ValueError): images.ImageMobject([[0]], height=-1)
        publish.assert_not_called()
        self.assertEqual(candidate.freed, 1)

    def test_unsupported_options_fail_before_preparation(self):
        with patch.object(images, "_image_options") as prepare:
            for options in [{"invert": True}, {"image_mode": "RGB"}, {"resampling_algorithm": 1}, {"mystery": 1}]:
                with self.subTest(options=options), self.assertRaises(NotImplementedError):
                    images.ImageMobject([[0]], **options)
        prepare.rgba8.assert_not_called()


if __name__ == "__main__":
    unittest.main()
