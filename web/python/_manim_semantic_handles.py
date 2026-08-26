"""Shared semantic-handle migration for the Manim-compatible Python facade.

Detached objects and `.animate` target-state copies live in Rust/WASM rather than in
Python-owned deep-copied snapshots. Scene-owned objects continue through the existing
scene adapter until the stable execution-slot integration is complete.
"""

from __future__ import annotations

import copy
import json
from contextvars import ContextVar
from typing import Any

import noon as _base
import _manim_compat as _compat
import _manim_phase_b as _phase_b


def _alignment_mask2(value: object) -> _base.Vec2:
    try:
        length = len(value)  # type: ignore[arg-type]
    except (TypeError, AttributeError):
        length = None
    if length in (2, 3):
        try:
            return _base.Vec2(float(value[0]), float(value[1]))  # type: ignore[index]
        except (TypeError, ValueError, IndexError) as error:
            raise TypeError("coordinate mask must contain numeric x/y values") from error
    raise TypeError("coordinate mask must be a two- or three-component vector")


def _alignment_is_mobject(value: object) -> bool:
    return isinstance(value, _base.Mobject)


def _alignment_critical(value: object, direction: _base.Vec2) -> _base.Vec2:
    if not _alignment_is_mobject(value):
        raise TypeError("critical-point target must be a Mobject")
    return value.get_critical_point(direction)  # type: ignore[union-attr]


def _alignment_indexed(value: object, index: int | None) -> object:
    if index is None:
        return value
    try:
        return value[index]  # type: ignore[index]
    except (TypeError, AttributeError, IndexError) as error:
        raise IndexError("alignment submobject index is unavailable") from error


