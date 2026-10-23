"""Shared semantic operations for the Manim-compatible Python facade.

Detached, scene-owned and live targets use typed Rust handles. Callback execution
uses its explicit staged property view; this module installs no public methods.
"""

from __future__ import annotations

from _noon_errors import engine_call, raise_engine_error

import copy
import json
import numbers
import operator
import sys
from typing import Any

import noon as _base
import _manim_compat as _compat

try:
    from pyodide.ffi import jsnull as _jsnull
except ImportError:  # Native CPython adapters receive None for optional handles.
    _jsnull = None


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
        and getattr(value, "_scene", None) is not None
        and getattr(value, "_object", None) is not None
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
    # Returning the player keeps its runtime alive for the next source operation.
    # Mutations and target creation must publish through that same session too.
    # A transferred runtime retains this context so its typed call rejects before
    # changing the shared store while another endpoint owns the player.
    return context if engine_call(context.requiresLiveMutationRouting) else None


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
    if handle_method == "pathQuery" and _is_shared_family(value):
        family = value._semantic_family_handle
        context = _live_mutation_context(value) or _live_constructor_context()
        if context is not None:
            return engine_call(context.queryFamilyOwnPath, family)
        return engine_call(family.pathQuery)
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
        return engine_call(query, semantic_handle)
    handle = _handle_for(value)
    if handle is None:
        raise NotImplementedError("typed Mobject observation is unavailable in this authoring phase")
    observation = engine_call(getattr, handle, handle_method)
    return observation if handle_property else engine_call(observation)


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
    if semantic_handle is None or not bool(getattr(value, "_semantic_handle_fresh", False)):
        raise NotImplementedError("exact Line admission requires a valid semantic handle")
    engine_call(_manim_line_endpoints_observation, value, operation="ShowPassingFlash")
    return True


def _layout_bounds(value: _base.Mobject) -> tuple[_base.Vec2, _base.Vec2] | None:
    """Read exact world-space layout bounds from a detached shared handle."""

    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")
    return (
        _base.Vec2(
            float(engine_call(handle.criticalX, -1.0, 0.0)),
            float(engine_call(handle.criticalY, 0.0, -1.0)),
        ),
        _base.Vec2(
            float(engine_call(handle.criticalX, 1.0, 0.0)),
            float(engine_call(handle.criticalY, 0.0, 1.0)),
        ),
    )


def _layout_center(value: _base.Mobject) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is None:
        raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")
    return _base.Vec2(float(handle.centerX), float(handle.centerY))


def _bound_query_context(value: _base.Mobject):
    if not _is_bound(value) or not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    if getattr(value, "_semantic_handle", None) is None:
        return None
    return getattr(value._scene, "_canonical_authoring_context", None)


def _bound_layout_observation(value: _base.Mobject, method="queryMobjectLayout", *arguments):
    """Ask the owning Rust context for one coherent ordinary live observation."""
    context = _bound_query_context(value)
    query = getattr(context, method, None)
    return None if query is None else engine_call(query, value._semantic_handle, *arguments)


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
    return _compat._as_color(name, value)


def _apply_shared_constructor_options(
    handle: object,
    kwargs: dict[str, Any],
    *,
    allow_sheen_direction: bool = False,
) -> None:
    """Apply Python constructor coercions to one shared typed target.

    The target is an inert Rust geometry candidate. Rust validates semantic
    state before publication; Python only applies public argument coercions.
    """
    options = dict(kwargs)
    cap_style = options.pop("cap_style", _CONSTRUCTOR_MISSING)
    if cap_style is not _CONSTRUCTOR_MISSING:
        if "stroke_cap" in options:
            raise TypeError("cap_style and stroke_cap cannot both be supplied")
        options["stroke_cap"] = _ir._stroke_cap(cap_style)
    allowed = {
        "position", "rotation", "scale", "fill", "stroke",
        "stroke_width", "stroke_width_mode", "stroke_join", "stroke_cap",
        "opacity", "fill_color", "stroke_color", "fill_opacity",
        "stroke_opacity", "z_index",
    }
    if allow_sheen_direction:
        allowed.update({"sheen_direction", "sheen_factor"})
    unknown = sorted(set(options) - allowed)
    if unknown:
        raise TypeError(f"unsupported Mobject constructor option(s): {', '.join(unknown)}")
    if "sheen_factor" in options and _ir._finite_number(
        "sheen factor", options["sheen_factor"]
    ) != 0.0:
        raise NotImplementedError("2D sheen requires shared gradient paint")

    if "z_index" in options:
        engine_call(handle.setZIndex, _ir._finite_number("z_index", options["z_index"]))
    if "sheen_direction" in options:
        values, dimensions = _sheen_direction_components(options["sheen_direction"])
        engine_call(handle.setSheenDirection, *values, dimensions)
    if "position" in options:
        value = _ir._vec2("position", options["position"])
        engine_call(handle.setTranslation, value["x"], value["y"])
    if "rotation" in options:
        engine_call(handle.setRotation, _ir._finite_number("rotation", options["rotation"]))
    if "scale" in options:
        value = _ir._vec2("scale", options["scale"])
        engine_call(handle.setScale, value["x"], value["y"])
    if "stroke_width" in options:
        engine_call(handle.setStrokeWidth, _compat._manim_stroke_width(options["stroke_width"]))
    if "stroke_width_mode" in options:
        engine_call(handle.setStrokeWidthMode, _ir._stroke_width_mode(options["stroke_width_mode"]))
    if "stroke_join" in options:
        engine_call(handle.setStrokeJoin, _ir._stroke_join(options["stroke_join"]))
    if "stroke_cap" in options:
        engine_call(handle.setStrokeCap, _ir._stroke_cap(options["stroke_cap"]))
    if "opacity" in options:
        engine_call(handle.setObjectOpacity, _ir._finite_number("opacity", options["opacity"]))

    fill = options.get("fill", _CONSTRUCTOR_MISSING)
    fill_color = options.get("fill_color", _CONSTRUCTOR_MISSING)
    if fill_color is not _CONSTRUCTOR_MISSING and fill_color is not None:
        fill = _compat._as_color("fill_color", fill_color)
    if fill is not _CONSTRUCTOR_MISSING:
        if fill is None:
            engine_call(handle.disableFill)
        else:
            parsed = _constructor_color("fill", fill)
            engine_call(handle.setFill, parsed.red, parsed.green, parsed.blue, parsed.alpha)

    stroke = options.get("stroke", _CONSTRUCTOR_MISSING)
    stroke_color = options.get("stroke_color", _CONSTRUCTOR_MISSING)
    if stroke_color is not _CONSTRUCTOR_MISSING and stroke_color is not None:
        stroke = _compat._as_color("stroke_color", stroke_color)
    if stroke is not _CONSTRUCTOR_MISSING:
        if stroke is None:
            engine_call(handle.disableStroke)
        else:
            parsed = _constructor_color("stroke", stroke)
            engine_call(handle.setStrokeColor, parsed.red, parsed.green, parsed.blue, parsed.alpha)
            engine_call(handle.setStrokeOpacity, parsed.alpha)

    if options.get("fill_opacity") is not None:
        engine_call(handle.setFillOpacity, _compat._opacity("fill_opacity", options["fill_opacity"]))
    if options.get("stroke_opacity") is not None:
        engine_call(handle.setStrokeOpacity, _compat._opacity("stroke_opacity", options["stroke_opacity"]))


def _apply_constructor_color(handle: object, color: _base.Color | None) -> None:
    if color is not None:
        parsed = _constructor_color("color", color)
        engine_call(handle.setColor, parsed.red, parsed.green, parsed.blue, parsed.alpha)


def _live_constructor_context(kind: str = "primitive", *, allow_unstarted: bool = False):
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
    if context is None and allow_unstarted and scene is not None:
        from _manim_scene import _context
        context = _context(scene)
    if context is None:
        return None
    ownership = str(engine_call(context.liveExecutionOwnership))
    if ownership in {"active", "returned"} or (allow_unstarted and ownership == "none"):
        return context
    if ownership == "transferred":
        raise RuntimeError(
            f"live {kind} construction is unavailable while execution is transferred"
        )
    return None


def _vector_path_options(path: dict[str, Any]):
    if set(path) != {"commands"}:
        raise ValueError("shared vector path supports commands only")
    candidate = engine_call(_authoring_vector_path)
    for command in path["commands"]:
        if command == "close":
            engine_call(candidate.close)
        elif isinstance(command, dict) and set(command) == {"move_to"}:
            point = command["move_to"]["to"]
            engine_call(candidate.moveTo, float(point["x"]), float(point["y"]))
        elif isinstance(command, dict) and set(command) == {"line_to"}:
            point = command["line_to"]["to"]
            engine_call(candidate.lineTo, float(point["x"]), float(point["y"]))
        elif isinstance(command, dict) and set(command) == {"quadratic_to"}:
            values = command["quadratic_to"]
            control, point = values["control"], values["to"]
            engine_call(candidate.quadraticTo,
                float(control["x"]), float(control["y"]),
                float(point["x"]), float(point["y"]),
            )
        elif isinstance(command, dict) and set(command) == {"cubic_to"}:
            values = command["cubic_to"]
            first, second, point = values["control1"], values["control2"], values["to"]
            engine_call(candidate.cubicTo,
                float(first["x"]), float(first["y"]),
                float(second["x"]), float(second["y"]),
                float(point["x"]), float(point["y"]),
            )
        else:
            raise ValueError("unsupported vector path command")
    return engine_call(_geometry_options.path, candidate)


def _consume_geometry_options(options: object, kind: str = "geometry"):
    context = _live_constructor_context(kind)
    if context is None:
        return engine_call(_create_geometry_handle, options), None
    return engine_call(context.liveCreateManimGeometry, options), context


def _attach_geometry_options(
    self: _base.Mobject, options: object, kind: str = "geometry"
) -> None:
    reactive = sys.modules.get("_manim_reactive")
    scene = None if reactive is None else reactive._current_authoring_scene()
    if scene is not None:
        from _manim_updaters import active_callback_membership_context

        callback = active_callback_membership_context(scene)
        if callback is not None:
            # The provisional name deliberately is not a durable semantic
            # handle. Scene.add stages it through the same callback collector;
            # the finalizer replaces this marker only after Rust commits.
            _initialize_shared_wrapper(self)
            self._callback_provisional_handle = callback.stage_provisional_geometry(options)
            self._callback_provisional_context = callback
            return
    handle, context = _consume_geometry_options(options, kind)
    _attach_shared_handle(self, handle)
    if context is not None:
        self._canonical_live_target_context = context


def _empty_path_init(
    self: _base.Mobject, *, color=None, kind: str = "Mobject", **kwargs: Any,
) -> None:
    """Construct a point-free wrapper through the shared Rust geometry owner."""
    parsed = _base.BLACK if color is None else _constructor_color("color", color)
    if _geometry_options is None:
        raise RuntimeError("Mobject construction requires the shared Rust authoring host")
    options = engine_call(_geometry_options.emptyPath)
    _apply_constructor_color(options, parsed)
    _apply_shared_constructor_options(
        options, kwargs, allow_sheen_direction=(kind == "VMobject")
    )
    if kind == "Mobject":
        engine_call(options.setDrawEnabled, False)
    _attach_geometry_options(self, options, kind)


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
    options = engine_call(_geometry_options.circle, value)
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
    options = engine_call(_geometry_options.rectangle, width_value, height_value)
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
    options = engine_call(_geometry_options.square, value)
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
    options = engine_call(_geometry_options.line,
        start_value.x, start_value.y, end_value.x, end_value.y
    )
    _apply_shared_constructor_options(options, kwargs)
    _apply_constructor_color(options, color)
    _attach_geometry_options(self, options, "Line")
    self.start = start_value
    self.end = end_value


def _current_raw(self: _base.Mobject) -> _ir.Mobject:
    if getattr(self, "_callback_line_target", None) is not None:
        raise RuntimeError(
            "callback-local Line operands cannot escape into scene, layout, or animation APIs"
        )
    handle = _handle_for(self)
    if handle is not None:
        return _raw_from_json(str(engine_call(handle.snapshotJson)))
    raise RuntimeError("Mobject queries require a current shared Rust semantic handle")


def _apply(self: _base.Mobject, raw: _ir.Mobject) -> _base.Mobject:
    raise NotImplementedError(
        "raw replacement is unsupported; use a shared semantic operation"
    )


def _copy_nested_mobject(
    value: _base.Mobject,
    context: object | None,
    memo: dict[int, object],
):
    """Preserve wrapper-specific copy behavior under the parent's live owner."""
    existing = memo.get(id(value))
    if existing is not None:
        return existing
    if not _is_shared_family(value):
        return _clone_mobject(value, context_override=context, memo=memo)
    members = _compat._leaf_mobjects(value)
    restore = []
    for member in members:
        if getattr(member, "_canonical_live_target_context", None) is None:
            restore.append((member, hasattr(member, "_canonical_live_target_context")))
            member._canonical_live_target_context = context
    try:
        copied = value.copy()
        memo[id(value)] = copied
        return copied
    finally:
        for member, had_attribute in restore:
            if had_attribute:
                member._canonical_live_target_context = None
            else:
                del member._canonical_live_target_context


