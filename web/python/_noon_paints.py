"""Immutable paint syntax over shared Rust values and ordinary fill operations."""
from __future__ import annotations

from _noon_errors import engine_call
from _noon_effects import _number


class LinearGradient:
    """Local-box paint with ordered ``(position, color)`` stops.

    Coordinates run from the lower-left (0, 0) to upper-right (1, 1).
    Colors interpolate in premultiplied encoded sRGB. Coverage, fill opacity
    and object opacity remain independent. Rust validates all values.
    """
    __slots__ = ("_definition",)

    def __init__(self, stops, *, start=(0.0, 0.5), end=(1.0, 0.5)):
        from _manim_compat import _as_color
        coordinates = lambda value: [_number(v, "gradient coordinate") for v in value]
        packed = []
        for position, color in stops:
            color = _as_color("gradient stop color", color)
            packed.extend((_number(position, "gradient stop position"),
                           color.red, color.green, color.blue, color.alpha))
        from js import noonLinearGradient
        from pyodide.ffi import to_js
        self._definition = engine_call(noonLinearGradient, to_js(coordinates(start)),
                                       to_js(coordinates(end)), to_js(packed))

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self


def set_fill(value, paint, opacity):
    from _manim_semantic_handles import _handle_for, _is_shared_family, _live_mutation_context
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError("paint callback edits require shared staged paint publication")
    if _is_shared_family(value):
        raise NotImplementedError("paint fills currently require a supported planar leaf")
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("paint fills require a shared Rust authoring handle")
    opacity = None if opacity is None else _number(opacity, "fill opacity")
    context = _live_mutation_context(value)
    if isinstance(paint, ImagePaint):
        if context is None:
            engine_call(handle.setFillImage, paint._definition, opacity)
        else:
            engine_call(context.liveSetFillImage, handle, paint._definition, opacity)
    elif context is None:
        engine_call(handle.setFillGradient, paint._definition, opacity)
    else:
        engine_call(context.liveSetFillGradient, handle, paint._definition, opacity)
    return value


class ImageMapping:
    """Immutable image fit and local UV placement, validated by Rust.

    Alignment and translation use lower-left normalized geometry coordinates;
    positive rotation is counterclockwise and signed scale can mirror the image.
    """
    __slots__ = ("_definition",)

    def __init__(self, *, fit="cover", alignment=(0.5, 0.5), address="clamp",
                 translation=(0.0, 0.0), scale=(1.0, 1.0), rotation=0.0):
        from js import noonImageMapping
        from pyodide.ffi import to_js
        pair = lambda value: to_js([_number(v, "image mapping coordinate") for v in value])
        self._definition = engine_call(noonImageMapping, str(fit), pair(alignment), str(address),
                                       pair(translation), pair(scale), _number(rotation, "image rotation"))

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self


class ImagePaint:
    """Immutable PNG/JPEG, uint8 array, or ImageMobject pixels for a planar fill.

    Construction admits no scene object. Reusing an ImageMobject shares its
    immutable pixels and sampling unless explicitly overridden. Array/encoded
    defaults come from Rust preparation. UV animation uses ordinary fill semantics.
    """
    __slots__ = ("_definition",)

    def __init__(self, source, *, mapping=None, sampling=None):
        from js import noonImagePaintOptions
        from _manim_image import ImageMobject, _sampling, _encoded_bytes, _rgba8_array, _js_bytes
        from _manim_semantic_handles import _handle_for
        from pathlib import Path
        if isinstance(source, ImageMobject):
            candidate = engine_call(noonImagePaintOptions.fromImage, _handle_for(source))
        elif isinstance(source, (str, Path, bytes, bytearray, memoryview)) or hasattr(source, "read"):
            candidate = engine_call(noonImagePaintOptions.encoded, _js_bytes(_encoded_bytes(source)))
        else:
            width, height, pixels = _rgba8_array(source)
            candidate = engine_call(noonImagePaintOptions.rgba8, width, height, _js_bytes(pixels))
        try:
            if sampling is not None:
                engine_call(candidate.setSampling, _sampling(sampling))
            if mapping is not None:
                if not isinstance(mapping, ImageMapping):
                    raise TypeError("image mapping must be an ImageMapping")
                engine_call(candidate.setMapping, mapping._definition)
        except BaseException:
            candidate.free()
            raise
        self._definition = candidate

    def with_mapping(self, mapping):
        if not isinstance(mapping, ImageMapping):
            raise TypeError("image mapping must be an ImageMapping")
        result = object.__new__(ImagePaint)
        result._definition = engine_call(self._definition.copy)
        engine_call(result._definition.setMapping, mapping._definition)
        return result

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self


def set_image_mapping(value, mapping, opacity):
    from _manim_semantic_handles import _handle_for, _is_shared_family, _live_mutation_context
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if not isinstance(mapping, ImageMapping):
        raise TypeError("image mapping must be an ImageMapping")
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None or _is_shared_family(value):
        raise NotImplementedError("image mapping edits require a supported planar leaf outside callbacks")
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("image mapping edits require a shared Rust authoring handle")
    opacity = None if opacity is None else _number(opacity, "fill opacity")
    context = _live_mutation_context(value)
    if context is None:
        engine_call(handle.setFillImageMapping, mapping._definition, opacity)
    else:
        engine_call(context.liveSetFillImageMapping, handle, mapping._definition, opacity)
    return value
