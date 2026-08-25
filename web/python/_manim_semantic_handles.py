"""Shared semantic-handle migration for the Manim-compatible Python facade.

Detached objects and `.animate` target-state copies live in Rust/WASM rather than in
Python-owned deep-copied snapshots. Scene-owned objects continue through the existing
scene adapter until the stable execution-slot integration is complete.
"""

from __future__ import annotations

import copy
import json
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_phase_b as _phase_b

_ir = _base._ir

try:
    from js import noonCreateAuthoringMobjectHandle as _create_handle
except ImportError:  # Native CPython tests do not have the browser bridge.
    _create_handle = None

_INSTALLED = False
_ORIGINAL_INIT = _base.Mobject.__init__
_ORIGINAL_CURRENT_RAW = _base.Mobject._current_raw
_ORIGINAL_APPLY = _base.Mobject._apply
_ORIGINAL_GET_CENTER = _base.Mobject.get_center
_ORIGINAL_SHIFT = _base.Mobject.shift
_ORIGINAL_MOVE_TO = _base.Mobject.move_to
_ORIGINAL_SCALE = _base.Mobject.scale
_ORIGINAL_ROTATE = _base.Mobject.rotate
_ORIGINAL_SET_COLOR = _base.Mobject.set_color
_ORIGINAL_NEXT_TO = _base.Mobject.next_to
_ORIGINAL_ALIGN_TO = _base.Mobject.align_to
_ORIGINAL_ALIGN_ON_FRAME = _base.Mobject._align_on_frame
_ORIGINAL_BECOME = _base.Mobject.become
_ORIGINAL_REPLACE = _base.Mobject.replace

_ORIGINAL_SET_FILL = _compat.VMobject.set_fill
_ORIGINAL_SET_STROKE = _compat.VMobject.set_stroke
_ORIGINAL_SET_OPACITY = _compat.VMobject.set_opacity
_ORIGINAL_GET_FILL_OPACITY = _compat.VMobject.get_fill_opacity
_ORIGINAL_GET_STROKE_OPACITY = _compat.VMobject.get_stroke_opacity


def _snapshot_json(raw: _ir.Mobject) -> str:
    return json.dumps(raw.to_ir(), separators=(",", ":"), allow_nan=False)


def _raw_from_json(value: str) -> _ir.Mobject:
    snapshot = json.loads(value)
    return _ir.Mobject(
        geometry=snapshot["geometry"],
        transform=snapshot["transform"],
        style=snapshot["style"],
    )


def _handle_for(value: object):
    if not isinstance(value, _base.Mobject):
        return None
    if value._scene is not None or value._object is not None:
        return None
    return getattr(value, "_semantic_handle", None)


def _init(self: _base.Mobject, raw: _ir.Mobject) -> None:
    _ORIGINAL_INIT(self, raw)
    if _create_handle is not None:
        self._semantic_handle = _create_handle(_snapshot_json(raw))
        # The handle is now authoritative for detached state. Keeping a second Python
        # snapshot here would recreate exactly the ownership split #61 is removing.
        self._raw = None


def _current_raw(self: _base.Mobject) -> _ir.Mobject:
    handle = _handle_for(self)
    if handle is not None:
        return _raw_from_json(str(handle.snapshotJson()))
    return _ORIGINAL_CURRENT_RAW(self)


def _apply(self: _base.Mobject, raw: _ir.Mobject) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is not None:
        handle.replaceSnapshotJson(_snapshot_json(raw))
        return self
    return _ORIGINAL_APPLY(self, raw)


def _copy_mobject(self: _base.Mobject) -> _base.Mobject:
    clone = object.__new__(type(self))
    handle = _handle_for(self)
    if handle is not None:
        clone._raw = None
        clone._scene = None
        clone._object = None
        clone._semantic_handle = handle.cloneHandle()
    else:
        _init(clone, self._current_raw())

    for name, value in self.__dict__.items():
        if name not in {"_raw", "_scene", "_object", "_semantic_handle"}:
            if isinstance(value, _base.Mobject):
                setattr(clone, name, value.copy())
            else:
                setattr(clone, name, copy.deepcopy(value))
    return clone


