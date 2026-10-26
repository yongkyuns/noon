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
        raise NotImplementedError("gradient callback edits require shared staged paint publication")
    if _is_shared_family(value):
        raise NotImplementedError("gradient fills currently require a supported planar leaf")
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("gradient fills require a shared Rust authoring handle")
    opacity = None if opacity is None else _number(opacity, "fill opacity")
    context = _live_mutation_context(value)
    if context is None:
        engine_call(handle.setFillGradient, paint._definition, opacity)
    else:
        engine_call(context.liveSetFillGradient, handle, paint._definition, opacity)
    return value
