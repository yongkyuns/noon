"""Shared semantic-handle migration for the Manim-compatible Python facade.

Detached objects and `.animate` target-state copies live in Rust/WASM rather than in
Python-owned deep-copied snapshots. Scene-owned objects continue through the existing
scene adapter until the stable execution-slot integration is complete.
"""

from __future__ import annotations

import copy
import json
import sys
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

try:
    from js import noonCreateAuthoringCircleHandle as _create_circle_handle
except ImportError:
    _create_circle_handle = None
try:
    from js import noonCreateAuthoringSquareHandle as _create_square_handle
except ImportError:
    _create_square_handle = None
try:
    from js import noonCreateAuthoringRectangleHandle as _create_rectangle_handle
except ImportError:
    _create_rectangle_handle = None
try:
    from js import noonCreateAuthoringLineHandle as _create_line_handle
except ImportError:
    _create_line_handle = None

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
_ORIGINAL_SET_OBJECT_OPACITY = _base.Mobject.set_object_opacity
_ORIGINAL_NEXT_TO = _base.Mobject.next_to
_ORIGINAL_ALIGN_TO = _base.Mobject.align_to
_ORIGINAL_ALIGN_ON_FRAME = _base.Mobject._align_on_frame
_ORIGINAL_BECOME = _base.Mobject.become
_ORIGINAL_REPLACE = _base.Mobject.replace

_ORIGINAL_SET_FILL = _compat.VMobject.set_fill
_ORIGINAL_VMOBJECT_SET_COLOR = _compat.VMobject.set_color
_ORIGINAL_SET_STROKE = _compat.VMobject.set_stroke
_ORIGINAL_SET_OPACITY = _compat.VMobject.set_opacity
_ORIGINAL_GET_FILL_OPACITY = _compat.VMobject.get_fill_opacity
_ORIGINAL_GET_STROKE_OPACITY = _compat.VMobject.get_stroke_opacity
_ORIGINAL_CIRCLE_INIT = _compat.Circle.__init__
_ORIGINAL_SQUARE_INIT = _compat.Square.__init__
_ORIGINAL_RECTANGLE_INIT = _compat.Rectangle.__init__
_ORIGINAL_LINE_INIT = _compat.Line.__init__
_ORIGINAL_GROUP_INIT = _compat.Group.__init__
_ORIGINAL_GROUP_ADD = _compat.Group.add
_ORIGINAL_GROUP_REMOVE = _compat.Group.remove
_ORIGINAL_GROUP_SHIFT = _compat.Group.shift
_ORIGINAL_GROUP_MOVE_TO = _compat.Group.move_to
_ORIGINAL_GROUP_NEXT_TO = _compat.Group.next_to
_ORIGINAL_GROUP_ALIGN_TO = _compat.Group.align_to
_ORIGINAL_GROUP_ARRANGE = _compat.Group.arrange
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


def _is_bound(value: object) -> bool:
    return (
        isinstance(value, _base.Mobject)
        and value._scene is not None
        and value._object is not None
    )


def _has_unmirrored_tracks(value: _base.Mobject) -> bool:
    if not _is_bound(value):
        return False
    scene = value._scene
    obj = value._object
    assert scene is not None and obj is not None
    # Generic Transform tracks authored through the aligned scheduler are committed
    # back to this handle after successful play. Low-level scalar position/rotation/
    # opacity tracks still live only in the legacy scene timeline, so fall back to an
    # evaluated snapshot if any of those have touched this object.
    return any(
        track["object"] == obj.id
        and track["property"] in {"position", "rotation", "opacity"}
        for track in scene._tracks
    )


def _handle_for(value: object):
    if not isinstance(value, _base.Mobject):
        return None
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    # Once a bound object has arbitrary host updater state attached, the authoritative
    # frame value is the runtime callback snapshot rather than this deterministic
    # authoring handle. Detached objects still need the handle to materialize their
    # initial scene snapshot before the runtime callback path exists.
    if _is_bound(value) and hasattr(value, "_noon_updaters"):
        return None
    if _has_unmirrored_tracks(value):
        return None
    return getattr(value, "_semantic_handle", None)


def _detached_handle_for(value: object):
    return None if _is_bound(value) else _handle_for(value)


def _live_mutation_context(value: object):
    """Return the retained-session context for a bound object or its target.

    The context reference is wrapper identity only. The handle and its current
    state remain in Rust; it lets a target cloned after bootstrap publish its
    detached creation and affine edits through the same live session.
    """
    context = getattr(value, "_canonical_live_target_context", None)
    if context is None:
        scene = getattr(value, "_scene", None)
        context = getattr(scene, "_canonical_authoring_context", None)
    if context is None:
        return None
    ownership = str(context.liveExecutionOwnership())
    # Returning the player keeps its runtime alive for the next source operation.
    # Mutations and target creation must publish through that same session too.
    # A transferred runtime retains this context so its typed call rejects before
    # changing the shared store while another endpoint owns the player.
    return context if ownership in {"active", "transferred", "returned"} else None


def _canonical_target_editor_source(value: object):
    """Return the opaque source/context pair for the narrow callback-safe copy path.

    Bound callback objects intentionally have no ordinary handle access because raw
    geometry remains unavailable. Outside a callback phase, a leaf copy can still
    enter Rust's existing target-editor boundary, which derives its basis from the
    coherent live row and creates a detached semantic target.
    """
    if not isinstance(value, _base.Mobject):
        return None
    handle = getattr(value, "_semantic_handle", None)
    if handle is None or not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    if _has_unmirrored_tracks(value):
        return None
    context = getattr(value, "_canonical_live_target_context", None)
    if context is None:
        scene = getattr(value, "_scene", None)
        context = getattr(scene, "_canonical_authoring_context", None)
    if context is None:
        return None

    from _manim_updaters import _canonical_phase_context

    if _canonical_phase_context(value) is not None:
        raise NotImplementedError(
            "canonical callback copies are unsupported while a callback phase is active"
        )
    return context, handle


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


def _bound_layout_observation(value: _base.Mobject):
    """Ask the owning Rust context for one coherent ordinary live observation."""

    if (
        not _is_bound(value)
        or getattr(value._scene, "_legacy_geometry_materialized", False)
        or not bool(getattr(value, "_semantic_handle_fresh", False))
    ):
        return None
    handle = getattr(value, "_semantic_handle", None)
    if handle is None:
        return None
    context = getattr(value._scene, "_canonical_authoring_context", None)
    query = getattr(context, "queryMobjectLayout", None)
    return None if query is None else query(handle)


