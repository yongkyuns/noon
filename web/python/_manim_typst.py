"""ManimCE retained Text/Typst wrappers over Noon's source-level authoring handles.

These wrappers are intentionally not geometry adapters. They never synthesize an
``_ir.Mobject`` and ``Scene.add`` intercepts them before legacy geometry lowering.
Only source-level text authoring state crosses the Python-worker boundary; shaping,
font bytes, glyph/vector resources, and GPU atlas state remain Rust-owned.
"""

from __future__ import annotations

import json
import math
from typing import Any

import noon as _base
import _manim_compat as _compat

try:
    from js import noonCreateRetainedNativeTextHandle as _create_native_text_handle
    from js import noonCreateRetainedTypstHandle as _create_typst_handle
except ImportError:  # Native CPython tests use the source-level fallbacks below.
    _create_native_text_handle = None
    _create_typst_handle = None

try:
    from js import noonCreateAuthoringFamilyMemberHandle as _create_family_member_handle
except ImportError:  # Shared semantic handles are browser-only.
    _create_family_member_handle = None


# Reserve the upper half of JavaScript's exact-integer range for retained text IDs.
# Legacy geometry IDs start at zero and no practical scene can approach 2^52 objects.
_RETAINED_OBJECT_ID_BASE = 1 << 52
_RETAINED_PROTOCOL_VERSION = 2
_DEFAULT_NATIVE_FONT = "DejaVu Sans Mono"
_INSTALLED = False
_ORIGINAL_SCENE_ADD = _compat.Scene.add
_ORIGINAL_SCENE_IS_PRESENT = _compat.Scene._is_present


def _color_dict(color: _base.Color) -> dict[str, float]:
    return {
        "red": float(color.red),
        "green": float(color.green),
        "blue": float(color.blue),
        "alpha": float(color.alpha),
    }


class _FallbackRetainedHandle:
    """CPython-only stand-in with the exact source-level WASM wire shape."""

    def __init__(self, spec: dict[str, Any]) -> None:
        self._spec = spec

    def shift(self, x: float, y: float) -> None:
        self._spec["transform"]["translation"]["x"] += float(x)
        self._spec["transform"]["translation"]["y"] += float(y)

    def moveTo(self, x: float, y: float) -> None:
        self._spec["transform"]["translation"] = {"x": float(x), "y": float(y)}

    def scale(self, factor: float) -> None:
        self._spec["transform"]["scale"]["x"] *= float(factor)
        self._spec["transform"]["scale"]["y"] *= float(factor)

    def rotate(self, angle: float) -> None:
        self._spec["transform"]["rotation"] += float(angle)

    def setOpacity(self, opacity: float) -> None:
        self._spec["opacity"] = float(opacity)

    def setColor(self, red: float, green: float, blue: float, alpha: float) -> None:
        self._spec["color"] = {
            "red": float(red),
            "green": float(green),
            "blue": float(blue),
            "alpha": float(alpha),
        }

    def specJson(self) -> str:
        return json.dumps(self._spec, separators=(",", ":"), allow_nan=False)


def _base_spec(source: str, backend: dict[str, Any], font_size: float) -> dict[str, Any]:
    return {
        "source": source,
        "backend": backend,
        "font_size": font_size,
        "transform": {
            "translation": {"x": 0.0, "y": 0.0},
            "scale": {"x": 1.0, "y": 1.0},
            "rotation": 0.0,
        },
        "color": _color_dict(_base.WHITE),
        "opacity": 1.0,
    }


def _validated_font_size(value: float) -> float:
    font_size = float(value)
    if not math.isfinite(font_size) or font_size <= 0.0:
        raise ValueError("font_size must be finite and positive")
    return font_size


def _new_typst_handle(source: str, math_mode: bool, font_size: float):
    if not isinstance(source, str) or source == "":
        raise ValueError("Typst source must be a non-empty string")
    font_size = _validated_font_size(font_size)
    if _create_typst_handle is None:
        return _FallbackRetainedHandle(
            _base_spec(source, {"kind": "typst", "math": bool(math_mode)}, font_size)
        )
    return _create_typst_handle(source, bool(math_mode), font_size)


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
    if _create_native_text_handle is None:
        return _FallbackRetainedHandle(
            _base_spec(
                source,
                {
                    "kind": "native",
                    "font_family": font_family,
                    "line_spacing": line_spacing,
                },
                font_size,
            )
        )
    return _create_native_text_handle(source, font_family, font_size, line_spacing)


