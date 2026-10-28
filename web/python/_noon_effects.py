"""Noon-native effect syntax over the shared Rust authoring handles.

Definitions are immutable Rust values; attachments and targets live in Rust.
Finite ordered effects execute through the shared Rust scene and renderer.
Shared execution/publication admission remains authoritative. No Python interpolation,
attachment table, family traversal, or callback-local shadow state is provided.
"""
from __future__ import annotations

from contextlib import contextmanager
from dataclasses import dataclass
from numbers import Real

import noon as _base
from _manim_rate_functions import there_and_back as _there_and_back
from _noon_errors import engine_call


@dataclass(frozen=True, slots=True)
class Pixels:
    """A length in final output pixels, validated when used in an effect."""
    value: float | tuple[float, float]


def _number(value, name):
    if isinstance(value, bool) or not isinstance(value, Real):
        raise TypeError(f"{name} must be a number, not a boolean or string")
    try:
        return float(value)
    except OverflowError as error:
        raise ValueError(f"{name} is outside the host numeric range") from error


@contextmanager
def _update(*, color=None, radius=None, intensity=None, source=None, offset=None, progress=None, softness=None, seed=None, size=None):
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
    progress = None if progress is None else _number(progress, "progress")
    softness = None if softness is None else _number(softness, "softness")
    seed = None if seed is None else _number(seed, "seed")
    if source is not None and not isinstance(source, str):
        raise TypeError("glow source must be a string")
    try:
        from js import noonEffectUpdate
        from pyodide.ffi import to_js
    except ImportError as error:
        raise RuntimeError("effects require the shared Rust authoring host") from error
    offset_pixels = isinstance(offset, Pixels)
    raw_offset = offset.value if offset_pixels else offset
    packed_offset = [] if raw_offset is None else [_number(v, "offset coordinate") for v in raw_offset]
    if raw_offset is not None and len(packed_offset) != 2:
        raise ValueError("effect offset requires two coordinates")
    request = engine_call(noonEffectUpdate, to_js(components), radius, pixels, intensity, source, to_js(packed_offset), offset_pixels, progress, softness, seed)
    try:
        if size is not None:
            size_pixels = isinstance(size, Pixels)
            engine_call(request.setSize, _number(size.value if size_pixels else size, "size"), size_pixels)
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


class Blur:
    """Immutable Gaussian blur of the painted source; radius is standard deviation.

    A plain radius uses scene units; Pixels uses physical output pixels.
    Attach with add_effect and animate the same radius with set_effect.
    """
    __slots__ = ("_definition",)

    def __init__(self, *, radius=None):
        with _update(radius=radius) as update:
            from js import noonBlur
            self._definition = engine_call(noonBlur, update)

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
    def radius(self):
        value = float(self._definition.radius)
        return Pixels(value) if bool(self._definition.pixels) else value


class BackdropBlur:
    """Gaussian blur of the earlier painter content beneath this object/group.

    The original object coverage masks the background treatment; its paint and
    final opacity remain ordinary. Radius zero preserves the ordinary image.
    Use one backdrop attachment per scope; nested backdrops and earlier spatial
    content are outside the current portable profile and fail shared admission.
    """
    __slots__ = ("_definition",)

    def __init__(self, *, radius=None):
        with _update(radius=radius) as update:
            from js import noonBackdropBlur
            self._definition = engine_call(noonBackdropBlur, update)

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
    def radius(self):
        value = float(self._definition.radius)
        return Pixels(value) if bool(self._definition.pixels) else value


class Shadow:
    """Colored Gaussian of painted alpha behind the original source.

    Color alpha controls shadow opacity. Radius is Gaussian sigma; radius zero
    is a hard shadow. Offset uses +X/right and +Y/up, independent of object pose.
    A tuple uses scene units; Pixels((x, y)) uses final physical output pixels.
    """
    __slots__ = ("_definition",)

    def __init__(self, *, color=None, radius=None, offset=None):
        with _update(color=color, radius=radius, offset=offset) as update:
            from js import noonShadow
            self._definition = engine_call(noonShadow, update)

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
    def offset(self):
        value = (float(self._definition.offsetX), float(self._definition.offsetY))
        return Pixels(value) if bool(self._definition.offsetPixels) else value


class Pixelate:
    """Nearest-centre square cells over the painted object or composed group.

    A plain size uses scene units; Pixels uses final output pixels. Cells follow
    the original source bounds in screen axes. Sizes <= one pixel bypass exactly.
    The portable renderer profile accepts at most 128 resolved output pixels.
    """
    __slots__ = ("_definition",)

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

    def __init__(self, *, size=None):
        with _update(size=size) as update:
            from js import noonPixelate
            self._definition = engine_call(noonPixelate, update)

    @property
    def size(self):
        value = float(self._definition.size)
        return Pixels(value) if bool(self._definition.sizePixels) else value


class NoiseDissolve:
    """Immutable seeded alpha mask over normalized source bounds.

    Progress zero preserves the input; one is transparent. Softness is the
    threshold feather width. Rust validates values and owns parameter animation.
    """
    __slots__ = ("_definition",)

    def __init__(self, *, seed=None, softness=None, progress=None):
        with _update(seed=seed, softness=softness, progress=progress) as update:
            from js import noonNoiseDissolve
            self._definition = engine_call(noonNoiseDissolve, update)

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
    def progress(self):
        return float(self._definition.progress)

    @property
    def softness(self):
        return float(self._definition.softness)

    @property
    def seed(self):
        return int(self._definition.seed)


