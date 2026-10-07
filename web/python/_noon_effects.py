"""Noon-native effect syntax over the shared Rust authoring handles.

Definitions are immutable Rust values; attachments and targets live in Rust.
This M0 surface supports leaf declarations, not effect playback. The shared
execution/publication gates remain authoritative. No Python interpolation,
attachment table, family traversal, or callback-local shadow state is provided.
"""
from __future__ import annotations

from contextlib import contextmanager
from dataclasses import dataclass
from numbers import Real

import noon as _base
from _noon_errors import engine_call


@dataclass(frozen=True, slots=True)
class Pixels:
    """Gaussian sigma in final output pixels, validated when used in an effect."""
    value: float


def _number(value, name):
    if isinstance(value, bool) or not isinstance(value, Real):
        raise TypeError(f"{name} must be a number, not a boolean or string")
    try:
        return float(value)
    except OverflowError as error:
        raise ValueError(f"{name} is outside the host numeric range") from error


@contextmanager
def _update(*, color=None, radius=None, intensity=None, source=None):
    # Only language shape/coercion here. Defaults, bounds and nonfinite checks
    # are shared with Rust, including validation before f64-to-f32 narrowing.
    components = []
    if color is not None:
        from _manim_compat import _as_color
        parsed = _as_color("glow color", color)
        components = [parsed.red, parsed.green, parsed.blue, parsed.alpha]
    pixels = isinstance(radius, Pixels)
    radius = None if radius is None else _number(radius.value if pixels else radius, "radius")
    intensity = None if intensity is None else _number(intensity, "intensity")
    if source is not None and not isinstance(source, str):
        raise TypeError("glow source must be a string")
    try:
        from js import noonGlowUpdate
        from pyodide.ffi import to_js
    except ImportError as error:
        raise RuntimeError("effects require the shared Rust authoring host") from error
    request = engine_call(noonGlowUpdate, to_js(components), radius, pixels, intensity, source)
    try:
        yield request
    finally:
        request.free()


class Glow:
    """Immutable shared-Rust glow definition, not a bound effect or animation.

    Constructing this value does not allocate semantic/GPU resources. Omitted
    constructor fields use Rust's defaults; omitted setter fields preserve state.
    """
    __slots__ = ("_definition",)

    def __init__(self, *, color=None, radius=None, intensity=None, source=None):
        with _update(color=color, radius=radius, intensity=intensity, source=source) as update:
            from js import noonGlow
            self._definition = engine_call(noonGlow, update)

    @classmethod
    def _from_definition(cls, definition):
        result = object.__new__(cls)
        result._definition = definition
        return result

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        memo[id(self)] = self
        return self

    @property
    def color(self):
        value = self._definition
        return _base.Color(float(value.red), float(value.green), float(value.blue), float(value.alpha))

    @property
    def radius(self):
        value = float(self._definition.radius)
        return Pixels(value) if bool(self._definition.pixels) else value

    @property
    def intensity(self):
        return float(self._definition.intensity)

    @property
    def source(self):
        return str(self._definition.source)


class EffectHandle:
    """Opaque Rust attachment reference; wrapper lifetime never controls removal."""
    __slots__ = ("_handle",)

    def __init__(self):
        raise TypeError("obtain an effect handle with object.get_effect(name)")

    @classmethod
    def _from_handle(cls, handle):
        result = object.__new__(cls)
        result._handle = handle
        return result

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        # A copied reference keeps its exact Rust generation, just like Rust Clone.
        # Copying an owning Mobject is a different shared semantic operation.
        memo[id(self)] = self
        return self

    @property
    def authored_definition(self):
        """A checked authored snapshot, explicitly not a runtime-effective query."""
        return Glow._from_definition(engine_call(self._handle.authoredDefinition))


def _target(value, scope=None):
    from _manim_semantic_handles import _handle_for, _is_shared_family, _live_mutation_context
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError("effect edits in callbacks require shared staged effect publication")
    if scope is not None or _is_shared_family(value):
        raise NotImplementedError("this effect profile supports leaf declarations only; group/view execution is unavailable")
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("effects require a current shared Rust object handle")
    return handle, _live_mutation_context(value)


def _call(value, method, *args, scope=None):
    handle, context = _target(value, scope)
    if context is None:
        return engine_call(getattr(handle, method), *args)
    return engine_call(getattr(context, "live" + method[0].upper() + method[1:]), handle, *args)


def _selector(selector):
    if isinstance(selector, str):
        return "", selector
    if isinstance(selector, EffectHandle):
        return "Handle", selector._handle
    raise TypeError("effect selector must be a name or an EffectHandle")


def set_glow(value, *, color=None, radius=None, intensity=None, source=None, scope=None):
    with _update(color=color, radius=radius, intensity=intensity, source=source) as update:
        _call(value, "setGlow", update, scope=scope)
    return value


def add_effect(value, definition, *, name, scope=None):
    if not isinstance(definition, Glow):
        raise TypeError("this effect profile supports Glow definitions only")
    if not isinstance(name, str):
        raise TypeError("effect name must be a string")
    _call(value, "addEffect", definition._definition, name, scope=scope)
    return value


def get_effect(value, name):
    if not isinstance(name, str):
        raise TypeError("effect name must be a string")
    handle, _context = _target(value)
    # Explicit authored attachment lookup, not an effective frame read.
    return EffectHandle._from_handle(engine_call(handle.getEffect, name))


def set_effect(value, selector, *, color=None, radius=None, intensity=None, source=None):
    suffix, selector = _selector(selector)
    with _update(color=color, radius=radius, intensity=intensity, source=source) as update:
        _call(value, "setEffect" + suffix, selector, update)
    return value


def remove_effect(value, selector):
    suffix, selector = _selector(selector)
    _call(value, "removeEffect" + suffix, selector)
    return value


def remove_glow(value):
    _call(value, "removeGlow")
    return value
