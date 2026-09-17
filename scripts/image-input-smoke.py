"""Executed by the ordinary Python authoring worker, never a mock image host.

The JS qualification harness supplies INPUT_CASES, INPUT_PNG_HEX,
INPUT_JPEG_HEX and INPUT_URL as explicit, small test inputs.
"""
from io import BytesIO
from pathlib import Path

from noon import ImageMobject, Scene, Square


class ImageInputs(Scene):
    async def construct(self):
        import pyodide_js
        await pyodide_js.loadPackage("numpy")
        import numpy as np
        from js import Blob
        from pyodide.ffi import to_js

        png = bytes.fromhex(INPUT_PNG_HEX)
        jpeg = bytes.fromhex(INPUT_JPEG_HEX)
        png_path = Path("/tmp/noon-input.png")
        jpeg_path = Path("/tmp/noon-input.jpg")
        png_path.write_bytes(png)
        jpeg_path.write_bytes(jpeg)
        gray = np.array([[32, 96, 160], [64, 128, 224]], dtype=np.uint8)
        rgb = np.repeat(gray[:, :, None], 3, axis=2)
        rgba = np.concatenate((rgb, np.full((2, 3, 1), 255, dtype=np.uint8)), axis=2)
        # A negative-stride view with the same logical pixels, not a contiguous copy.
        view = rgba[:, ::-1].copy()[:, ::-1]
        assert not view.flags.c_contiguous
        inputs = {
            "nested-gray": gray.tolist(),
            "nested-gray-channel": gray[:, :, None].tolist(),
            "nested-rgb": rgb.tolist(),
            "nested-rgba": rgba.tolist(),
            "numpy-gray": gray,
            "numpy-gray-channel": gray[:, :, None],
            "numpy-rgb": rgb,
            "numpy-rgba": rgba,
            "numpy-strided": view,
            "png-bytes": png,
            "png-file-object": BytesIO(png),
            "png-path": png_path,
            "png-filename": str(png_path),
            "jpeg-bytes": jpeg,
            "jpeg-path": jpeg_path,
        }
        assert set(INPUT_CASES) == set(inputs) | {"png-url", "png-blob"}
        background = Square(8).set_fill("#14283c", opacity=1).set_stroke(width=0)
        self.add(background)

        # Fail before copying an oversized broadcast view, then prove that a
        # valid construction and the existing scene still work after every error.
        for invalid in [
            [], [[1], [2, 3]], np.zeros((2, 3), dtype=np.float32),
            np.zeros((2, 3, 2), dtype=np.uint8),
            np.broadcast_to(np.uint8(0), (4097, 4097)),
            b"GIF89a", png[:20],
        ]:
            try:
                ImageMobject(invalid)
            except (TypeError, ValueError, NotImplementedError):
                pass
            else:
                raise AssertionError("invalid image input was admitted")

        for case in INPUT_CASES:
            if case == "png-url":
                image = await ImageMobject.from_url(INPUT_URL)
            elif case == "png-blob":
                blob = Blob.new(to_js([memoryview(png)]))
                image = await ImageMobject.from_blob(blob)
            else:
                image = ImageMobject(inputs[case])
            assert (image.pixel_width, image.pixel_height) == (3, 2), case
            intrinsic_height = 2 / 1080 * 8
            assert abs(image.height - intrinsic_height) < 1e-12, case
            assert abs(image.width - intrinsic_height * 1.5) < 1e-12, case
            duplicate = image.copy().scale(2)
            assert abs(image.height - intrinsic_height) < 1e-12, case
            assert abs(duplicate.height - 2 * intrinsic_height) < 1e-12, case
            image.height = 4
            assert abs(image.width - 6) < 1e-12, case
            image.set_resampling_algorithm("nearest")
            self.add(image)
            await self.wait(1)
            self.remove(image)
        # Sampling this last interval proves all constructors and assertions ran.
        await self.wait(1)
