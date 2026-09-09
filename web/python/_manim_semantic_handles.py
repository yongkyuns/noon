"""Shared semantic operations for the Manim-compatible Python facade.

Detached, scene-owned and live targets use typed Rust handles. Callback execution
uses its explicit staged property view; this module installs no public methods.
"""

from __future__ import annotations

import copy
import json
import sys
from typing import Any

import noon as _base
import _manim_compat as _compat


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


_ir = _base._ir

try:
    from js import noonAuthoringGeometryOptions as _geometry_options
    from js import noonAuthoringVectorPath as _authoring_vector_path
    from js import noonCreateAuthoringGeometryHandle as _create_geometry_handle
except ImportError:  # Native CPython tests do not have the browser bridge.
    _geometry_options = None
    _authoring_vector_path = None
    _create_geometry_handle = None

try:
    from js import noonCreateAuthoringFamilyHandle as _create_family_handle
    from js import noonAuthoringMembershipBatch as _new_membership_batch
except ImportError:  # Native CPython tests install explicit bridge fixtures.
    _create_family_handle = None
    _new_membership_batch = None




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


def _handle_for(value: object):
    if not isinstance(value, _base.Mobject):
        return None
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    # Only an active callback phase owns an effective overlay. Registration
    # metadata cannot disable ordinary typed authoring/live operations.
    if _is_bound(value):
        from _manim_updaters import _canonical_phase_context
        if _canonical_phase_context(value) is not None:
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


def _typed_manim_observation(
    value: object,
    handle_method: str,
    context_method: str,
    *,
    handle_property: bool = False,
):
    """Read one narrow Manim observation from its current Rust authority.

    Bound live objects use the execution publication; detached objects use their
    authored semantic handle. A typed wrapper never falls through to a Python
    scene projection.
    """
    semantic_handle = getattr(value, "_semantic_handle", None)
    if semantic_handle is None:
        return None
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        raise NotImplementedError("typed Mobject observation requires a valid semantic handle")
    if _is_bound(value):
        # Match layout observation routing: only the scene that bound this identity
        # can supply its effective execution state. A live-constructor context on a
        # detached wrapper owns mutations, but has no bound execution identity yet.
        context = getattr(value._scene, "_canonical_authoring_context", None)
        query = getattr(context, context_method, None)
        if query is None:
            raise NotImplementedError(
                "typed Mobject observation is unavailable in this authoring phase"
            )
        return query(semantic_handle)
    handle = _handle_for(value)
    if handle is None:
        raise NotImplementedError("typed Mobject observation is unavailable in this authoring phase")
    observation = getattr(handle, handle_method)
    return observation if handle_property else observation()


def _manim_line_endpoints_observation(value: object):
    return _typed_manim_observation(
        value,
        "manimLineEndpoints",
        "queryMobjectLineEndpoints",
    )


def _manim_color_observation(value: object):
    return _typed_manim_observation(value, "manimColor", "queryMobjectColor")


def _require_typed_manim_line(value: object) -> bool:
    """Validate exact Line content through the current Rust observation owner."""
    semantic_handle = getattr(value, "_semantic_handle", None)
    if semantic_handle is None:
        return False
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        raise NotImplementedError("exact Line admission requires a valid semantic handle")
    try:
        _manim_line_endpoints_observation(value)
    except Exception:
        # Preserve ownership/effective-driver failures for an authored Line. A
        # direct authored query is used only to classify wrong semantic content.
        try:
            semantic_handle.manimLineEndpoints()
        except Exception as content_error:
            raise NotImplementedError(
                "ShowPassingFlash currently qualifies the exact Line subset; "
                "general VMobject path windows remain partial"
            ) from content_error
        raise
    return True