def _as_color(value: object) -> _base.Color:
    if isinstance(value, _base.Color):
        return value
    raise TypeError("retained text color must be a Noon/Manim Color")


def _native_layout_handle(handle: object):
    required = ("centerX", "centerY", "width", "height", "criticalX", "criticalY")
    if not all(hasattr(handle, name) for name in required):
        raise NotImplementedError(
            "native Text layout queries require the Rust/WASM retained authoring handle"
        )
    return handle


class _RetainedTextMobject(_base.Mobject):
    """Python semantic identity for one retained resource-backed text object."""

    def _initialize_retained(
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
        self._retained_object_id: int | None = None
        self._retained_order: int | None = None
        self._source = source
        self._font_size = float(font_size)
        self._retained_handle = handle
        self._semantic_family_member_handle = (
            None
            if _create_family_member_handle is None
            else _create_family_member_handle()
        )
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
        if self._retained_object_id is None:
            raise AttributeError("detached retained text Mobject has no scene object id")
        return self._retained_object_id

    def _current_raw(self):
        raise TypeError("retained text objects do not have legacy geometry snapshots")

    def _apply(self, raw):
        del raw
        raise TypeError("retained text objects cannot be lowered through legacy geometry")

    def _bind_retained(self, scene: _compat.Scene, object_id: int, order: int) -> None:
        if self._scene is not None and self._scene is not scene:
            raise ValueError("retained text Mobject already belongs to another Scene")
        self._scene = scene
        self._retained_object_id = int(object_id)
        self._retained_order = int(order)

    def _retained_entry(self) -> dict[str, Any]:
        if self._retained_object_id is None or self._retained_order is None:
            raise ValueError("retained text object must belong to a Scene before serialization")
        return {
            "object": self._retained_object_id,
            "order": self._retained_order,
            "text": json.loads(str(self._retained_handle.specJson())),
        }

    def _spec(self) -> dict[str, Any]:
        return json.loads(str(self._retained_handle.specJson()))

    def get_center(self) -> _base.Vec2:
        point = self._spec()["transform"]["translation"]
        return _base.Vec2(float(point["x"]), float(point["y"]))

    def shift(self, direction: object) -> _RetainedTextMobject:
        offset = _compat._as_vec2(direction)
        self._retained_handle.shift(float(offset.x), float(offset.y))
        return self

    def move_to(self, point: object, *args: Any, **kwargs: Any) -> _RetainedTextMobject:
        if args or kwargs:
            raise NotImplementedError("retained text move_to currently supports point targets only")
        target = _compat._as_vec2(point)
        self._retained_handle.moveTo(float(target.x), float(target.y))
        return self

    def center(self) -> _RetainedTextMobject:
        return self.move_to(_base.ORIGIN)

    def set_x(self, x: float) -> _RetainedTextMobject:
        center = self.get_center()
        self._retained_handle.moveTo(float(x), float(center.y))
        return self

    def set_y(self, y: float) -> _RetainedTextMobject:
        center = self.get_center()
        self._retained_handle.moveTo(float(center.x), float(y))
        return self

    def scale(self, factor: float, **kwargs: Any) -> _RetainedTextMobject:
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported retained text scale option(s): {unsupported}")
        value = float(factor)
        if not math.isfinite(value) or value <= 0.0:
            raise ValueError("scale factor must be finite and positive")
        self._retained_handle.scale(value)
        return self

    def rotate(self, angle: float, *args: Any, **kwargs: Any) -> _RetainedTextMobject:
        if args or kwargs:
            raise NotImplementedError("retained text rotate currently supports angle only")
        value = float(angle)
        if not math.isfinite(value):
            raise ValueError("rotation angle must be finite")
        self._retained_handle.rotate(value)
        return self

    def set_color(self, color: _base.Color, family: bool = True) -> _RetainedTextMobject:
        del family
        value = _as_color(color)
        self._retained_handle.setColor(
            float(value.red),
            float(value.green),
            float(value.blue),
            float(value.alpha),
        )
        return self

    def set_opacity(self, opacity: float, family: bool = True) -> _RetainedTextMobject:
        del family
        value = float(opacity)
        if not math.isfinite(value) or not 0.0 <= value <= 1.0:
            raise ValueError("opacity must be finite and between 0 and 1")
        self._retained_handle.setOpacity(value)
        return self

    def _copy_constructor(self) -> _RetainedTextMobject:
        raise NotImplementedError

    def copy(self) -> _RetainedTextMobject:
        spec = self._spec()
        clone = self._copy_constructor()
        transform = spec["transform"]
        translation = transform["translation"]
        scale = transform["scale"]
        if not math.isclose(float(scale["x"]), float(scale["y"]), rel_tol=0.0, abs_tol=1e-12):
            raise NotImplementedError("copy of non-uniform retained text scale is not implemented")
        clone._retained_handle.moveTo(float(translation["x"]), float(translation["y"]))
        clone._retained_handle.scale(float(scale["x"]))
        clone._retained_handle.rotate(float(transform["rotation"]))
        color = spec["color"]
        clone._retained_handle.setColor(
            float(color["red"]),
            float(color["green"]),
            float(color["blue"]),
            float(color["alpha"]),
        )
        clone._retained_handle.setOpacity(float(spec["opacity"]))
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
        self._initialize_retained(str(source), float(font_size), handle, color, opacity)

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
        self._initialize_retained(str(text), float(font_size), handle, color, opacity)

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
        handle = _native_layout_handle(self._retained_handle)
        return _base.Vec2(float(handle.centerX), float(handle.centerY))

    @property
    def width(self) -> float:
        return float(_native_layout_handle(self._retained_handle).width)

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
        return float(_native_layout_handle(self._retained_handle).height)

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
        handle = _native_layout_handle(self._retained_handle)
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


def _ensure_scene_state(scene: _compat.Scene) -> None:
    if not hasattr(scene, "_retained_text_objects"):
        scene._retained_text_objects = []
        scene._retained_next_object_id = _RETAINED_OBJECT_ID_BASE
        # Existing geometry objects already occupy the leading global painter slots.
        scene._retained_next_painter_order = len(scene._objects)


def _add_retained(scene: _compat.Scene, mobject: _RetainedTextMobject, key: str | None) -> None:
    if key is not None:
        raise NotImplementedError("explicit Scene.add keys for retained text are not implemented")
    _ensure_scene_state(scene)
    if mobject._scene is scene:
        scene._register_top_level(mobject)
        return
    if mobject._scene is not None:
        raise ValueError("retained text Mobject already belongs to another Scene")
    object_id = int(scene._retained_next_object_id)
    order = int(scene._retained_next_painter_order)
    scene._retained_next_object_id += 1
    scene._retained_next_painter_order += 1
    mobject._bind_retained(scene, object_id, order)
    scene._retained_text_objects.append(mobject)
    scene._register_top_level(mobject)


def _scene_add(self: _compat.Scene, *mobjects: object, key: str | None = None):
    if not mobjects:
        return self
    if key is not None and len(mobjects) != 1:
        raise ValueError("an explicit key can only be used when adding one Mobject")
    _ensure_scene_state(self)

    single_result: object = self
    for value in mobjects:
        if isinstance(value, _RetainedTextMobject):
            _add_retained(self, value, key)
            single_result = value
            continue

        before = len(self._objects)
        result = _ORIGINAL_SCENE_ADD(self, value, key=key)
        added = len(self._objects) - before
        if added > 0:
            self._retained_next_painter_order += added
        single_result = result

    return single_result if len(mobjects) == 1 else self


def _scene_is_present(self: _compat.Scene, value: object) -> bool:
    if isinstance(value, _RetainedTextMobject):
        return value._scene is self
    return _ORIGINAL_SCENE_IS_PRESENT(self, value)


def _retained_document(self: _compat.Scene) -> dict[str, Any]:
    _ensure_scene_state(self)
    objects = [mobject._retained_entry() for mobject in self._retained_text_objects]
    return {
        "channel": "noon.authoring.retained",
        "protocol_version": _RETAINED_PROTOCOL_VERSION,
        "objects": objects,
    }


def install() -> None:
    """Install retained text authoring without changing legacy geometry lowering."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    _compat.Scene.add = _scene_add
    _compat.Scene._is_present = _scene_is_present
    _compat.Scene.retained_document = _retained_document
    _base.Scene = _compat.Scene

    public = {"Text": Text, "Typst": Typst, "MathTypst": MathTypst}
    for name, value in public.items():
        setattr(_base, name, value)
    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports
