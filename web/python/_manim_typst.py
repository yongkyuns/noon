"""ManimCE Text/Typst wrappers over Noon's shared semantic text resources.

Text, Typst, and MathTypst are normal semantic Mobjects in browser authoring. They
never synthesize fake geometry; shaping, font bytes, glyph/vector resources, and GPU
atlas state remain Rust-owned. Explicit export is derived from that same semantic
store rather than from a Python-owned text document.
"""

from __future__ import annotations

from dataclasses import dataclass
from _noon_errors import engine_call

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


@dataclass(frozen=True, slots=True)
class TextSourcePart:
    """Immutable observation of one Rust-owned semantic text source part.

    ``source_start`` and ``source_end`` are UTF-8 byte offsets into the authored
    source. Cluster/vector ranges are observations of the current retained resource;
    ``semantic_key`` is the stable source-part identity.
    """

    source_start: int
    source_end: int
    first_cluster: int
    cluster_count: int
    first_vector: int
    vector_count: int
    semantic_key: str | None


def _validated_font_size(value: float) -> float:
    font_size = float(value)
    if not math.isfinite(font_size) or font_size <= 0.0:
        raise ValueError("font_size must be finite and positive")
    return font_size


def _validated_opacity(value: float) -> float:
    opacity = float(value)
    if not math.isfinite(opacity) or not 0.0 <= opacity <= 1.0:
        raise ValueError("opacity must be finite and between 0 and 1")
    return opacity


def _live_text_context():
    # Lazy import keeps standalone Text construction independent of semantic
    # handle installation while sharing its one authoring-scope ownership test.
    import _manim_semantic_handles

    return _manim_semantic_handles._live_constructor_context("Text")


def _new_typst_handle(source: str, math_mode: bool, font_size: float):
    if not isinstance(source, str) or source == "":
        raise ValueError("Typst source must be a non-empty string")
    font_size = _validated_font_size(font_size)
    if _create_authoring_typst_handle is None:
        raise RuntimeError("Typst requires Noon's shared Rust authoring runtime")
    return engine_call(_create_authoring_typst_handle, source, bool(math_mode), font_size)


def _validated_native_text_options(
    source: str,
    font_family: str,
    font_size: float,
    line_spacing: float,
) -> tuple[str, str, float, float]:
    if not isinstance(source, str):
        raise TypeError("Text source must be a string")
    if not isinstance(font_family, str) or font_family.strip() == "":
        raise ValueError("font must be a non-empty string")
    font_size = _validated_font_size(font_size)
    line_spacing = float(line_spacing)
    if not math.isfinite(line_spacing) or (line_spacing != -1.0 and line_spacing <= -1.0):
        raise ValueError("line_spacing must be -1 or a finite value greater than -1")
    return source, font_family, font_size, line_spacing


def _new_native_text_handle(
    source: str,
    font_family: str,
    font_size: float,
    line_spacing: float,
):
    source, font_family, font_size, line_spacing = _validated_native_text_options(
        source, font_family, font_size, line_spacing
    )
    if _create_authoring_text_handle is None:
        raise RuntimeError("Text requires Noon's shared Rust authoring runtime")
    return engine_call(_create_authoring_text_handle, source, font_family, font_size, line_spacing)


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

    def _bind_to_scene(self, scene: _base.Scene, *, key: str | None = None) -> object:
        if self._scene is scene and self._object is not None:
            return self._object
        return super()._bind_to_scene(scene, key=key)

    def _initialize_text(
        self,
        source: str,
        font_size: float,
        handle: object,
        color: _base.Color,
        opacity: float,
        *,
        presentation_applied: bool = False,
    ) -> None:
        opacity = _validated_opacity(opacity)

        # Do not call Mobject.__init__: that constructor requires legacy geometry.
        self._raw = None
        self._scene = None
        self._object = None
        self._source = source
        self._font_size = float(font_size)
        self._semantic_handle = handle
        self._semantic_handle_fresh = True
        if not presentation_applied:
            self.set_color(_as_color(color))
            self.set_opacity(opacity)

    @property
    def font_size(self) -> float:
        return self._font_size

    @property
    def source(self) -> str:
        return self._source

    def source_parts_for(self, needle: str) -> tuple[TextSourcePart, ...]:
        """Return Rust-owned non-overlapping source matches in authored order."""

        if not isinstance(needle, str):
            raise TypeError("text source-part needle must be a string")
        handle = self._semantic_handle
        if not hasattr(handle, "textSourcePartsFor"):
            raise NotImplementedError(
                "text source-part queries require the shared Rust authoring handle"
            )
        raw_parts = engine_call(handle.textSourcePartsFor, needle)
        try:
            parts: list[TextSourcePart] = []
            for index in range(int(raw_parts.length)):
                raw = engine_call(raw_parts.item, index)
                try:
                    parts.append(
                        TextSourcePart(
                            source_start=int(raw.sourceStart),
                            source_end=int(raw.sourceEnd),
                            first_cluster=int(raw.firstCluster),
                            cluster_count=int(raw.clusterCount),
                            first_vector=int(raw.firstVector),
                            vector_count=int(raw.vectorCount),
                            semantic_key=(None if raw.semanticKey is None else str(raw.semanticKey)),
                        )
                    )
                finally:
                    raw.free()
            return tuple(parts)
        finally:
            raw_parts.free()

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
        opacity = _validated_opacity(kwargs.pop("opacity", 1.0))
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported Typst option(s): {unsupported}")
        if not isinstance(source, str) or source == "":
            raise ValueError("Typst source must be a non-empty string")
        font_size = _validated_font_size(font_size)
        color = _as_color(color)
        live_context = _live_text_context()
        if live_context is None:
            handle = _new_typst_handle(source, self._math_mode, font_size)
        else:
            handle = engine_call(
                live_context.liveCreateManimTypst,
                source,
                bool(self._math_mode),
                font_size,
                float(color.red),
                float(color.green),
                float(color.blue),
                float(color.alpha),
                opacity,
            )
            self._canonical_live_target_context = live_context
        self._initialize_text(
            source,
            font_size,
            handle,
            color,
            opacity,
            presentation_applied=live_context is not None,
        )


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
        opacity = _validated_opacity(kwargs.pop("opacity", 1.0))
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported Text option(s): {unsupported}")
        text, font, font_size, line_spacing = _validated_native_text_options(
            text, font, font_size, line_spacing
        )
        color = _as_color(color)
        live_context = _live_text_context()
        if live_context is None:
            handle = _new_native_text_handle(text, font, font_size, line_spacing)
        else:
            handle = engine_call(
                live_context.liveCreateManimText,
                text,
                font,
                font_size,
                line_spacing,
                float(color.red),
                float(color.green),
                float(color.blue),
                float(color.alpha),
                opacity,
            )
        self._font = str(font)
        self._line_spacing = float(line_spacing)
        if live_context is not None:
            self._canonical_live_target_context = live_context
        self._initialize_text(
            str(text),
            float(font_size),
            handle,
            color,
            opacity,
            presentation_applied=live_context is not None,
        )

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
        axis = _base._as_vec2(direction)
        handle = _native_layout_handle(self._semantic_handle)
        return _base.Vec2(
            float(engine_call(handle.criticalX, float(axis.x), float(axis.y))),
            float(engine_call(handle.criticalY, float(axis.x), float(axis.y))),
        )