def _layout_bounds(value: _base.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Read exact world-space layout bounds from a detached shared handle."""

    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")
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
    if handle is None:
        raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")
    return _base.Vec2(float(handle.centerX), float(handle.centerY))


def _bound_layout_observation(value: _base.Mobject):
    """Ask the owning Rust context for one coherent ordinary live observation."""

    if (
        not _is_bound(value)
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
        handle.setStrokeWidth(_compat._manim_stroke_width(options["stroke_width"]))
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
        fill = _compat._as_color("fill_color", fill_color)
    if fill is not _CONSTRUCTOR_MISSING:
        if fill is None:
            handle.disableFill()
        else:
            parsed = _constructor_color("fill", fill)
            handle.setFill(parsed.red, parsed.green, parsed.blue, parsed.alpha)

    stroke = options.get("stroke", _CONSTRUCTOR_MISSING)
    stroke_color = options.get("stroke_color", _CONSTRUCTOR_MISSING)
    if stroke_color is not _CONSTRUCTOR_MISSING and stroke_color is not None:
        stroke = _compat._as_color("stroke_color", stroke_color)
    if stroke is not _CONSTRUCTOR_MISSING:
        if stroke is None:
            handle.disableStroke()
        else:
            parsed = _constructor_color("stroke", stroke)
            handle.setStrokeColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
            handle.setStrokeOpacity(parsed.alpha)

    if options.get("fill_opacity") is not None:
        handle.setFillOpacity(_compat._opacity("fill_opacity", options["fill_opacity"]))
    if options.get("stroke_opacity") is not None:
        handle.setStrokeOpacity(_compat._opacity("stroke_opacity", options["stroke_opacity"]))


def _apply_shared_constructor_kwargs(self: _base.Mobject, kwargs: dict[str, Any]) -> None:
    _apply_shared_constructor_options(self._semantic_handle, kwargs)


def _apply_constructor_color(handle: object, color: _base.Color | None) -> None:
    if color is not None:
        parsed = _constructor_color("color", color)
        handle.setColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)


def _live_constructor_context(kind: str = "primitive"):
    """Return the one retained context that may publish a new Mobject.

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
        raise RuntimeError(
            f"live {kind} construction is unavailable while execution is transferred"
        )
    return None


def _vector_path_options(path: dict[str, Any]):
    if set(path) != {"commands"}:
        raise ValueError("shared vector path supports commands only")
    candidate = _authoring_vector_path()
    for command in path["commands"]:
        if command == "close":
            candidate.close()
        elif isinstance(command, dict) and set(command) == {"move_to"}:
            point = command["move_to"]["to"]
            candidate.moveTo(float(point["x"]), float(point["y"]))
        elif isinstance(command, dict) and set(command) == {"line_to"}:
            point = command["line_to"]["to"]
            candidate.lineTo(float(point["x"]), float(point["y"]))
        elif isinstance(command, dict) and set(command) == {"quadratic_to"}:
            values = command["quadratic_to"]
            control, point = values["control"], values["to"]
            candidate.quadraticTo(
                float(control["x"]), float(control["y"]),
                float(point["x"]), float(point["y"]),
            )
        elif isinstance(command, dict) and set(command) == {"cubic_to"}:
            values = command["cubic_to"]
            first, second, point = values["control1"], values["control2"], values["to"]
            candidate.cubicTo(
                float(first["x"]), float(first["y"]),
                float(second["x"]), float(second["y"]),
                float(point["x"]), float(point["y"]),
            )
        else:
            raise ValueError("unsupported vector path command")
    return _geometry_options.path(candidate)


def _geometry_options_from_raw(raw: _ir.Mobject):
    geometry = raw.geometry
    if len(geometry) != 1:
        raise ValueError("shared geometry must contain exactly one geometry variant")
    if "circle" in geometry:
        options = _geometry_options.circle(float(geometry["circle"]["radius"]))
    elif "rectangle" in geometry:
        size = geometry["rectangle"]["size"]
        options = _geometry_options.rectangle(float(size["x"]), float(size["y"]))
    elif "line" in geometry:
        line = geometry["line"]
        start, end = line["start"], line["end"]
        options = _geometry_options.line(
            float(start["x"]), float(start["y"]),
            float(end["x"]), float(end["y"]),
        )
    elif "vector_path" in geometry:
        options = _vector_path_options(geometry["vector_path"])
    else:
        raise ValueError("shared geometry supports circle, rectangle, line, or vector path")

    transform = raw.transform
    translation, scale = transform["translation"], transform["scale"]
    options.setTranslation(float(translation["x"]), float(translation["y"]))
    options.setRotation(float(transform["rotation"]))
    options.setScale(float(scale["x"]), float(scale["y"]))

    style = raw.style
    # Width editing intentionally materializes a default stroke in shared Rust.
    # Apply it before the authored paints so an explicit absent stroke stays absent.
    options.setStrokeWidth(float(style["stroke_width"]))
    options.setStrokeWidthMode(style["stroke_width_mode"])
    options.setStrokeJoin(style["stroke_join"])
    options.setStrokeCap(style["stroke_cap"])
    fill = style["fill"]
    if fill is None:
        options.disableFill()
    else:
        options.setFill(
            float(fill["red"]), float(fill["green"]),
            float(fill["blue"]), float(fill["alpha"]),
        )
    stroke = style["stroke"]
    if stroke is None:
        options.disableStroke()
    else:
        options.setStroke(
            float(stroke["red"]), float(stroke["green"]),
            float(stroke["blue"]), float(stroke["alpha"]),
        )
    options.setObjectOpacity(float(style["opacity"]))
    return options


def _consume_geometry_options(options: object, kind: str = "geometry"):
    context = _live_constructor_context(kind)
    if context is None:
        return _create_geometry_handle(options), None
    return context.liveCreateManimGeometry(options), context


def _attach_geometry_options(
    self: _base.Mobject, options: object, kind: str = "geometry"
) -> None:
    handle, context = _consume_geometry_options(options, kind)
    _attach_shared_handle(self, handle)
    if context is not None:
        self._canonical_live_target_context = context


def _circle_init(
    self: _compat.Circle,
    radius: float = 1.0,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    if _create_geometry_handle is None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")
    value = _ir._positive_number("radius", radius)
    options = _geometry_options.circle(value)
    _apply_shared_constructor_options(options, kwargs)
    _apply_constructor_color(options, color)
    _attach_geometry_options(self, options, "Circle")
    self.radius = value


def _rectangle_init(
    self: _compat.Rectangle,
    width: float = 4.0,
    height: float = 2.0,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    if _create_geometry_handle is None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")
    width_value = _ir._positive_number("width", width)
    height_value = _ir._positive_number("height", height)
    options = _geometry_options.rectangle(width_value, height_value)
    _apply_shared_constructor_options(options, kwargs)
    _apply_constructor_color(options, color)
    _attach_geometry_options(self, options, "Rectangle")
    self.width_value = width_value
    self.height_value = height_value


def _square_init(
    self: _compat.Square,
    side_length: float = 2.0,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    if _create_geometry_handle is None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")
    value = _ir._positive_number("side_length", side_length)
    options = _geometry_options.square(value)
    _apply_shared_constructor_options(options, kwargs)
    _apply_constructor_color(options, color)
    _attach_geometry_options(self, options, "Square")
    self.side_length = value
    self.width_value = value
    self.height_value = value


def _path_init(
    self: _compat.Path,
    path: _base.VectorPath,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    if _create_geometry_handle is None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")
    if not isinstance(path, _base.VectorPath):
        raise TypeError("path must be a VectorPath")
    options = _vector_path_options(path.to_ir())
    _apply_shared_constructor_options(options, kwargs)
    _apply_constructor_color(options, color)
    _attach_geometry_options(self, options, "Path")
    self.path = path


def _line_init(
    self: _compat.Line,
    start: object = None,
    end: object = None,
    *,
    color: _base.Color | None = None,
    **kwargs: Any,
) -> None:
    start_value = _base.LEFT if start is None else _base._as_vec2(start)
    end_value = _base.RIGHT if end is None else _base._as_vec2(end)
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
    if _create_geometry_handle is None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")
    options = _geometry_options.line(
        start_value.x, start_value.y, end_value.x, end_value.y
    )
    _apply_shared_constructor_options(options, kwargs)
    _apply_constructor_color(options, color)
    _attach_geometry_options(self, options, "Line")
    self.start = start_value
    self.end = end_value


def _init(self: _base.Mobject, raw: _ir.Mobject) -> None:
    if _create_geometry_handle is None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")
    handle, context = _consume_geometry_options(_geometry_options_from_raw(raw))
    _attach_shared_handle(self, handle)
    if context is not None:
        self._canonical_live_target_context = context


def _current_raw(self: _base.Mobject) -> _ir.Mobject:
    if getattr(self, "_callback_line_target", None) is not None:
        raise RuntimeError(
            "callback-local Line operands cannot escape into scene, layout, or animation APIs"
        )
    handle = _handle_for(self)
    if handle is not None:
        return _raw_from_json(str(handle.snapshotJson()))
    raise RuntimeError("Mobject queries require a current shared Rust semantic handle")


def _apply(self: _base.Mobject, raw: _ir.Mobject) -> _base.Mobject:
    raise NotImplementedError(
        "raw replacement is unsupported; use a shared semantic operation"
    )


def _clone_mobject(
    self: _base.Mobject, *, target_state: bool = False
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        from _manim_updaters import _canonical_phase_context
        if (getattr(self, "_semantic_handle", None) is not None
                and bool(getattr(self, "_semantic_handle_fresh", False))
                and _canonical_phase_context(self) is not None):
            raise NotImplementedError(
                "canonical callback copies are unsupported while a callback phase is active"
            )
        raise RuntimeError("Mobject copy requires a current shared Rust semantic handle")
    context = _live_mutation_context(self)
    clone = object.__new__(type(self))
    clone._raw = None
    clone._scene = None
    clone._object = None
    clone._semantic_handle = (
        context.liveTargetEditor(handle)
        if context is not None
        else handle.targetEditor() if target_state else handle.cloneHandle()
    )
    clone._semantic_handle_fresh = True
    if context is not None:
        clone._canonical_live_target_context = context

    excluded = {
        "_raw",
        "_scene",
        "_object",
        "_semantic_handle",
        "_semantic_handle_fresh",
        "_canonical_live_target_context",
    }
    # A callback registry belongs to its source occurrence. The detached target
    # carries only its opaque semantic handle, never copied callback ownership.
    if context is not None:
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
    return _clone_mobject(self)


def _target_mobject(self: _base.Mobject) -> _base.Mobject:
    """Clone a detached target through Rust's explicit target-editor boundary."""

    # Family wrappers use the shared family-copy operation installed on Group.
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
    raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")


def _get_critical_point(self: _base.Mobject, direction: object) -> _base.Vec2:
    """Read a leaf critical point from the authoritative semantic layout."""
    axis = _base._as_vec2(direction)
    if isinstance(self, _compat.Group):
        # Groups query their shared family handle rather than a leaf binding.
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
    if handle is not None:
        return float(handle.width)
    raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")


def _height(self: _base.Mobject) -> float:
    observed = _bound_layout_observation(self)
    if observed is not None:
        return float(observed.height)
    handle = _handle_for(self)
    if handle is not None:
        return float(handle.height)
    raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")


def _set_width_property(self: _base.Mobject, width: float) -> None:
    self.scale_to_fit_width(float(width))


def _set_height_property(self: _base.Mobject, height: float) -> None:
    self.scale_to_fit_height(float(height))


def _shift(self: _base.Mobject, direction: object) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
    offset = _base._as_vec2(direction)
    context = _live_mutation_context(self)
    if context is not None:
        try:
            context.liveShift(handle, offset.x, offset.y)
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
    handle.shift(offset.x, offset.y)
    return self


def _move_to(
    self: _base.Mobject,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _manim_move_to(
            self,
            point_or_mobject,
            aligned_edge=aligned_edge,
            coor_mask=coor_mask,
        )

    context = _live_mutation_context(self)
    if context is not None:
        edge = _base._as_vec2(aligned_edge)
        mask = _alignment_mask2(coor_mask)
        try:
            if _alignment_is_mobject(point_or_mobject):
                target_handle = _handle_for(point_or_mobject)
                if target_handle is None:
                    raise ValueError("live move_to requires a shared semantic target")
                context.liveMoveToMobject(handle, target_handle, edge.x, edge.y, mask.x, mask.y)
            else:
                point = _base._as_vec2(point_or_mobject)
                context.liveMoveToPoint(handle, point.x, point.y, edge.x, edge.y, mask.x, mask.y)
        except Exception as error:
            raise ValueError(str(error)) from None
        return self

    edge = _base._as_vec2(aligned_edge)
    if _alignment_is_mobject(point_or_mobject):
        target_handle = _handle_for(point_or_mobject)
        if target_handle is None or not hasattr(handle, "manimMoveToHandle"):
            return _manim_move_to(
                self,
                point_or_mobject,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        mask = _alignment_mask2(coor_mask)
        handle.manimMoveToHandle(target_handle, edge.x, edge.y, mask.x, mask.y)
    else:
        if not hasattr(handle, "manimMoveToPoint"):
            return _manim_move_to(
                self,
                point_or_mobject,
                aligned_edge=aligned_edge,
                coor_mask=coor_mask,
            )
        point = _base._as_vec2(point_or_mobject)
        mask = _alignment_mask2(coor_mask)
        handle.manimMoveToPoint(point.x, point.y, edge.x, edge.y, mask.x, mask.y)
    return self


def _scale(self: _base.Mobject, factor: object) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
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

    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
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
        pivot = _base._as_vec2(about_point)
    elif about_edge is None:
        pivot = _base.Vec2(float(handle.centerX), float(handle.centerY))
    else:
        edge = _base._as_vec2(about_edge)
        pivot = _base.Vec2(
            float(handle.criticalX(edge.x, edge.y)),
            float(handle.criticalY(edge.x, edge.y)),
        )
    handle.rotateAboutPoint(signed_angle, pivot.x, pivot.y)
    return self


def _set_color(self: _base.Mobject, color: _base.Color) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
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
        return self

    handle.setColor(color.red, color.green, color.blue, color.alpha)
    return self


def _set_vmobject_color(
    self: _compat.VMobject,
    color: object,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject paint requires the shared Rust authoring host")
    del family
    return _set_color(self, _compat._as_color("color", color))


def _become(
    self: _base.Mobject,
    mobject: _base.Mobject,
    match_height: bool = False,
    match_width: bool = False,
    match_depth: bool = False,
    match_center: bool = False,
    stretch: bool = False,
) -> _base.Mobject:
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("state target must be a Mobject")
    if match_depth:
        raise NotImplementedError("depth matching requires the shared 2.5D family model")

    handle = _handle_for(self)
    other_handle = _handle_for(mobject)
    if handle is not None and other_handle is not None:
        flags = (
            bool(match_height),
            bool(match_width),
            bool(match_center),
            bool(stretch),
        )
        context = _live_mutation_context(self)
        if context is None:
            context = _live_mutation_context(mobject)
        if context is None:
            # A handle authored before live bootstrap may still be detached and
            # therefore carry no wrapper-local context. Once a continuation is
            # active, its store must only be mutated through that live session;
            # Rust validates that both operands belong to the session's store.
            context = _live_constructor_context("become")
        if context is not None:
            context.liveBecomeMobject(handle, other_handle, *flags)
        else:
            handle.becomeHandle(other_handle, *flags)
        return self

    has_typed_operand = any(
        getattr(value, "_semantic_handle", None) is not None
        for value in (self, mobject)
    )
    if has_typed_operand:
        raise NotImplementedError(
            "become requires valid shared semantic handles for both Mobjects"
        )
    return _compat._mobject_become(
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
    return _compat._mobject_replace(self, mobject, dim_to_match=dim_to_match, stretch=stretch)


def _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is not None:
        return _base.Vec2(
            float(handle.criticalX(direction.x, direction.y)),
            float(handle.criticalY(direction.x, direction.y)),
        )
    raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")


def _semantic_member_index(index):
    if index is None:
        return None
    import operator
    index = operator.index(index)
    if not -(1 << 31) <= index < (1 << 31):
        raise IndexError("alignment submobject index is unavailable")
    return index


def _layout_reference_handle(value):
    """Borrow typed identity for placement without granting raw geometry access."""
    handle = getattr(value, "_semantic_handle", None)
    if handle is None or not hasattr(handle, "layoutAnchor"):
        return None
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    from _manim_updaters import _canonical_phase_context
    if _canonical_phase_context(value) is not None:
        raise NotImplementedError("layout placement is unsupported during an active callback phase")
    return handle


def _layout_anchor(value, index=None):
    if isinstance(value, _compat.Group):
        handle = getattr(value, "_semantic_family_handle", None)
        if handle is None or not hasattr(handle, "layoutAnchor"):
            return None
        if any(_layout_reference_handle(leaf) is None for leaf in _compat._leaf_mobjects(value)):
            return None
    else:
        handle = _layout_reference_handle(value)
    if handle is None:
        return None
    return handle.layoutAnchor(_semantic_member_index(index))


def _shared_next_to(self, target, direction, buff, aligned_edge,
                      submobject_to_align, index, coor_mask):
    """Pass selection intent; Rust resolves members, observes bounds and places."""
    source = _layout_anchor(self)
    aligner = (_layout_anchor(submobject_to_align) if submobject_to_align is not None
               else _layout_anchor(self, index))
    if source is None or aligner is None:
        return False
    target_anchor = None
    if isinstance(target, (_base.Mobject, _compat.Group)):
        target_anchor = _layout_anchor(target, index)
        if target_anchor is None:
            return False
    vector = _base._as_vec2(direction)
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)
    arguments = (vector.x, vector.y, float(buff), edge.x, edge.y, mask.x, mask.y)
    context = (_group_live_layout_context(self) if isinstance(self, _compat.Group)
               else _live_mutation_context(self) or _live_constructor_context())
    try:
        if target_anchor is not None:
            if context is None:
                source.nextTo(target_anchor, aligner, *arguments)
            else:
                context.liveNextLayoutTo(source, target_anchor, aligner, *arguments)
        else:
            point = _base._as_vec2(target)
            if context is None:
                source.nextToPoint(point.x, point.y, aligner, *arguments)
            else:
                context.liveNextLayoutToPoint(source, point.x, point.y, aligner, *arguments)
    except Exception as error:
        if "alignment submobject index" in str(error):
            raise IndexError(str(error)) from None
        raise ValueError(str(error)) from None
    return True


def _next_to(
    self: _base.Mobject | _compat.Group,
    mobject_or_point: object,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    aligned_edge: object = _base.ORIGIN,
    submobject_to_align: object | None = None,
    index_of_submobject_to_align: int | None = None,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject | _compat.Group:
    if _shared_next_to(self, mobject_or_point, direction, buff, aligned_edge,
                       submobject_to_align, index_of_submobject_to_align, coor_mask):
        return self
    return _manim_next_to(
        self, mobject_or_point, direction, buff,
        aligned_edge=aligned_edge, submobject_to_align=submobject_to_align,
        index_of_submobject_to_align=index_of_submobject_to_align, coor_mask=coor_mask,
    )


def _align_to(
    self: _base.Mobject,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        return _manim_align_to(self, mobject_or_point, direction)
    if _live_mutation_context(self) is not None:
        raise NotImplementedError(
            "canonical live affine targets do not support layout alignment"
        )
    axis = _base._as_vec2(direction)
    if _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is None or not hasattr(handle, "alignToHandle"):
            return _manim_align_to(self, mobject_or_point, direction)
        handle.alignToHandle(target_handle, axis.x, axis.y)
    else:
        if not hasattr(handle, "alignToPoint"):
            return _manim_align_to(self, mobject_or_point, direction)
        point = _base._as_vec2(mobject_or_point)
        handle.alignToPoint(point.x, point.y, axis.x, axis.y)
    return self


def _align_on_frame(
    self: _base.Mobject,
    direction: _base.Vec2,
    buff: float,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None or not hasattr(handle, "alignOnFrame"):
        raise RuntimeError("Mobject frame alignment requires a current shared Rust semantic handle")
    if _live_mutation_context(self) is not None:
        raise NotImplementedError(
            "canonical live affine targets do not support frame alignment"
        )
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
        raise RuntimeError("Mobject paint requires the shared Rust authoring host")
    live_context = _live_mutation_context(self)
    if live_context is not None:
        try:
            if color is not None and opacity is not None:
                parsed = _compat._as_color("fill color", color)
                live_context.liveSetFill(
                    handle,
                    parsed.red,
                    parsed.green,
                    parsed.blue,
                    _compat._opacity("fill opacity", opacity),
                )
            elif color is not None:
                parsed = _compat._as_color("fill color", color)
                live_context.liveSetFillColor(
                    handle, parsed.red, parsed.green, parsed.blue, parsed.alpha
                )
            elif opacity is None:
                live_context.liveDisableFill(handle)
            if opacity is not None and color is None:
                live_context.liveSetFillOpacity(
                    handle, _compat._opacity("fill opacity", opacity)
                )
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
    if color is not None and opacity is not None:
        parsed = _compat._as_color("fill color", color)
        handle.setFill(
            parsed.red,
            parsed.green,
            parsed.blue,
            _compat._opacity("fill opacity", opacity),
        )
        return self
    if color is not None:
        parsed = _compat._as_color("fill color", color)
        handle.setFillColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif opacity is None:
        handle.disableFill()
    if opacity is not None:
        handle.setFillOpacity(_compat._opacity("fill opacity", opacity))
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
        raise RuntimeError("Mobject paint requires the shared Rust authoring host")
    live_context = _live_mutation_context(self)
    if live_context is not None:
        if width is not None:
            raise NotImplementedError(
                "canonical live style targets do not support stroke-width animation"
            )
        try:
            if color is not None and opacity is not None:
                parsed = _compat._as_color("stroke color", color)
                live_context.liveSetStroke(
                    handle,
                    parsed.red,
                    parsed.green,
                    parsed.blue,
                    _compat._opacity("stroke opacity", opacity),
                )
            elif color is not None:
                parsed = _compat._as_color("stroke color", color)
                live_context.liveSetStrokeColor(
                    handle, parsed.red, parsed.green, parsed.blue, parsed.alpha
                )
            elif opacity is None:
                live_context.liveDisableStroke(handle)
            else:
                live_context.liveSetStrokeOpacity(
                    handle, _compat._opacity("stroke opacity", opacity)
                )
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
    if color is not None:
        parsed = _compat._as_color("stroke color", color)
        handle.setStrokeColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif width is None and opacity is None:
        handle.disableStroke()
    if width is not None:
        handle.setStrokeWidth(_compat._manim_stroke_width(width))
    if opacity is not None:
        handle.setStrokeOpacity(_compat._opacity("stroke opacity", opacity))
    return self


def _set_opacity(
    self: _compat.VMobject,
    opacity: float,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject paint requires the shared Rust authoring host")
    live_context = _live_mutation_context(self)
    if live_context is not None:
        try:
            live_context.liveSetOpacity(handle, _compat._opacity("opacity", opacity))
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
    handle.setOpacity(_compat._opacity("opacity", opacity))
    return self


def _set_object_opacity(
    self: _base.Mobject,
    opacity: float,
) -> _base.Mobject:
    """Set the object-composite multiplier, distinct from Manim paint opacity."""

    handle = _handle_for(self)
    if handle is None:
        raise NotImplementedError("set_object_opacity requires the shared semantic authoring handle")
    alpha = _compat._opacity("object opacity", opacity)
    live_context = _live_mutation_context(self)
    try:
        if live_context is not None:
            live_context.liveSetObjectOpacity(handle, alpha)
        else:
            handle.setObjectOpacity(alpha)
    except Exception as error:
        raise ValueError(str(error)) from None
    return self


def _paint_opacity_observation(value: object, layer: str):
    # The callback row is Rust-published state plus preceding ordered writes.
    # Reading it is a scalar projection, with no geometry or extra WASM call.
    from _manim_updaters import _canonical_row

    phase = _canonical_row(value)
    if phase is not None:
        paint = getattr(phase[2].style, layer)
        return 0.0 if paint is None else float(paint[3])
    return _typed_manim_observation(
        value, f"{layer}Opacity", f"queryMobject{layer.title()}Opacity", handle_property=True
    )


def _get_fill_opacity(self: _compat.VMobject) -> float:
    observed = _paint_opacity_observation(self, "fill")
    if observed is None:
        raise RuntimeError("Mobject paint requires the shared Rust authoring host")
    return float(observed)


def _get_stroke_opacity(self: _compat.VMobject) -> float:
    observed = _paint_opacity_observation(self, "stroke")
    if observed is None:
        raise RuntimeError("Mobject paint requires the shared Rust authoring host")
    return float(observed)


def _family_layout_leaf_adapter(value: object, *, mutation: bool = False):
    return _handle_for(value)


def _shared_family_layout(value: object, *, mutation: bool = False):
    if not isinstance(value, _compat.Group):
        return None
    family_handle = getattr(value, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "layout"):
        return None
    leaves = _compat._leaf_mobjects(value)
    leaf_handles = [
        _family_layout_leaf_adapter(member, mutation=mutation) for member in leaves
    ]
    if not all(handle is not None for handle in leaf_handles):
        return None
    return family_handle.layout()


def _group_paint(self, operation, arguments, callback_method, callback_arguments):
    handle = getattr(self, "_semantic_family_handle", None)
    if handle is None:
        raise RuntimeError("Group paint requires the shared Rust authoring host")
    # #955 owns replacing the existing callback overlay's per-leaf dispatch.
    # Keep callback writes effective; never publish them as authored style edits.
    from _manim_updaters import _canonical_phase_context
    leaves = _compat._leaf_mobjects(self)
    if any(_canonical_phase_context(leaf) is not None for leaf in leaves):
        for leaf in leaves:
            getattr(leaf, callback_method)(*callback_arguments)
        return self
    context = _group_target_context(self)
    try:
        if context is None:
            getattr(handle, f"set{operation}")(*arguments)
        else:
            getattr(context, f"liveSetFamily{operation}")(handle, *arguments)
    except Exception as error:
        raise ValueError(str(error)) from None
    return self


def _family_color_arguments(color):
    if color is None:
        return (False, 0.0, 0.0, 0.0, 1.0)
    parsed = _compat._as_color("color", color)
    return (True, parsed.red, parsed.green, parsed.blue, parsed.alpha)


def _group_set_color(self, color):
    parsed = _compat._as_color("color", color)
    arguments = (parsed.red, parsed.green, parsed.blue, parsed.alpha)
    return _group_paint(self, "Color", arguments, "set_color", (color,))


def _group_set_fill(self, color=None, opacity=None):
    alpha = None if opacity is None else _compat._opacity("fill opacity", opacity)
    return _group_paint(self, "Fill", (*_family_color_arguments(color), alpha),
                        "set_fill", (color, opacity))


def _group_set_stroke(self, color=None, width=None, opacity=None):
    stroke_width = None if width is None else _compat._manim_stroke_width(width)
    alpha = None if opacity is None else _compat._opacity("stroke opacity", opacity)
    return _group_paint(self, "Stroke", (*_family_color_arguments(color), stroke_width, alpha),
                        "set_stroke", (color, width) if opacity is None else (color, width, opacity))


def _group_set_opacity(self, opacity):
    alpha = _compat._opacity("opacity", opacity)
    return _group_paint(self, "Opacity", (alpha,), "set_opacity", (opacity,))


def _group_arrange_in_grid(self, rows=None, cols=None, buff=_base.MED_SMALL_BUFF):
    import operator

    def dimension(value):
        if value is None:
            return None
        value = operator.index(value)
        if not 0 < value <= 0xFFFFFFFF:
            raise ValueError("grid dimensions must be positive 32-bit integers")
        return value

    rows, cols = dimension(rows), dimension(cols)
    gap = (_base._as_vec2(buff) if isinstance(buff, (tuple, list, _base.Vec2))
           else _base.Vec2(float(buff), float(buff)))
    handle = getattr(self, "_semantic_family_handle", None)
    if handle is None:
        raise RuntimeError("Group grid requires the shared Rust authoring host")
    context = _group_live_layout_context(self)
    try:
        if context is None:
            handle.arrangeInGrid(rows, cols, gap.x, gap.y)
        else:
            context.liveArrangeFamilyInGrid(handle, rows, cols, gap.x, gap.y)
    except Exception as error:
        raise ValueError(str(error)) from None
    return self


def _group_scale(self: _compat.Group, factor: object) -> _compat.Group:
    scale = (_base._as_vec2(factor) if isinstance(factor, (tuple, list, _base.Vec2))
             else _base.Vec2(float(factor), float(factor)))
    handle = getattr(self, "_semantic_family_handle", None)
    if handle is None:
        raise RuntimeError("Group scale requires the shared Rust authoring host")
    context = _group_live_layout_context(self)
    try:
        if context is not None:
            context.liveScaleFamily(handle, scale.x, scale.y)
        else:
            handle.scale(scale.x, scale.y)
    except Exception as error:
        raise ValueError(str(error)) from None
    return self


def _group_rotate(self: _compat.Group, angle: float, axis: object = _compat.OUT,
                  *, about_point=None, about_edge=None, **kwargs) -> _compat.Group:
    signed_angle = _compat._rotation_angle_2d(angle, axis)
    point = (_base._as_vec2(about_point) if about_point is not None
             else _base._as_vec2(_base.ORIGIN if about_edge is None else about_edge))
    handle = getattr(self, "_semantic_family_handle", None)
    if handle is None:
        raise RuntimeError("Group rotation requires the shared Rust authoring host")
    context = _group_live_layout_context(self)
    try:
        if context is not None:
            context.liveRotateFamily(handle, signed_angle, point.x, point.y, about_point is not None)
        else:
            handle.rotate(signed_angle, point.x, point.y, about_point is not None)
    except Exception as error:
        raise ValueError(str(error)) from None
    return self


def _group_shift(self: _compat.Group, direction: object) -> _compat.Group:
    context = _group_target_context(self)
    if context is not None:
        offset = _base._as_vec2(direction)
        try:
            context.liveShiftFamily(self._semantic_family_handle, offset.x, offset.y)
        except Exception as error:
            raise ValueError(str(error)) from None
        return self
    shared = _shared_family_layout(self, mutation=True)
    if shared is None:
        return _compat._shift_group_members(self, direction)
    session = shared
    if not hasattr(session, "shiftBy"):
        return _compat._shift_group_members(self, direction)
    offset = _base._as_vec2(direction)
    session.shiftBy(offset.x, offset.y)
    return self


def _group_live_layout_context(value: _compat.Group):
    # Callback registration does not invalidate the coherent live publication.
    # Family observations inside an ordered overlay need shared phase-local
    # aggregation; neither the authored state nor the last publication suffices.
    from _manim_updaters import _canonical_phase_context

    leaves = _compat._leaf_mobjects(value)
    if any(_canonical_phase_context(leaf) is not None for leaf in leaves):
        raise NotImplementedError("family layout is unsupported during an active callback phase")
    context = _group_target_context(value)
    if context is None:
        return None
    for leaf in leaves:
        if (not bool(getattr(leaf, "_semantic_handle_fresh", False))
                or getattr(leaf, "_semantic_handle", None) is None):
            return None
    return context


def _live_family_placement(context, family, target, operation, *arguments):
    if isinstance(target, _compat.Group):
        method = getattr(context, f"live{operation}FamilyToFamily")
        method(family, target._semantic_family_handle, *arguments)
    elif isinstance(target, _base.Mobject):
        method = getattr(context, f"live{operation}FamilyToMobject")
        method(family, target._semantic_handle, *arguments)
    else:
        point = _base._as_vec2(target)
        method = getattr(context, f"live{operation}FamilyToPoint")
        method(family, point.x, point.y, *arguments)


def _group_move_to(
    self: _compat.Group,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _compat.Group:
    context = _group_live_layout_context(self)
    if context is not None:
        edge = _base._as_vec2(aligned_edge)
        mask = _alignment_mask2(coor_mask)
        _live_family_placement(context, self._semantic_family_handle, point_or_mobject, "Move",
                               edge.x, edge.y, mask.x, mask.y)
        return self
    shared = _shared_family_layout(self, mutation=True)
    if shared is None:
        return _manim_move_to(self, point_or_mobject, aligned_edge, coor_mask)
    session = shared
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)

    applied = False
    if isinstance(point_or_mobject, _compat.Group):
        target_shared = _shared_family_layout(point_or_mobject)
        if target_shared is not None and hasattr(session, "moveToFamily"):
            target_session = target_shared
            session.moveToFamily(
                target_session, edge.x, edge.y, mask.x, mask.y
            )
            applied = True
    elif _alignment_is_mobject(point_or_mobject):
        target_adapter = _family_layout_leaf_adapter(point_or_mobject)
        if target_adapter is not None and hasattr(session, "moveToMobject"):
            session.moveToMobject(
                target_adapter, edge.x, edge.y, mask.x, mask.y
            )
            applied = True
    elif hasattr(session, "moveToPoint"):
        point = _base._as_vec2(point_or_mobject)
        session.moveToPoint(
            point.x, point.y, edge.x, edge.y, mask.x, mask.y
        )
        applied = True

    if not applied:
        return _manim_move_to(self, point_or_mobject, aligned_edge, coor_mask)
    return self




def _group_align_to(
    self: _compat.Group,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _compat.Group:
    context = _group_live_layout_context(self)
    if context is not None:
        axis = _base._as_vec2(direction)
        _live_family_placement(context, self._semantic_family_handle, mobject_or_point, "Align", axis.x, axis.y)
        return self
    shared = _shared_family_layout(self, mutation=True)
    if shared is None:
        return _manim_align_to(self, mobject_or_point, direction)
    session = shared
    axis = _base._as_vec2(direction)

    applied = False
    if isinstance(mobject_or_point, _compat.Group):
        target_shared = _shared_family_layout(mobject_or_point)
        if target_shared is not None and hasattr(session, "alignToFamily"):
            session.alignToFamily(target_shared, axis.x, axis.y)
            applied = True
    elif _alignment_is_mobject(mobject_or_point):
        target_adapter = _family_layout_leaf_adapter(mobject_or_point)
        if target_adapter is not None and hasattr(session, "alignToMobject"):
            session.alignToMobject(target_adapter, axis.x, axis.y)
            applied = True
    elif hasattr(session, "alignToPoint"):
        point = _base._as_vec2(mobject_or_point)
        session.alignToPoint(point.x, point.y, axis.x, axis.y)
        applied = True

    if not applied:
        return _manim_align_to(self, mobject_or_point, direction)
    return self



def _group_arrange(
    self: _compat.Group,
    direction: object = _base.RIGHT,
    buff: float = _base.DEFAULT_MOBJECT_TO_MOBJECT_BUFFER,
    center: bool = True,
    **kwargs: Any,
) -> _compat.Group:
    family_handle = getattr(self, "_semantic_family_handle", None)
    if family_handle is None or not hasattr(family_handle, "arrangeOptions"):
        return _manim_arrange(self, direction=direction, buff=buff, center=center, **kwargs)
    if not self.submobjects:
        return self
    unknown = set(kwargs) - {"aligned_edge", "coor_mask", "submobject_to_align", "index_of_submobject_to_align"}
    if unknown:
        raise TypeError(f"arrange got unexpected placement keyword {sorted(unknown)[0]!r}")
    axis = _base._as_vec2(_base.RIGHT if direction is None else direction)
    edge = _base._as_vec2(kwargs.get("aligned_edge", _base.ORIGIN))
    mask = _alignment_mask2(kwargs.get("coor_mask", (1, 1, 1)))
    index = _semantic_member_index(kwargs.get("index_of_submobject_to_align"))
    options = family_handle.arrangeOptions(axis.x, axis.y, float(buff), bool(center),
                                           edge.x, edge.y, mask.x, mask.y, index)
    aligner = kwargs.get("submobject_to_align")
    if aligner is not None:
        anchor = _layout_anchor(aligner)
        if anchor is None:
            return _manim_arrange(self, direction=direction, buff=buff, center=center, **kwargs)
        options.setAligner(anchor)
    context = _group_target_context(self)
    try:
        if context is not None:
            context.liveArrangeFamily(family_handle, options)
            return self
        leaves = _compat._leaf_mobjects(self)
        leaf_handles = [_family_layout_leaf_adapter(member, mutation=True) for member in leaves]
        if any(handle is None for handle in leaf_handles):
            return _manim_arrange(self, direction=direction, buff=buff, center=center, **kwargs)
        family_handle.arrange(options)
    except Exception as error:
        if "alignment submobject index" in str(error):
            raise IndexError(str(error)) from None
        raise ValueError(str(error)) from None
    return self


def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:
    if isinstance(value, _compat.Group):
        context = _group_live_layout_context(value)
        if context is not None:
            layout = context.queryFamilyLayout(value._semantic_family_handle)
            return (
                _base.Vec2(float(layout.criticalX(-1.0, 0.0)), float(layout.criticalY(0.0, -1.0))),
                _base.Vec2(float(layout.criticalX(1.0, 0.0)), float(layout.criticalY(0.0, 1.0))),
            )
    # Rust observes the complete semantic family directly. The wrapper list only
    # selects whether this caller is eligible for the shared query.
    if isinstance(value, _compat.Group):
        shared = _shared_family_layout(value)
        if shared is not None:
            session = shared
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

    if isinstance(value, _base.Mobject) and not isinstance(value, _compat.Group):
        observed = _bound_layout_observation(value)
        if observed is not None:
            return (
                _base.Vec2(float(observed.criticalX(-1.0, 0.0)), float(observed.criticalY(0.0, -1.0))),
                _base.Vec2(float(observed.criticalX(1.0, 0.0)), float(observed.criticalY(0.0, 1.0))),
            )
        return _layout_bounds(value)
    raise RuntimeError("family layout requires a current shared Rust semantic handle")


def _family_member_handle(value: object) -> tuple[str | None, object | None]:
    if isinstance(value, _compat.Group):
        return "family", getattr(value, "_semantic_family_handle", None)
    if isinstance(value, _base.Mobject):
        return "mobject", getattr(value, "_semantic_handle", None)
    return None, None


def _validate_group_members(owner: _compat.Group, mobjects: tuple[object, ...]) -> None:
    for mobject in mobjects:
        if not isinstance(mobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Group members must be Mobjects or Groups")
        if mobject is owner:
            raise ValueError("Group cannot contain itself")


def _family_membership_batch(context: object, kind: str, mobjects: tuple[object, ...]):
    batch = (
        context.beginMembershipBatch(kind)
        if context is not None
        else _new_membership_batch(kind)
    )
    for value in mobjects:
        member_kind, handle = _family_member_handle(value)
        if handle is None:
            raise RuntimeError("family member has no shared semantic identity")
        if member_kind == "family":
            batch.appendFamily(handle)
        else:
            batch.appendMobject("", handle)
    return batch


def _group_init(self: _compat.Group, *mobjects: object) -> None:
    if _create_family_handle is None or _new_membership_batch is None:
        raise RuntimeError("Group construction requires the shared Rust authoring host")
    _validate_group_members(self, mobjects)
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", mobjects)
    family = (
        context.liveCreateFamily(batch)
        if context is not None
        else _create_family_handle(batch)
    )
    # Rust selects the authoritative ordered members; this map retains Python identity.
    wrappers = {}
    for value in mobjects:
        _, handle = _family_member_handle(value)
        key = f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"
        wrappers.setdefault(key, value)
    self._semantic_family_handle = family
    self.submobjects = [wrappers[str(key)] for key in family.memberKeys()]


def _group_add(self: _compat.Group, *mobjects: object) -> _compat.Group:
    _validate_group_members(self, mobjects)
    if not mobjects:
        return self
    family_handle = self._semantic_family_handle
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", mobjects)
    changed = (
        context.liveEditFamilyMembership(family_handle, batch)
        if context is not None
        else family_handle.editMembership(batch)
    )
    accepted = tuple(value for value, changed in zip(mobjects, changed) if changed)
    if accepted:
        self.submobjects.extend(accepted)
    return self


def _group_remove(self: _compat.Group, *mobjects: object) -> _compat.Group:
    if not mobjects:
        return self
    family_handle = self._semantic_family_handle
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "remove", mobjects)
    changed = (
        context.liveEditFamilyMembership(family_handle, batch)
        if context is not None
        else family_handle.editMembership(batch)
    )
    accepted = tuple(value for value, changed in zip(mobjects, changed) if changed)
    if accepted:
        removed = {id(value) for value in accepted}
        self.submobjects = [value for value in self.submobjects if id(value) not in removed]
    return self


def _group_target_context(value: object) -> object | None:
    contexts: list[object] = []

    def collect(member: object) -> None:
        if isinstance(member, _compat.Group):
            for child in member.submobjects:
                collect(child)
            return
        context = getattr(member, "_canonical_live_target_context", None)
        if context is not None:
            contexts.append(context)

    collect(value)
    if not contexts:
        # A detached family may have been created before the first live segment,
        # so its wrappers do not carry per-target context markers. Once source
        # execution resumes, the current Scene's returned player is still the
        # one mutation authority for that same-store family.
        return _live_constructor_context()
    context = contexts[0]
    if any(candidate is not context for candidate in contexts[1:]):
        raise RuntimeError("Group target members belong to different canonical contexts")
    return context


def _group_copy(self: _compat.Group) -> _compat.Group:
    context = _group_target_context(self)
    # Bound families outside construct() still have the same context on leaves.
    if context is None:
        contexts = [candidate for leaf in _compat._leaf_mobjects(self)
                    if (candidate := _live_mutation_context(leaf)) is not None]
        if contexts:
            context = contexts[0]
            if any(candidate is not context for candidate in contexts[1:]):
                raise RuntimeError("family copy members belong to different live contexts")

    def excluded_fields(value):
        excluded = {
            "_raw", "_scene", "_object", "_semantic_handle", "_semantic_handle_fresh",
            "_semantic_family_handle", "_canonical_live_target_context",
            "_noon_updater_registrations", "_noon_updater_registration_history",
        }
        if not isinstance(value, _compat.Group) and _is_bound(value) and hasattr(value, "_noon_updaters"):
            excluded.add("_noon_updaters")
        return excluded

    clone, pairs = _compat.prepare_family_wrapper_copy(self, excluded_fields)
    # Verify host identity metadata before committing the semantic copy. Rust owns
    # the graph; Python cannot add, reorder, or omit a copied semantic member.
    for source, _ in pairs:
        if isinstance(source, _compat.Group):
            keys = []
            for member in source.submobjects:
                _, handle = _family_member_handle(member)
                if handle is None:
                    raise RuntimeError("family member has no shared semantic identity")
                keys.append(f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}")
            if keys != [str(key) for key in source._semantic_family_handle.memberKeys()]:
                raise RuntimeError("Group wrapper mirror diverged from shared family membership")
    references = _family_membership_batch(context, "add", tuple(source for source, _ in pairs))
    copied = (context.liveCopyFamily(self._semantic_family_handle, references)
              if context is not None else self._semantic_family_handle.copyFamily(references))
    for source, target in pairs:
        if isinstance(source, _compat.Group):
            target._semantic_family_handle = copied.familyFor(source._semantic_family_handle)
        else:
            _initialize_shared_wrapper(target)
            target._semantic_handle = copied.mobjectFor(source._semantic_handle)
            target._semantic_handle_fresh = True
            if context is not None:
                target._canonical_live_target_context = context
    return clone