def _get_center(self: _base.Mobject) -> _base.Vec2:
    handle = _handle_for(self)
    if handle is not None:
        return _base.Vec2(float(handle.centerX), float(handle.centerY))
    return _ORIGINAL_GET_CENTER(self)


def _width(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if handle is not None:
        return float(handle.width)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].x - bounds[0].x


def _height(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if handle is not None:
        return float(handle.height)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].y - bounds[0].y


def _shift(self: _base.Mobject, direction: object) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SHIFT(self, direction)
    offset = _base._as_vec2(direction)
    handle.shift(offset.x, offset.y)
    return self


def _move_to(self: _base.Mobject, point: object) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_MOVE_TO(self, point)
    value = _base._as_vec2(point)
    handle.moveTo(value.x, value.y)
    return self


def _scale(self: _base.Mobject, factor: object) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SCALE(self, factor)
    if isinstance(factor, (tuple, list, _base.Vec2)):
        value = _base._as_vec2(factor)
    else:
        scalar = float(factor)
        value = _base.Vec2(scalar, scalar)
    handle.scale(value.x, value.y)
    return self


def _rotate(self: _base.Mobject, angle: float) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_ROTATE(self, angle)
    handle.rotate(float(angle))
    return self


def _set_color(self: _base.Mobject, color: _base.Color) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_COLOR(self, color)
    if not isinstance(color, _base.Color):
        raise TypeError("color must be a Color")
    handle.setColor(color.red, color.green, color.blue, color.alpha)
    return self


def _become(
    self: _base.Mobject,
    mobject: _base.Mobject,
    match_height: bool = False,
    match_width: bool = False,
    match_depth: bool = False,
    match_center: bool = False,
    stretch: bool = False,
) -> _base.Mobject:
    handle = _handle_for(self)
    other_handle = _handle_for(mobject)
    if (
        handle is not None
        and other_handle is not None
        and not (match_height or match_width or match_depth or match_center or stretch)
    ):
        handle.becomeHandle(other_handle)
        return self
    return _ORIGINAL_BECOME(
        self,
        mobject,
        match_height=match_height,
        match_width=match_width,
        match_depth=match_depth,
        match_center=match_center,
        stretch=stretch,
    )


def _replace(
    self: _base.Mobject,
    mobject: _base.Mobject,
    dim_to_match: int = 0,
    stretch: bool = False,
) -> _base.Mobject:
    handle = _handle_for(self)
    other_handle = _handle_for(mobject)
    if handle is not None and other_handle is not None:
        if dim_to_match not in (0, 1):
            raise NotImplementedError("replace currently supports width (0) or height (1)")
        handle.replaceHandle(other_handle, int(dim_to_match), bool(stretch))
        return self
    return _ORIGINAL_REPLACE(self, mobject, dim_to_match=dim_to_match, stretch=stretch)


def _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is not None:
        return _base.Vec2(
            float(handle.criticalX(direction.x, direction.y)),
            float(handle.criticalY(direction.x, direction.y)),
        )
    return _base._critical(value._current_raw(), direction)