def _manim_move_to(
    self: _base.Mobject,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject:
    """Pinned ManimCE v0.21.0 ``Mobject.move_to`` in Noon's x/y plane."""

    edge = _base._as_vec2(aligned_edge)
    if _alignment_is_mobject(point_or_mobject):
        target = _alignment_critical(point_or_mobject, edge)
    else:
        target = _base._as_vec2(point_or_mobject)
    source = _alignment_critical(self, edge)
    mask = _alignment_mask2(coor_mask)
    delta = target - source
    return self.shift(_base.Vec2(delta.x * mask.x, delta.y * mask.y))


def _manim_next_to(
    self: _base.Mobject,
    mobject_or_point: object,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    aligned_edge: object = _base.ORIGIN,
    submobject_to_align: object | None = None,
    index_of_submobject_to_align: int | None = None,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject:
    """Pinned Manim ``next_to`` semantics, including unnormalized direction."""

    vector = _base._as_vec2(direction)
    edge = _base._as_vec2(aligned_edge)

    if _alignment_is_mobject(mobject_or_point):
        target_aligner = _alignment_indexed(
            mobject_or_point, index_of_submobject_to_align
        )
        target = _alignment_critical(target_aligner, edge + vector)
    else:
        target = _base._as_vec2(mobject_or_point)

    if submobject_to_align is not None:
        aligner = submobject_to_align
    elif index_of_submobject_to_align is not None:
        aligner = _alignment_indexed(self, index_of_submobject_to_align)
    else:
        aligner = self
    source = _alignment_critical(aligner, edge - vector)

    mask = _alignment_mask2(coor_mask)
    delta = target - source + vector * float(buff)
    return self.shift(_base.Vec2(delta.x * mask.x, delta.y * mask.y))


def _manim_align_to(
    self: _base.Mobject,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    """Pinned Manim ``align_to`` semantics for Mobject and point targets."""

    axis = _base._as_vec2(direction)
    target = (
        _alignment_critical(mobject_or_point, axis)
        if _alignment_is_mobject(mobject_or_point)
        else _base._as_vec2(mobject_or_point)
    )
    source = _alignment_critical(self, axis)
    return self.shift(
        _base.Vec2(
            target.x - source.x if axis.x != 0.0 else 0.0,
            target.y - source.y if axis.y != 0.0 else 0.0,
        )
    )


def _manim_arrange(
    self: _compat.Group,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    center: bool = True,
    **kwargs: Any,
) -> _compat.Group:
    """Pinned Manim ``arrange`` forwarding placement kwargs to ``next_to``."""

    vector = _base.RIGHT if direction is None else direction
    for previous, current in zip(self.submobjects, self.submobjects[1:]):
        current.next_to(previous, vector, buff, **kwargs)
    if center:
        self.center()
    return self


# Install compatibility placement before capturing fallbacks below. The generic
# formulas use dynamic ``shift``/query dispatch, so after semantic-handle install
# the same code mutates Rust/WASM-owned detached objects and ordinary scene objects.
_base.Mobject.move_to = _manim_move_to
_base.Mobject.next_to = _manim_next_to
_base.Mobject.align_to = _manim_align_to
_compat.Group.move_to = _manim_move_to
_compat.Group.next_to = _manim_next_to
_compat.Group.align_to = _manim_align_to
_compat.Group.arrange = _manim_arrange

_ir = _base._ir

try:
    from js import noonCreateAuthoringMobjectHandle as _create_handle
except ImportError:  # Native CPython tests do not have the browser bridge.
    _create_handle = None

try:
    from js import noonCreateAuthoringFamilyHandle as _create_family_handle
except ImportError:  # Older/mock bridges may expose only leaf Mobject handles.
    _create_family_handle = None

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
_ORIGINAL_GROUP_INIT = _compat.Group.__init__
_ORIGINAL_GROUP_ADD = _compat.Group.add
_ORIGINAL_GROUP_REMOVE = _compat.Group.remove
_GROUP_COPY_DELEGATE = None
_GROUP_TARGET_COPY = ContextVar("noon_group_target_copy", default=False)


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


def _has_shared_layout_queries(handle: object) -> bool:
    return handle is not None and all(
        hasattr(handle, name)
        for name in ("centerX", "centerY", "width", "height", "criticalX", "criticalY")
    )


def _layout_bounds(value: _base.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Read exact world-space layout bounds from a detached shared handle."""

    handle = _handle_for(value)
    if not _has_shared_layout_queries(handle):
        return _base._bounds(value._current_raw())
    return (
        _base.Vec2(
            float(handle.criticalX(-1.0, 0.0)),
            float(handle.criticalY(0.0, -1.0)),
        ),
        _base.Vec2(
            float(handle.criticalX(1.0, 0.0)),
            float(handle.criticalY(0.0, 1.0)),
        ),
    )


def _layout_center(value: _base.Mobject) -> _base.Vec2:
    handle = _handle_for(value)
    if not _has_shared_layout_queries(handle):
        raw = value._current_raw()
        bounds = _base._bounds(raw)
        if bounds is not None:
            return (bounds[0] + bounds[1]) * 0.5
        translation = raw.transform["translation"]
        return _base.Vec2(float(translation["x"]), float(translation["y"]))
    return _base.Vec2(float(handle.centerX), float(handle.centerY))


def _init(self: _base.Mobject, raw: _ir.Mobject) -> None:
    _ORIGINAL_INIT(self, raw)
    if _create_handle is not None:
        # Preserve exact Python authoring opacity before the wire/render snapshot
        # lowers color alpha to f32. The semantic handle owns the f64 API contract.
        fill = raw.style.get("fill")
        stroke = raw.style.get("stroke")
        fill_opacity = None if fill is None else float(fill["alpha"])
        stroke_opacity = None if stroke is None else float(stroke["alpha"])
        self._semantic_handle = _create_handle(_snapshot_json(raw))
        if fill_opacity is not None:
            self._semantic_handle.setFillOpacity(fill_opacity)
        if stroke_opacity is not None:
            self._semantic_handle.setStrokeOpacity(stroke_opacity)
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


def _clone_mobject(
    self: _base.Mobject, *, target_state: bool = False
) -> _base.Mobject:
    clone = object.__new__(type(self))
    handle = _handle_for(self)
    if handle is not None:
        clone._raw = None
        clone._scene = None
        clone._object = None
        clone._semantic_handle = (
            handle.targetEditor() if target_state else handle.cloneHandle()
        )
    else:
        _init(clone, self._current_raw())

    for name, value in self.__dict__.items():
        if name not in {"_raw", "_scene", "_object", "_semantic_handle"}:
            if isinstance(value, _base.Mobject):
                setattr(clone, name, value.copy())
            else:
                setattr(clone, name, copy.deepcopy(value))
    return clone


def _copy_mobject(self: _base.Mobject) -> _base.Mobject:
    return _clone_mobject(self, target_state=bool(_GROUP_TARGET_COPY.get()))


def _target_mobject(self: _base.Mobject) -> _base.Mobject:
    """Clone a detached target through Rust's explicit target-editor boundary."""

    # Group/VGroup inherit the Mobject protocol but intentionally retain their
    # Python-owned family copy path until shared family handles land under #61.
    if not hasattr(self, "_scene") or not hasattr(self, "_object"):
        return self.copy()
    return _clone_mobject(self, target_state=True)


def _get_center(self: _base.Mobject) -> _base.Vec2:
    if _handle_for(self) is not None:
        return _layout_center(self)
    return _ORIGINAL_GET_CENTER(self)


def _width(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if _has_shared_layout_queries(handle):
        return float(handle.width)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].x - bounds[0].x


def _height(self: _base.Mobject) -> float:
    handle = _handle_for(self)
    if _has_shared_layout_queries(handle):
        return float(handle.height)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].y - bounds[0].y


def _set_width_property(self: _base.Mobject, width: float) -> None:
    self.scale_to_fit_width(float(width))


def _set_height_property(self: _base.Mobject, height: float) -> None:
    self.scale_to_fit_height(float(height))


def _shift(self: _base.Mobject, direction: object) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SHIFT(self, direction)
    offset = _base._as_vec2(direction)
    handle.shift(offset.x, offset.y)
    return self


def _move_to(
    self: _base.Mobject,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject:
    return _ORIGINAL_MOVE_TO(
        self,
        point_or_mobject,
        aligned_edge=aligned_edge,
        coor_mask=coor_mask,
    )


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


def _rotate(
    self: _base.Mobject,
    angle: float,
    axis: object = _compat.OUT,
    *,
    about_point: object | None = None,
    about_edge: object | None = None,
    **kwargs: Any,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_ROTATE(
            self,
            angle,
            axis,
            about_point=about_point,
            about_edge=about_edge,
            **kwargs,
        )
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim rotate option(s): {unsupported}")
    signed_angle = _compat._rotation_angle_2d(angle, axis)
    if about_point is not None:
        pivot = _compat._as_vec2(about_point)
    elif about_edge is None:
        pivot = _base.Vec2(float(handle.centerX), float(handle.centerY))
    else:
        edge = _compat._as_vec2(about_edge)
        pivot = _base.Vec2(
            float(handle.criticalX(edge.x, edge.y)),
            float(handle.criticalY(edge.x, edge.y)),
        )
    handle.rotateAboutPoint(signed_angle, pivot.x, pivot.y)
    return self


def _set_color(self: _base.Mobject, color: _base.Color) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_COLOR(self, color)
    if not isinstance(color, _base.Color):
        raise TypeError("color must be a Color")

    # Manim changes fill/stroke RGB independently of the channels' existing opacity.
    # The semantic handle's broad setColor API intentionally applies one alpha to both
    # channels, so use the channel-specific mutations here instead. Those preserve the
    # current semantic opacity when the channel already exists.
    style = self._current_raw().style
    had_fill = style.get("fill") is not None
    had_stroke = style.get("stroke") is not None
    if had_fill:
        handle.setFillColor(color.red, color.green, color.blue, color.alpha)
    if had_stroke:
        handle.setStrokeColor(color.red, color.green, color.blue, color.alpha)
    if not had_fill and not had_stroke:
        handle.setFillColor(color.red, color.green, color.blue, color.alpha)
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
    if _has_shared_layout_queries(handle):
        return _base.Vec2(
            float(handle.criticalX(direction.x, direction.y)),
            float(handle.criticalY(direction.x, direction.y)),
        )
    return _base._critical(value._current_raw(), direction)


def _next_to(
    self: _base.Mobject,
    mobject_or_point: object,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    aligned_edge: object = _base.ORIGIN,
    submobject_to_align: object | None = None,
    index_of_submobject_to_align: int | None = None,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject:
    return _ORIGINAL_NEXT_TO(
        self,
        mobject_or_point,
        direction,
        buff,
        aligned_edge=aligned_edge,
        submobject_to_align=submobject_to_align,
        index_of_submobject_to_align=index_of_submobject_to_align,
        coor_mask=coor_mask,
    )


def _align_to(
    self: _base.Mobject,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    return _ORIGINAL_ALIGN_TO(self, mobject_or_point, direction)


def _align_on_frame(
    self: _base.Mobject,
    direction: _base.Vec2,
    buff: float,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None or not hasattr(handle, "alignOnFrame"):
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
    if color is not None and opacity is not None:
        parsed = _phase_b._as_color("fill color", color)
        handle.setFill(
            parsed.red,
            parsed.green,
            parsed.blue,
            _phase_b._opacity("fill opacity", opacity),
        )
        return self
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
        handle.setStrokeWidth(_phase_b._manim_stroke_width(width))
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
        bounds = (
            _layout_bounds(member)
            if _handle_for(member) is not None
            else _base._bounds(member._current_raw())
        )
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


def _family_member_handle(value: object) -> tuple[str | None, object | None]:
    if isinstance(value, _compat.Group):
        return "family", getattr(value, "_semantic_family_handle", None)
    if isinstance(value, _base.Mobject):
        # Family identity survives scene binding even though ordinary detached-state
        # mutations stop using this handle after binding.
        return "mobject", getattr(value, "_semantic_handle", None)
    return None, None


def _family_add_handle(family_handle: object, value: object) -> bool:
    kind, handle = _family_member_handle(value)
    if handle is None:
        raise RuntimeError("family member has no shared semantic identity")
    if kind == "family":
        return bool(family_handle.addFamily(handle))
    return bool(family_handle.addMobject(handle))


def _family_remove_handle(family_handle: object, value: object) -> bool:
    kind, handle = _family_member_handle(value)
    if handle is None:
        raise RuntimeError("family member has no shared semantic identity")
    if kind == "family":
        return bool(family_handle.removeFamily(handle))
    return bool(family_handle.removeMobject(handle))


def _validate_group_members(owner: _compat.Group, mobjects: tuple[object, ...]) -> None:
    for mobject in mobjects:
        if not isinstance(mobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Group members must be Mobjects or Groups")
        if mobject is owner:
            raise ValueError("Group cannot contain itself")


def _group_init(self: _compat.Group, *mobjects: object) -> None:
    self._semantic_family_handle = _create_family_handle()
    _ORIGINAL_GROUP_INIT(self, *mobjects)


def _group_add(self: _compat.Group, *mobjects: object) -> _compat.Group:
    _validate_group_members(self, mobjects)
    family_handle = self._semantic_family_handle
    for mobject in mobjects:
        if _family_add_handle(family_handle, mobject):
            _ORIGINAL_GROUP_ADD(self, mobject)
    return self


def _group_remove(self: _compat.Group, *mobjects: object) -> _compat.Group:
    family_handle = self._semantic_family_handle
    for mobject in mobjects:
        if _family_remove_handle(family_handle, mobject):
            _ORIGINAL_GROUP_REMOVE(self, mobject)
    return self


def _family_target_accept(editor: object, source: object, target: object) -> None:
    source_kind, source_handle = _family_member_handle(source)
    target_kind, target_handle = _family_member_handle(target)
    if source_kind != target_kind or source_handle is None or target_handle is None:
        raise RuntimeError("Group target wrapper mirror diverged from shared family membership")
    if source_kind == "family":
        editor.acceptFamily(source_handle, target_handle)
    elif source_kind == "mobject":
        editor.acceptMobject(source_handle, target_handle)
    else:
        raise RuntimeError("unsupported Group target member kind")


def _group_target_copy(self: _compat.Group) -> _compat.Group:
    delegate = _GROUP_COPY_DELEGATE
    if delegate is None:
        raise RuntimeError("shared Group copy delegate is not installed")
    source_family_handle = getattr(self, "_semantic_family_handle", None)
    if source_family_handle is None:
        raise RuntimeError("Group has no shared semantic family identity")

    # Reuse the geometry layer's constructor-free wrapper clone so custom Group
    # subclasses preserve named child references. During this call only, member
    # `copy()` operations route leaf state through the shared target editor and
    # nested Groups recursively construct their own shared target families.
    token = _GROUP_TARGET_COPY.set(True)
    family_handle = self.__dict__.pop("_semantic_family_handle", None)
    try:
        clone = delegate(self)
    finally:
        if family_handle is not None:
            self._semantic_family_handle = family_handle
        _GROUP_TARGET_COPY.reset(token)

    if len(clone.submobjects) != len(self.submobjects):
        raise RuntimeError("Group target wrapper copy changed direct membership")
    editor = source_family_handle.targetEditor()
    for source_member, target_member in zip(
        self.submobjects, clone.submobjects, strict=True
    ):
        _family_target_accept(editor, source_member, target_member)
    clone._semantic_family_handle = editor.finish()
    return clone


def _group_target_mobject(self: _compat.Group) -> _compat.Group:
    """Build a Group.animate target through the shared family target editor."""

    return _group_target_copy(self)


def _group_copy(self: _compat.Group) -> _compat.Group:
    if _GROUP_TARGET_COPY.get():
        return _group_target_copy(self)
    delegate = _GROUP_COPY_DELEGATE
    if delegate is None:
        raise RuntimeError("shared Group copy delegate is not installed")

    # The geometry layer owns the constructor-free wrapper-copy algorithm, including
    # remapping custom subclass attributes such as Arrow._shaft/_tip. A Pyodide
    # JsProxy cannot be deep-copied, so temporarily remove only the shared family
    # handle from that host-language metadata pass. Nested Groups recurse through
    # this adapter and receive their own fresh family identities.
    family_handle = self.__dict__.pop("_semantic_family_handle", None)
    try:
        clone = delegate(self)
    finally:
        if family_handle is not None:
            self._semantic_family_handle = family_handle

    # Constructor-based delegates may already have created a family handle. The
    # browser geometry delegate uses object.__new__ and therefore needs one here.
    if getattr(clone, "_semantic_family_handle", None) is None:
        clone._semantic_family_handle = _create_family_handle()
        for member in clone.submobjects:
            _family_add_handle(clone._semantic_family_handle, member)
    return clone


def install() -> None:
    global _INSTALLED, _GROUP_COPY_DELEGATE
    if _INSTALLED or _create_handle is None:
        return
    _INSTALLED = True

    _base.Mobject.__init__ = _init
    _base.Mobject._current_raw = _current_raw
    _base.Mobject._apply = _apply
    _base.Mobject.copy = _copy_mobject
    _base.Mobject._copy_for_animate_target = _target_mobject
    _base.Mobject.get_center = _get_center
    _base.Mobject.width = property(_width, _set_width_property)
    _base.Mobject.height = property(_height, _set_height_property)
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
    _compat.VMobject._copy_for_animate_target = _target_mobject
    _compat.VMobject.set_fill = _set_fill
    _compat.VMobject.set_stroke = _set_stroke
    _compat.VMobject.set_opacity = _set_opacity
    _compat.VMobject.get_fill_opacity = _get_fill_opacity
    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity
    _compat._bounds_for = _compat_bounds_for

    if _create_family_handle is not None:
        _GROUP_COPY_DELEGATE = _compat.Group.copy
        _compat.Group.__init__ = _group_init
        _compat.Group.add = _group_add
        _compat.Group.remove = _group_remove
        _compat.Group.copy = _group_copy
        _compat.Group._copy_for_animate_target = _group_target_mobject