_CONSTRUCTOR_MISSING = object()


def _initialize_shared_wrapper(self: _base.Mobject) -> None:
    """Initialize one opaque semantic wrapper without constructing legacy geometry."""
    self._raw = None
    self._scene = None
    self._object = None
    self._semantic_handle = None
    self._semantic_handle_fresh = False


def _attach_shared_handle(self: _base.Mobject, handle: object) -> None:
    _initialize_shared_wrapper(self)
    self._semantic_handle = handle
    self._semantic_handle_fresh = True


def _constructor_color(name: str, value: object) -> _base.Color:
    if not isinstance(value, _base.Color):
        raise TypeError(f"{name} must be a Color or None")
    return value


def _apply_shared_constructor_options(handle: object, kwargs: dict[str, Any]) -> None:
    """Apply Python constructor coercions to one shared typed target.

    The target is either an already-published opaque handle during initial
    authoring or an inert Rust primitive candidate. Both routes perform the
    semantic validation in Rust; Python only applies public argument coercions.
    """
    options = dict(kwargs)
    allowed = {
        "position", "rotation", "scale", "fill", "stroke",
        "stroke_width", "stroke_width_mode", "stroke_join", "stroke_cap",
        "opacity", "fill_color", "stroke_color", "fill_opacity",
        "stroke_opacity",
    }
    unknown = sorted(set(options) - allowed)
    if unknown:
        raise TypeError(f"unsupported Mobject constructor option(s): {', '.join(unknown)}")

    if "position" in options:
        value = _ir._vec2("position", options["position"])
        handle.setTranslation(value["x"], value["y"])
    if "rotation" in options:
        handle.setRotation(_ir._finite_number("rotation", options["rotation"]))
    if "scale" in options:
        value = _ir._vec2("scale", options["scale"])
        handle.setScale(value["x"], value["y"])
    if "stroke_width" in options:
        handle.setStrokeWidth(_phase_b._manim_stroke_width(options["stroke_width"]))
    if "stroke_width_mode" in options:
        handle.setStrokeWidthMode(_ir._stroke_width_mode(options["stroke_width_mode"]))
    if "stroke_join" in options:
        handle.setStrokeJoin(_ir._stroke_join(options["stroke_join"]))
    if "stroke_cap" in options:
        handle.setStrokeCap(_ir._stroke_cap(options["stroke_cap"]))
    if "opacity" in options:
        handle.setObjectOpacity(_ir._finite_number("opacity", options["opacity"]))

    fill = options.get("fill", _CONSTRUCTOR_MISSING)
    fill_color = options.get("fill_color", _CONSTRUCTOR_MISSING)
    if fill_color is not _CONSTRUCTOR_MISSING and fill_color is not None:
        fill = _phase_b._as_color("fill_color", fill_color)
    if fill is not _CONSTRUCTOR_MISSING:
        if fill is None:
            handle.disableFill()
        else:
            parsed = _constructor_color("fill", fill)
            handle.setFill(parsed.red, parsed.green, parsed.blue, parsed.alpha)

    stroke = options.get("stroke", _CONSTRUCTOR_MISSING)
    stroke_color = options.get("stroke_color", _CONSTRUCTOR_MISSING)
    if stroke_color is not _CONSTRUCTOR_MISSING and stroke_color is not None:
        stroke = _phase_b._as_color("stroke_color", stroke_color)
    if stroke is not _CONSTRUCTOR_MISSING:
        if stroke is None:
            handle.disableStroke()
        else:
            parsed = _constructor_color("stroke", stroke)
            handle.setStrokeColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
            handle.setStrokeOpacity(parsed.alpha)

    if options.get("fill_opacity") is not None:
        handle.setFillOpacity(_phase_b._opacity("fill_opacity", options["fill_opacity"]))
    if options.get("stroke_opacity") is not None:
        handle.setStrokeOpacity(_phase_b._opacity("stroke_opacity", options["stroke_opacity"]))


def _apply_shared_constructor_kwargs(self: _base.Mobject, kwargs: dict[str, Any]) -> None:
    _apply_shared_constructor_options(self._semantic_handle, kwargs)


def _apply_constructor_color(handle: object, color: _base.Color | None) -> None:
    if color is not None:
        parsed = _constructor_color("color", color)
        handle.setColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)


def _live_primitive_context():
    """Return the one retained context that may publish a new primitive.

    Before an ordinary segment starts there is no live session to protect, so
    the normal constructor keeps the initial authoring route. Once a session
    exists, direct authoring-store insertion would advance the revision outside
    its published mutation transaction.
    """
    # Without the authoring-scope module there cannot be an active Scene
    # continuation. Standalone primitive authoring need not initialize it.
    reactive = sys.modules.get("_manim_reactive")
    if reactive is None:
        return None
    scene = reactive._current_authoring_scene()
    context = getattr(scene, "_canonical_authoring_context", None)
    if context is None:
        return None
    ownership = str(context.liveExecutionOwnership())
    if ownership in {"active", "returned"}:
        return context
    if ownership == "transferred":
        raise RuntimeError("live primitive construction is unavailable while execution is transferred")
    return None


def _live_primitive_handle(
    context: object,
    shape: str,
    size: float,
    color: _base.Color | None,
    kwargs: dict[str, Any],
):
    begin = (
        context.beginLiveManimCircle
        if shape == "circle"
        else context.beginLiveManimSquare
    )
    candidate = begin(size)
    _apply_shared_constructor_options(candidate, kwargs)
    _apply_constructor_color(candidate, color)
    return context.liveCreateManimPrimitive(candidate)


