"""Immutable raster ImageMobject over shared Rust image resources.

Python adapts file/array/URL/blob inputs and wrapper identity only. Decoding,
resource admission, dimensions, transforms, opacity, and rendering live in Rust.
"""
from __future__ import annotations

from numbers import Integral
from pathlib import Path

import noon as _base
from _noon_errors import engine_call
from _manim_semantic_handles import (
    _attach_shared_handle, _handle_for, _live_constructor_context, _live_mutation_context,
)

try:
    from js import (
        noonAuthoringImageOptions as _image_options,
        noonCreateAuthoringImageHandle as _create_image,
        noonLoadImageUrl as _load_url,
        Uint8Array,
    )
except ImportError:
    _image_options = _create_image = _load_url = Uint8Array = None

# Pillow's documented integer constants; unsupported filters fail explicitly.
RESAMPLING_ALGORITHMS = {
    "nearest": 0, "none": 0, "bilinear": 2, "linear": 2,
    "bicubic": 3, "cubic": 3,
}
_SAMPLERS = {0: "nearest", 2: "bilinear", 3: "bicubic"}
_MAX_ENCODED_BYTES = 32 * 1024 * 1024
_MAX_RGBA_BYTES = 64 * 1024 * 1024


def _sampling(value):
    if isinstance(value, str):
        value = RESAMPLING_ALGORITHMS.get(value.lower(), value)
    if value not in _SAMPLERS:
        raise NotImplementedError("ImageMobject supports nearest, bilinear, and bicubic resampling")
    return _SAMPLERS[value]


def _encoded_bytes(source):
    if isinstance(source, (str, Path)):
        path = Path(source)
        if path.stat().st_size > _MAX_ENCODED_BYTES:
            raise ValueError("encoded image exceeds 32 MiB")
        with path.open("rb") as file:
            data = file.read(_MAX_ENCODED_BYTES + 1)
    elif hasattr(source, "read"):
        data = source.read(_MAX_ENCODED_BYTES + 1)
    else:
        data = bytes(source)
    if len(data) > _MAX_ENCODED_BYTES:
        raise ValueError("encoded image exceeds 32 MiB")
    return data


def _rgba8_array(source):
    """Normalize supported byte arrays without requiring NumPy for nested lists."""
    shape = getattr(source, "shape", None)
    if shape is not None and hasattr(source, "tobytes"):
        if str(getattr(source, "dtype", "")) != "uint8":
            raise TypeError("ImageMobject arrays must have dtype uint8")
        shape = tuple(shape)
        if len(shape) not in (2, 3):
            raise ValueError("image array must be HxW grayscale or HxWx3/4 RGB(A)")
        height, width = map(int, shape[:2])
        channels = 1 if len(shape) == 2 else int(shape[2])
        _validate_shape(width, height, channels)
        raw = source.tobytes(order="C")
    else:
        height = len(source)
        if not height:
            raise ValueError("image array must be nonempty")
        width = len(source[0])
        if not width:
            raise ValueError("image array must be nonempty")
        first = source[0][0]
        channels = 1 if isinstance(first, Integral) else len(first)
        _validate_shape(width, height, channels)
        raw = bytearray()
        for row in source:
            if len(row) != width:
                raise ValueError("image array rows must have equal width")
            for pixel in row:
                values = (pixel,) if channels == 1 else pixel
                if len(values) != channels:
                    raise ValueError("image array must have consistent channel counts")
                for value in values:
                    if not isinstance(value, Integral) or not 0 <= value <= 255:
                        raise ValueError("image samples must be integers between 0 and 255")
                    raw.append(int(value))
    if channels == 4:
        return width, height, bytes(raw)
    rgba = bytearray(width * height * 4)
    rgba[3::4] = b"\xff" * (width * height)
    for channel in range(3):
        rgba[channel::4] = raw[channel::3] if channels == 3 else raw
    return width, height, bytes(rgba)


def _validate_shape(width, height, channels):
    if width <= 0 or height <= 0 or channels not in (1, 3, 4):
        raise ValueError("image array must be nonempty grayscale, RGB, or RGBA")
    if width * height * 4 > _MAX_RGBA_BYTES:
        raise ValueError("RGBA8 image exceeds 64 MiB")