def _next_to(
    self: _base.Mobject,
    other: object,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_NEXT_TO(self, other, direction, buff)
    axis = _base._as_vec2(direction).normalized()
    other_handle = _handle_for(other)
    if other_handle is not None:
        handle.nextToHandle(other_handle, axis.x, axis.y, float(buff))
    elif isinstance(other, _base.Mobject):
        target = _critical(other, axis)
        handle.nextToPoint(target.x, target.y, axis.x, axis.y, float(buff))
    else:
        target = _base._as_vec2(other)
        handle.nextToPoint(target.x, target.y, axis.x, axis.y, float(buff))
    return self


def _align_to(
    self: _base.Mobject,
    other: _base.Mobject,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_ALIGN_TO(self, other, direction)
    axis = _base._as_vec2(direction)
    other_handle = _handle_for(other)
    if other_handle is not None:
        handle.alignToHandle(other_handle, axis.x, axis.y)
    else:
        target = _critical(other, axis)
        handle.alignToPoint(target.x, target.y, axis.x, axis.y)
    return self


def _align_on_frame(
    self: _base.Mobject,
    direction: _base.Vec2,
    buff: float,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_ALIGN_ON_FRAME(self, direction, buff)
    handle.alignOnFrame(direction.x, direction.y, float(buff))
    return self



def _set_fill(
    self: _compat.VMobject,
    color: object = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_FILL(self, color=color, opacity=opacity, family=family)
    if color is not None:
        parsed = _phase_b._as_color("fill color", color)
        handle.setFillColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif opacity is None:
        handle.disableFill()
    if opacity is not None:
        handle.setFillOpacity(_phase_b._opacity("fill opacity", opacity))
    return self


def _set_stroke(
    self: _compat.VMobject,
    color: object = None,
    width: float | None = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_STROKE(
            self, color=color, width=width, opacity=opacity, family=family
        )
    if color is not None:
        parsed = _phase_b._as_color("stroke color", color)
        handle.setStrokeColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif width is None and opacity is None:
        handle.disableStroke()
    if width is not None:
        value = _base._ir._finite_number("stroke width", width)
        if value < 0.0:
            raise ValueError("stroke width must be non-negative")
        handle.setStrokeWidth(value)
    if opacity is not None:
        handle.setStrokeOpacity(_phase_b._opacity("stroke opacity", opacity))
    return self


def _set_opacity(
    self: _compat.VMobject,
    opacity: float,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_OPACITY(self, opacity, family=family)
    handle.setOpacity(_phase_b._opacity("opacity", opacity))
    return self


def _get_fill_opacity(self: _compat.VMobject) -> float:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_GET_FILL_OPACITY(self)
    return float(handle.fillOpacity)


def _get_stroke_opacity(self: _compat.VMobject) -> float:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_GET_STROKE_OPACITY(self)
    return float(handle.strokeOpacity)


def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:
    leaves = _compat._leaf_mobjects(value)
    present: list[tuple[_base.Vec2, _base.Vec2]] = []
    for member in leaves:
        handle = _handle_for(member)
        if handle is not None:
            center = _base.Vec2(float(handle.centerX), float(handle.centerY))
            half = _base.Vec2(float(handle.width) * 0.5, float(handle.height) * 0.5)
            present.append((center - half, center + half))
            continue
        bounds = _base._bounds(member._current_raw())
        if bounds is not None:
            present.append(bounds)
    if not present:
        return None
    return (
        _base.Vec2(
            min(bound[0].x for bound in present),
            min(bound[0].y for bound in present),
        ),
        _base.Vec2(
            max(bound[1].x for bound in present),
            max(bound[1].y for bound in present),
        ),
    )


def install() -> None:
    global _INSTALLED
    if _INSTALLED or _create_handle is None:
        return
    _INSTALLED = True

    _base.Mobject.__init__ = _init
    _base.Mobject._current_raw = _current_raw
    _base.Mobject._apply = _apply
    _base.Mobject.copy = _copy_mobject
    _base.Mobject.get_center = _get_center
    _base.Mobject.width = property(_width)
    _base.Mobject.height = property(_height)
    _base.Mobject.shift = _shift
    _base.Mobject.move_to = _move_to
    _base.Mobject.scale = _scale
    _base.Mobject.rotate = _rotate
    _base.Mobject.set_color = _set_color
    _base.Mobject.become = _become
    _base.Mobject.replace = _replace
    _base.Mobject.next_to = _next_to
    _base.Mobject.align_to = _align_to
    _base.Mobject._align_on_frame = _align_on_frame

    # VMobject historically had its own deep-copy implementation; route it through
    # the same Rust-owned handle so `.animate` does not recreate Python snapshots.

    _compat.VMobject.copy = _copy_mobject
    _compat.VMobject.set_fill = _set_fill
    _compat.VMobject.set_stroke = _set_stroke
    _compat.VMobject.set_opacity = _set_opacity
    _compat.VMobject.get_fill_opacity = _get_fill_opacity
    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity
    _compat._bounds_for = _compat_bounds_for