def _clone_mobject(
    self: _base.Mobject, *, target_state: bool = False, subcurve=None,
    context_override: object | None = None, memo: dict[int, object] | None = None,
) -> _base.Mobject:
    memo = {} if memo is None else memo
    existing = memo.get(id(self))
    if existing is not None:
        return existing
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
    if context is None:
        context = context_override
    clone = object.__new__(type(self))
    clone._raw = None
    clone._scene = None
    clone._object = None
    clone._semantic_handle = (
        (engine_call(context.liveSubcurve, handle, *subcurve) if context is not None
         else engine_call(handle.subcurve, *subcurve)) if subcurve is not None else
        engine_call(context.liveTargetEditor, handle)
        if context is not None
        else engine_call(handle.targetEditor) if target_state else engine_call(handle.cloneHandle)
    )
    clone._semantic_handle_fresh = True
    if context is not None:
        clone._canonical_live_target_context = context
    memo[id(self)] = clone
    if _compat._FAMILY_COPY_METADATA not in memo:
        memo[_compat._FAMILY_COPY_METADATA] = lambda value: _copy_nested_mobject(
            value, context, memo
        )

    excluded = {
        "_raw",
        "_scene",
        "_object",
        "_semantic_handle",
        "_semantic_handle_fresh",
        "_canonical_live_target_context",
        "_semantic_member_reference_scope",
    }
    # A callback registry belongs to its source occurrence. The detached target
    # carries only its opaque semantic handle, never copied callback ownership.
    excluded.update({
        "_noon_updaters",
        "_noon_updater_registrations",
        "_noon_updater_registration_history",
    })
    for name, value in self.__dict__.items():
        if name not in excluded:
            if isinstance(value, _base.Mobject):
                # Snapshot Mobjects such as ``saved_state`` can predate the
                # live session even when their parent is copied after playback.
                # Route their detached copies through the same owner so they
                # cannot advance the semantic store outside its publication.
                setattr(clone, name, _copy_nested_mobject(value, context, memo))
            else:
                setattr(clone, name, copy.deepcopy(value, memo))
    return clone


def _copy_mobject(self: _base.Mobject) -> _base.Mobject:
    if getattr(self, "_semantic_member_wrappers", None):
        return _group_copy_operation(self, cyclic_replace=False)
    return _clone_mobject(self)


def _vmobject_path_boundary_queries_available(value: _base.Mobject) -> bool:
    if _is_shared_family(value):
        return False
    if _is_bound(value):
        context = getattr(value._scene, "_canonical_authoring_context", None)
        return (
            getattr(context, "queryMobjectPath", None) is not None
            and getattr(
                context, "queryMobjectPathBoundaryExtremumAlongDim", None
            ) is not None
        )
    handle = _handle_for(value)
    return (
        handle is not None
        and hasattr(handle, "pathQuery")
        and hasattr(handle, "pathBoundaryExtremumAlongDim")
    )


def _target_mobject(self: _base.Mobject) -> _base.Mobject:
    """Clone a detached target through Rust's explicit target-editor boundary."""

    # Family wrappers use the shared family-copy operation installed on Group.
    if not hasattr(self, "_scene") or not hasattr(self, "_object"):
        return self.copy()
    return _clone_mobject(self, target_state=True)


def _uses_content_layout(value: _base.Mobject) -> bool:
    # Class dispatch only: Rust owns both retained-content bounds and points.
    for module_name, type_name in (("_manim_typst", "_RetainedTextMobject"),
                                   ("_noon_spatial", "_WorldMobject")):
        content_type = getattr(sys.modules.get(module_name), type_name, ())
        if isinstance(value, content_type):
            return True
    return False


def _default_point_query_hooks(value: _base.Mobject) -> bool:
    from inspect import getattr_static
    boundary = (_compat.VMobject.get_points_defining_boundary
                if isinstance(value, _compat.VMobject)
                else _base.Mobject.get_points_defining_boundary)
    return (
        getattr_static(value, "get_family", None) is _base.Mobject.get_family
        and getattr_static(value, "get_all_points", None) is _base.Mobject.get_all_points
        and getattr_static(value, "get_points_defining_boundary", None) is boundary
        and getattr_static(value, "get_extremum_along_dim", None) is _base.Mobject.get_extremum_along_dim
    )


def _point_boundary_query_kind(value: _base.Mobject):
    if _uses_content_layout(value):
        return "content"
    if _is_shared_family(value):
        from _manim_updaters import canonical_callback_phase_active
        if canonical_callback_phase_active():
            # Keep family layout reads behind their existing staged guard;
            # selecting a point route must not inspect callback membership.
            return None
        handle = value._semantic_family_handle
        available = hasattr(handle, "requiresContentBoundary")
    else:
        handle = _handle_for(value)
        if _is_bound(value):
            context = getattr(value._scene, "_canonical_authoring_context", None)
            available = getattr(context, "queryMobjectRequiresContentBoundary", None) is not None
        else:
            available = handle is not None and hasattr(handle, "requiresContentBoundary")
    if not available:
        return None
    if _default_point_query_hooks(value) and handle is not None:
        # Wrapper subclasses do not identify retained content reliably (for
        # example generic Table entries). Rust selects from current semantic
        # content; no geometry or membership snapshot crosses this boundary.
        return ("content" if bool(_point_query_observation(value, "RequiresContentBoundary"))
                else "points")
    return "points"


def _point_boundary_queries_available(value: _base.Mobject) -> bool:
    return _point_boundary_query_kind(value) == "points"


def _get_center(self: _base.Mobject) -> Any:
    kind = _point_boundary_query_kind(self)
    if kind == "points":
        from _manim_path_queries import critical_point
        return critical_point(self, (0.0, 0.0, 0.0))
    if kind == "content":
        return _content_critical_point_3d(self, (0.0, 0.0, 0.0))
    if _is_shared_family(self):
        layout = _group_layout_observation(self)
        return _base.Vec2(float(layout.centerX), float(layout.centerY))
    observed = _bound_layout_observation(self)
    if observed is not None:
        return _base.Vec2(float(observed.centerX), float(observed.centerY))
    handle = _handle_for(self)
    if handle is not None:
        return _layout_center(self)
    raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")


def _get_critical_point(self: _base.Mobject, direction: object) -> Any:
    """Read full-coordinate point or retained-content bounds through Rust."""
    kind = _point_boundary_query_kind(self)
    if kind == "points":
        from _manim_path_queries import critical_point
        return critical_point(self, direction)
    if kind == "content" or (
        not isinstance(direction, _base.Vec2) and len(direction) == 3 and direction[2] != 0
    ):
        return _content_critical_point_3d(self, direction)
    axis = _base._as_vec2(direction)
    if _is_shared_family(self):
        layout = _group_layout_observation(self)
        return _base.Vec2(float(engine_call(layout.criticalX, axis.x, axis.y)),
                          float(engine_call(layout.criticalY, axis.x, axis.y)))
    if isinstance(self, _compat.VMobject):
        if self.get_num_points() == 0:
            return _base.ORIGIN
        return _base.Vec2(
            _extremum_along_dim_observation(self, 0, axis.x),
            _extremum_along_dim_observation(self, 1, axis.y),
        )
    observed = _bound_layout_observation(self)
    if observed is not None:
        return _base.Vec2(
            float(engine_call(observed.criticalX, axis.x, axis.y)),
            float(engine_call(observed.criticalY, axis.x, axis.y)),
        )
    return _critical(self, axis)


def _content_critical_point_3d(value: _base.Mobject, direction):
    """Forward opaque-content XYZ queries without constructing point geometry."""
    coordinates = _compat._manim_point3(direction)
    family = _is_shared_family(value)
    # Resolve the callback/live guard before creating an owned observation.
    if family:
        context = _group_live_layout_context(value)
    else:
        context = _bound_query_context(value)
        if context is None:
            context = _live_mutation_context(value)
        elif not engine_call(context.requiresLiveMutationRouting):
            # Scene.add may bind this identity before the first execution exists.
            # Rust owns that lifecycle; the authored anchor is current until then.
            context = None
    if family:
        handle = value._semantic_family_handle
        anchor = engine_call(handle.layoutAnchor, _semantic_member_index(None))
    else:
        anchor = _layout_anchor(value)
    if anchor is None:
        raise RuntimeError("content critical points require a current shared Rust layout handle")
    try:
        result = (engine_call(anchor.contentCriticalPoint3D, *coordinates) if context is None
                  else engine_call(context.queryLayoutContentCriticalPoint3D, anchor, *coordinates))
        return _compat._manim_point3_result(result)
    finally:
        anchor.free()


def _layout_dimension(self: _base.Mobject, dimension: int) -> float:
    """Read Rust content extents without asking for vector boundary anchors."""
    if _is_shared_family(self):
        anchor = _layout_anchor(self)
        if anchor is None:
            raise RuntimeError("family layout requires a current shared Rust semantic handle")
        return float(_dimension_fit_extent(anchor, _group_live_layout_context(self), dimension))
    observed = _bound_layout_observation(self, "queryMobjectDimension", dimension)
    if observed is not None:
        return float(observed)
    handle = _handle_for(self)
    if handle is not None:
        if dimension == 2:
            return float(engine_call(engine_call(handle.layoutAnchor, None).dimensionLength, dimension))
        return float(handle.width if dimension == 0 else handle.height)
    raise RuntimeError("Mobject layout requires a current shared Rust semantic handle")


def _width(self: _base.Mobject) -> float:
    return _layout_dimension(self, 0)


def _height(self: _base.Mobject) -> float:
    return _layout_dimension(self, 1)


def _set_width_property(self: _base.Mobject, width: float) -> None:
    self.scale_to_fit_width(float(width))


def _set_height_property(self: _base.Mobject, height: float) -> None:
    self.scale_to_fit_height(float(height))


def _shift(self: _base.Mobject, direction: object) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
    from _manim_path_editing import _raw_point3
    offset = _raw_point3(direction)
    arguments = offset if offset[2] != 0.0 else offset[:2]
    context = _live_mutation_context(self)
    if context is not None:
        try:
            engine_call(context.liveShift, handle, *arguments)
        except Exception as error:
            raise_engine_error(error)
        return self
    engine_call(handle.shift, *arguments)
    return self


def _move_to(
    self: _base.Mobject,
    point_or_mobject: object,
    aligned_edge: object = _base.ORIGIN,
    coor_mask: object = (1.0, 1.0, 1.0),
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject placement requires a current shared Rust semantic handle")
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)
    args = (edge.x, edge.y, mask.x, mask.y)
    context = _live_mutation_context(self)
    if _is_shared_family(point_or_mobject):
        target = _layout_anchor(point_or_mobject)
        if target is None:
            raise RuntimeError("placement target requires a current shared Rust semantic handle")
        if context is None:
            engine_call(engine_call(handle.layoutAnchor, None).moveTo, target, *args)
        else:
            engine_call(context.liveMoveToLayout, handle, target, *args)
    elif _alignment_is_mobject(point_or_mobject):
        target = _handle_for(point_or_mobject)
        if target is None:
            raise RuntimeError("placement target requires a current shared Rust semantic handle")
        if context is None:
            engine_call(handle.manimMoveToHandle, target, *args)
        else:
            engine_call(context.liveMoveToMobject, handle, target, *args)
    else:
        point = _base._as_vec2(point_or_mobject)
        if context is None:
            engine_call(handle.manimMoveToPoint, point.x, point.y, *args)
        else:
            engine_call(context.liveMoveToPoint, handle, point.x, point.y, *args)
    return self


def _dimension_fit_source(self, dim, *, planar_depth=False):
    if dim not in ((0, 1, 2) if planar_depth else (0, 1)):
        raise NotImplementedError("dimension fitting requires X, Y or Z")
    anchor = _layout_anchor(self)
    if anchor is None:
        raise RuntimeError("dimension fitting requires a shared Rust layout handle")
    context = (_group_live_layout_context(self) if _is_shared_family(self)
               else _live_mutation_context(self))
    return anchor, context


def _stretch(self, factor, dim, *, about_point=None, about_edge=None):
    anchor, context = _dimension_fit_source(self, dim, planar_depth=True)
    arguments = (float(factor), dim, *_pivot_arguments_3d(about_point, about_edge))
    if context is None:
        engine_call(anchor.stretch, *arguments)
    else:
        engine_call(context.liveStretchLayout, anchor, *arguments)
    return self


def _rescale_to_fit(self, length, dim, stretch=False, *, about_point=None, about_edge=None):
    dim = _fitting_dimension_index(dim)
    anchor, context = _dimension_fit_source(self, dim, planar_depth=True)
    # Manim ignores length/pivot arguments when the shared source extent is
    # zero. Avoid coercing them, but still let Rust validate the anchor and own
    # the no-op decision from its coherent fitting snapshot.
    zero_extent = _dimension_fit_extent(anchor, context, dim) == 0
    pivot = ((0., 0., 0., False) if zero_extent
             else _pivot_arguments_3d(about_point, about_edge))
    length = 0.0 if zero_extent else float(length)
    try:
        if context is None:
            engine_call(anchor.rescaleToFit, length, dim, bool(stretch), *pivot)
        else:
            engine_call(context.liveRescaleToFit, anchor, length, dim, bool(stretch), *pivot)
    except Exception as error:
        _raise_boundary_error(error)
    return self


def _match_dim_size(self, mobject, dim, stretch=False, *, about_point=None, about_edge=None):
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("dimension match target must be a Mobject")
    dim = _fitting_dimension_index(dim)
    anchor, context = _dimension_fit_source(self, dim, planar_depth=True)
    zero_extent = _dimension_fit_extent(anchor, context, dim) == 0
    pivot = ((0., 0., 0., False) if zero_extent
             else _pivot_arguments_3d(about_point, about_edge))
    target = _layout_anchor(mobject)
    if target is None:
        raise RuntimeError("dimension matching requires a shared Rust target")
    try:
        if context is None:
            engine_call(anchor.matchDimSize, target, dim, bool(stretch), *pivot)
        else:
            engine_call(context.liveMatchDimSize, anchor, target, dim, bool(stretch), *pivot)
    except Exception as error:
        _raise_boundary_error(error)
    return self


def _raise_boundary_error(error):
    diagnostic = error
    while diagnostic is not None:
        if getattr(diagnostic, "code", None) == "path.unpaired_anchors":
            raise ValueError("zip() argument 2 is shorter than argument 1") from error
        diagnostic = (getattr(error, "rust_cause", None) if diagnostic is error
                      else diagnostic.cause)
    raise_engine_error(error)


def _dimension_fit_extent(anchor, context, dim):
    if context is None:
        return engine_call(anchor.dimensionLength, dim)
    return engine_call(context.queryLayoutDimensionLength, anchor, dim)