def _js_bytes(data):
    # One bulk byte transfer into WASM at construction, never a JSON pixel list.
    from pyodide.ffi import to_js
    return to_js(memoryview(data))


class ImageMobject(_base.Mobject):
    """PNG/JPEG files or uint8 grayscale/RGB/RGBA arrays.

    Pixels are immutable. Color replacement, mutable pixel-array access, other
    file formats, and pixel-to-pixel morphing are not supported. Copies share
    pixels but retain independent ordinary semantic object state.
    """
    def __init__(self, filename_or_array, scale_to_resolution=1080, invert=False,
                 image_mode="RGBA", *, resampling_algorithm=3, height=None,
                 opacity=1.0, z_index=0.0, pixel_array_dtype="uint8", **kwargs):
        if kwargs:
            raise NotImplementedError("unsupported ImageMobject option(s): " + ", ".join(sorted(kwargs)))
        if invert or image_mode != "RGBA" or pixel_array_dtype != "uint8":
            raise NotImplementedError("ImageMobject currently supports non-inverted RGBA uint8 resources")
        if _image_options is None:
            raise RuntimeError("ImageMobject requires the shared Rust authoring host")
        sampler = _sampling(resampling_algorithm)
        if isinstance(filename_or_array, (str, Path, bytes, bytearray, memoryview)) or hasattr(filename_or_array, "read"):
            candidate = engine_call(_image_options.encoded, _js_bytes(_encoded_bytes(filename_or_array)))
        else:
            width, pixel_height, pixels = _rgba8_array(filename_or_array)
            candidate = engine_call(_image_options.rgba8, width, pixel_height, _js_bytes(pixels))
        try:
            engine_call(candidate.setScaleToResolution, float(scale_to_resolution))
            engine_call(candidate.setFrameHeight, float(_base.DEFAULT_FRAME_HEIGHT))
            engine_call(candidate.setSampling, sampler)
            engine_call(candidate.setOpacity, float(opacity))
            engine_call(candidate.setZIndex, float(z_index))
            if height is not None:
                engine_call(candidate.setHeight, float(height))
            context = _live_constructor_context("image")
        except BaseException:
            candidate.free()
            raise
        # Both successful and rejected Rust constructors consume the candidate;
        # only a failure while configuring inert options requires explicit free.
        handle = engine_call(_create_image, candidate) if context is None else engine_call(context.liveCreateImage, candidate)
        _attach_shared_handle(self, handle)
        if context is not None:
            self._canonical_live_target_context = context

    @classmethod
    async def from_url(cls, url, **kwargs):
        """Load an explicitly requested CORS-accessible URL before admission."""
        if _load_url is None:
            raise RuntimeError("URL loading requires the browser image host")
        data = await _load_url(str(url))
        return cls(bytes(data.to_py()), **kwargs)

    @classmethod
    async def from_blob(cls, blob, **kwargs):
        """Read one browser Blob before ordinary bounded PNG/JPEG preparation."""
        if int(blob.size) > _MAX_ENCODED_BYTES:
            raise ValueError("encoded image exceeds 32 MiB")
        data = Uint8Array.new(await blob.arrayBuffer())
        return cls(bytes(data.to_py()), **kwargs)

    @property
    def pixel_width(self):
        return int(engine_call(self._semantic_handle.imagePixelWidth))

    @property
    def pixel_height(self):
        return int(engine_call(self._semantic_handle.imagePixelHeight))

    def set_resampling_algorithm(self, algorithm):
        sampler = _sampling(algorithm)
        handle = _handle_for(self)
        if handle is None:
            raise NotImplementedError("image sampling cannot change inside an active callback overlay")
        context = _live_mutation_context(self)
        if context is None:
            engine_call(handle.setImageSampling, sampler)
        else:
            engine_call(context.liveSetImageSampling, handle, sampler)
        return self

    def set_color(self, *args, **kwargs):
        raise NotImplementedError("image pixels are immutable; construct a new image to replace colors")

    def get_pixel_array(self):
        raise NotImplementedError("mutable pixel-array access is not supported by immutable retained images")
