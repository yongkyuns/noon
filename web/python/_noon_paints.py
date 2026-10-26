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


def _coordinates(value, name):
    if isinstance(value, (str, bytes)):
        raise TypeError(f"{name} must contain two numbers")
    try:
        values = tuple(value)
    except TypeError as error:
        raise TypeError(f"{name} must contain two numbers") from error
    if len(values) != 2:
        raise ValueError(f"{name} requires two coordinates")
    return [_number(component, name) for component in values]


def _mode(value, name):
    if not isinstance(value, str):
        raise TypeError(f"{name} must be a string")
    return value


class ImagePaint:
    """Fill a planar leaf from an existing immutable ``ImageMobject`` resource.

    ``fit`` is ``"cover"``, ``"contain"``, or ``"stretch"``; ``address`` is
    ``"clamp"``, ``"repeat"``, or ``"transparent"``. Alignment and placement
    use the lower-left normalized local box. Rotation is counterclockwise in
    radians about its center; negative scales mirror the image. ``sampling``
    defaults to the source image's current mode, or accepts ``"nearest"``,
    ``"linear"``/``"bilinear"``, or ``"bicubic"``.

    Construction shares pixels without copying them or displaying the source.
    Rust owns value validation and rendering. Reuse a value with ``set_fill``;
    compatible mapping changes use ordinary ``animate.set_fill`` targets.
    """
    __slots__ = ("_definition",)

    def __init__(self, image, *, fit="cover", alignment=(0.5, 0.5),
                 address="clamp", translation=(0.0, 0.0), scale=(1.0, 1.0),
                 rotation=0.0, sampling=None):
        from _manim_image import ImageMobject
        from _manim_semantic_handles import _handle_for
        if not isinstance(image, ImageMobject):
            raise TypeError("ImagePaint source must be an ImageMobject")
        fit = _mode(fit, "image fit")
        address = _mode(address, "image address")
        sampling = None if sampling is None else _mode(sampling, "image sampling")
        alignment = _coordinates(alignment, "image alignment")
        translation = _coordinates(translation, "image translation")
        scale = _coordinates(scale, "image scale")
        rotation = _number(rotation, "image rotation")
        handle = _handle_for(image)
        if handle is None:
            raise RuntimeError("ImagePaint requires a current shared Rust image handle")
        from js import noonImagePaint
        from pyodide.ffi import to_js
        # The immutable WASM value retains its source Rust store capability.
        # Python owns no image registry, pixels, mapping math, or render state.
        self._definition = engine_call(
            noonImagePaint, handle, fit, to_js(alignment), address,
            to_js(translation), to_js(scale), rotation, sampling,
        )

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
    method = "setFillImage" if isinstance(paint, ImagePaint) else "setFillGradient"
    if context is None:
        engine_call(getattr(handle, method), paint._definition, opacity)
    else:
        engine_call(getattr(context, "live" + method[0].upper() + method[1:]),
                    handle, paint._definition, opacity)
    return value