def _fitting_dimension_index(dim):
    # rescale_to_fit measures length through reduce_across_dimension, whose
    # range assertions precede NumPy's integer-index validation.
    assert dim >= 0
    assert dim <= 2
    try:
        return operator.index(dim)
    except TypeError as error:
        raise IndexError("only integers are valid dimension indices") from error


def _scale(self: _base.Mobject, factor: object, *, about_point=None, about_edge=None) -> _base.Mobject:
    if _handle_for(self) is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
    return _scale_xyz(self, factor, about_point, about_edge)


def _scale_factors_xyz(factor):
    # Preserve Vec2/two-component authoring ergonomics; scalar and three-axis
    # factors use the complete Manim coordinates. This is argument adaptation,
    # never a frontend transform or family/bounds calculation.
    if isinstance(factor, _base.Vec2):
        return float(factor.x), float(factor.y), 1.0
    try:
        count = len(factor)
    except (TypeError, AttributeError):
        scalar = float(factor)
        return scalar, scalar, scalar
    if count == 1:
        scalar = float(factor[0])
        return scalar, scalar, scalar
    if count in (2, 3):
        return float(factor[0]), float(factor[1]), float(factor[2]) if count == 3 else 1.0
    raise ValueError("scale factor must be scalar or have one, two or three components")


def _scale_xyz(self, factor, about_point, about_edge):
    anchor = _layout_anchor(self)
    if anchor is None:
        raise RuntimeError("scale requires the shared Rust authoring host")
    context = (_group_live_layout_context(self) if _is_shared_family(self)
               else _live_mutation_context(self))
    arguments = (*_scale_factors_xyz(factor), *_pivot_arguments_3d(about_point, about_edge))
    try:
        if context is None:
            engine_call(anchor.scale3d, *arguments)
        else:
            engine_call(context.liveScaleLayout3d, anchor, *arguments)
    except Exception as error:
        _raise_boundary_error(error)
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

    if kwargs:
        raise NotImplementedError(f"unsupported Manim rotate option(s): {', '.join(sorted(kwargs))}")
    return _planar_affine(self, "rotate", (_compat._rotation_angle_2d(angle, axis),), about_point, about_edge)


def _pivot_arguments_3d(about_point, about_edge):
    from _manim_path_editing import _raw_point3
    value = about_point if about_point is not None else (_base.ORIGIN if about_edge is None else about_edge)
    return (*_raw_point3(value), about_point is not None)


def _pivot_arguments(about_point, about_edge):
    point = _base._as_vec2(about_point if about_point is not None
                           else (_base.ORIGIN if about_edge is None else about_edge))
    return point.x, point.y, about_point is not None


def _planar_affine(self, operation, arguments, about_point, about_edge):
    anchor = _layout_anchor(self)
    if anchor is None:
        raise RuntimeError("affine edits require the shared Rust authoring host")
    context = (_group_live_layout_context(self) if _is_shared_family(self)
               else _live_mutation_context(self))
    arguments = (*arguments, *_pivot_arguments(about_point, about_edge))
    if context is None:
        engine_call(getattr(anchor, operation), *arguments)
    else:
        engine_call(getattr(context, f"live{operation.title()}Layout"), anchor, *arguments)
    return self


def _get_z_index(self):
    anchor = _layout_anchor(self)
    if anchor is None:
        raise RuntimeError("painter priority requires the shared Rust authoring host")
    context = (_group_live_layout_context(self) if _is_shared_family(self)
               else _live_mutation_context(self))
    if context is not None:
        return float(engine_call(context.liveZIndex, anchor))
    return float(engine_call(anchor.zIndex))


def _set_z_index(self, value, family=True):
    from _manim_updaters import _canonical_phase_context
    if not _is_shared_family(self) and _canonical_phase_context(self) is not None:
        raise NotImplementedError("z-index during a host callback requires phase-local publication")
    value = float(value)
    anchor = _layout_anchor(self)
    if anchor is None:
        raise RuntimeError("painter priority requires the shared Rust authoring host")
    context = (_group_live_layout_context(self) if _is_shared_family(self)
               else _live_mutation_context(self))
    if context is None:
        engine_call(anchor.setZIndex, value, bool(family))
    else:
        engine_call(context.liveSetZIndex, anchor, value, bool(family))
    return self


def _flip(self, axis=_base.UP, *, about_point=None, about_edge=None):
    from _manim_updaters import _canonical_phase_context
    if not _is_shared_family(self) and _canonical_phase_context(self) is not None:
        raise NotImplementedError("flip during a host callback needs shared phase-local affine capture")
    try:
        components = tuple(float(component) for component in axis)
    except (TypeError, ValueError) as error:
        raise TypeError("flip axis must be a two- or three-component vector") from error
    if len(components) == 2:
        components += (0.0,)
    if len(components) != 3:
        raise TypeError("flip axis must be a two- or three-component vector")
    return _planar_affine(self, "flip", components, about_point, about_edge)


def _set_color(self: _base.Mobject, color: _base.Color) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
    if not isinstance(color, _base.Color):
        raise TypeError("color must be a Color")
    live_context = _live_mutation_context(self)
    if live_context is not None:
        try:
            engine_call(live_context.liveSetColor,
                handle, color.red, color.green, color.blue, color.alpha
            )
        except Exception as error:
            raise_engine_error(error)
        return self

    engine_call(handle.setColor, color.red, color.green, color.blue, color.alpha)
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

    from _manim_updaters import _canonical_phase_context, _canonical_provisional_context
    provisional = _canonical_provisional_context(mobject)
    if provisional is not None:
        if any((match_height, match_width, match_center, stretch)):
            raise NotImplementedError(
                "callback provisional become does not support dimension matching"
            )
        context = _canonical_phase_context(self)
        if context is None or context is not provisional[0]:
            raise RuntimeError("callback provisional become requires a target in the same phase")
        context.replace_effective_provisional(self, mobject)
        return self

    if _is_shared_family(self) or _is_shared_family(mobject):
        if not _is_shared_family(self) or not _is_shared_family(mobject):
            raise NotImplementedError("become between an object and a family requires topology alignment")
        source = self._semantic_family_handle
        target = mobject._semantic_family_handle
        flags = (bool(match_height), bool(match_width), bool(match_center), bool(stretch))
        context = _group_target_context(self) or _group_target_context(mobject)
        if context is None:
            engine_call(source.becomeFamily, target, *flags)
        else:
            engine_call(context.liveBecomeFamily, source, target, *flags)
        return self

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
            engine_call(context.liveBecomeMobject, handle, other_handle, *flags)
        else:
            engine_call(handle.becomeHandle, other_handle, *flags)
        return self

    raise NotImplementedError(
        "become requires valid shared semantic handles for both Mobjects"
    )


def _replace(
    self: _base.Mobject,
    mobject: _base.Mobject,
    dim_to_match: int = 0,
    stretch: bool = False,
) -> _base.Mobject:
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("replacement target must be a Mobject")
    from _manim_updaters import _canonical_phase_context

    callback_context = _canonical_phase_context(self)
    if callback_context is not None:
        if getattr(mobject, "_scene", None) not in (None, callback_context._scene):
            raise ValueError("callback replacement target belongs to another Scene")
        callback_context.replace_layout(self, mobject, dim_to_match, stretch)
        return self
    source, target, context = _replacement_anchors(self, mobject, dim_to_match)
    if context is None:
        engine_call(source.replaceLayout, target, int(dim_to_match), bool(stretch))
    else:
        engine_call(context.liveReplaceLayout, source, target, int(dim_to_match), bool(stretch))
    return self


def _replacement_anchors(self, mobject, dimension):
    source, context = _dimension_fit_source(self, dimension)
    target = _layout_anchor(mobject)
    if target is None:
        raise RuntimeError("replacement requires a shared Rust target layout")
    if context is None:
        context = (_group_live_layout_context(mobject) if _is_shared_family(mobject)
                   else _live_mutation_context(mobject))
    if context is None:
        context = _live_constructor_context("replace")
    return source, target, context


def _circle_surround(self, mobject, dim_to_match=0, stretch=False, buffer_factor=1.2):
    if not isinstance(mobject, _base.Mobject):
        raise TypeError("Circle surround target must be a Mobject")
    from _manim_updaters import _canonical_phase_context

    if (_canonical_phase_context(self) is not None
            or _canonical_phase_context(mobject) is not None):
        raise NotImplementedError("Circle.surround requires a non-staged layout")
    stretch = bool(stretch)
    # replace(stretch=True) never inspects the dimension. Otherwise Manim's
    # reduce_across_dimension asserts its range before NumPy indexes the points.
    dimension = 0
    if not stretch:
        assert dim_to_match >= 0
        assert dim_to_match <= 2
        try:
            dimension = operator.index(dim_to_match)
        except TypeError as error:
            raise IndexError("Circle surround dimension must be an integer index") from error
    source, target, context = _replacement_anchors(self, mobject, 0)
    if context is None:
        engine_call(source.surroundCircle, target, dimension, stretch, float(buffer_factor))
    else:
        engine_call(context.liveSurroundCircle, source, target, dimension, stretch, float(buffer_factor))
    return self


def _critical(value: _base.Mobject, direction: _base.Vec2) -> _base.Vec2:
    handle = _handle_for(value)
    if handle is not None:
        return _base.Vec2(
            float(engine_call(handle.criticalX, direction.x, direction.y)),
            float(engine_call(handle.criticalY, direction.x, direction.y)),
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


def _layout_reference_handle(value, callback_context=None):
    """Borrow typed identity for placement without granting raw geometry access."""
    handle = getattr(value, "_semantic_handle", None)
    if handle is None or not hasattr(handle, "layoutAnchor"):
        return None
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    from _manim_updaters import _canonical_phase_context
    active_context = _canonical_phase_context(value)
    if active_context is not None and active_context is not callback_context:
        raise NotImplementedError("layout placement is unsupported during an active callback phase")
    if (callback_context is not None
            and getattr(value, "_scene", None) is not callback_context._scene):
        raise RuntimeError("callback layout anchors must belong to the active Scene")
    return handle


def _layout_anchor(value, index=None, *, callback_context=None):
    if _is_shared_family(value):
        handle = getattr(value, "_semantic_family_handle", None)
        if handle is None or not hasattr(handle, "layoutAnchor"):
            return None
        if any(_layout_reference_handle(leaf, callback_context) is None
               for leaf in _compat._leaf_mobjects(value)):
            return None
    else:
        handle = _layout_reference_handle(value, callback_context)
    if handle is None:
        return None
    return engine_call(handle.layoutAnchor, _semantic_member_index(index))


def _next_to_edge_in_frame(self, target, edge, direction, buff, frame_buff):
    """Forward placement intent; Rust owns bounds, frame fitting and publication."""
    source = _layout_anchor(self)
    target_anchor = _layout_anchor(target)
    if source is None or target_anchor is None:
        raise RuntimeError("placement requires current shared Rust source and target handles")
    context = (_group_live_layout_context(self) if _is_shared_family(self)
               else _live_mutation_context(self) or _live_constructor_context())
    arguments = (edge.x, edge.y, direction.x, direction.y, float(buff), float(frame_buff))
    try:
        if context is None:
            engine_call(source.nextToEdgeInFrame, target_anchor, *arguments)
        else:
            engine_call(context.liveNextLayoutToEdgeInFrame, source, target_anchor, *arguments)
    except Exception as error:
        raise_engine_error(error)


def _shared_next_to(self, target, direction, buff, aligned_edge,
                      submobject_to_align, index, coor_mask):
    """Pass selection intent; Rust resolves members, observes bounds and places."""
    source = _layout_anchor(self)
    aligner = (_layout_anchor(submobject_to_align) if submobject_to_align is not None
               else _layout_anchor(self, index))
    if source is None or aligner is None:
        raise RuntimeError("placement requires current shared Rust source and aligner handles")
    target_anchor = None
    if isinstance(target, (_base.Mobject, _compat.Group)):
        target_anchor = _layout_anchor(target, index)
        if target_anchor is None:
            raise RuntimeError("placement target requires a current shared Rust semantic handle")
    vector = _base._as_vec2(direction)
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)
    arguments = (vector.x, vector.y, float(buff), edge.x, edge.y, mask.x, mask.y)
    context = (_group_live_layout_context(self) if _is_shared_family(self)
               else _live_mutation_context(self) or _live_constructor_context())
    try:
        if target_anchor is not None:
            if context is None:
                engine_call(source.nextTo, target_anchor, aligner, *arguments)
            else:
                engine_call(context.liveNextLayoutTo, source, target_anchor, aligner, *arguments)
        else:
            point = _base._as_vec2(target)
            if context is None:
                engine_call(source.nextToPoint, point.x, point.y, aligner, *arguments)
            else:
                engine_call(context.liveNextLayoutToPoint, source, point.x, point.y, aligner, *arguments)
    except Exception as error:
        raise_engine_error(error)


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
    _shared_next_to(self, mobject_or_point, direction, buff, aligned_edge,
                    submobject_to_align, index_of_submobject_to_align, coor_mask)
    return self