class Dissolve:
    """Animate an object or composed family's seeded visibility mask in Rust.

    Construction is inert. Direction is ``"in"`` or ``"out"``; omitted style
    fields preserve an existing visibility mask. Rust owns the clock, enrollment
    and completion. ``remover=False`` keeps an out endpoint for a later reveal.
    """
    __slots__ = ("mobject", "direction", "seed", "softness", "introducer", "remover", "anim_args")

    def __init__(self, mobject, *, direction="out", seed=None, softness=None,
                 introducer=None, remover=None, **kwargs):
        from _manim_semantic_handles import _is_shared_family
        if not isinstance(mobject, _base.Mobject) and not _is_shared_family(mobject):
            raise TypeError("Dissolve target must be a Mobject or Group")
        if direction not in ("in", "out"):
            raise ValueError('Dissolve direction must be "in" or "out"')
        for name, value in (("introducer", introducer), ("remover", remover)):
            if value is not None and not isinstance(value, bool):
                raise TypeError(f"Dissolve {name} must be a boolean")
        self.mobject = mobject
        self.direction = direction
        self.seed = None if seed is None else _number(seed, "seed")
        self.softness = None if softness is None else _number(softness, "softness")
        self.introducer = introducer
        self.remover = remover
        self.anim_args = dict(kwargs)


class GlowPulse:
    """Temporarily animate one leaf's canonical glow intensity in shared Rust.

    Construction is inert. Playback captures the effective intensity, restores
    its still-owned channel at the mapped child endpoint. An unadopted temporary
    binding retires at the enclosing segment's completion barrier. Color, radius and source
    remain ordinary persistent glow parameters.

    Scene.on_click(target, GlowPulse(target, ...)) registers the same request for
    Rust input activation without starting playback or capturing a baseline.
    """
    __slots__ = ("mobject", "intensity", "anim_args")

    def __init__(
        self, mobject, *, intensity, run_time=1.0,
        rate_func=_there_and_back, **kwargs,
    ):
        if not isinstance(mobject, _base.Mobject):
            raise TypeError("GlowPulse target must be a leaf Mobject")
        self.mobject = mobject
        self.intensity = _number(intensity, "intensity")
        # Timing validation and option precedence use the ordinary Rust resolver.
        self.anim_args = dict(kwargs, run_time=run_time, rate_func=rate_func)


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
        definition = engine_call(self._handle.authoredDefinition)
        return {"glow": Glow, "blur": Blur, "backdrop_blur": BackdropBlur, "shadow": Shadow, "noise_dissolve": NoiseDissolve, "pixelate": Pixelate}[str(definition.kind)]._from_definition(definition)


def _target(value, scope=None):
    from _manim_semantic_handles import (
        _handle_for, _is_shared_family, _live_mutation_context, _live_constructor_context,
    )
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError("effect edits in callbacks require shared staged effect publication")
    family = _is_shared_family(value)
    if scope is not None and not (family and scope == "composed"):
        raise NotImplementedError("explicit effect scope requires a composed family target")
    handle = value._semantic_family_handle if family else _handle_for(value)
    if handle is None:
        raise RuntimeError("effects require a current shared Rust object handle")
    context = _live_mutation_context(value)
    if context is None and family:
        context = _live_constructor_context("family effect")
    return handle, context, family


_FAMILY_LIVE_METHODS = {
    "setGlow": "liveSetFamilyGlow",
    "addEffect": "liveAddFamilyEffect",
    "setEffect": "liveSetFamilyEffect",
    "setEffectHandle": "liveSetFamilyEffectHandle",
    "removeEffect": "liveRemoveFamilyEffect",
    "removeEffectHandle": "liveRemoveFamilyEffectHandle",
    "removeGlow": "liveRemoveFamilyGlow",
}


def _call(value, method, *args, scope=None):
    handle, context, family = _target(value, scope)
    if context is None:
        return engine_call(getattr(handle, method), *args)
    live_method = _FAMILY_LIVE_METHODS[method] if family else "live" + method[0].upper() + method[1:]
    return engine_call(getattr(context, live_method), handle, *args)


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
    if not isinstance(definition, (Glow, Blur, BackdropBlur, Shadow, NoiseDissolve, Pixelate)):
        raise TypeError("this effect profile supports Glow, Blur, BackdropBlur, Shadow, NoiseDissolve and Pixelate definitions")
    if not isinstance(name, str):
        raise TypeError("effect name must be a string")
    _call(value, "addEffect", definition._definition, name, scope=scope)
    return value


def get_effect(value, name):
    if not isinstance(name, str):
        raise TypeError("effect name must be a string")
    handle, _context, _family = _target(value)
    # Explicit authored attachment lookup, not an effective frame read.
    return EffectHandle._from_handle(engine_call(handle.getEffect, name))


def set_effect(value, selector, *, color=None, radius=None, intensity=None, source=None, offset=None, progress=None, softness=None, seed=None, size=None):
    suffix, selector = _selector(selector)
    with _update(color=color, radius=radius, intensity=intensity, source=source, offset=offset, progress=progress, softness=softness, seed=seed, size=size) as update:
        _call(value, "setEffect" + suffix, selector, update)
    return value


def remove_effect(value, selector):
    suffix, selector = _selector(selector)
    _call(value, "removeEffect" + suffix, selector)
    return value


def remove_glow(value):
    _call(value, "removeGlow")
    return value