def _circle_init(
    self: _compat.Circle,
    radius: float = 1.0,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    if _create_circle_handle is None:
        _ORIGINAL_CIRCLE_INIT(self, radius, color=color, **kwargs)
        return
    value = _ir._positive_number("radius", radius)
    context = _live_primitive_context()
    if context is not None:
        handle = _live_primitive_handle(context, "circle", value, color, kwargs)
        _attach_shared_handle(self, handle)
        self._canonical_live_target_context = context
    else:
        _attach_shared_handle(self, _create_circle_handle(value))
        _apply_shared_constructor_kwargs(self, kwargs)
        _apply_constructor_color(self._semantic_handle, color)
    self.radius = value


def _rectangle_init(
    self: _compat.Rectangle,
    width: float = 4.0,
    height: float = 2.0,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    if _create_rectangle_handle is None:
        _ORIGINAL_RECTANGLE_INIT(self, width, height, color=color, **kwargs)
        return
    width_value = _ir._positive_number("width", width)
    height_value = _ir._positive_number("height", height)
    _attach_shared_handle(self, _create_rectangle_handle(width_value, height_value))
    self.width_value = width_value
    self.height_value = height_value
    _apply_shared_constructor_kwargs(self, kwargs)
    if color is not None:
        self.set_color(color)


def _square_init(
    self: _compat.Square,
    side_length: float = 2.0,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    if _create_square_handle is None:
        _ORIGINAL_SQUARE_INIT(self, side_length, color=color, **kwargs)
        return
    value = _ir._positive_number("side_length", side_length)
    context = _live_primitive_context()
    if context is not None:
        handle = _live_primitive_handle(context, "square", value, color, kwargs)
        _attach_shared_handle(self, handle)
        self._canonical_live_target_context = context
    else:
        _attach_shared_handle(self, _create_square_handle(value))
        _apply_shared_constructor_kwargs(self, kwargs)
        _apply_constructor_color(self._semantic_handle, color)
    self.side_length = value
    self.width_value = value
    self.height_value = value


def _line_init(
    self: _compat.Line,
    start: object = None,
    end: object = None,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    start_value = _base.LEFT if start is None else _compat._as_vec2(start)
    end_value = _base.RIGHT if end is None else _compat._as_vec2(end)
    # A temporary Line created inside a canonical callback is an operand, not a
    # new authored object. Ask the active Rust callback context for an opaque,
    # identity-free endpoint value before touching the shared authoring store.
    try:
        from _manim_updaters import callback_line_target

        callback_target = callback_line_target(start_value, end_value)
    except ImportError:
        callback_target = None
    if callback_target is not None:
        if color is not None or kwargs:
            raise NotImplementedError(
                "callback-local Line supports endpoint matching only"
            )
        self._raw = None
        self._scene = None
        self._object = None
        self._semantic_handle = None
        self._semantic_handle_fresh = False
        callback_context, operand = callback_target
        self._callback_line_context = callback_context
        self._callback_line_target = operand
        self.start = start_value
        self.end = end_value
        return
    if _create_line_handle is None:
        _ORIGINAL_LINE_INIT(self, start, end, color=color, **kwargs)
        return
    _attach_shared_handle(
        self,
        _create_line_handle(start_value.x, start_value.y, end_value.x, end_value.y),
    )
    self.start = start_value
    self.end = end_value
    _apply_shared_constructor_kwargs(self, kwargs)
    if color is not None:
        self.set_color(color)


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
        self._semantic_handle_fresh = True
        if fill_opacity is not None:
            self._semantic_handle.setFillOpacity(fill_opacity)
        if stroke_opacity is not None:
            self._semantic_handle.setStrokeOpacity(stroke_opacity)
        # The handle is now authoritative for detached state. Keeping a second Python
        # snapshot here would recreate exactly the ownership split #61 is removing.
        self._raw = None


def _current_raw(self: _base.Mobject) -> _ir.Mobject:
    if getattr(self, "_callback_line_target", None) is not None:
        raise RuntimeError(
            "callback-local Line operands cannot escape into scene, layout, or animation APIs"
        )
    handle = (_handle_for(self) if not getattr(self._scene, "_legacy_geometry_materialized", False)
              else _detached_handle_for(self))
    if handle is not None:
        return _raw_from_json(str(handle.snapshotJson()))
    return _ORIGINAL_CURRENT_RAW(self)


def _apply(self: _base.Mobject, raw: _ir.Mobject) -> _base.Mobject:
    handle = (_handle_for(self) if not getattr(self._scene, "_legacy_geometry_materialized", False)
              else _detached_handle_for(self))
    if handle is not None:
        if _live_mutation_context(self) is not None:
            raise NotImplementedError(
                "raw Mobject replacement is unsupported while canonical live execution is active"
            )
        handle.replaceSnapshotJson(_snapshot_json(raw))
        return self
    result = _ORIGINAL_APPLY(self, raw)
    if _is_bound(self):
        # Arbitrary raw/geometry replacement bypasses the typed shared mutation API.
        # Keep correctness by switching future copy/animate seeding to the evaluated
        # scene snapshot until a shared geometry operation owns this path too.
        self._semantic_handle_fresh = False
    return result


def _clone_mobject(
    self: _base.Mobject, *, target_state: bool = False
) -> _base.Mobject:
    clone = object.__new__(type(self))
    handle = _handle_for(self)
    live_context = _live_mutation_context(self)
    target_context = None
    if handle is None:
        target_source = _canonical_target_editor_source(self)
        if target_source is not None:
            target_context, handle = target_source
    if handle is not None:
        clone._raw = None
        clone._scene = None
        clone._object = None
        context = live_context or target_context
        clone._semantic_handle = (
            context.liveTargetEditor(handle)
            if context is not None
            else handle.targetEditor() if target_state else handle.cloneHandle()
        )
        clone._semantic_handle_fresh = True
        if context is not None:
            clone._canonical_live_target_context = context
    else:
        _init(clone, self._current_raw())

    excluded = {
        "_raw",
        "_scene",
        "_object",
        "_semantic_handle",
        "_semantic_handle_fresh",
        "_canonical_live_target_context",
    }
    if handle is not None and getattr(self, "_retained_handle", None) is handle:
        # Canonical Text exposes the same opaque Rust handle through its text
        # methods. Rebind that alias to the target; JsProxy cannot be deep-copied.
        excluded.add("_retained_handle")
        clone._retained_handle = clone._semantic_handle
    # A callback registry belongs to its source occurrence. The detached target
    # carries only its opaque semantic handle, never copied callback ownership.
    if target_context is not None:
        excluded.update({
            "_noon_updaters",
            "_noon_updater_registrations",
            "_noon_updater_registration_history",
        })
    for name, value in self.__dict__.items():
        if name not in excluded:
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
    observed = _bound_layout_observation(self)
    if observed is not None:
        return _base.Vec2(float(observed.centerX), float(observed.centerY))
    handle = _handle_for(self)
    if handle is not None:
        return _layout_center(self)
    return _ORIGINAL_GET_CENTER(self)


def _get_critical_point(self: _base.Mobject, direction: object) -> _base.Vec2:
    """Read a leaf critical point from the authoritative semantic layout."""
    axis = _compat._as_vec2(direction)
    if isinstance(self, _compat.Group):
        # Shared family layout is the separate #61 migration; Group has no leaf binding.
        return _compat._critical_for(self, axis)
    observed = _bound_layout_observation(self)
    if observed is not None:
        return _base.Vec2(
            float(observed.criticalX(axis.x, axis.y)),
            float(observed.criticalY(axis.x, axis.y)),
        )
    return _critical(self, axis)


def _width(self: _base.Mobject) -> float:
    observed = _bound_layout_observation(self)
    if observed is not None:
        return float(observed.width)
    handle = _handle_for(self)
    if _has_shared_layout_queries(handle):
        return float(handle.width)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].x - bounds[0].x


def _height(self: _base.Mobject) -> float:
    observed = _bound_layout_observation(self)
    if observed is not None:
        return float(observed.height)
    handle = _handle_for(self)
    if _has_shared_layout_queries(handle):
        return float(handle.height)
    bounds = _base._bounds(self._current_raw())
    return 0.0 if bounds is None else bounds[1].y - bounds[0].y


def _set_width_property(self: _base.Mobject, width: float) -> None:
    self.scale_to_fit_width(float(width))


def _set_height_property(self: _base.Mobject, height: float) -> None:
    self.scale_to_fit_height(float(height))


def _has_wire_projection(handle: object) -> bool:
    return handle is not None and all(
        hasattr(handle, name)
        for name in (
            "wireTranslationX",
            "wireTranslationY",
            "wireScaleX",
            "wireScaleY",
            "wireRotation",
            "wireHasFill",
            "wireFillRed",
            "wireFillGreen",
            "wireFillBlue",
            "wireFillAlpha",
            "wireHasStroke",
            "wireStrokeRed",
            "wireStrokeGreen",
            "wireStrokeBlue",
            "wireStrokeAlpha",
            "wireStrokeWidth",
            "wireObjectOpacity",
        )
    )


def _ensure_bound_static_mutation_available(value: _base.Mobject) -> None:
    if not _is_bound(value):
        return
    scene = value._scene
    obj = value._object
    assert scene is not None and obj is not None
    if any(track["object"] == obj.id for track in scene._tracks):
        raise ValueError(
            "direct Mobject mutation after animation authoring is ambiguous; use mobject.animate"
        )


def _mutation_handle_for(value: _base.Mobject):
    handle = _handle_for(value)
    if handle is None:
        return None
    if _is_bound(value):
        if not _has_wire_projection(handle):
            return None
        _ensure_bound_static_mutation_available(value)
    return handle


def _sync_bound_transform(value: _base.Mobject, handle: object) -> None:
    if (not _is_bound(value) or
            hasattr(value._scene, "_semantic_geometry_handles") and
            not getattr(value._scene, "_legacy_geometry_materialized", False)):
        return
    scene = value._scene
    obj = value._object
    assert scene is not None and obj is not None
    transform = scene._objects[obj.id]["transform"]
    transform["translation"]["x"] = float(handle.wireTranslationX)
    transform["translation"]["y"] = float(handle.wireTranslationY)
    transform["scale"]["x"] = float(handle.wireScaleX)
    transform["scale"]["y"] = float(handle.wireScaleY)
    transform["rotation"] = float(handle.wireRotation)


def _wire_color(handle: object, prefix: str) -> dict[str, float] | None:
    if not bool(getattr(handle, f"wireHas{prefix}")):
        return None
    return {
        "red": float(getattr(handle, f"wire{prefix}Red")),
        "green": float(getattr(handle, f"wire{prefix}Green")),
        "blue": float(getattr(handle, f"wire{prefix}Blue")),
        "alpha": float(getattr(handle, f"wire{prefix}Alpha")),
    }


def _sync_bound_style(value: _base.Mobject, handle: object) -> None:
    if (not _is_bound(value) or
            hasattr(value._scene, "_semantic_geometry_handles") and
            not getattr(value._scene, "_legacy_geometry_materialized", False)):
        return
    scene = value._scene
    obj = value._object
    assert scene is not None and obj is not None
    style = scene._objects[obj.id]["style"]
    style["fill"] = _wire_color(handle, "Fill")
    style["stroke"] = _wire_color(handle, "Stroke")
    style["stroke_width"] = float(handle.wireStrokeWidth)
    style["opacity"] = float(handle.wireObjectOpacity)


def invalidate_semantic_handle(value: object) -> None:
    if isinstance(value, _base.Mobject) and hasattr(value, "_semantic_handle"):
        value._semantic_handle_fresh = False


def commit_transform_target(source: object, target: object) -> None:
    if not isinstance(source, _base.Mobject):
        return
    source_handle = _handle_for(source)
    target_handle = _handle_for(target)
    if source_handle is None:
        return
    if target_handle is None:
        invalidate_semantic_handle(source)
        return
    source_handle.becomeHandle(target_handle)
    source._semantic_handle_fresh = True


def _shift(self: _base.Mobject, direction: object) -> _base.Mobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_SHIFT(self, direction)
    offset = _base._as_vec2(direction)
    context = _live_mutation_context(self)
    if context is not None:
        try:
            context.liveShift(handle, offset.x, offset.y)
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
    handle.shift(offset.x, offset.y)
    _sync_bound_transform(self, handle)
    return self


def _move_to(
    self: _base.Mobject,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_MOVE_TO(
            self,
            point_or_mobject,
            aligned_edge=aligned_edge,
            coor_mask=coor_mask,
        )

    context = _live_mutation_context(self)
    if context is not None:
        if _alignment_is_mobject(point_or_mobject):
            raise NotImplementedError(
                "canonical live move_to currently supports point targets only"
            )
        edge = _base._as_vec2(aligned_edge)
        mask = _alignment_mask2(coor_mask)
        if edge.x != 0.0 or edge.y != 0.0:
            raise NotImplementedError(
                "canonical live move_to currently supports center alignment only"
            )
        if mask.x != 1.0 or mask.y != 1.0:
            raise NotImplementedError(
                "canonical live move_to currently supports the default coordinate mask only"
            )
        point = _base._as_vec2(point_or_mobject)
        try:
            context.liveMoveToPoint(handle, point.x, point.y)
        except Exception as error:
            raise ValueError(str(error)) from None
        return self

    edge = _base._as_vec2(aligned_edge)
    if _alignment_is_mobject(point_or_mobject):
        target_handle = _handle_for(point_or_mobject)
        if target_handle is None or not hasattr(handle, "manimMoveToHandle"):
            return _ORIGINAL_MOVE_TO(
                self,
                point_or_mobject,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        mask = _alignment_mask2(coor_mask)
        handle.manimMoveToHandle(target_handle, edge.x, edge.y, mask.x, mask.y)
    else:
        if not hasattr(handle, "manimMoveToPoint"):
            return _ORIGINAL_MOVE_TO(
                self,
                point_or_mobject,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        point = _base._as_vec2(point_or_mobject)
        mask = _alignment_mask2(coor_mask)
        handle.manimMoveToPoint(point.x, point.y, edge.x, edge.y, mask.x, mask.y)
    _sync_bound_transform(self, handle)
    return self


def _scale(self: _base.Mobject, factor: object) -> _base.Mobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_SCALE(self, factor)
    if isinstance(factor, (tuple, list, _base.Vec2)):
        value = _base._as_vec2(factor)
    else:
        scalar = float(factor)
        value = _base.Vec2(scalar, scalar)
    context = _live_mutation_context(self)
    if context is not None:
        try:
            context.liveScale(handle, value.x, value.y)
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
    handle.scale(value.x, value.y)
    _sync_bound_transform(self, handle)
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
    # Shared line geometry routes ``rotate_about_origin`` here after the
    # semantic-handle wrapper becomes the public rotate implementation. During
    # a callback, bypass handle/raw dispatch entirely and mutate the exact
    # property row through the shared Rust transform operation.
    from _manim_updaters import _canonical_phase_context, _canonical_rotate

    if _canonical_phase_context(self) is not None:
        return _canonical_rotate(
            self,
            angle,
            axis,
            about_point=about_point,
            about_edge=about_edge,
            **kwargs,
        )

    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_ROTATE(
            self,
            angle,
            axis,
            about_point=about_point,
            about_edge=about_edge,
            **kwargs,
        )
    context = _live_mutation_context(self)
    if context is not None:
        if kwargs or about_point is not None or about_edge is not None:
            raise NotImplementedError(
                "canonical live affine rotation supports only rotation about the current center"
            )
        try:
            context.liveRotate(handle, _compat._rotation_angle_2d(angle, axis))
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
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
    _sync_bound_transform(self, handle)
    return self


def _set_color(self: _base.Mobject, color: _base.Color) -> _base.Mobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_COLOR(self, color)
    if not isinstance(color, _base.Color):
        raise TypeError("color must be a Color")
    live_context = _live_mutation_context(self)
    if live_context is not None:
        try:
            live_context.liveSetColor(
                handle, color.red, color.green, color.blue, color.alpha
            )
        except Exception as error:
            raise ValueError(str(error)) from None
        _sync_bound_style(self, handle)
        return self

    # Manim changes fill/stroke RGB independently of the channels' existing opacity.
    # The shared wire projection lets both detached and bound objects choose channels
    # without materializing a JSON snapshot.
    if _has_wire_projection(handle):
        had_fill = bool(handle.wireHasFill)
        had_stroke = bool(handle.wireHasStroke)
    else:
        style = self._current_raw().style
        had_fill = style.get("fill") is not None
        had_stroke = style.get("stroke") is not None
    if had_fill:
        handle.setFillColor(color.red, color.green, color.blue, color.alpha)
    if had_stroke:
        handle.setStrokeColor(color.red, color.green, color.blue, color.alpha)
    if not had_fill and not had_stroke:
        handle.setFillColor(color.red, color.green, color.blue, color.alpha)
    _sync_bound_style(self, handle)
    return self


def _set_vmobject_color(
    self: _compat.VMobject,
    color: object,
    family: bool = True,
) -> _compat.VMobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_VMOBJECT_SET_COLOR(self, color, family=family)
    del family
    return _set_color(self, _phase_b._as_color("color", color))


def _become(
    self: _base.Mobject,
    mobject: _base.Mobject,
    match_height: bool = False,
    match_width: bool = False,
    match_depth: bool = False,
    match_center: bool = False,
    stretch: bool = False,
) -> _base.Mobject:
    if _live_mutation_context(self) is not None:
        raise NotImplementedError("canonical live affine targets do not support become")
    handle = _detached_handle_for(self)
    other_handle = _detached_handle_for(mobject)
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
    if _live_mutation_context(self) is not None:
        raise NotImplementedError("canonical live affine targets do not support replace")
    handle = _detached_handle_for(self)
    other_handle = _detached_handle_for(mobject)
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
    handle = _mutation_handle_for(self)
    if (
        handle is None
        or submobject_to_align is not None
        or index_of_submobject_to_align is not None
    ):
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
    if _live_mutation_context(self) is not None:
        raise NotImplementedError(
            "canonical live affine targets do not support layout placement"
        )

    vector = _base._as_vec2(direction)
    edge = _base._as_vec2(aligned_edge)
    if _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is None or not hasattr(handle, "manimNextToHandle"):
            return _ORIGINAL_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        mask = _alignment_mask2(coor_mask)
        handle.manimNextToHandle(
            target_handle,
            vector.x,
            vector.y,
            float(buff),
            edge.x,
            edge.y,
            mask.x,
            mask.y,
        )
    else:
        if not hasattr(handle, "manimNextToPoint"):
            return _ORIGINAL_NEXT_TO(
                self,
                mobject_or_point,
                direction,
                buff,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        point = _base._as_vec2(mobject_or_point)
        mask = _alignment_mask2(coor_mask)
        handle.manimNextToPoint(
            point.x,
            point.y,
            vector.x,
            vector.y,
            float(buff),
            edge.x,
            edge.y,
            mask.x,
            mask.y,
        )
    _sync_bound_transform(self, handle)
    return self


def _align_to(
    self: _base.Mobject,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_ALIGN_TO(self, mobject_or_point, direction)
    if _live_mutation_context(self) is not None:
        raise NotImplementedError(
            "canonical live affine targets do not support layout alignment"
        )
    axis = _base._as_vec2(direction)
    if _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is None or not hasattr(handle, "alignToHandle"):
            return _ORIGINAL_ALIGN_TO(self, mobject_or_point, direction)
        handle.alignToHandle(target_handle, axis.x, axis.y)
    else:
        if not hasattr(handle, "alignToPoint"):
            return _ORIGINAL_ALIGN_TO(self, mobject_or_point, direction)
        point = _base._as_vec2(mobject_or_point)
        handle.alignToPoint(point.x, point.y, axis.x, axis.y)
    _sync_bound_transform(self, handle)
    return self


def _align_on_frame(
    self: _base.Mobject,
    direction: _base.Vec2,
    buff: float,
) -> _base.Mobject:
    handle = _mutation_handle_for(self)
    if handle is None or not hasattr(handle, "alignOnFrame"):
        return _ORIGINAL_ALIGN_ON_FRAME(self, direction, buff)
    if _live_mutation_context(self) is not None:
        raise NotImplementedError(
            "canonical live affine targets do not support frame alignment"
        )
    handle.alignOnFrame(direction.x, direction.y, float(buff))
    _sync_bound_transform(self, handle)
    return self


def _set_fill(
    self: _compat.VMobject,
    color: object = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_FILL(self, color=color, opacity=opacity, family=family)
    live_context = _live_mutation_context(self)
    if live_context is not None:
        try:
            if color is not None and opacity is not None:
                parsed = _phase_b._as_color("fill color", color)
                live_context.liveSetFill(
                    handle,
                    parsed.red,
                    parsed.green,
                    parsed.blue,
                    _phase_b._opacity("fill opacity", opacity),
                )
            elif color is not None:
                parsed = _phase_b._as_color("fill color", color)
                live_context.liveSetFillColor(
                    handle, parsed.red, parsed.green, parsed.blue, parsed.alpha
                )
            elif opacity is None:
                live_context.liveDisableFill(handle)
            if opacity is not None and color is None:
                live_context.liveSetFillOpacity(
                    handle, _phase_b._opacity("fill opacity", opacity)
                )
        except Exception as error:
            raise ValueError(str(error)) from None
        _sync_bound_style(self, handle)
        return self
    if color is not None and opacity is not None:
        parsed = _phase_b._as_color("fill color", color)
        handle.setFill(
            parsed.red,
            parsed.green,
            parsed.blue,
            _phase_b._opacity("fill opacity", opacity),
        )
        _sync_bound_style(self, handle)
        return self
    if color is not None:
        parsed = _phase_b._as_color("fill color", color)
        handle.setFillColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif opacity is None:
        handle.disableFill()
    if opacity is not None:
        handle.setFillOpacity(_phase_b._opacity("fill opacity", opacity))
    _sync_bound_style(self, handle)
    return self


def _set_stroke(
    self: _compat.VMobject,
    color: object = None,
    width: float | None = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_STROKE(
            self, color=color, width=width, opacity=opacity, family=family
        )
    live_context = _live_mutation_context(self)
    if live_context is not None:
        if width is not None:
            raise NotImplementedError(
                "canonical live style targets do not support stroke-width animation"
            )
        try:
            if color is not None and opacity is not None:
                parsed = _phase_b._as_color("stroke color", color)
                live_context.liveSetStroke(
                    handle,
                    parsed.red,
                    parsed.green,
                    parsed.blue,
                    _phase_b._opacity("stroke opacity", opacity),
                )
            elif color is not None:
                parsed = _phase_b._as_color("stroke color", color)
                live_context.liveSetStrokeColor(
                    handle, parsed.red, parsed.green, parsed.blue, parsed.alpha
                )
            elif opacity is None:
                live_context.liveDisableStroke(handle)
            else:
                live_context.liveSetStrokeOpacity(
                    handle, _phase_b._opacity("stroke opacity", opacity)
                )
        except Exception as error:
            raise ValueError(str(error)) from None
        _sync_bound_style(self, handle)
        return self
    if color is not None:
        parsed = _phase_b._as_color("stroke color", color)
        handle.setStrokeColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif width is None and opacity is None:
        handle.disableStroke()
    if width is not None:
        handle.setStrokeWidth(_phase_b._manim_stroke_width(width))
    if opacity is not None:
        handle.setStrokeOpacity(_phase_b._opacity("stroke opacity", opacity))
    _sync_bound_style(self, handle)
    return self


def _set_opacity(
    self: _compat.VMobject,
    opacity: float,
    family: bool = True,
) -> _compat.VMobject:
    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_OPACITY(self, opacity, family=family)
    live_context = _live_mutation_context(self)
    if live_context is not None:
        try:
            live_context.liveSetOpacity(handle, _phase_b._opacity("opacity", opacity))
        except Exception as error:
            raise ValueError(str(error)) from None
        _sync_bound_style(self, handle)
        return self
    handle.setOpacity(_phase_b._opacity("opacity", opacity))
    _sync_bound_style(self, handle)
    return self


def _set_object_opacity(
    self: _base.Mobject,
    opacity: float,
) -> _base.Mobject:
    """Set the object-composite multiplier, distinct from Manim paint opacity."""

    handle = _mutation_handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_OBJECT_OPACITY(self, opacity)
    alpha = _phase_b._opacity("object opacity", opacity)
    live_context = _live_mutation_context(self)
    try:
        if live_context is not None:
            live_context.liveSetObjectOpacity(handle, alpha)
        else:
            handle.setObjectOpacity(alpha)
    except Exception as error:
        raise ValueError(str(error)) from None
    _sync_bound_style(self, handle)
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



def _retained_family_layout_handles(value: object):
    identity = getattr(value, "_semantic_family_member_handle", None)
    retained = getattr(value, "_retained_handle", None)
    if identity is None or retained is None:
        return None
    if not _has_shared_layout_queries(retained):
        raise NotImplementedError(
            "Typst/MathTypst family layout requires Rust-owned retained layout bounds"
        )
    return identity, retained


def _family_layout_leaf_adapter(value: object, *, mutation: bool = False):
    resolver = _mutation_handle_for if mutation else _handle_for
    handle = resolver(value)
    if handle is not None:
        return "mobject", handle
    retained = _retained_family_layout_handles(value)
    if retained is not None:
        return "retained_native_text", retained
    return None


def _shared_family_layout_session(value: object, *, mutation: bool = False):
    if not isinstance(value, _compat.Group):
        return None
    family_handle = getattr(value, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "layoutSession"):
        return None
    leaves = _compat._leaf_mobjects(value)
    leaf_adapters = [
        _family_layout_leaf_adapter(member, mutation=mutation) for member in leaves
    ]
    if not all(adapter is not None for adapter in leaf_adapters):
        return None
    session = family_handle.layoutSession()
    for adapter in leaf_adapters:
        assert adapter is not None
        kind, payload = adapter
        if kind == "mobject":
            session.includeMobject(payload)
        else:
            identity, retained = payload
            session.includeRetainedNativeText(identity, retained)
    return session, leaves, leaf_adapters


def _apply_family_translation(
    self: _compat.Group,
    translation: object,
    leaves: list[_base.Mobject],
    leaf_adapters: list[object],
) -> _compat.Group:
    for member, adapter in zip(leaves, leaf_adapters):
        kind, payload = adapter
        if kind == "mobject":
            translation.applyMobject(payload)
            _sync_bound_transform(member, payload)
        else:
            identity, retained = payload
            translation.applyRetainedNativeText(identity, retained)
    translation.finish()
    return self


def _group_shift(self: _compat.Group, direction: object) -> _compat.Group:
    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_SHIFT(self, direction)
    session, leaves, leaf_handles = shared
    if not hasattr(session, "shiftBy"):
        return _ORIGINAL_GROUP_SHIFT(self, direction)
    offset = _base._as_vec2(direction)
    translation = session.shiftBy(offset.x, offset.y)
    return _apply_family_translation(self, translation, leaves, leaf_handles)


def _group_move_to(
    self: _compat.Group,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _compat.Group:
    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_MOVE_TO(self, point_or_mobject, aligned_edge, coor_mask)
    session, leaves, leaf_handles = shared
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)

    translation = None
    if isinstance(point_or_mobject, _compat.Group):
        target_shared = _shared_family_layout_session(point_or_mobject)
        if target_shared is not None and hasattr(session, "moveToFamily"):
            target_session = target_shared[0]
            translation = session.moveToFamily(
                target_session, edge.x, edge.y, mask.x, mask.y
            )
    elif _alignment_is_mobject(point_or_mobject):
        target_adapter = _family_layout_leaf_adapter(point_or_mobject)
        if target_adapter is not None:
            kind, payload = target_adapter
            if kind == "mobject" and hasattr(session, "moveToMobject"):
                translation = session.moveToMobject(
                    payload, edge.x, edge.y, mask.x, mask.y
                )
            elif kind == "retained_native_text" and hasattr(
                session, "moveToRetainedNativeText"
            ):
                identity, retained = payload
                translation = session.moveToRetainedNativeText(
                    identity, retained, edge.x, edge.y, mask.x, mask.y
                )
    elif hasattr(session, "moveToPoint"):
        point = _base._as_vec2(point_or_mobject)
        translation = session.moveToPoint(
            point.x, point.y, edge.x, edge.y, mask.x, mask.y
        )

    if translation is None:
        return _ORIGINAL_GROUP_MOVE_TO(self, point_or_mobject, aligned_edge, coor_mask)
    return _apply_family_translation(self, translation, leaves, leaf_handles)



def _group_next_to(
    self: _compat.Group,
    mobject_or_point: object,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    aligned_edge: object = _base.ORIGIN,
    submobject_to_align: object | None = None,
    index_of_submobject_to_align: int | None = None,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _compat.Group:
    # Selecting a specific wrapper/member remains explicit #61 debt until shared
    # family-member handles expose that selection. Do not silently rederive it here.
    if submobject_to_align is not None or index_of_submobject_to_align is not None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge,
            submobject_to_align,
            index_of_submobject_to_align,
            coor_mask,
        )

    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge,
            submobject_to_align,
            index_of_submobject_to_align,
            coor_mask,
        )
    session, leaves, leaf_handles = shared
    vector = _base._as_vec2(direction)
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)

    translation = None
    if isinstance(mobject_or_point, _compat.Group):
        target_shared = _shared_family_layout_session(mobject_or_point)
        if target_shared is not None and hasattr(session, "nextToFamily"):
            translation = session.nextToFamily(
                target_shared[0],
                vector.x,
                vector.y,
                float(buff),
                edge.x,
                edge.y,
                mask.x,
                mask.y,
            )
    elif _alignment_is_mobject(mobject_or_point):
        target_adapter = _family_layout_leaf_adapter(mobject_or_point)
        if target_adapter is not None:
            kind, payload = target_adapter
            if kind == "mobject" and hasattr(session, "nextToMobject"):
                translation = session.nextToMobject(
                    payload,
                    vector.x,
                    vector.y,
                    float(buff),
                    edge.x,
                    edge.y,
                    mask.x,
                    mask.y,
                )
            elif kind == "retained_native_text" and hasattr(
                session, "nextToRetainedNativeText"
            ):
                identity, retained = payload
                translation = session.nextToRetainedNativeText(
                    identity,
                    retained,
                    vector.x,
                    vector.y,
                    float(buff),
                    edge.x,
                    edge.y,
                    mask.x,
                    mask.y,
                )
    elif hasattr(session, "nextToPoint"):
        point = _base._as_vec2(mobject_or_point)
        translation = session.nextToPoint(
            point.x,
            point.y,
            vector.x,
            vector.y,
            float(buff),
            edge.x,
            edge.y,
            mask.x,
            mask.y,
        )

    if translation is None:
        return _ORIGINAL_GROUP_NEXT_TO(
            self,
            mobject_or_point,
            direction,
            buff,
            aligned_edge,
            submobject_to_align,
            index_of_submobject_to_align,
            coor_mask,
        )
    return _apply_family_translation(self, translation, leaves, leaf_handles)


def _group_align_to(
    self: _compat.Group,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _compat.Group:
    shared = _shared_family_layout_session(self, mutation=True)
    if shared is None:
        return _ORIGINAL_GROUP_ALIGN_TO(self, mobject_or_point, direction)
    session, leaves, leaf_handles = shared
    axis = _base._as_vec2(direction)

    translation = None
    if isinstance(mobject_or_point, _compat.Group):
        target_shared = _shared_family_layout_session(mobject_or_point)
        if target_shared is not None and hasattr(session, "alignToFamily"):
            translation = session.alignToFamily(target_shared[0], axis.x, axis.y)
    elif _alignment_is_mobject(mobject_or_point):
        target_adapter = _family_layout_leaf_adapter(mobject_or_point)
        if target_adapter is not None:
            kind, payload = target_adapter
            if kind == "mobject" and hasattr(session, "alignToMobject"):
                translation = session.alignToMobject(payload, axis.x, axis.y)
            elif kind == "retained_native_text" and hasattr(
                session, "alignToRetainedNativeText"
            ):
                identity, retained = payload
                translation = session.alignToRetainedNativeText(
                    identity, retained, axis.x, axis.y
                )
    elif hasattr(session, "alignToPoint"):
        point = _base._as_vec2(mobject_or_point)
        translation = session.alignToPoint(point.x, point.y, axis.x, axis.y)

    if translation is None:
        return _ORIGINAL_GROUP_ALIGN_TO(self, mobject_or_point, direction)
    return _apply_family_translation(self, translation, leaves, leaf_handles)



def _group_arrange(
    self: _compat.Group,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    center: bool = True,
    **kwargs: Any,
) -> _compat.Group:
    # Forwarded placement kwargs can select additional alignment semantics; retain
    # the pinned compatibility path until shared member-selection support lands.
    if kwargs:
        return _ORIGINAL_GROUP_ARRANGE(
            self,
            direction=direction,
            buff=buff,
            center=center,
            **kwargs,
        )
    if not self.submobjects:
        return self

    family_handle = getattr(self, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "arrangeSession"):
        return _ORIGINAL_GROUP_ARRANGE(self, direction=direction, buff=buff, center=center)

    axis = _base._as_vec2(_base.RIGHT if direction is None else direction)
    arrangement = family_handle.arrangeSession(axis.x, axis.y, float(buff), bool(center))
    prepared: list[tuple[object, list[_base.Mobject], list[object]]] = []

    for member in self.submobjects:
        if isinstance(member, _compat.Group):
            shared = _shared_family_layout_session(member, mutation=True)
            if shared is None or not hasattr(arrangement, "includeFamily"):
                return _ORIGINAL_GROUP_ARRANGE(
                    self, direction=direction, buff=buff, center=center
                )
            arrangement.includeFamily(shared[0])
            prepared.append((member, shared[1], shared[2]))
        elif isinstance(member, _base.Mobject):
            adapter = _family_layout_leaf_adapter(member, mutation=True)
            if adapter is None:
                return _ORIGINAL_GROUP_ARRANGE(
                    self, direction=direction, buff=buff, center=center
                )
            kind, payload = adapter
            if kind == "mobject":
                arrangement.includeMobject(payload)
            else:
                identity, retained = payload
                arrangement.includeRetainedNativeText(identity, retained)
            prepared.append((member, [member], [adapter]))
        else:
            return _ORIGINAL_GROUP_ARRANGE(self, direction=direction, buff=buff, center=center)

    for member, leaves, leaf_adapters in prepared:
        translation = arrangement.nextTranslation()
        _apply_family_translation(member, translation, leaves, leaf_adapters)
    arrangement.finish()
    return self


def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:
    leaves = _compat._leaf_mobjects(value)

    # Group/VGroup wrapper traversal remains host-language metadata, but the shared
    # family graph independently derives the expected recursive leaf sequence and
    # rejects any wrapper divergence. Rust owns the actual aggregate bounds math.
    if isinstance(value, _compat.Group):
        shared = _shared_family_layout_session(value)
        if shared is not None:
            session = shared[0]
            return (
                _base.Vec2(
                    float(session.criticalX(-1.0, 0.0)),
                    float(session.criticalY(0.0, -1.0)),
                ),
                _base.Vec2(
                    float(session.criticalX(1.0, 0.0)),
                    float(session.criticalY(0.0, 1.0)),
                ),
            )

    # Host-dynamic/stale bound leaves intentionally retain the evaluated-snapshot
    # fallback until runtime family queries exist. Deterministic shared handles do
    # not execute this aggregation path.
    present: list[tuple[_base.Vec2, _base.Vec2]] = []
    for member in leaves:
        handle = _handle_for(member)
        if handle is not None:
            bounds = _layout_bounds(member)
        else:
            retained = _retained_family_layout_handles(member)
            if retained is not None:
                retained_handle = retained[1]
                bounds = (
                    _base.Vec2(
                        float(retained_handle.criticalX(-1.0, 0.0)),
                        float(retained_handle.criticalY(0.0, -1.0)),
                    ),
                    _base.Vec2(
                        float(retained_handle.criticalX(1.0, 0.0)),
                        float(retained_handle.criticalY(0.0, 1.0)),
                    ),
                )
            else:
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

def _family_member_handle(value: object) -> tuple[str | None, object | None]:
    if isinstance(value, _compat.Group):
        return "family", getattr(value, "_semantic_family_handle", None)
    if isinstance(value, _base.Mobject):
        retained_identity = getattr(value, "_semantic_family_member_handle", None)
        if retained_identity is not None:
            return "member", retained_identity
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
    if kind == "member":
        return bool(family_handle.addMember(handle))
    return bool(family_handle.addMobject(handle))


def _family_remove_handle(family_handle: object, value: object) -> bool:
    kind, handle = _family_member_handle(value)
    if handle is None:
        raise RuntimeError("family member has no shared semantic identity")
    if kind == "family":
        return bool(family_handle.removeFamily(handle))
    if kind == "member":
        return bool(family_handle.removeMember(handle))
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
    elif source_kind == "member":
        editor.acceptMember(source_handle, target_handle)
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
    _base.Mobject.get_critical_point = _get_critical_point
    _base.Mobject.width = property(_width, _set_width_property)
    _base.Mobject.height = property(_height, _set_height_property)
    _base.Mobject.shift = _shift
    _base.Mobject.move_to = _move_to
    _base.Mobject.scale = _scale
    _base.Mobject.rotate = _rotate
    _base.Mobject.set_color = _set_color
    _base.Mobject.set_object_opacity = _set_object_opacity
    _base.Mobject.become = _become
    _base.Mobject.replace = _replace
    _base.Mobject.next_to = _next_to
    _base.Mobject.align_to = _align_to
    _base.Mobject._align_on_frame = _align_on_frame

    # VMobject historically had its own deep-copy implementation; route it through
    # the same Rust-owned handle so `.animate` does not recreate Python snapshots.

    _compat.VMobject.copy = _copy_mobject
    _compat.VMobject._copy_for_animate_target = _target_mobject
    _compat.VMobject.set_color = _set_vmobject_color
    _compat.VMobject.set_fill = _set_fill
    _compat.VMobject.set_stroke = _set_stroke
    _compat.VMobject.set_opacity = _set_opacity
    _compat.VMobject.get_fill_opacity = _get_fill_opacity
    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity
    _compat._bounds_for = _compat_bounds_for

    if _create_circle_handle is not None:
        _compat.Circle.__init__ = _circle_init
    if _create_square_handle is not None:
        _compat.Square.__init__ = _square_init
    if _create_rectangle_handle is not None:
        _compat.Rectangle.__init__ = _rectangle_init
    if _create_line_handle is not None:
        _compat.Line.__init__ = _line_init

    if _create_family_handle is not None:
        _GROUP_COPY_DELEGATE = _compat.Group.copy
        _compat.Group.__init__ = _group_init
        _compat.Group.add = _group_add
        _compat.Group.remove = _group_remove
        _compat.Group.shift = _group_shift
        _compat.Group.move_to = _group_move_to
        _compat.Group.next_to = _group_next_to
        _compat.Group.align_to = _group_align_to
        _compat.Group.arrange = _group_arrange
        _compat.Group.copy = _group_copy
        _compat.Group._copy_for_animate_target = _group_target_mobject