def _align_to(
    self: _base.Mobject,
    mobject_or_point: object,
    direction: object = _base.ORIGIN,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("alignment requires current shared Rust semantic handles")
    axis = _base._as_vec2(direction)
    context = _live_mutation_context(self) or _live_constructor_context()
    if context is not None:
        mask_x = 1.0 if axis.x != 0.0 else 0.0
        mask_y = 1.0 if axis.y != 0.0 else 0.0
        try:
            if isinstance(mobject_or_point, (_base.Mobject, _compat.Group)):
                target = _layout_anchor(mobject_or_point)
                if target is None:
                    raise RuntimeError(
                        "alignment target requires current shared Rust semantic handles"
                    )
                engine_call(context.liveMoveToLayout, handle, target,
                            axis.x, axis.y, mask_x, mask_y)
            else:
                point = _base._as_vec2(mobject_or_point)
                engine_call(context.liveMoveToPoint, handle, point.x, point.y,
                            axis.x, axis.y, mask_x, mask_y)
        except Exception as error:
            raise_engine_error(error)
        return self
    if _is_shared_family(mobject_or_point):
        target = _layout_anchor(mobject_or_point)
        if target is None:
            raise RuntimeError("alignment requires current shared Rust semantic handles")
        engine_call(engine_call(handle.layoutAnchor, None).alignTo, target, axis.x, axis.y)
    elif _alignment_is_mobject(mobject_or_point):
        target_handle = _handle_for(mobject_or_point)
        if target_handle is None:
            raise RuntimeError("alignment requires current shared Rust semantic handles")
        engine_call(handle.alignToHandle, target_handle, axis.x, axis.y)
    else:
        point = _base._as_vec2(mobject_or_point)
        engine_call(handle.alignToPoint, point.x, point.y, axis.x, axis.y)
    return self


def _align_on_frame(
    self: _base.Mobject,
    direction: _base.Vec2,
    buff: float,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None or not hasattr(handle, "alignOnFrame"):
        raise RuntimeError("Mobject frame alignment requires a current shared Rust semantic handle")
    context = _live_mutation_context(self) or _live_constructor_context()
    if context is not None:
        try:
            engine_call(
                context.liveAlignMobjectOnFrame,
                handle,
                direction.x,
                direction.y,
                float(buff),
            )
        except Exception as error:
            raise_engine_error(error)
        return self
    engine_call(handle.alignOnFrame, direction.x, direction.y, float(buff))
    return self


def _paint_color(self, channel):
    from _manim_updaters import _canonical_phase_context
    phase = _canonical_phase_context(self)
    if phase is not None:
        raise NotImplementedError("paint color getters in host callbacks require staged color observation")
    family_handle = getattr(self, "_semantic_family_handle", None)
    if family_handle is not None:
        method = getattr(family_handle, f"own{channel.title()}Color", None)
        if method is not None:
            color = engine_call(method)
            if color is None:
                return None
            return _base.Color(float(color.red), float(color.green), float(color.blue), 1.0)
    color = _typed_manim_observation(self, f"{channel}Color", f"queryMobject{channel.title()}Color")
    if color is None:
        return None
    return _base.Color(float(color.red), float(color.green), float(color.blue), 1.0)


def _get_stroke_width(self, background=False):
    if background:
        return _get_background_stroke_width(self)
    from _manim_updaters import _canonical_phase_context
    if _canonical_phase_context(self) is not None:
        raise NotImplementedError("stroke width getters in host callbacks require staged width observation")
    family_handle = getattr(self, "_semantic_family_handle", None)
    if family_handle is not None:
        method = getattr(family_handle, "ownStrokeWidth", None)
        if method is not None:
            return float(engine_call(method)) * 100.0
    width = _typed_manim_observation(self, "strokeWidth", "queryMobjectStrokeWidth", handle_property=True)
    if width is None:
        raise RuntimeError("paint observations require the shared Rust authoring host")
    return float(width) * 100.0


def _gradient_components(colors):
    # This only converts the language-boundary argument; Rust owns interpolation.
    from pyodide.ffi import to_js
    return to_js([component for color in colors for component in
                  (color.red, color.green, color.blue, color.alpha)])


def _set_color_by_gradient(self, *colors):
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError("per-member gradients in host callbacks require staged paint publication")
    parsed = [_compat._as_color("gradient color", color) for color in colors]
    target, context, suffix = _style_target(self)
    if target is None:
        raise RuntimeError("gradients require the shared Rust authoring host")
    arguments = _gradient_components(parsed)
    if context is None:
        engine_call(target.setColorGradient, arguments)
    else:
        engine_call(getattr(context, f"liveSet{suffix}ColorGradient"), target, arguments)
    return self


def _set_color_by_radial_gradient(self, center, radius, inner_color, outer_color):
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError(
            "radial gradients in host callbacks require staged paint publication"
        )
    inner = _compat._as_color("inner gradient color", inner_color)
    outer = _compat._as_color("outer gradient color", outer_color)
    radius = _compat._ir._finite_number("radius", radius)
    if center is None:
        center = self.get_center()
    if isinstance(center, _base.Vec2):
        center = (center.x, center.y, 0.0)
    else:
        try:
            dimensions = len(center)
        except (TypeError, AttributeError):
            dimensions = None
        if dimensions not in (2, 3):
            raise TypeError("expected a two- or three-component vector")
        try:
            center = tuple(float(center[index]) for index in range(dimensions))
        except (TypeError, ValueError, IndexError) as error:
            raise TypeError("expected a two- or three-component numeric vector") from error
        if dimensions == 2:
            center = (*center, 0.0)
    target, context, suffix = _style_target(self)
    if target is None:
        raise RuntimeError("radial gradients require the shared Rust authoring host")
    arguments = _gradient_components((inner, outer))
    if context is None:
        engine_call(
            target.setRadialColorGradient,
            *center,
            radius,
            arguments,
        )
    else:
        engine_call(
            getattr(context, f"liveSet{suffix}ColorRadialGradient"),
            target,
            *center,
            radius,
            arguments,
        )
    return self


def _style_target(value):
    if _is_shared_family(value):
        return value._semantic_family_handle, _group_target_context(value), "Family"
    return _handle_for(value), _live_mutation_context(value), ""


def _style_value(value, name):
    if hasattr(value, "tolist"):
        value = value.tolist()
    if isinstance(value, (list, tuple)):
        if len(value) != 1:
            raise NotImplementedError(
                f"multi-value {name} requires shared gradient paint"
            )
        return value[0]
    return value


def _style_color(value, name):
    if value is None:
        return None
    if hasattr(value, "tolist"):
        value = value.tolist()
    if isinstance(value, (list, tuple)) and len(value) == 1:
        value = value[0]
        if value is None:
            return None
        if hasattr(value, "tolist"):
            value = value.tolist()
    if (
        isinstance(value, (list, tuple))
        and len(value) in (3, 4)
        and all(isinstance(component, numbers.Real) for component in value)
    ):
        components = [
            _ir._unit_interval(f"{name} component", component)
            for component in value
        ]
        return _base.Color(*components)
    if isinstance(value, (list, tuple)):
        raise NotImplementedError(
            f"multi-color {name} requires shared gradient paint"
        )
    return _compat._as_color(name, value)


def _interpolate_manim_style(self, mobject1, mobject2, alpha):
    """Publish VMobject's supported color fields through one Rust operation."""
    objects = (self, mobject1, mobject2)
    family_profile = all(
        isinstance(value, _compat.VGroup) and _is_shared_family(value)
        for value in objects
    )
    leaf_profile = all(
        isinstance(value, _compat.VMobject) and not isinstance(value, (_compat.Group, _compat.VGroup))
        for value in objects
    )
    if not family_profile and not leaf_profile:
        raise NotImplementedError(
            "VMobject.interpolate_color requires three VMobjects or three shared VGroups"
        )
    from _manim_updaters import _canonical_phase_context
    if any(_canonical_phase_context(value) is not None for value in objects):
        raise NotImplementedError(
            "VMobject.interpolate_color in host callbacks requires staged style publication"
        )

    amount = _ir._finite_number("alpha", alpha)
    if family_profile:
        receiver = self._semantic_family_handle
        start = mobject1._semantic_family_handle
        end = mobject2._semantic_family_handle
        context = _group_target_context(self)
        if context is None:
            engine_call(receiver.interpolateManimStyle, start, end, amount)
        else:
            engine_call(
                context.liveInterpolateFamilyManimStyle,
                receiver,
                start,
                end,
                amount,
            )
        return self

    target = _handle_for(self)
    start = _handle_for(mobject1)
    end = _handle_for(mobject2)
    if target is None or start is None or end is None:
        raise RuntimeError(
            "VMobject.interpolate_color requires current shared Rust semantic handles"
        )

    context = _live_mutation_context(self)
    if context is None:
        engine_call(target.interpolateManimStyle, start, end, amount)
    else:
        engine_call(
            context.liveInterpolateMobjectStyle,
            target,
            start,
            end,
            amount,
        )
    return self


def _interpolate_manim(self, mobject1, mobject2, alpha, path_func):
    """Publish VMobject's default straight-path point and style interpolation in Rust."""
    family_receiver = isinstance(self, _compat.VGroup) and _is_shared_family(self)
    if not isinstance(self, _compat.VMobject) and not family_receiver:
        raise NotImplementedError("Please override in a child class.")
    objects = (self, mobject1, mobject2)
    family_profile = all(
        isinstance(value, _compat.VGroup) and _is_shared_family(value)
        for value in objects
    )
    leaf_profile = all(
        isinstance(value, _compat.VMobject) and not isinstance(value, (_compat.Group, _compat.VGroup))
        for value in objects
    )
    if not family_profile and not leaf_profile:
        raise NotImplementedError(
            "Mobject.interpolate requires three VMobjects or three shared VGroups"
        )
    if type(self).interpolate_color is not _compat.VMobject.interpolate_color:
        raise NotImplementedError(
            "Mobject.interpolate subclass interpolate_color overrides require host dispatch"
        )
    from _manim_user_vectors import straight_path
    if path_func is not straight_path():
        raise NotImplementedError(
            "Mobject.interpolate custom path_func callbacks require shared Rust path support"
        )
    from _manim_updaters import _canonical_phase_context
    if any(_canonical_phase_context(value) is not None for value in objects):
        raise NotImplementedError(
            "Mobject.interpolate in host callbacks requires staged geometry publication"
        )

    amount = _ir._finite_number("alpha", alpha)
    if family_profile:
        if any(
            engine_call(value._semantic_family_handle.pointCount) != 0
            for value in objects
        ):
            raise NotImplementedError(
                "Mobject.interpolate requires point-free shared VGroup roots"
            )
        return _interpolate_manim_style(self, mobject1, mobject2, amount)

    target = _handle_for(self)
    start = _handle_for(mobject1)
    end = _handle_for(mobject2)
    if target is None or start is None or end is None:
        raise RuntimeError(
            "Mobject.interpolate requires current shared Rust semantic handles"
        )

    context = _live_mutation_context(self)
    if context is None:
        engine_call(target.interpolateManim, start, end, amount)
    else:
        engine_call(
            context.liveInterpolateMobject,
            target,
            start,
            end,
            amount,
        )
    return self


def _set_style(
    self,
    fill_color=None,
    fill_opacity=None,
    stroke_color=None,
    stroke_width=None,
    stroke_opacity=None,
    background_stroke_color=None,
    background_stroke_width=None,
    background_stroke_opacity=None,
    sheen_factor=None,
    sheen_direction=None,
    background_image=None,
    family=True,
):
    fill_color = _style_color(fill_color, "fill color")
    stroke_color = _style_color(stroke_color, "stroke color")
    fill_opacity = _style_value(fill_opacity, "fill opacity")
    stroke_opacity = _style_value(stroke_opacity, "stroke opacity")
    background_stroke_color = _style_color(
        background_stroke_color, "background stroke color"
    )
    background_stroke_width = _style_value(
        background_stroke_width, "background stroke width"
    )
    background_stroke_opacity = _style_value(
        background_stroke_opacity, "background stroke opacity"
    )

    # VGroup family=False addresses the point-free family root, whose style is
    # stored separately from its rendered leaves.
    root_background_stroke = None
    if not family and isinstance(self, _compat.VGroup) and any(
        value is not None
        for value in (
            background_stroke_color,
            background_stroke_width,
            background_stroke_opacity,
        )
    ):
        root_background_stroke = (
            background_stroke_color,
            None
            if background_stroke_width is None
            else _compat._manim_stroke_width(background_stroke_width),
            None
            if background_stroke_opacity is None
            else _compat._opacity("background stroke opacity", background_stroke_opacity),
        )
    if sheen_factor is not None and _ir._finite_number("sheen factor", sheen_factor) != 0.0:
        raise NotImplementedError("2D sheen requires shared gradient paint")
    if background_image:
        raise NotImplementedError("background images require shared image paint")

    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError("atomic set_style in host callbacks requires staged style publication")
    own_family_style = not family and _is_shared_family(self)
    arguments = (*_family_color_arguments(fill_color),
                 None if fill_opacity is None else _compat._opacity("fill opacity", fill_opacity),
                 *_family_color_arguments(stroke_color),
                 None if stroke_width is None else _compat._manim_stroke_width(stroke_width),
                 None if stroke_opacity is None else _compat._opacity("stroke opacity", stroke_opacity))
    background_arguments = (
        *_family_color_arguments(background_stroke_color),
        None
        if background_stroke_width is None
        else _compat._manim_stroke_width(background_stroke_width),
        None
        if background_stroke_opacity is None
        else _compat._opacity("background stroke opacity", background_stroke_opacity),
    )
    handle, context, suffix = _style_target(self)
    if handle is None:
        raise RuntimeError("style updates require the shared Rust authoring host")
    if root_background_stroke is not None and context is None:
        engine_call(
            handle.setOwnStyleWithBackgroundStroke,
            *arguments,
            *_family_color_arguments(root_background_stroke[0]),
            root_background_stroke[1],
            root_background_stroke[2],
        )
    elif root_background_stroke is not None:
        engine_call(
            context.liveSetFamilyOwnStyleWithBackgroundStroke,
            handle,
            *arguments,
            *_family_color_arguments(root_background_stroke[0]),
            root_background_stroke[1],
            root_background_stroke[2],
        )
    elif context is None:
        method_name = "setOwnStyle" if own_family_style else "setStyle"
        if own_family_style:
            engine_call(getattr(handle, method_name), *arguments)
        else:
            engine_call(getattr(handle, method_name), *arguments, *background_arguments)
    else:
        method_suffix = f"{suffix}Own" if own_family_style else suffix
        if own_family_style:
            engine_call(getattr(context, f"liveSet{method_suffix}Style"), handle, *arguments)
        else:
            engine_call(
                getattr(context, f"liveSet{method_suffix}Style"),
                handle,
                *arguments,
                *background_arguments,
            )
    return self


def _set_background_stroke(self, color=None, width=None, opacity=None, family=True):
    if color is None and width is None and opacity is None:
        return self
    return _set_style(
        self,
        background_stroke_color=color,
        background_stroke_width=width,
        background_stroke_opacity=opacity,
        family=family,
    )


def _get_background_stroke_style(self):
    family_handle = getattr(self, "_semantic_family_handle", None)
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT, _canonical_phase_context

    if (
        _ACTIVE_CANONICAL_CONTEXT.get() is not None
        or _canonical_phase_context(self) is not None
    ):
        raise NotImplementedError(
            "background-stroke queries require staged style observation"
        )
    if family_handle is not None:
        color_method = getattr(family_handle, "ownBackgroundStrokeColor", None)
        width_method = getattr(family_handle, "ownBackgroundStrokeWidth", None)
        opacity_method = getattr(family_handle, "ownBackgroundStrokeOpacity", None)
        if color_method is not None and width_method is not None and opacity_method is not None:
            color = engine_call(color_method)
            return (
                _base.Color(float(color.red), float(color.green), float(color.blue), float(color.alpha)),
                float(engine_call(width_method)),
                float(engine_call(opacity_method)),
            )

    color = _typed_manim_observation(
        self, "backgroundStrokeColor", "queryMobjectBackgroundStrokeColor"
    )
    width = _typed_manim_observation(
        self,
        "backgroundStrokeWidth",
        "queryMobjectBackgroundStrokeWidth",
        handle_property=True,
    )
    opacity = _typed_manim_observation(
        self,
        "backgroundStrokeOpacity",
        "queryMobjectBackgroundStrokeOpacity",
        handle_property=True,
    )
    if width is None or opacity is None:
        raise RuntimeError("background-stroke observations require the shared Rust authoring host")
    if color is None:
        color = _base.BLACK
    return (
        _base.Color(float(color.red), float(color.green), float(color.blue), float(color.alpha)),
        float(width),
        float(opacity),
    )


def _get_background_stroke_color(self):
    return _get_background_stroke_style(self)[0]


def _get_background_stroke_width(self):
    return _get_background_stroke_style(self)[1] * 100.0


def _get_background_stroke_opacity(self):
    return _get_background_stroke_style(self)[2]


def _sheen_direction_components(direction):
    if hasattr(direction, "tolist"):
        direction = direction.tolist()
    elif not isinstance(direction, (list, tuple)):
        try:
            direction = tuple(direction)
        except TypeError:
            raise TypeError("sheen direction must be a 2D or 3D vector") from None
    if not isinstance(direction, (list, tuple)) or len(direction) not in (2, 3):
        raise ValueError("sheen direction must contain two or three components")
    dimensions = len(direction)
    values = tuple(
        _ir._finite_number(f"sheen direction[{index}]", value)
        for index, value in enumerate(direction)
    )
    if dimensions == 2:
        values = (*values, 0.0)
    return values, dimensions


def _sheen_rotation_axis(axis):
    values, _dimensions = _sheen_direction_components(axis)
    return values


def _check_sheen_callback_write(self):
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT, _canonical_phase_context
    if (
        _ACTIVE_CANONICAL_CONTEXT.get() is not None
        or _canonical_phase_context(self) is not None
    ):
        raise NotImplementedError(
            "sheen direction updates in staged callbacks require staged metadata publication"
        )


def _set_sheen_direction(self, direction, family=True):
    values, dimensions = _sheen_direction_components(direction)
    _check_sheen_callback_write(self)
    target, context, suffix = _style_target(self)
    if target is None:
        raise RuntimeError("sheen direction updates require the shared Rust authoring host")
    if suffix == "Family":
        if context is None:
            engine_call(target.setSheenDirection, *values, dimensions, bool(family))
        else:
            engine_call(
                context.liveSetFamilySheenDirection,
                target,
                *values,
                dimensions,
                bool(family),
            )
    elif context is None:
        engine_call(target.setSheenDirection, *values, dimensions, bool(family))
    else:
        engine_call(
            context.liveSetMobjectSheenDirection,
            target,
            *values,
            dimensions,
            bool(family),
        )
    return self


def _set_sheen(self, factor, direction=None, family=True):
    value = _ir._finite_number("sheen factor", factor)
    if value != 0.0:
        raise NotImplementedError("2D sheen requires shared gradient paint")
    if direction is not None:
        return _set_sheen_direction(self, direction, family)
    return self


def _set_cap_style(self, cap_style):
    cap = _ir._stroke_cap(cap_style)
    target, context, suffix = _style_target(self)
    if target is None:
        raise RuntimeError("cap styles require the shared Rust authoring host")
    if suffix == "Family":
        if context is None:
            engine_call(target.setStrokeCap, cap)
        else:
            engine_call(context.liveSetFamilyOwnStrokeCap, target, cap)
    elif context is None:
        engine_call(target.setStrokeCap, cap)
    else:
        engine_call(context.liveSetStrokeCap, target, cap)
    return self


def _get_cap_style(self):
    target, context, suffix = _style_target(self)
    if target is None:
        raise RuntimeError("cap styles require the shared Rust authoring host")
    if suffix == "Family":
        cap = engine_call(target.ownStrokeCap)
    elif context is None:
        # wasm-bindgen exposes strokeCap as a JavaScript property getter.
        cap = target.strokeCap
    else:
        cap = engine_call(context.queryMobjectStrokeCap, target)
    return getattr(_base.CapStyleType, str(cap).upper())


def _rotate_sheen_direction(self, angle, axis, family=True):
    value = _ir._finite_number("sheen rotation angle", angle)
    if value == 0.0:
        return self
    axis = _sheen_rotation_axis(axis)
    _check_sheen_callback_write(self)
    target, context, suffix = _style_target(self)
    if target is None:
        raise RuntimeError("sheen direction updates require the shared Rust authoring host")
    if suffix == "Family":
        if context is None:
            engine_call(target.rotateSheenDirection, value, *axis, bool(family))
        else:
            engine_call(
                context.liveRotateFamilySheenDirection,
                target,
                value,
                *axis,
                bool(family),
            )
    elif context is None:
        engine_call(target.rotateSheenDirection, value, *axis, bool(family))
    else:
        engine_call(
            context.liveRotateMobjectSheenDirection,
            target,
            value,
            *axis,
            bool(family),
        )
    return self


def _get_sheen_direction(self):
    target, _context, suffix = _style_target(self)
    if target is None:
        values = (-1.0, 1.0, 0.0)
    elif suffix == "Family":
        values = engine_call(target.ownSheenDirection)
    else:
        values = engine_call(target.sheenDirection)
    try:
        import numpy as np
    except ImportError:
        return list(values)
    return np.asarray(values, dtype=np.float64)


def _get_gradient_start_and_end_points(self):
    direction = [float(value) for value in self.get_sheen_direction()]
    if direction[2] != 0.0:
        raise NotImplementedError("3D sheen gradients require spatial geometry")
    center = self.get_center()
    right = self.get_edge_center(_base.RIGHT) - center
    up = self.get_edge_center(_base.UP) - center
    offset = right * direction[0] + up * direction[1]
    return (
        _compat._manim_point3(center - offset),
        _compat._manim_point3(center + offset),
    )


def _color_using_background_image(self, background_image):
    if background_image is not None:
        raise NotImplementedError("background images require shared image paint")
    self.set_color(_base.WHITE)
    return self


def _match_background_image(self, vmobject):
    background_image = vmobject.get_background_image()
    if background_image is not None:
        raise NotImplementedError("background images require shared image paint")
    return self.color_using_background_image(None)


def _match_style(self, vmobject, family=True):
    if not isinstance(vmobject, _base.Mobject):
        raise TypeError("match_style target must be a Mobject")
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
        raise NotImplementedError("match_style in host callbacks requires staged style capture")
    if not family:
        get_style = getattr(vmobject, "get_style", None)
        receiver_get_style = getattr(self, "get_style", None)
        set_style = getattr(self, "set_style", None)
        if (
            not callable(receiver_get_style)
            or not callable(get_style)
            or not callable(set_style)
        ):
            raise NotImplementedError(
                "non-recursive style matching requires vector style access"
            )
        # Match only the receiver's VMobject root. set_style(..., family=False)
        # keeps the write atomic and routes live VGroup roots through Rust too.
        return set_style(**get_style(simple=True), family=False)
    if _is_shared_family(self) != _is_shared_family(vmobject):
        raise NotImplementedError("mixed object/family style matching requires shared family style state")
    source, context, suffix = _style_target(self)
    target, target_context, _ = _style_target(vmobject)
    if source is None or target is None:
        raise RuntimeError("style matching requires the shared Rust authoring host")
    context = context or target_context
    if context is None:
        engine_call(source.matchStyle, target)
    else:
        engine_call(getattr(context, f"liveMatch{suffix}Style"), source, target)
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
                engine_call(live_context.liveSetFill,
                    handle,
                    parsed.red,
                    parsed.green,
                    parsed.blue,
                    _compat._opacity("fill opacity", opacity),
                )
            elif color is not None:
                parsed = _compat._as_color("fill color", color)
                engine_call(live_context.liveSetFillColor,
                    handle, parsed.red, parsed.green, parsed.blue, parsed.alpha
                )
            if opacity is not None and color is None:
                engine_call(live_context.liveSetFillOpacity,
                    handle, _compat._opacity("fill opacity", opacity)
                )
        except Exception as error:
            raise_engine_error(error)
        return self
    if color is not None and opacity is not None:
        parsed = _compat._as_color("fill color", color)
        engine_call(handle.setFill,
            parsed.red,
            parsed.green,
            parsed.blue,
            _compat._opacity("fill opacity", opacity),
        )
        return self
    if color is not None:
        parsed = _compat._as_color("fill color", color)
        engine_call(handle.setFillColor, parsed.red, parsed.green, parsed.blue, parsed.alpha)
    if opacity is not None:
        engine_call(handle.setFillOpacity, _compat._opacity("fill opacity", opacity))
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
            return _set_style(self, stroke_color=color, stroke_width=width,
                              stroke_opacity=opacity, family=family)
        try:
            if color is not None and opacity is not None:
                parsed = _compat._as_color("stroke color", color)
                engine_call(live_context.liveSetStroke,
                    handle,
                    parsed.red,
                    parsed.green,
                    parsed.blue,
                    _compat._opacity("stroke opacity", opacity),
                )
            elif color is not None:
                parsed = _compat._as_color("stroke color", color)
                engine_call(live_context.liveSetStrokeColor,
                    handle, parsed.red, parsed.green, parsed.blue, parsed.alpha
                )
            elif opacity is not None:
                engine_call(live_context.liveSetStrokeOpacity,
                    handle, _compat._opacity("stroke opacity", opacity)
                )
        except Exception as error:
            raise_engine_error(error)
        return self
    if color is not None:
        parsed = _compat._as_color("stroke color", color)
        engine_call(handle.setStrokeColor, parsed.red, parsed.green, parsed.blue, parsed.alpha)
    if width is not None:
        engine_call(handle.setStrokeWidth, _compat._manim_stroke_width(width))
    if opacity is not None:
        engine_call(handle.setStrokeOpacity, _compat._opacity("stroke opacity", opacity))
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
            engine_call(live_context.liveSetOpacity, handle, _compat._opacity("opacity", opacity))
        except Exception as error:
            raise_engine_error(error)
        return self
    engine_call(handle.setOpacity, _compat._opacity("opacity", opacity))
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
            engine_call(live_context.liveSetObjectOpacity, handle, alpha)
        else:
            engine_call(handle.setObjectOpacity, alpha)
    except Exception as error:
        raise_engine_error(error)
    return self


def _paint_opacity_observation(value: object, layer: str):
    # The callback row is Rust-published state plus preceding ordered writes.
    # Reading it is a scalar projection, with no geometry or extra WASM call.
    from _manim_updaters import _canonical_row

    phase = _canonical_row(value)
    if phase is not None:
        paint = getattr(phase[2].style, layer)
        return 0.0 if paint is None else float(paint[3])
    family_handle = getattr(value, "_semantic_family_handle", None)
    if family_handle is not None:
        from _manim_updaters import _canonical_phase_context
        if _canonical_phase_context(value) is not None:
            raise NotImplementedError("family root paint getters require staged style observation")
        method = getattr(family_handle, f"own{layer.title()}Opacity", None)
        if method is not None:
            return engine_call(method)
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
    if not _is_shared_family(value):
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
    return engine_call(family_handle.layout)


def _group_paint(self, operation, arguments):
    handle = getattr(self, "_semantic_family_handle", None)
    if handle is None:
        raise RuntimeError("Group paint requires the shared Rust authoring host")
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    phase = _ACTIVE_CANONICAL_CONTEXT.get()
    if phase is not None:
        if operation == "OwnColor":
            raise NotImplementedError("callback root-only Group color requires shared family metadata writes")
        if operation in {"OwnFill", "OwnStroke", "OwnOpacity"}:
            raise NotImplementedError("callback root-only Group paint requires staged family style writes")
        phase.paint_family(handle, operation, arguments)
        return self
    context = _group_target_context(self)
    try:
        if context is None:
            engine_call(getattr(handle, f"set{operation}"), *arguments)
        else:
            engine_call(getattr(context, f"liveSetFamily{operation}"), handle, *arguments)
    except Exception as error:
        raise_engine_error(error)
    return self


def _family_color_arguments(color):
    if color is None:
        return (False, 0.0, 0.0, 0.0, 1.0)
    parsed = _compat._as_color("color", color)
    return (True, parsed.red, parsed.green, parsed.blue, parsed.alpha)


def _group_set_color(self, color, family=True):
    parsed = _compat._as_color("color", color)
    arguments = (parsed.red, parsed.green, parsed.blue, parsed.alpha)
    return _group_paint(self, "Color" if family else "OwnColor", arguments)


def _group_set_fill(self, color=None, opacity=None, family=True):
    alpha = None if opacity is None else _compat._opacity("fill opacity", opacity)
    operation = "Fill" if family else "OwnFill"
    return _group_paint(self, operation, (*_family_color_arguments(color), alpha))


def _group_set_stroke(self, color=None, width=None, opacity=None, family=True):
    stroke_width = None if width is None else _compat._manim_stroke_width(width)
    alpha = None if opacity is None else _compat._opacity("stroke opacity", opacity)
    operation = "Stroke" if family else "OwnStroke"
    return _group_paint(
        self, operation, (*_family_color_arguments(color), stroke_width, alpha)
    )


def _group_set_opacity(self, opacity, family=True):
    alpha = _compat._opacity("opacity", opacity)
    operation = "Opacity" if family else "OwnOpacity"
    return _group_paint(self, operation, (alpha,))


def _fade(self, darkness=0.5, family=True):
    if (
        not family
        and isinstance(self, _compat.Group)
        and not isinstance(self, _compat.VGroup)
    ):
        # Base Manim Mobject.fade only walks submobjects; with family=False a
        # plain Group therefore returns unchanged without reading darkness.
        return self
    darkness = _compat._opacity("darkness", darkness)
    if not family and isinstance(self, _compat.VGroup):
        from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT

        if _ACTIVE_CANONICAL_CONTEXT.get() is not None:
            raise NotImplementedError(
                "root-only VGroup fade is unavailable inside staged callbacks"
            )
        handle, context, _suffix = _style_target(self)
        if handle is None:
            raise RuntimeError("VGroup fade requires the shared Rust authoring host")
        if context is None:
            engine_call(handle.fadeOwn, darkness, operation="VMobject.fade")
        else:
            engine_call(
                context.liveFadeFamilyOwn,
                handle,
                darkness,
                operation="VMobject.fade",
            )
        return self
    if not family and _is_shared_family(self):
        raise NotImplementedError("non-recursive Group fade requires shared family style state")
    handle, context, suffix = _style_target(self)
    if handle is None:
        raise RuntimeError("Mobject fade requires the shared Rust authoring host")
    if context is None:
        engine_call(handle.fade, darkness, operation="Mobject.fade")
    else:
        operation = context.liveSetFamilyFade if suffix else context.liveFade
        engine_call(operation, handle, darkness, operation="Mobject.fade")
    return self


def _fade_to(self, color, alpha, family=True):
    if not family and _is_shared_family(self):
        return self
    target = tuple(_ir._finite_number(f"color.{name}", value) for name, value in zip(
        ("red", "green", "blue", "alpha"), color
    ))
    amount = _ir._finite_number("alpha", alpha)
    handle, context, suffix = _style_target(self)
    if handle is None:
        raise RuntimeError("Mobject.fade_to requires the shared Rust authoring host")
    if context is None:
        engine_call(handle.fadeTo, *target, amount, operation="Mobject.fade_to")
    else:
        operation = context.liveSetFamilyFadeTo if suffix else context.liveFadeTo
        engine_call(operation, handle, *target, amount, operation="Mobject.fade_to")
    return self


def _group_arrange_in_grid(self, rows=None, cols=None, buff=_base.MED_SMALL_BUFF,
                           cell_alignment=_base.ORIGIN, row_alignments=None,
                           col_alignments=None, row_heights=None, col_widths=None,
                           flow_order="rd"):
    import operator

    def dimension(value):
        if value is None:
            return None
        value = operator.index(value)
        if not 0 < value <= 0xFFFFFFFF:
            raise ValueError("grid dimensions must be positive 32-bit integers")
        return value

    rows, cols = engine_call(dimension, rows), engine_call(dimension, cols)
    gap = (_base._as_vec2(buff) if isinstance(buff, (tuple, list, _base.Vec2))
           else _base.Vec2(float(buff), float(buff)))
    handle = getattr(self, "_semantic_family_handle", None)
    if handle is None:
        raise RuntimeError("Group grid requires the shared Rust authoring host")
    options = engine_call(handle.gridOptions, rows, cols, gap.x, gap.y)
    alignment = _base._as_vec2(cell_alignment)
    engine_call(options.setAlignment, alignment.x, alignment.y, row_alignments, col_alignments)
    engine_call(options.setFlow, flow_order)
    engine_call(options.setSizeLists, row_heights is not None, col_widths is not None)
    for values, add in ((row_heights, options.addRowHeight), (col_widths, options.addColumnWidth)):
        if values is not None:
            for value in values:
                engine_call(add, None if value is None else float(value))
    context = _group_live_layout_context(self)
    try:
        if context is None:
            engine_call(handle.arrangeInGrid, options)
        else:
            engine_call(context.liveArrangeFamilyInGrid, handle, options)
    except Exception as error:
        raise_engine_error(error)
    return self


def _group_scale(self: _compat.Group, factor: object, *, about_point=None, about_edge=None) -> _compat.Group:
    return _scale_xyz(self, factor, about_point, about_edge)


def _group_rotate(self: _compat.Group, angle: float, axis: object = _compat.OUT,
                  *, about_point=None, about_edge=None, **kwargs) -> _compat.Group:
    if kwargs:
        raise NotImplementedError(f"unsupported Manim rotate option(s): {', '.join(sorted(kwargs))}")
    return _planar_affine(self, "rotate", (_compat._rotation_angle_2d(angle, axis),), about_point, about_edge)


def _group_shift(self: _compat.Group, direction: object) -> _compat.Group:
    from _manim_updaters import _ACTIVE_CANONICAL_CONTEXT
    phase = _ACTIVE_CANONICAL_CONTEXT.get()
    if phase is not None:
        handle = getattr(self, "_semantic_family_handle", None)
        if handle is None:
            raise RuntimeError("Group shift requires the shared Rust authoring host")
        phase.shift_family(handle, _base._as_vec2(direction))
        return self
    from _manim_path_editing import _raw_point3
    offset = _raw_point3(direction)
    arguments = offset if offset[2] != 0.0 else offset[:2]
    context = _group_target_context(self)
    if context is not None:
        try:
            engine_call(context.liveShiftFamily, self._semantic_family_handle, *arguments)
        except Exception as error:
            raise_engine_error(error)
        return self
    handle = getattr(self, "_semantic_family_handle", None)
    if handle is None:
        raise RuntimeError("Group shift requires the shared Rust authoring host")
    engine_call(handle.shift, *arguments)
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
    if _is_shared_family(target):
        method = getattr(context, f"live{operation}FamilyToFamily")
        engine_call(method, family, target._semantic_family_handle, *arguments)
    elif isinstance(target, _base.Mobject):
        method = getattr(context, f"live{operation}FamilyToMobject")
        engine_call(method, family, target._semantic_handle, *arguments)
    else:
        point = _base._as_vec2(target)
        method = getattr(context, f"live{operation}FamilyToPoint")
        engine_call(method, family, point.x, point.y, *arguments)


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
        raise RuntimeError("placement requires current shared Rust semantic handles")
    session = shared
    edge = _base._as_vec2(aligned_edge)
    mask = _alignment_mask2(coor_mask)

    if _is_shared_family(point_or_mobject):
        target = _shared_family_layout(point_or_mobject)
        if target is None:
            raise RuntimeError("placement target requires a current shared Rust semantic handle")
        engine_call(session.moveToFamily, target, edge.x, edge.y, mask.x, mask.y)
    elif _alignment_is_mobject(point_or_mobject):
        target = _family_layout_leaf_adapter(point_or_mobject)
        if target is None:
            raise RuntimeError("placement target requires a current shared Rust semantic handle")
        engine_call(session.moveToMobject, target, edge.x, edge.y, mask.x, mask.y)
    else:
        point = _base._as_vec2(point_or_mobject)
        engine_call(session.moveToPoint, point.x, point.y, edge.x, edge.y, mask.x, mask.y)

    return self




def _group_align_on_frame(self: _compat.Group, direction: _base.Vec2, buff: float):
    context = _group_live_layout_context(self)
    if context is not None:
        engine_call(context.liveAlignFamilyOnFrame, self._semantic_family_handle,
                                       direction.x, direction.y, float(buff))
    else:
        layout = _shared_family_layout(self, mutation=True)
        if layout is None:
            raise RuntimeError("frame alignment requires current shared Rust semantic handles")
        engine_call(layout.alignOnFrame, direction.x, direction.y, float(buff))
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
        raise RuntimeError("alignment requires current shared Rust semantic handles")
    session = shared
    axis = _base._as_vec2(direction)

    if _is_shared_family(mobject_or_point):
        target = _shared_family_layout(mobject_or_point)
        if target is None:
            raise RuntimeError("alignment target requires a current shared Rust semantic handle")
        engine_call(session.alignToFamily, target, axis.x, axis.y)
    elif _alignment_is_mobject(mobject_or_point):
        target = _family_layout_leaf_adapter(mobject_or_point)
        if target is None:
            raise RuntimeError("alignment target requires a current shared Rust semantic handle")
        engine_call(session.alignToMobject, target, axis.x, axis.y)
    else:
        point = _base._as_vec2(mobject_or_point)
        engine_call(session.alignToPoint, point.x, point.y, axis.x, axis.y)

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
        raise RuntimeError("arrange requires current shared Rust semantic handles")
    if not self.submobjects:
        return self
    unknown = set(kwargs) - {"aligned_edge", "coor_mask", "submobject_to_align", "index_of_submobject_to_align"}
    if unknown:
        raise TypeError(f"arrange got unexpected placement keyword {sorted(unknown)[0]!r}")
    axis = _base._as_vec2(_base.RIGHT if direction is None else direction)
    edge = _base._as_vec2(kwargs.get("aligned_edge", _base.ORIGIN))
    mask = _alignment_mask2(kwargs.get("coor_mask", (1, 1, 1)))
    index = _semantic_member_index(kwargs.get("index_of_submobject_to_align"))
    options = engine_call(family_handle.arrangeOptions, axis.x, axis.y, float(buff), bool(center),
                                           edge.x, edge.y, mask.x, mask.y, index)
    aligner = kwargs.get("submobject_to_align")
    if aligner is not None:
        anchor = _layout_anchor(aligner)
        if anchor is None:
            raise RuntimeError("arrange requires current shared Rust semantic handles")
        options.setAligner(anchor)
    context = _group_live_layout_context(self)
    try:
        if context is not None:
            engine_call(context.liveArrangeFamily, family_handle, options)
            return self
        leaves = _compat._leaf_mobjects(self)
        leaf_handles = [_family_layout_leaf_adapter(member, mutation=True) for member in leaves]
        if any(handle is None for handle in leaf_handles):
            raise RuntimeError("arrange requires current shared Rust semantic handles")
        engine_call(family_handle.arrange, options)
    except Exception as error:
        raise_engine_error(error)
    return self


def _group_layout_observation(value: _compat.Group):
    context = _group_live_layout_context(value)
    if context is not None:
        return engine_call(context.queryFamilyLayout, value._semantic_family_handle)
    layout = _shared_family_layout(value)
    if layout is None:
        raise RuntimeError("family layout requires a current shared Rust semantic handle")
    return layout


def _all_points_observation(value: _base.Mobject):
    if _is_shared_family(value):
        family = value._semantic_family_handle
        context = _group_live_layout_context(value)
        if context is not None:
            return engine_call(context.queryFamilyAllPoints3D, family)
        return engine_call(family.allPoints3D)
    return _typed_manim_observation(
        value,
        "allPoints3D",
        "queryMobjectAllPoints3D",
    )


def _point_array_observation(value: _base.Mobject):
    """Return shared coordinates and dimensions from one Rust observation."""
    return _point_query_observation(value, "PointArray")


def _point_reduction_tree_observation(value: _base.Mobject):
    """Return Rust-owned family nesting and per-object effective point arrays."""
    if not _is_shared_family(value):
        return _typed_manim_observation(
            value,
            "pointReductionTree",
            "queryMobjectPointReductionTree",
        )
    family = value._semantic_family_handle
    context = _group_live_layout_context(value)
    if context is not None:
        return engine_call(context.queryFamilyPointReductionTree, family)
    return engine_call(family.pointReductionTree)


def _family_point_count_observation(value: _base.Mobject) -> int | None:
    if not _is_shared_family(value):
        return None
    # Only root-owned points contribute; direct-child membership stays separate.
    return int(engine_call(value._semantic_family_handle.pointCount))


def _extremum_along_dim_observation(value: _base.Mobject, dim: int, key: float):
    if isinstance(value, _compat.VMobject) and not _is_shared_family(value):
        try:
            stride = operator.index(
                getattr(value, "n_points_per_cubic_curve", 4)
            )
        except TypeError:
            stride = None
        if stride != 4:
            from _manim_updaters import canonical_callback_phase_active

            if canonical_callback_phase_active():
                raise NotImplementedError(
                    "custom-stride VMobject extrema are unavailable during staged callbacks"
                )
            if dim == 2:
                return 0.0
            anchors = value.get_anchors()
            coordinates = [
                float(coordinate)
                for point in anchors
                for coordinate in (point[0], point[1])
            ]
            result = _extremum_from_points_observation(
                value, coordinates, 2, dim, key
            )
            if result is None:
                raise NotImplementedError(
                    "custom-stride VMobject extrema require a shared Rust handle"
                )
            return float(result)
        return _point_query_observation(
            value,
            "PathBoundaryExtremumAlongDim",
            dim,
            key,
        )
    if _is_shared_family(value):
        family = value._semantic_family_handle
        context = _group_live_layout_context(value)
        if context is not None:
            return engine_call(context.queryFamilyExtremumAlongDim, family, dim, key)
        return engine_call(family.extremumAlongDim, dim, key)

    semantic_handle = getattr(value, "_semantic_handle", None)
    if semantic_handle is None:
        return None
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        raise NotImplementedError("typed Mobject observation requires a valid semantic handle")
    if _is_bound(value):
        context = getattr(value._scene, "_canonical_authoring_context", None)
        query = getattr(context, "queryMobjectExtremumAlongDim", None)
        if query is None:
            raise NotImplementedError(
                "typed Mobject observation is unavailable in this authoring phase"
            )
        return engine_call(query, semantic_handle, dim, key)
    handle = _handle_for(value)
    if handle is None:
        raise NotImplementedError("typed Mobject observation is unavailable in this authoring phase")
    return engine_call(handle.extremumAlongDim, dim, key)


def _point_query_observation(value: _base.Mobject, operation: str, *arguments):
    handle_method = operation[0].lower() + operation[1:]
    if _is_shared_family(value):
        family = value._semantic_family_handle
        context = _group_live_layout_context(value)
        if context is not None:
            return engine_call(
                getattr(context, f"queryFamily{operation}"),
                family,
                *arguments,
            )
        return engine_call(getattr(family, handle_method), *arguments)

    semantic_handle = getattr(value, "_semantic_handle", None)
    if semantic_handle is None:
        return None
    if not bool(getattr(value, "_semantic_handle_fresh", False)):
        raise NotImplementedError("typed Mobject observation requires a valid semantic handle")
    if _is_bound(value):
        context = getattr(value._scene, "_canonical_authoring_context", None)
        query = getattr(context, f"queryMobject{operation}", None)
        if query is None:
            raise NotImplementedError(
                "typed Mobject observation is unavailable in this authoring phase"
            )
        return engine_call(query, semantic_handle, *arguments)
    handle = _handle_for(value)
    if handle is None:
        raise NotImplementedError("typed Mobject observation is unavailable in this authoring phase")
    return engine_call(getattr(handle, handle_method), *arguments)


def _center_of_mass_observation(value: _base.Mobject):
    return _point_query_observation(value, "CenterOfMass")


def _boundary_point_observation(
    value: _base.Mobject,
    direction_x: float,
    direction_y: float,
):
    if isinstance(value, _compat.VMobject) and not _is_shared_family(value):
        try:
            stride = operator.index(
                getattr(value, "n_points_per_cubic_curve", 4)
            )
        except TypeError:
            stride = None
        if stride != 4:
            from _manim_updaters import canonical_callback_phase_active

            if canonical_callback_phase_active():
                raise NotImplementedError(
                    "custom-stride VMobject boundary points are unavailable during staged callbacks"
                )
            anchors = value.get_anchors()
            coordinates = [
                float(coordinate)
                for point in anchors
                for coordinate in (point[0], point[1])
            ]
            result = _boundary_point_from_points_observation(
                value, coordinates, 2, direction_x, direction_y
            )
            if result is None:
                raise NotImplementedError(
                    "custom-stride VMobject boundary points require a shared Rust handle"
                )
            return result
        return _point_query_observation(
            value,
            "PathBoundaryPoint",
            direction_x,
            direction_y,
        )
    return _point_query_observation(
        value,
        "BoundaryPoint",
        direction_x,
        direction_y,
    )


def _critical_point_observation(value: _base.Mobject, direction, vector_boundary):
    return _point_query_observation(value, "CriticalPoint3D", *direction, vector_boundary)


def _vector_boundary_points_observation(value: _base.Mobject):
    return _point_query_observation(value, "VectorBoundaryPoints3D")


def _path_boundary_points_observation(value: _base.Mobject):
    return _point_query_observation(value, "PathBoundaryPoints3D")


def _path_boundary_extremum_along_dim_observation(
    value: _base.Mobject,
    dim: int,
    key: float,
):
    return _point_query_observation(
        value,
        "PathBoundaryExtremumAlongDim",
        dim,
        key,
    )


def _length_over_dim_observation(value: _base.Mobject, dim: int):
    return _point_query_observation(value, "LengthOverDim", dim)


def _extremum_from_points_observation(
    value: _base.Mobject,
    coordinates: list[float],
    point_dimensions: int,
    dim: int,
    key: float,
):
    if _is_shared_family(value):
        return engine_call(
            value._semantic_family_handle.extremumFromPoints,
            coordinates,
            point_dimensions,
            dim,
            key,
        )
    handle = getattr(value, "_semantic_handle", None)
    if handle is None or not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    return engine_call(
        handle.extremumFromPoints,
        coordinates,
        point_dimensions,
        dim,
        key,
    )


def _boundary_point_from_points_observation(
    value: _base.Mobject,
    coordinates: list[float],
    point_dimensions: int,
    direction_x: float,
    direction_y: float,
):
    if _is_shared_family(value):
        return None
    handle = getattr(value, "_semantic_handle", None)
    if handle is None or not bool(getattr(value, "_semantic_handle_fresh", False)):
        return None
    return engine_call(
        handle.boundaryPointFromPoints,
        coordinates,
        point_dimensions,
        direction_x,
        direction_y,
    )


def _is_shared_family(value: object) -> bool:
    return (
        isinstance(value, (_compat.Group, _compat.VGroup))
        and getattr(value, "_semantic_family_handle", None) is not None
    )


def _family_member_handle(value: object) -> tuple[str | None, object | None]:
    if _is_shared_family(value):
        return "family", value._semantic_family_handle
    if isinstance(value, _base.Mobject):
        return "mobject", getattr(value, "_semantic_handle", None)
    return None, None


def _validate_group_members(owner: _compat.Group, mobjects: tuple[object, ...]) -> None:
    if not isinstance(owner, (_compat.Group, _compat.VGroup)):
        owner._assert_valid_submobjects(mobjects)
        return
    for mobject in mobjects:
        if not isinstance(mobject, (_base.Mobject, _compat.Group)):
            raise TypeError("Group members must be Mobjects or Groups")
        if mobject is owner:
            raise ValueError("Group cannot contain itself")


def _family_membership_batch(context: object, kind: str, mobjects: tuple[object, ...]):
    batch = (
        engine_call(context.beginMembershipBatch, kind)
        if context is not None
        else engine_call(_new_membership_batch, kind)
    )
    for value in mobjects:
        member_kind, handle = _family_member_handle(value)
        if handle is None:
            raise RuntimeError("family member has no shared semantic identity")
        if member_kind == "family":
            engine_call(batch.appendFamily, handle)
        else:
            engine_call(batch.appendMobject, "", handle)
    return batch


def _family_wrapper_key(value: object) -> str:
    _, handle = _family_member_handle(value)
    if handle is None:
        raise RuntimeError("family member has no shared semantic identity")
    return _shared_handle_key(handle)


def _shared_handle_key(handle: object) -> str:
    return f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"


def _is_missing_handle(handle: object) -> bool:
    return handle is None or handle is _jsnull


def _attach_shared_family(wrapper, handle, context=None, leaf_type=None):
    if leaf_type is None:
        leaf_type = _compat.VMobject
    if wrapper is None:
        wrapper = object.__new__(_compat.Group)
    if context is None:
        context = getattr(wrapper, "_canonical_live_target_context", None)
    old_members = getattr(wrapper, "_semantic_member_wrappers", {})
    keys = list(engine_call(handle.memberKeys, operation="family.members"))
    members = {}
    for index, key in enumerate(keys):
        key = str(key)
        if bool(engine_call(handle.memberIsFamily, index, operation="family.members")):
            member_handle = engine_call(handle.memberFamily, index, operation="family.members")
            member = old_members.get(key)
            if not _is_shared_family(member):
                member = object.__new__(_compat.Group)
            _attach_shared_family(member, member_handle, context, leaf_type)
        else:
            member_handle = engine_call(handle.memberMobject, index, operation="family.members")
            member = old_members.get(key)
            if not isinstance(member, _base.Mobject):
                member = object.__new__(leaf_type)
            _attach_shared_handle(member, member_handle)
            if context is not None:
                member._canonical_live_target_context = context
        members[key] = member
    wrapper._semantic_family_handle = handle
    wrapper._semantic_member_wrappers = members
    if context is not None:
        wrapper._canonical_live_target_context = context
    return wrapper



def _group_members(self: _compat.Group) -> list[object]:
    # Ordering is observed from Rust only when requested. Mutations update this
    # identity registry locally; it is neither a membership nor an order cache.
    refresh = getattr(self, "_refresh_semantic_members", None)
    if refresh is not None:
        refresh()
    return [self._semantic_member_wrappers[str(key)]
            for key in engine_call(_family_query_handle(self).memberKeys)]


def _family_query_handle(root):
    handle = getattr(root, "_semantic_family_handle", None)
    if handle is None:
        handle = _handle_for(root)
    if handle is None:
        raise RuntimeError("member queries require a shared Rust semantic handle")
    return handle


class _FamilyWrapperReferences:
    """Host object references only; Rust selects membership and order."""
    __slots__ = ("wrappers", "readers")

    def __init__(self, wrappers):
        self.wrappers = wrappers
        self.readers = 0


def _retain_group_wrapper_references(root):
    if getattr(root, "_semantic_member_wrappers", None) is None:
        root._semantic_member_wrappers = {}
    scope = getattr(root, "_semantic_member_reference_scope", None)
    if scope is None or scope.wrappers is not root._semantic_member_wrappers:
        scope = _FamilyWrapperReferences(root._semantic_member_wrappers)
        root._semantic_member_reference_scope = scope
    scope.readers += 1
    return scope


def _fork_group_wrapper_references(root):
    if getattr(root, "_semantic_member_wrappers", None) is None:
        root._semantic_member_wrappers = {}
    scope = getattr(root, "_semantic_member_reference_scope", None)
    if scope is not None and scope.wrappers is root._semantic_member_wrappers and scope.readers:
        # A native list replacement has committed. Preserve original host
        # identities for retained old-list readers, without mirroring its order.
        root._semantic_member_wrappers = dict(root._semantic_member_wrappers)
        root._semantic_member_reference_scope = None


def _group_point_inclusion(root, predicate="has_points"):
    method = getattr(root, predicate)
    default = getattr(_base.Mobject, predicate)
    intrinsic = getattr(method, "__func__", None) is default
    if predicate == "has_no_points":
        intrinsic = intrinsic and getattr(root.has_points, "__func__", None) is _base.Mobject.has_points
    if intrinsic:
        return None
    value = bool(method())
    return not value if predicate == "has_no_points" else value


def _group_wrapper_for_key(root, key, references=None):
    if key == _family_wrapper_key(root):
        return root
    if references is not None and key in references:
        return references[key]
    try:
        return root._semantic_member_wrappers[key]
    except KeyError as error:
        raise RuntimeError("shared family identity has no Python wrapper") from error


def _free_family_query_handle(handle):
    free = getattr(handle, "free", None)
    if free is not None:
        free()


class _SharedFamilyIterator:
    """Python iterator protocol and wrapper identities over a Rust cursor."""
    __slots__ = ("_root", "_native", "_references")

    def __init__(self, root):
        self._root, self._native, self._references = root, None, None
        inclusion = _group_point_inclusion(root)
        refresh = getattr(root, "_refresh_semantic_members", None)
        if refresh is not None:
            refresh()
        self._native = engine_call(_family_query_handle(root).splitIterator, inclusion)
        self._references = _retain_group_wrapper_references(root)

    def __iter__(self):
        return self

    def __next__(self):
        if self._native is None:
            raise StopIteration
        try:
            selected = engine_call(self._native.nextMember)
            if _is_missing_handle(selected):
                self.close()
                raise StopIteration
            try:
                key = _shared_handle_key(selected)
            finally:
                _free_family_query_handle(selected)
            root = self._root
            references = self._references.wrappers
            if key not in references and key != _family_wrapper_key(root):
                refresh = getattr(root, "_refresh_semantic_members", None)
                if refresh is not None:
                    refresh()
            return _group_wrapper_for_key(root, key, references)
        except BaseException:
            self.close()
            raise

    def close(self):
        native, self._native = self._native, None
        scope, self._references = self._references, None
        self._root = None
        if scope is not None:
            scope.readers -= 1
        if native is not None:
            _free_family_query_handle(native)

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass


def _group_iter(root):
    return _SharedFamilyIterator(root)


def _group_length(root):
    return int(engine_call(_family_query_handle(root).splitCount, _group_point_inclusion(root)))


def _group_split(root):
    # CE split reads the intrinsic point array; unlike len/iteration/indexing,
    # it does not invoke an overridden has_points predicate.
    keys = engine_call(_family_query_handle(root).splitKeys, None)
    refresh = getattr(root, "_refresh_semantic_members", None)
    if refresh is not None:
        refresh()
    return [_group_wrapper_for_key(root, str(key)) for key in keys]


def _group_indexed_member(self: _compat.Group, index: object) -> object:
    # Python normalizes before invoking __index__; Rust then reads current
    # root inclusion and current direct-member rank after that callback.
    normalized = range(len(self))[index]
    selected = engine_call(_family_query_handle(self).splitAt, normalized,
                           _group_point_inclusion(self, "has_no_points"))
    if _is_missing_handle(selected):
        raise IndexError("list index out of range")
    try:
        key = _shared_handle_key(selected)
    finally:
        _free_family_query_handle(selected)
    if key == _family_wrapper_key(self):
        return self
    refresh = getattr(self, "_refresh_semantic_members", None)
    if refresh is not None:
        refresh()
    return _group_wrapper_for_key(self, key)


def _slice_operand(value):
    if value is None:
        return None
    try:
        return operator.index(value)
    except TypeError:
        # Preserve user protocol errors; match Python's slice diagnostic only
        # for a value with no index slot, without invoking it a second time.
        mro = type.__getattribute__(type(value), "__mro__")
        if not any("__index__" in type.__getattribute__(cls, "__dict__") for cls in mro):
            raise TypeError("slice indices must be integers or None or have an __index__ method") from None
        raise


def _group_slice(root, value):
    group_class = root.get_group_class()
    inclusion = _group_point_inclusion(root, "has_no_points")
    refresh = getattr(root, "_refresh_semantic_members", None)
    if refresh is not None:
        refresh()
    plan = engine_call(_family_query_handle(root).prepareSlice, inclusion)
    scope = None
    try:
        if bool(plan.includesSelf):
            normalized = value.indices(len(root))
        else:
            scope = _retain_group_wrapper_references(root)
            # List slicing coerces step/start/stop before adjusting bounds to
            # the captured list's length, which callbacks may have changed.
            step = _slice_operand(value.step)
            if step == 0:
                raise ValueError("slice step cannot be zero")
            start, stop = _slice_operand(value.start), _slice_operand(value.stop)
            normalized = slice(start, stop, step).indices(int(plan.normalizationCount))
        # Normalized ranks fit WASM's address space. A larger step has the same
        # selection as this exact JS integer bound; Python coercion stays exact.
        safe = (1 << 53) - 1
        operands = [float(max(-safe, min(safe, part))) for part in normalized]
        keys = engine_call(plan.selectNormalized, *operands)
        if _is_missing_handle(keys):
            raise IndexError("list index out of range")
        if refresh is not None:
            refresh()
        references = None if scope is None else scope.wrappers
        members = [_group_wrapper_for_key(root, str(key), references) for key in keys]
    finally:
        if scope is not None:
            scope.readers -= 1
        _free_family_query_handle(plan)
    return group_class(*members)


def _get_family(self: _base.Mobject) -> list[_base.Mobject]:
    from _manim_updaters import canonical_callback_phase_active
    if canonical_callback_phase_active():
        raise NotImplementedError("get_family during staged callback membership is not supported")
    # Rust selects the membership and order. This walk only resolves returned
    # semantic identities to their existing host wrappers, preserving aliases.
    _, handle = _family_member_handle(self)
    if handle is None:
        raise RuntimeError("get_family requires a shared Rust semantic handle")
    keys = [str(key) for key in engine_call(handle.familyKeys)]
    selected = set(keys)
    wrappers = {}
    pending = [self]
    while pending:
        wrapper = pending.pop()
        key = _family_wrapper_key(wrapper)
        if key not in selected or key in wrappers:
            continue
        wrappers[key] = wrapper
        if _is_shared_family(wrapper):
            pending.extend(wrapper.submobjects)
        else:
            pending.extend(member for member_key, member in
                           getattr(wrapper, "_semantic_member_wrappers", {}).items()
                           if str(member_key) in selected)
    try:
        return [wrappers[key] for key in keys]
    except KeyError as error:
        raise RuntimeError("shared family identity has no Python wrapper") from error


def _group_init(self: _compat.Group, *mobjects: object, z_index: float = 0, color=_base.WHITE) -> None:
    if _create_family_handle is None or _new_membership_batch is None:
        raise RuntimeError("Group construction requires the shared Rust authoring host")
    z_index = _ir._finite_number("z_index", z_index)
    parsed = _base.BLACK if color is None else _compat._as_color("color", color)
    arguments = () if parsed == _base.WHITE else (parsed.red, parsed.green, parsed.blue, parsed.alpha)
    draw_enabled = isinstance(self, _compat.VMobject)
    _validate_group_members(self, mobjects)
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", mobjects)
    family = (
        engine_call(context.liveCreateFamilyWithColor if arguments else context.liveCreateFamily,
                    batch, z_index, *arguments, draw_enabled)
        if context is not None
        else engine_call(_create_family_handle, batch, z_index, draw_enabled, *arguments)
    )
    self._semantic_family_handle = family
    self._semantic_member_wrappers = {_family_wrapper_key(value): value for value in mobjects}
    if context is not None:
        self._canonical_live_target_context = context


def _membership_call(root, context, family_method, object_method, direct_method, *args, **kwargs):
    handle = _family_query_handle(root)
    if context is None:
        return engine_call(getattr(handle, direct_method), *args, **kwargs)
    method = family_method if _is_shared_family(root) else object_method
    return engine_call(getattr(context, method), handle, *args, **kwargs)


def _replace_submobjects(root, members):
    members = tuple(members)
    _validate_group_members(root, members)
    keys = [_family_wrapper_key(member) for member in members]
    if len(set(keys)) != len(keys):
        raise NotImplementedError("repeated direct child identities require a shared occurrence model")
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", members)
    _membership_call(root, context, "liveReplaceFamilyMembers", "liveReplaceObjectMembers", "replaceMembers", batch)
    _fork_group_wrapper_references(root)
    root._semantic_member_wrappers = dict(zip(keys, members))


def _group_add(self: _compat.Group, *mobjects: object) -> _compat.Group:
    _validate_group_members(self, mobjects)
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", mobjects)
    _membership_call(self, context, "liveEditFamilyMembership", "liveEditObjectMembership", "editMembership", batch)
    _fork_group_wrapper_references(self)
    for value in mobjects:
        self._semantic_member_wrappers[_family_wrapper_key(value)] = value
    return self


def _group_insert(self: _compat.Group, index: int, mobject: object) -> _compat.Group:
    _validate_group_members(self, (mobject,))

    # Let Python's own list implementation apply __index__, negative-index
    # adjustment, clamping, and overflow behavior exactly as Manim does.
    current = list(self.submobjects)
    probe = list(current)
    marker = object()
    probe.insert(index, marker)
    position = probe.index(marker)

    key = _family_wrapper_key(mobject)
    if any(_family_wrapper_key(member) == key for member in current):
        raise NotImplementedError(
            "Mobject.insert cannot repeat a direct member identity because Noon "
            "semantic families store each child identity once"
        )

    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", (mobject,))
    _membership_call(self, context, "liveInsertFamilyMember", "liveInsertObjectMember", "insertMember", position, batch, operation="Mobject.insert")
    if getattr(self, "_semantic_member_wrappers", None) is None:
        self._semantic_member_wrappers = {}
    self._semantic_member_wrappers[key] = mobject
    return self


def _group_add_to_back(self: _compat.Group, *mobjects: object) -> _compat.Group:
    _validate_group_members(self, mobjects)
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", mobjects)
    _membership_call(self, context, "livePrependFamilyMembers", "livePrependObjectMembers", "prependMembers", batch, operation="Mobject.add_to_back")
    _fork_group_wrapper_references(self)
    for value in mobjects:
        self._semantic_member_wrappers[_family_wrapper_key(value)] = value
    return self


def _group_add_n_more_submobjects(
    self: _compat.Group, n: int
) -> _compat.Group | None:
    if n == 0:
        return None
    additions = operator.index(n)
    current = tuple(self.submobjects)
    current_count = len(current)
    if current_count == 0:
        if additions <= 0:
            return None
        ordered = tuple(self.get_point_mobject() for _ in range(additions))
        empty_result = True
    else:
        target_count = current_count + additions
        if target_count <= 0:
            return self
        repeat_indices = [
            (index * current_count) // target_count
            for index in range(target_count)
        ]
        split_factors = [
            repeat_indices.count(index) for index in range(current_count)
        ]
        expanded = []
        for member, split_factor in zip(current, split_factors, strict=True):
            expanded.append(member)
            for _ in range(1, split_factor):
                repeated = self.repeat_submobject(member)
                repeated.fade(1)
                expanded.append(repeated)
        ordered = tuple(expanded)
        empty_result = False
        if len(ordered) == current_count and all(
            before is after for before, after in zip(current, ordered, strict=True)
        ):
            return self

    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "add", ordered)
    family_handle = self._semantic_family_handle
    if context is not None:
        engine_call(
            context.livePrependFamilyMembers,
            family_handle,
            batch,
            operation="Mobject.add_n_more_submobjects",
        )
    else:
        engine_call(
            family_handle.prependMembers,
            batch,
            operation="Mobject.add_n_more_submobjects",
        )
    _fork_group_wrapper_references(self)
    for member in ordered:
        self._semantic_member_wrappers[_family_wrapper_key(member)] = member
    return None if empty_result else self


def _group_invert(self: _compat.Group, recursive: bool = False) -> _compat.Group:
    family_handle = self._semantic_family_handle
    context = _live_constructor_context("family")
    recursive = bool(recursive)
    if context is not None:
        engine_call(
            context.liveInvertFamilyMembers,
            family_handle,
            recursive,
            operation="Mobject.invert",
        )
    else:
        engine_call(
            family_handle.invertMembers,
            recursive,
            operation="Mobject.invert",
        )
    return self


def _group_sort(
    self: _compat.Group,
    point_to_num_func: Callable[[Any], float] = lambda point: point[0],
    submob_func: Callable[[_compat.Mobject], Any] | None = None,
) -> _compat.Group:
    context = _live_constructor_context("family")
    members = tuple(self.submobjects)
    key = submob_func
    if key is None:
        key = lambda member: point_to_num_func(member.get_center())
    ordered = tuple(sorted(members, key=key))
    if all(before is after for before, after in zip(members, ordered)):
        return self
    batch = _family_membership_batch(context, "add", ordered)
    family_handle = self._semantic_family_handle
    if context is not None:
        engine_call(context.livePrependFamilyMembers, family_handle, batch)
    else:
        engine_call(family_handle.prependMembers, batch)
    return self


def _group_shuffle(self: _compat.Group, recursive: bool = False) -> _compat.Group:
    import random

    if recursive:
        raise NotImplementedError(
            "recursive family shuffle is not supported until Rust owns its family traversal"
        )
    context = _live_constructor_context("family")
    members = list(self.submobjects)
    random.shuffle(members)
    if all(before is after for before, after in zip(self.submobjects, members)):
        return self
    batch = _family_membership_batch(context, "add", tuple(members))
    family_handle = self._semantic_family_handle
    if context is not None:
        engine_call(context.livePrependFamilyMembers, family_handle, batch)
    else:
        engine_call(family_handle.prependMembers, batch)
    return self


def _group_remove(self: _compat.Group, *mobjects: object) -> _compat.Group:
    if not mobjects:
        return self
    context = _live_constructor_context("family")
    batch = _family_membership_batch(context, "remove", mobjects)
    changed = _membership_call(self, context, "liveEditFamilyMembership", "liveEditObjectMembership", "editMembership", batch)
    for value, removed in zip(mobjects, changed):
        if removed:
            self._semantic_member_wrappers.pop(_family_wrapper_key(value), None)
    return self


def _group_target_context(value: object) -> object | None:
    contexts: list[object] = []
    seen: set[int] = set()

    def collect(member: object) -> None:
        if id(member) in seen:
            return
        seen.add(id(member))
        context = _live_mutation_context(member)
        if context is not None:
            contexts.append(context)
        if _is_shared_family(member):
            for child in member.submobjects:
                collect(child)
            return

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


def _group_copy_operation(
    self: _base.Mobject, *, cyclic_replace: bool, promote_own_points: bool = False
) -> _base.Mobject:
    context = _group_target_context(self)

    def excluded_fields(value):
        excluded = {
            "_raw", "_scene", "_object", "_semantic_handle", "_semantic_handle_fresh",
            "_semantic_family_handle", "_semantic_member_wrappers", "_canonical_live_target_context",
            # Reader ownership belongs to the source wrapper, not its copy.
            "_semantic_member_reference_scope",
            "_table_query_wrappers",
            "_sample_space_handle", "_brace_label_handle", "_matrix_handle", "_table_handle", "_code_handle",
            "_bar_chart_handle", "_bar_chart_context", "_semantic_latex_handle", "_numeric_handle",
            # Arrow and ArrowVectorField keep this aggregate JS capability only for
            # convenience queries and dependent edits. Family copying already maps
            # the authoritative family and every leaf below; there is no valid
            # Python deepcopy or aggregate-handle alias for the target family.
            "_semantic_arrow_handle",
            "_noon_updater_registrations", "_noon_updater_registration_history",
        }
        if not _is_shared_family(value) and _is_bound(value) and hasattr(value, "_noon_updaters"):
            excluded.add("_noon_updaters")
        return excluded

    clone, pairs, family_members = _compat.prepare_family_wrapper_copy(self, excluded_fields)
    references = _family_membership_batch(context, "add", tuple(source for source, _ in pairs))
    if promote_own_points:
        copied = _membership_call(
            self, context, "livePromoteFamilyOwnPoints", "livePromoteMobjectOwnPoints",
            "promoteOwnPoints", references,
        )
    elif cyclic_replace:
        copied = (
            engine_call(context.liveCyclicReplaceTarget, self._semantic_family_handle, references)
            if context is not None
            else engine_call(self._semantic_family_handle.cyclicReplaceTarget, references)
        )
    else:
        copied = (
            _membership_call(self, context, "liveCopyFamily", "liveCopyMobjectFamily", "copyFamily", references)
        )
    for source, target in pairs:
        if _is_shared_family(source):
            target._semantic_family_handle = engine_call(copied.familyFor, source._semantic_family_handle)
            owner = context or getattr(source, "_canonical_live_target_context", None)
            if owner is not None:
                target._canonical_live_target_context = owner
        else:
            _initialize_shared_wrapper(target)
            target._semantic_handle = engine_call(copied.mobjectFor, source._semantic_handle)
            target._semantic_handle_fresh = True
            owner = context or getattr(source, "_canonical_live_target_context", None)
            if owner is not None:
                target._canonical_live_target_context = owner
    for target, members in family_members:
        if promote_own_points and target is clone:
            members = ()
        target._semantic_member_wrappers = {_family_wrapper_key(member): member for member in members}
    if promote_own_points:
        _fork_group_wrapper_references(self)
        self._semantic_member_wrappers[_family_wrapper_key(clone)] = clone
    for source, target in pairs:
        latex = getattr(source, "_semantic_latex_handle", None)
        if latex is not None:
            target._semantic_latex_handle = engine_call(latex.rebindFamily, target._semantic_family_handle)
            target._part_views()
        if not _is_shared_family(target):
            rebind = getattr(target, "_rebind_copied_semantic_handle", None)
            if rebind is not None:
                rebind()
        aggregate = getattr(source, "_semantic_arrow_handle", None)
        if aggregate is not None:
            index = getattr(source, "_semantic_arrow_index", None)
            if index is None:
                target._semantic_arrow_handle = engine_call(copied.arrowFor, aggregate)
            else:
                target._semantic_arrow_handle = engine_call(copied.arrowFor, aggregate, index)
                target.__dict__.pop("_semantic_arrow_index", None)
    for source, target in pairs:
        if _is_shared_family(source):
            rehydrate = getattr(target, "_rehydrate_semantic_family_handle", None)
            if rehydrate is not None:
                rehydrate()
    return clone


def _group_copy(self: _compat.Group) -> _compat.Group:
    return _group_copy_operation(self, cyclic_replace=False)


def _group_cyclic_replace_target(self: _compat.Group) -> _compat.Group:
    return _group_copy_operation(self, cyclic_replace=True)
