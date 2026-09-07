"""ManimCE Text/Typst wrappers over Noon's shared semantic text resources.

Text, Typst, and MathTypst are normal semantic Mobjects in browser authoring. They
never synthesize fake geometry; shaping, font bytes, glyph/vector resources, and GPU
atlas state remain Rust-owned. Explicit export is derived from that same semantic
store rather than from a Python-owned text document.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _manim_compat as _compat

try:
    from js import noonCreateAuthoringTextHandle as _create_authoring_text_handle
except ImportError:  # Import remains possible for source-only CPython tests.
    _create_authoring_text_handle = None

try:
    from js import noonCreateAuthoringTypstHandle as _create_authoring_typst_handle
except ImportError:  # Import remains possible for source-only CPython tests.
    _create_authoring_typst_handle = None


_DEFAULT_NATIVE_FONT = "DejaVu Sans Mono"
_INSTALLED = False


def _validated_font_size(value: float) -> float:
    font_size = float(value)
    if not math.isfinite(font_size) or font_size <= 0.0:
        raise ValueError("font_size must be finite and positive")
    return font_size


def _new_typst_handle(source: str, math_mode: bool, font_size: float):
    if not isinstance(source, str) or source == "":
        raise ValueError("Typst source must be a non-empty string")
    font_size = _validated_font_size(font_size)
    if _create_authoring_typst_handle is None:
        raise RuntimeError("Typst requires Noon's shared Rust authoring runtime")
    return _create_authoring_typst_handle(source, bool(math_mode), font_size)


def _new_native_text_handle(
    source: str,
    font_family: str,
    font_size: float,
    line_spacing: float,
):
    if not isinstance(source, str):
        raise TypeError("Text source must be a string")
    if not isinstance(font_family, str) or font_family.strip() == "":
        raise ValueError("font must be a non-empty string")
    font_size = _validated_font_size(font_size)
    line_spacing = float(line_spacing)
    if not math.isfinite(line_spacing) or (line_spacing != -1.0 and line_spacing <= -1.0):
        raise ValueError("line_spacing must be -1 or a finite value greater than -1")
    if _create_authoring_text_handle is None:
        raise RuntimeError("Text requires Noon's shared Rust authoring runtime")
    return _create_authoring_text_handle(source, font_family, font_size, line_spacing)


def _as_color(value: object) -> _base.Color:
    if isinstance(value, _base.Color):
        return value
    raise TypeError("text color must be a Noon/Manim Color")


def _native_layout_handle(handle: object):
    required = ("centerX", "centerY", "width", "height", "criticalX", "criticalY")
    if not all(hasattr(handle, name) for name in required):
        raise NotImplementedError(
            "native Text layout queries require the shared Rust authoring handle"
        )
    return handle


def _in_canonical_callback_phase(mobject: _base.Mobject) -> bool:
    """Inspect the existing property overlay without creating Text-local state."""

    # Lazy import avoids making Text initialization depend on updater
    # installation. Production installs the updater adapter after this module.
    import _manim_updaters

    return _manim_updaters._canonical_phase_context(mobject) is not None


class _RetainedTextMobject(_base.Mobject):
    """Python wrapper for one shared semantic resource-backed text object."""

    def _initialize_text(
        self,
        source: str,
        font_size: float,
        handle: object,
        color: _base.Color,
        opacity: float,
    ) -> None:
        opacity = float(opacity)
        if not math.isfinite(opacity) or not 0.0 <= opacity <= 1.0:
            raise ValueError("opacity must be finite and between 0 and 1")

        # Do not call Mobject.__init__: that constructor requires legacy geometry.
        self._raw = None
        self._scene = None
        self._object = None
        self._source = source
        self._font_size = float(font_size)
        self._semantic_handle = handle
        self._semantic_handle_fresh = True
        self.set_color(_as_color(color))
        self.set_opacity(opacity)

    @property
    def font_size(self) -> float:
        return self._font_size

    @property
    def source(self) -> str:
        return self._source

    @property
    def id(self) -> int:
        if self._object is None:
            raise AttributeError("detached text Mobject has no scene object id")
        return int(self._object.id)

    def _current_raw(self):
        raise TypeError("semantic text objects do not have legacy geometry snapshots")

    def _apply(self, raw):
        del raw
        raise TypeError("semantic text objects cannot be lowered through legacy geometry")

    def get_center(self) -> _base.Vec2:
        return _base.Mobject.get_center(self)

    def shift(self, direction: object) -> _RetainedTextMobject:
        _base.Mobject.shift(self, direction)
        return self

    def move_to(self, point: object, *args: Any, **kwargs: Any) -> _RetainedTextMobject:
        if args or kwargs:
            raise NotImplementedError("text move_to currently supports point targets only")
        _base.Mobject.move_to(self, point)
        return self

    def center(self) -> _RetainedTextMobject:
        return self.move_to(_base.ORIGIN)

    def set_x(self, x: float) -> _RetainedTextMobject:
        _base.Mobject.set_x(self, x)
        return self

    def set_y(self, y: float) -> _RetainedTextMobject:
        _base.Mobject.set_y(self, y)
        return self

    def scale(self, factor: float, **kwargs: Any) -> _RetainedTextMobject:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported text scale option(s): {unsupported}")
        value = float(factor)
        if not math.isfinite(value) or value <= 0.0:
            raise ValueError("scale factor must be finite and positive")
        _base.Mobject.scale(self, value)
        return self

    def rotate(self, angle: float, *args: Any, **kwargs: Any) -> _RetainedTextMobject:
        if args or kwargs:
            raise NotImplementedError("text rotate currently supports angle only")
        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("rotation angle must be finite")
        _base.Mobject.rotate(self, value)
        return self

    def set_color(self, color: _base.Color, family: bool = True) -> _RetainedTextMobject:
        del family
        value = _as_color(color)
        _base.Mobject.set_color(self, value)
        return self

    def set_opacity(self, opacity: float, family: bool = True) -> _RetainedTextMobject:
        del family
        value = float(opacity)
        if not math.isfinite(value) or not 0.0 <= value <= 1.0:
            raise ValueError("opacity must be finite and between 0 and 1")
        if _in_canonical_callback_phase(self):
            _base.Mobject.set_opacity(self, value)
            return self
        # The shared semantic handler publishes through an active live session,
        # rejects a transferred lease, and mutates authored state only before handoff.
        _base.Mobject.set_object_opacity(self, value)
        return self

    def _copy_constructor(self) -> _RetainedTextMobject:
        raise NotImplementedError

    def copy(self) -> _RetainedTextMobject:
        if _in_canonical_callback_phase(self):
            raise NotImplementedError(
                "canonical callback Text copy is unsupported; use property operations"
            )
        clone = self._copy_constructor()
        source = self._semantic_handle
        clone._semantic_handle.moveTo(
            float(source.wireTranslationX), float(source.wireTranslationY)
        )
        clone._semantic_handle.setScale(float(source.wireScaleX), float(source.wireScaleY))
        clone._semantic_handle.setRotation(float(source.wireRotation))
        clone._semantic_handle.setColor(
            float(source.wireFillRed),
            float(source.wireFillGreen),
            float(source.wireFillBlue),
            float(source.wireFillAlpha),
        )
        clone._semantic_handle.setObjectOpacity(float(source.wireObjectOpacity))
        return clone


class _RetainedTypstMobject(_RetainedTextMobject):
    _math_mode = False

    def __init__(
        self,
        source: str,
        *,
        font_size: float = 48.0,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        opacity = float(kwargs.pop("opacity", 1.0))
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported Typst option(s): {unsupported}")
        handle = _new_typst_handle(source, self._math_mode, font_size)
        self._initialize_text(str(source), float(font_size), handle, color, opacity)

    def _copy_constructor(self) -> _RetainedTypstMobject:
        return type(self)(self._source, font_size=self._font_size, color=_base.WHITE)


class Typst(_RetainedTypstMobject):
    _math_mode = False


class MathTypst(_RetainedTypstMobject):
    _math_mode = True


class Text(_RetainedTextMobject):
    """Deterministic native plain text compiled and rendered entirely by Rust."""

    def __init__(
        self,
        text: str,
        *,
        font: str = _DEFAULT_NATIVE_FONT,
        font_size: float = 48.0,
        line_spacing: float = -1.0,
        color: _base.Color = _base.WHITE,
        **kwargs: Any,
    ) -> None:
        opacity = float(kwargs.pop("opacity", 1.0))
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported Text option(s): {unsupported}")
        handle = _new_native_text_handle(text, font, font_size, line_spacing)
        self._font = str(font)
        self._line_spacing = float(line_spacing)
        self._initialize_text(str(text), float(font_size), handle, color, opacity)

    @property
    def text(self) -> str:
        return self._source

    @property
    def font(self) -> str:
        return self._font

    @property
    def line_spacing(self) -> float:
        return self._line_spacing

    def get_center(self) -> _base.Vec2:
        return _base.Mobject.get_center(self)

    @property
    def width(self) -> float:
        if _in_canonical_callback_phase(self):
            raise NotImplementedError(
                "canonical callback Text width requires a shared effective layout query"
            )
        return float(_base.Mobject.width.__get__(self, type(self)))

    @width.setter
    def width(self, value: float) -> None:
        target = float(value)
        current = self.width
        if not math.isfinite(target) or target <= 0.0:
            raise ValueError("Text width must be finite and positive")
        if current <= 0.0:
            raise ValueError("cannot set width of zero-width Text")
        self.scale(target / current)

    @property
    def height(self) -> float:
        if _in_canonical_callback_phase(self):
            raise NotImplementedError(
                "canonical callback Text height requires a shared effective layout query"
            )
        return float(_base.Mobject.height.__get__(self, type(self)))

    @height.setter
    def height(self, value: float) -> None:
        target = float(value)
        current = self.height
        if not math.isfinite(target) or target <= 0.0:
            raise ValueError("Text height must be finite and positive")
        if current <= 0.0:
            raise ValueError("cannot set height of zero-height Text")
        self.scale(target / current)

    def get_critical_point(self, direction: object) -> _base.Vec2:
        axis = _compat._as_vec2(direction)
        handle = _native_layout_handle(self._semantic_handle)
        return _base.Vec2(
            float(handle.criticalX(float(axis.x), float(axis.y))),
            float(handle.criticalY(float(axis.x), float(axis.y))),
        )

    def _copy_constructor(self) -> Text:
        return type(self)(
            self._source,
            font=self._font,
            font_size=self._font_size,
            line_spacing=self._line_spacing,
            color=_base.WHITE,
        )


def install() -> None:
    """Install shared semantic Text and Typst wrappers."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    _base.Scene = _compat.Scene

    public = {"Text": Text, "Typst": Typst, "MathTypst": MathTypst}
    for name, value in public.items():
        setattr(_base, name, value)
    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports
