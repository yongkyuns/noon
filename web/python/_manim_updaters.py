"""Manim-style callback ergonomics over the canonical callback-phase contract.

Python owns callable identity and invocation. Rust owns semantic registration,
activation ordering, the staged effective snapshot, and publication. Callbacks
return one property-only effective batch to their existing session.
"""

from __future__ import annotations

import inspect
import json
import math
from contextvars import ContextVar
from dataclasses import dataclass, replace
from typing import Any, Callable

import noon as _base

_INSTALLED = False
_NEXT_SESSION_ID = 0
_TRACKED_MOBJECTS: list[_base.Mobject] = []
_CANONICAL_SESSIONS: dict[int, "_CanonicalCallbackSession"] = {}
_ACTIVE_CONTEXTS: dict[int, Any] = {}
_ACTIVE_CANONICAL_CONTEXT: ContextVar["_CanonicalCallbackContext | None"] = ContextVar(
    "noon_active_canonical_callback", default=None
)

_ORIGINAL_CURRENT_RAW = _base.Mobject._current_raw
_ORIGINAL_APPLY = _base.Mobject._apply
_ORIGINAL_GET_CENTER = _base.Mobject.get_center
_ORIGINAL_SHIFT = _base.Mobject.shift
_ORIGINAL_MOVE_TO = _base.Mobject.move_to
_ORIGINAL_SET_X = _base.Mobject.set_x
_ORIGINAL_SET_Y = _base.Mobject.set_y
_ORIGINAL_SCALE = _base.Mobject.scale
_ORIGINAL_ROTATE = _base.Mobject.rotate
_ORIGINAL_SET_COLOR = _base.Mobject.set_color
_ORIGINAL_SET_FILL = _base.Mobject.set_fill
_ORIGINAL_SET_STROKE = _base.Mobject.set_stroke
_ORIGINAL_SET_OPACITY = _base.Mobject.set_opacity
_ORIGINAL_VMOBJECT_SET_COLOR: Callable[..., _base.Mobject] | None = None
_ORIGINAL_VMOBJECT_SET_FILL: Callable[..., _base.Mobject] | None = None
_ORIGINAL_VMOBJECT_SET_STROKE: Callable[..., _base.Mobject] | None = None
_ORIGINAL_VMOBJECT_SET_OPACITY: Callable[..., _base.Mobject] | None = None


def _track(mobject: _base.Mobject) -> None:
    if not any(existing is mobject for existing in _TRACKED_MOBJECTS):
        _TRACKED_MOBJECTS.append(mobject)


@dataclass(slots=True)
class _UpdaterRegistration:
    mobject: _base.Mobject
    callback: Callable[..., Any]
    active_after: float | None
    position: int | None = None
    active_through: float | None = None
    callback_id: int | None = None
    canonical_registered: bool = False


def _updaters(mobject: _base.Mobject) -> list[Callable[..., Any]]:
    value = getattr(mobject, "_noon_updaters", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updaters", value)
    return value


def _registrations(mobject: _base.Mobject) -> list[_UpdaterRegistration]:
    """Active updater occurrences, kept index-aligned with ``_updaters``."""
    value = getattr(mobject, "_noon_updater_registrations", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updater_registrations", value)
    return value


def _registration_history(mobject: _base.Mobject) -> list[_UpdaterRegistration]:
    """All authored updater intervals, including registrations later removed."""
    value = getattr(mobject, "_noon_updater_registration_history", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updater_registration_history", value)
    return value


def _scene_time(mobject: _base.Mobject) -> float | None:
    scene = getattr(mobject, "_scene", None)
    if scene is None:
        return None
    return float(scene.time)


def _registration_end_time(
    mobject: _base.Mobject, registration: _UpdaterRegistration
) -> float:
    scene_time = _scene_time(mobject)
    if scene_time is not None:
        return scene_time
    if registration.active_after is not None:
        return registration.active_after
    return 0.0


def _canonical_context(mobject: _base.Mobject) -> object | None:
    scene = getattr(mobject, "_scene", None)
    if scene is None or getattr(scene, "_legacy_geometry_materialized", False):
        return None
    return getattr(scene, "_canonical_authoring_context", None)


def _semantic_key(mobject: _base.Mobject) -> tuple[int, int]:
    if getattr(mobject, "_semantic_family_handle", None) is not None:
        raise NotImplementedError(
            "canonical callbacks on Group/VGroup families are not supported yet; "
            "#70 owns shared family property operations"
        )
    handle = getattr(mobject, "_semantic_handle", None)
    if handle is None:
        raise RuntimeError("canonical callback target requires a typed semantic Mobject")
    try:
        return (int(handle.semanticSlot), int(handle.semanticGeneration))
    except (AttributeError, TypeError, ValueError) as error:
        raise RuntimeError(
            "canonical callback target has no generational semantic identity"
        ) from error


@dataclass(slots=True)
class _CanonicalCallbackSession:
    """Host-only callable table for one canonical authoring context.

    IDs are scoped to this table and never stand in for semantic identity. Rust
    receives each ID only as an opaque callable lookup key; target identity and
    occurrence order remain part of the compiler-owned callback plan.
    """

    scene: _base.Scene
    context: object
    session_id: int
    callbacks: dict[int, Callable[..., Any]]
    callback_ids: list[tuple[Callable[..., Any], int]]
    targets: dict[tuple[int, int], _base.Mobject]
    next_callback_id: int = 0

    def callback_id(self, callback: Callable[..., Any]) -> tuple[int, bool]:
        for existing, callback_id in self.callback_ids:
            if existing is callback:
                return callback_id, False
        return self.next_callback_id, True

    def commit_callback_id(self, callback: Callable[..., Any], callback_id: int) -> None:
        if callback_id != self.next_callback_id:
            raise RuntimeError("canonical callback ID reservation was not current")
        self.callback_ids.append((callback, callback_id))
        self.callbacks[callback_id] = callback
        self.next_callback_id += 1

    def bind_target(self, mobject: _base.Mobject) -> tuple[int, int]:
        key = _semantic_key(mobject)
        self.targets[key] = mobject
        return key


def _canonical_session(scene: _base.Scene, context: object) -> _CanonicalCallbackSession:
    global _NEXT_SESSION_ID

    existing = getattr(scene, "_noon_canonical_callback_session", None)
    if existing is not None:
        if existing.context is not context:
            raise RuntimeError("canonical callback session context changed during authoring")
        return existing
    session = _CanonicalCallbackSession(
        scene=scene,
        context=context,
        session_id=_NEXT_SESSION_ID,
        callbacks={},
        callback_ids=[],
        targets={},
    )
    _NEXT_SESSION_ID += 1
    _CANONICAL_SESSIONS[session.session_id] = session
    scene._noon_canonical_callback_session = session
    return session


def _register_canonical_occurrence(
    session: _CanonicalCallbackSession,
    registration: _UpdaterRegistration,
    *,
    position: int | None,
) -> None:
    """Publish a prevalidated host callable occurrence before Python bookkeeping.

    The context operation is a shared semantic transaction. Keeping the Python
    list update after it succeeds leaves failed registration invisible to the
    wrapper and avoids a partial callback table.
    """

    target = registration.mobject
    # Family traversal and group-layout mutation must be one shared semantic
    # operation. The property overlay currently addresses ordinary Mobjects only,
    # so reject this before it can publish a partial occurrence.
    _semantic_key(target)
    handle = getattr(target, "_semantic_handle", None)
    if handle is None:
        raise RuntimeError("canonical callback target requires a typed semantic Mobject")
    callback_id, newly_reserved = session.callback_id(registration.callback)
    active_from = 0.0 if registration.active_after is None else registration.active_after
    session.context.addUpdater(handle, str(callback_id), active_from, position)
    if newly_reserved:
        session.commit_callback_id(registration.callback, callback_id)
    registration.callback_id = callback_id
    registration.canonical_registered = True
    session.bind_target(target)


def _register_canonical_removal(
    session: _CanonicalCallbackSession,
    registration: _UpdaterRegistration,
    inactive_from: float,
) -> None:
    if registration.callback_id is None:
        raise RuntimeError("canonical updater removal has no callback ID")
    handle = getattr(registration.mobject, "_semantic_handle", None)
    if handle is None:
        raise RuntimeError("canonical callback target requires a typed semantic Mobject")
    session.context.removeUpdater(handle, str(registration.callback_id), inactive_from)


def prepare_canonical_callbacks(scene: _base.Scene, context: object) -> int | None:
    """Replay Python-authored callback occurrences into the one semantic store.

    Detached registrations are authored at time zero and become semantic only
    once their Mobject is bound. Replaying the historical intervals here is a
    bootstrap operation before the session is created; it never creates a second
    scheduler or callback-plan representation.
    """

    history: list[_UpdaterRegistration] = []
    for mobject in _TRACKED_MOBJECTS:
        if mobject._scene is scene:
            history.extend(_registration_history(mobject))
    if not history:
        return None

    session = _canonical_session(scene, context)
    for registration in history:
        if registration.canonical_registered:
            continue
        _register_canonical_occurrence(session, registration, position=registration.position)
        if registration.active_through is not None:
            _register_canonical_removal(
                session, registration, registration.active_through
            )
    return session.session_id


def canonical_callback_session_id(scene: _base.Scene) -> int | None:
    session = getattr(scene, "_noon_canonical_callback_session", None)
    return None if session is None else session.session_id


def add_updater(
    self: _base.Mobject,
    update_function: Callable[..., Any],
    index: int | None = None,
    call_updater: bool = False,
) -> _base.Mobject:
    if not callable(update_function):
        raise TypeError("updater must be callable")
    if call_updater and getattr(self, "_semantic_handle", None) is not None:
        raise NotImplementedError(
            "call_updater=True is not supported for canonical callbacks until "
            "immediate invocation has one atomic shared-semantic operation"
        )
    callbacks = _updaters(self)
    registrations = _registrations(self)
    registration = _UpdaterRegistration(
        mobject=self,
        callback=update_function,
        active_after=_scene_time(self),
    )
    position: int | None
    if index is None:
        position = None
    else:
        if isinstance(index, bool) or not isinstance(index, int):
            raise TypeError("updater index must be an integer")
        # Match Python list.insert's observable active-list position before Rust
        # validates the equivalent compiler-owned occurrence insertion.
        position = min(max(index, 0), len(callbacks))
    registration.position = position

    context = _canonical_context(self)
    if context is not None:
        # A detached updater may have become scene-bound before this operation.
        # Flush every earlier authored occurrence first so the active-list index
        # is interpreted against the same semantic order Python exposes.
        prepare_canonical_callbacks(self._scene, context)
        session = _canonical_session(self._scene, context)
        _register_canonical_occurrence(session, registration, position=position)

    if position is None:
        callbacks.append(update_function)
        registrations.append(registration)
    else:
        callbacks.insert(position, update_function)
        registrations.insert(position, registration)
    _registration_history(self).append(registration)
    _track(self)
    if call_updater:
        _invoke(update_function, self, 0.0)
    return self


def remove_updater(
    self: _base.Mobject, update_function: Callable[..., Any]
) -> _base.Mobject:
    callbacks = _updaters(self)
    registrations = _registrations(self)
    for index, callback in enumerate(callbacks):
        if callback is update_function:
            registration = registrations[index]
            inactive_from = _registration_end_time(self, registration)
            context = _canonical_context(self)
            if context is not None:
                prepare_canonical_callbacks(self._scene, context)
                session = _canonical_session(self._scene, context)
                _register_canonical_removal(session, registration, inactive_from)
            del callbacks[index]
            registrations.pop(index)
            registration.active_through = inactive_from
            break
    return self


def clear_updaters(self: _base.Mobject, recursive: bool = True) -> _base.Mobject:
    # Noon has no persisted runtime hierarchy, but Group/VGroup recurse in their own
    # Python wrappers. The flag is accepted for Manim source compatibility.
    del recursive
    registrations = _registrations(self)
    inactive_from = [_registration_end_time(self, registration) for registration in registrations]
    context = _canonical_context(self)
    if context is not None and registrations:
        prepare_canonical_callbacks(self._scene, context)
        session = _canonical_session(self._scene, context)
        _semantic_key(self)
        handle = getattr(self, "_semantic_handle", None)
        if handle is None:
            raise RuntimeError("canonical callback target requires a typed semantic Mobject")
        # The shared transaction validates all open occurrences before commit.
        # Every active Python registration has the same scene authored time.
        if any(time != inactive_from[0] for time in inactive_from):
            raise RuntimeError("canonical updater clear has inconsistent authored times")
        session.context.clearUpdaters(handle, inactive_from[0])
    for registration, end_time in zip(registrations, inactive_from, strict=True):
        registration.active_through = end_time
    _updaters(self).clear()
    registrations.clear()
    return self


def get_updaters(self: _base.Mobject) -> list[Callable[..., Any]]:
    return list(_updaters(self))


def has_updaters(self: _base.Mobject) -> bool:
    return bool(_updaters(self))


def _invoke(callback: Callable[..., Any], mobject: _base.Mobject, dt: float) -> None:
    try:
        signature = inspect.signature(callback)
    except (TypeError, ValueError):
        callback(mobject, dt)
        return

    positional = [
        parameter
        for parameter in signature.parameters.values()
        if parameter.kind
        in (inspect.Parameter.POSITIONAL_ONLY, inspect.Parameter.POSITIONAL_OR_KEYWORD)
    ]
    accepts_varargs = any(
        parameter.kind is inspect.Parameter.VAR_POSITIONAL
        for parameter in signature.parameters.values()
    )
    if accepts_varargs or len(positional) >= 2:
        callback(mobject, dt)
    else:
        callback(mobject)


@dataclass(frozen=True, slots=True)
class _PhaseTransform:
    translation_x: float
    translation_y: float
    rotation: float
    scale_x: float
    scale_y: float

    @classmethod
    def from_wire(cls, value: object) -> "_PhaseTransform":
        if not isinstance(value, dict):
            raise TypeError("canonical callback transform must be an object")
        translation = value.get("translation")
        scale = value.get("scale")
        if not isinstance(translation, dict) or not isinstance(scale, dict):
            raise TypeError("canonical callback transform is malformed")
        return cls(
            _phase_number("transform.translation.x", translation.get("x")),
            _phase_number("transform.translation.y", translation.get("y")),
            _phase_number("transform.rotation", value.get("rotation")),
            _phase_number("transform.scale.x", scale.get("x")),
            _phase_number("transform.scale.y", scale.get("y")),
        )

    def to_wire(self) -> dict[str, object]:
        return {
            "translation": {"x": self.translation_x, "y": self.translation_y},
            "rotation": self.rotation,
            "scale": {"x": self.scale_x, "y": self.scale_y},
        }


_DEFAULT_STROKE_WIDTH_MODE = "scale_with_object"


@dataclass(frozen=True, slots=True)
class _PhaseStyle:
    fill: tuple[float, float, float, float] | None
    stroke: tuple[float, float, float, float] | None
    stroke_width: float
    stroke_width_mode: str
    stroke_join: str
    stroke_cap: str
    opacity: float

    @classmethod
    def from_wire(cls, value: object) -> "_PhaseStyle":
        if not isinstance(value, dict):
            raise TypeError("canonical callback style must be an object")
        stroke_width_mode = value.get(
            "stroke_width_mode", _DEFAULT_STROKE_WIDTH_MODE
        )
        string_fields = {
            "stroke_width_mode": stroke_width_mode,
            "stroke_join": value.get("stroke_join"),
            "stroke_cap": value.get("stroke_cap"),
        }
        for key, field in string_fields.items():
            if not isinstance(field, str):
                raise TypeError(f"canonical callback style.{key} must be a string")
        return cls(
            _phase_color("style.fill", value.get("fill")),
            _phase_color("style.stroke", value.get("stroke")),
            _phase_number("style.stroke_width", value.get("stroke_width")),
            stroke_width_mode,
            value["stroke_join"],
            value["stroke_cap"],
            _phase_number("style.opacity", value.get("opacity")),
        )

    def to_wire(self) -> dict[str, object]:
        return {
            "fill": _phase_color_wire(self.fill),
            "stroke": _phase_color_wire(self.stroke),
            "stroke_width": self.stroke_width,
            "stroke_width_mode": self.stroke_width_mode,
            "stroke_join": self.stroke_join,
            "stroke_cap": self.stroke_cap,
            "opacity": self.opacity,
        }


@dataclass(slots=True)
class _PhasePropertyRow:
    """Small callback-boundary row, never an authored scene object or raw geometry."""

    transform: _PhaseTransform
    style: _PhaseStyle
    bounds: tuple[float, float, float, float] | None
    bounds_translation_only: bool

    @classmethod
    def from_wire(cls, item: dict[str, Any]) -> "_PhasePropertyRow":
        bounds = _phase_bounds(item.get("bounds"))
        return cls(
            _PhaseTransform.from_wire(item.get("transform")),
            _PhaseStyle.from_wire(item.get("style")),
            bounds,
            bounds is not None,
        )

    def center(self) -> _base.Vec2:
        bounds = self.require_bounds()
        return _base.Vec2((bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0)

    def require_bounds(self) -> tuple[float, float, float, float]:
        if self.bounds is None:
            raise NotImplementedError(
                "canonical callback bounds require a Rust-published effective bound"
            )
        if not self.bounds_translation_only:
            raise NotImplementedError(
                "canonical callback bounds are unavailable after a spatial property change"
            )
        return self.bounds

    def shift(self, offset: _base.Vec2) -> None:
        self.transform = replace(
            self.transform,
            translation_x=self.transform.translation_x + offset.x,
            translation_y=self.transform.translation_y + offset.y,
        )
        if self.bounds is not None and self.bounds_translation_only:
            min_x, min_y, max_x, max_y = self.bounds
            self.bounds = (
                min_x + offset.x,
                min_y + offset.y,
                max_x + offset.x,
                max_y + offset.y,
            )

    def invalidate_bounds(self) -> None:
        self.bounds_translation_only = False


class _CanonicalCallbackContext:
    """Property-only Python overlay over a Rust-prepared phase view.

    Rows contain only effective scalar properties and a Rust-derived world AABB.
    They intentionally cannot carry geometry, identity, membership, or authored state.
    """

    def __init__(self, frame: dict[str, Any], authoring_context: object) -> None:
        self.time = float(frame["time"])
        self.delta_time = float(frame["delta_time"])
        self.token = frame["token"]
        self._authoring_context = authoring_context
        self._operations = authoring_context
        self._frame_items = {
            _phase_node_key(item["node"]): item for item in frame["objects"]
        }
        self._rows: dict[tuple[int, int], _PhasePropertyRow] = {}
        self._signals: dict[tuple[int, int], float] = {}
        self._next_read_request_id = 0
        self._writes: list[dict[str, Any]] = []

    def _read(self, kind: str, key: tuple[int, int]) -> dict[str, Any]:
        """Suspend this exact callback invocation for one Rust-pinned read miss."""
        try:
            from js import (
                noonReadSemanticContinuationCallback,
                noonSemanticContinuationGeneration,
            )
            from pyodide.ffi import can_run_sync, run_sync
        except ImportError as error:
            raise NotImplementedError(
                "canonical callback sparse reads require a suspended Pyodide continuation"
            ) from error
        if noonSemanticContinuationGeneration(self._authoring_context) is None:
            raise NotImplementedError(
                "canonical callback sparse reads require a suspended semantic continuation"
            )
        if not can_run_sync():
            raise NotImplementedError(
                "canonical callback sparse reads require Pyodide JS Promise Integration"
            )
        request_id = self._next_read_request_id
        self._next_read_request_id += 1
        request = {
            "request_id": request_id,
            "kind": kind,
            "node": _phase_node_json(key),
        }
        try:
            result_json = run_sync(
                noonReadSemanticContinuationCallback(
                    self._authoring_context,
                    json.dumps(self.token, separators=(",", ":")),
                    json.dumps(request, separators=(",", ":")),
                )
            )
            result = json.loads(str(result_json))
        except Exception as error:
            raise RuntimeError(f"canonical callback sparse read failed: {error}") from None
        expected_kind = "scalar" if kind == "scalar_signal" else "object"
        if not isinstance(result, dict) or result.get("kind") != expected_kind:
            raise RuntimeError("canonical callback sparse read returned the wrong typed value")
        return result

    def _object_item(self, key: tuple[int, int]) -> dict[str, Any]:
        try:
            return self._frame_items[key]
        except KeyError:
            result = self._read("object", key)
            item = result.get("object")
            if not isinstance(item, dict) or _phase_node_key(item.get("node")) != key:
                raise RuntimeError("canonical callback object read returned a foreign semantic node")
            self._frame_items[key] = item
            return item

    def scalar(self, key: tuple[int, int]) -> float:
        cached = self._signals.get(key)
        if cached is not None:
            return cached
        result = self._read("scalar_signal", key)
        value = _phase_number("scalar callback read", result.get("value"))
        self._signals[key] = value
        return value

    def row(self, mobject: _base.Mobject) -> tuple[tuple[int, int], _PhasePropertyRow]:
        key = _semantic_key(mobject)
        existing = self._rows.get(key)
        if existing is not None:
            return key, existing
        row = _PhasePropertyRow.from_wire(self._object_item(key))
        self._rows[key] = row
        return key, row

    def transform_changed(
        self, key: tuple[int, int], before: _PhaseTransform, row: _PhasePropertyRow
    ) -> None:
        if before != row.transform:
            self._writes.append(
                {
                    "kind": "transform",
                    "object": _phase_node_json(key),
                    "transform": row.transform.to_wire(),
                }
            )

    def rotate_transform_about_point(
        self,
        transform: _PhaseTransform,
        angle: float,
        pivot: _base.Vec2,
    ) -> _PhaseTransform:
        result = self._operations.callbackRotateTransformAboutPoint(
            transform.translation_x,
            transform.translation_y,
            transform.rotation,
            transform.scale_x,
            transform.scale_y,
            angle,
            pivot.x,
            pivot.y,
        )
        return _PhaseTransform(
            _phase_number("rotation result.translation.x", result.translationX),
            _phase_number("rotation result.translation.y", result.translationY),
            _phase_number("rotation result.rotation", result.rotation),
            _phase_number("rotation result.scale.x", result.scaleX),
            _phase_number("rotation result.scale.y", result.scaleY),
        )

    def paint_set_color(
        self,
        style: _PhaseStyle,
        color: tuple[float, float, float, float],
    ) -> tuple[
        tuple[float, float, float, float] | None,
        tuple[float, float, float, float] | None,
    ]:
        result = self._operations.callbackPaintSetColor(
            *_phase_optional_color_args(style.fill),
            *_phase_optional_color_args(style.stroke),
            *color,
        )
        return (
            _phase_callback_paint_color(result, "fill"),
            _phase_callback_paint_color(result, "stroke"),
        )

    def paint_set_fill(
        self,
        style: _PhaseStyle,
        color: tuple[float, float, float, float] | None,
        opacity: float | None,
    ) -> tuple[float, float, float, float] | None:
        result = self._operations.callbackPaintSetFill(
            *_phase_optional_color_args(style.fill),
            *_phase_optional_color_args(style.stroke),
            *_phase_optional_color_args(color),
            opacity,
        )
        return _phase_callback_paint_color(result, "fill")

    def paint_set_stroke(
        self,
        style: _PhaseStyle,
        color: tuple[float, float, float, float],
    ) -> tuple[float, float, float, float] | None:
        result = self._operations.callbackPaintSetStroke(
            *_phase_optional_color_args(style.fill),
            *_phase_optional_color_args(style.stroke),
            *color,
        )
        return _phase_callback_paint_color(result, "stroke")
    def line_target(
        self, start: _base.Vec2, end: _base.Vec2
    ) -> object:
        return self._operations.callbackLineTarget(start.x, start.y, end.x, end.y)

    def line_match_transform(
        self, source: _base.Mobject, target: object
    ) -> _PhaseTransform:
        handle = getattr(source, "_semantic_handle", None)
        if handle is None or not bool(getattr(source, "_semantic_handle_fresh", False)):
            raise NotImplementedError(
                "canonical Line.match_points requires an opaque semantic Line source"
            )
        result = self._operations.callbackMatchLineTransform(handle, target)
        return _PhaseTransform(
            _phase_number("Line.match_points result.translation.x", result.translationX),
            _phase_number("Line.match_points result.translation.y", result.translationY),
            _phase_number("Line.match_points result.rotation", result.rotation),
            _phase_number("Line.match_points result.scale.x", result.scaleX),
            _phase_number("Line.match_points result.scale.y", result.scaleY),
        )

    def style_changed(
        self, key: tuple[int, int], before: _PhaseStyle, row: _PhasePropertyRow
    ) -> None:
        if before != row.style:
            self._writes.append(
                {
                    "kind": "style",
                    "object": _phase_node_json(key),
                    "style": row.style.to_wire(),
                }
            )

    def effective_batch(self) -> dict[str, Any]:
        return {"token": self.token, "writes": self._writes}


def callback_line_target(
    start: _base.Vec2, end: _base.Vec2
) -> tuple["_CanonicalCallbackContext", object] | None:
    """Create a Rust-owned endpoint operand only during a canonical phase."""

    context = _ACTIVE_CANONICAL_CONTEXT.get()
    return None if context is None else (context, context.line_target(start, end))


def canonical_line_match(source: _base.Mobject, target: object) -> bool:
    """Stage one shared analytic Line transform in the active ordered overlay."""

    value = _canonical_row(source)
    if value is None:
        return False
    context, key, row = value
    operand = getattr(target, "_callback_line_target", None)
    if operand is None or getattr(target, "_callback_line_context", None) is not context:
        raise NotImplementedError(
            "canonical Line.match_points target must belong to this callback phase"
        )
    before = row.transform
    row.transform = context.line_match_transform(source, operand)
    row.invalidate_bounds()
    context.transform_changed(key, before, row)
    return True


def canonical_callback_scalar_value(scene: _base.Scene, handle: object) -> float:
    """Return one phase-local Rust scalar without consulting published tracker state."""
    context = _ACTIVE_CONTEXTS.get(id(scene))
    if not isinstance(context, _CanonicalCallbackContext):
        raise RuntimeError("canonical callback scalar reads require an active callback phase")
    try:
        key = (int(handle.semanticSlot), int(handle.semanticGeneration))
    except (AttributeError, TypeError, ValueError) as error:
        raise RuntimeError("canonical ValueTracker has no generational semantic identity") from error
    return context.scalar(key)


def _canonical_callback_time(mobject: _base.Mobject) -> float:
    """Read Rust's prepared time from the active callback phase."""
    context = _canonical_phase_context(mobject)
    if context is None:
        raise RuntimeError("canonical callback time is available only during a callback phase")
    return context.time


def _phase_number(name: str, value: object) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"canonical callback {name} must be a number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"canonical callback {name} must be finite")
    return result


def _phase_color(name: str, value: object) -> tuple[float, float, float, float] | None:
    if value is None:
        return None
    if not isinstance(value, dict):
        raise TypeError(f"canonical callback {name} must be an object or null")
    return tuple(
        _phase_number(f"{name}.{channel}", value.get(channel))
        for channel in ("red", "green", "blue", "alpha")
    )  # type: ignore[return-value]


def _phase_color_wire(value: tuple[float, float, float, float] | None) -> dict[str, float] | None:
    if value is None:
        return None
    return {"red": value[0], "green": value[1], "blue": value[2], "alpha": value[3]}


def _phase_optional_color_args(
    value: tuple[float, float, float, float] | None,
) -> tuple[float | None, float | None, float | None, float | None]:
    return (None, None, None, None) if value is None else value


def _phase_callback_paint_color(
    result: object, layer: str
) -> tuple[float, float, float, float] | None:
    title = layer.capitalize()
    if not bool(getattr(result, f"has{title}")):
        return None
    return tuple(
        _phase_number(
            f"paint result.{layer}.{channel}",
            getattr(result, f"{layer}{channel.capitalize()}"),
        )
        for channel in ("red", "green", "blue", "alpha")
    )  # type: ignore[return-value]


def _phase_bounds(value: object) -> tuple[float, float, float, float] | None:
    if value is None:
        return None
    if not isinstance(value, dict) or not isinstance(value.get("min"), dict) or not isinstance(value.get("max"), dict):
        raise TypeError("canonical callback bounds must be an object or null")
    min_x = _phase_number("bounds.min.x", value["min"].get("x"))
    min_y = _phase_number("bounds.min.y", value["min"].get("y"))
    max_x = _phase_number("bounds.max.x", value["max"].get("x"))
    max_y = _phase_number("bounds.max.y", value["max"].get("y"))
    if min_x > max_x or min_y > max_y:
        raise ValueError("canonical callback bounds are inverted")
    return min_x, min_y, max_x, max_y


def _phase_node_key(value: object) -> tuple[int, int]:
    if not isinstance(value, dict):
        raise TypeError("callback phase semantic node must be an object")
    slot = value.get("slot")
    generation = value.get("generation")
    if (isinstance(slot, bool) or not isinstance(slot, int) or slot < 0 or
            isinstance(generation, bool) or not isinstance(generation, int) or generation < 0):
        raise TypeError("callback phase semantic node must contain u32 slot/generation")
    return slot, generation


def _phase_node_json(key: tuple[int, int]) -> dict[str, int]:
    return {"slot": key[0], "generation": key[1]}


def _canonical_phase_context(mobject: _base.Mobject) -> _CanonicalCallbackContext | None:
    scene = mobject._scene
    if scene is None or mobject._object is None:
        return None
    context = _ACTIVE_CONTEXTS.get(id(scene))
    if isinstance(context, _CanonicalCallbackContext):
        return context
    return None


def _canonical_row(mobject: _base.Mobject) -> tuple[_CanonicalCallbackContext, tuple[int, int], _PhasePropertyRow] | None:
    context = _canonical_phase_context(mobject)
    if context is None:
        return None
    key, row = context.row(mobject)
    return context, key, row


def _canonical_current_raw(self: _base.Mobject):
    if _canonical_phase_context(self) is not None:
        raise NotImplementedError(
            "canonical callback raw geometry access is not supported; use property operations"
        )
    return _ORIGINAL_CURRENT_RAW(self)


def _canonical_apply(self: _base.Mobject, raw: object) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        raise NotImplementedError(
            "canonical callbacks support property operations only; raw replacement is unsupported"
        )
    return _ORIGINAL_APPLY(self, raw)


def _canonical_get_center(self: _base.Mobject) -> _base.Vec2:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_GET_CENTER(self)
    _, _, row = value
    return row.center()


def _canonical_shift(self: _base.Mobject, direction: object) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_SHIFT(self, direction)
    context, key, row = value
    before = row.transform
    row.shift(_base._as_vec2(direction))
    context.transform_changed(key, before, row)
    return self


def _canonical_move_to(self: _base.Mobject, point: object, *args: object, **kwargs: object) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_MOVE_TO(self, point, *args, **kwargs)
    if args or kwargs:
        raise NotImplementedError("callback move_to currently supports center point placement only")
    _, _, row = value
    return _canonical_shift(self, _base._as_vec2(point) - row.center())


def _canonical_set_x(self: _base.Mobject, x: float) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_SET_X(self, x)
    _, _, row = value
    return _canonical_shift(self, _base.Vec2(float(x) - row.center().x, 0.0))


def _canonical_set_y(self: _base.Mobject, y: float) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_SET_Y(self, y)
    _, _, row = value
    return _canonical_shift(self, _base.Vec2(0.0, float(y) - row.center().y))


def _canonical_scale(self: _base.Mobject, *args: object, **kwargs: object) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        raise NotImplementedError(
            "canonical callback scale is not supported; use shared semantic operations"
        )
    return _ORIGINAL_SCALE(self, *args, **kwargs)


def _canonical_rotate(self: _base.Mobject, *args: object, **kwargs: object) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_ROTATE(self, *args, **kwargs)
    context, key, row = value
    if not args or len(args) > 2:
        raise TypeError("canonical callback rotate expects angle and optional axis")
    options = dict(kwargs)
    if len(args) == 2 and "axis" in options:
        raise TypeError("canonical callback rotate received axis twice")
    axis = args[1] if len(args) == 2 else options.pop("axis", (0.0, 0.0, 1.0))
    about_point = options.pop("about_point", None)
    about_edge = options.pop("about_edge", None)
    if options:
        unsupported = ", ".join(sorted(options))
        raise NotImplementedError(
            f"unsupported canonical callback rotate option(s): {unsupported}"
        )
    if about_point is None or about_edge is not None:
        raise NotImplementedError(
            "canonical callback rotation currently requires one explicit about_point"
        )
    import _manim_compat as compat

    angle = compat._rotation_angle_2d(args[0], axis)
    pivot = compat._as_vec2(about_point)
    before = row.transform
    row.transform = context.rotate_transform_about_point(before, angle, pivot)
    row.invalidate_bounds()
    context.transform_changed(key, before, row)
    return self

def _canonical_set_color(self: _base.Mobject, color: _base.Color) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_SET_COLOR(self, color)
    context, key, row = value
    color_value = _phase_color("set_color", color.to_ir())
    assert color_value is not None
    before = row.style
    fill, stroke = context.paint_set_color(row.style, color_value)
    row.style = replace(row.style, fill=fill, stroke=stroke)
    context.style_changed(key, before, row)
    return self


def _canonical_set_fill(
    self: _base.Mobject, color: _base.Color | None = None, opacity: float | None = None
) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_SET_FILL(self, color, opacity)
    context, key, row = value
    before = row.style
    fill = None if color is None else _phase_color("set_fill", color.to_ir())
    row.style = replace(
        row.style,
        fill=context.paint_set_fill(row.style, fill, opacity),
    )
    context.style_changed(key, before, row)
    return self


def _canonical_set_stroke(
    self: _base.Mobject, color: _base.Color | None = None, width: float | None = None
) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_SET_STROKE(self, color, width)
    context, key, row = value
    if width is not None:
        raise NotImplementedError(
            "canonical callback stroke width is not supported"
        )
    before = row.style
    if color is None:
        style = replace(row.style, stroke=None)
    else:
        parsed = _phase_color("set_stroke", color.to_ir())
        assert parsed is not None
        stroke = context.paint_set_stroke(row.style, parsed)
        style = replace(row.style, stroke=stroke)
    row.style = style
    if (row.style.stroke is None) != (before.stroke is None):
        row.invalidate_bounds()
    context.style_changed(key, before, row)
    return self


def _canonical_set_opacity(self: _base.Mobject, opacity: float) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_SET_OPACITY(self, opacity)
    context, key, row = value
    before = row.style
    row.style = replace(row.style, opacity=float(opacity))
    context.style_changed(key, before, row)
    return self


def _canonical_vmobject_set_color(
    self: _base.Mobject,
    color: object,
    family: bool = True,
) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        del family
        from _manim_phase_b import _as_color

        return _canonical_set_color(self, _as_color("color", color))
    assert _ORIGINAL_VMOBJECT_SET_COLOR is not None
    return _ORIGINAL_VMOBJECT_SET_COLOR(self, color, family=family)


def _canonical_vmobject_set_fill(
    self: _base.Mobject,
    color: object = None,
    opacity: float | None = None,
    family: bool = True,
) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        del family
        if color is not None:
            from _manim_phase_b import _as_color

            color = _as_color("fill color", color)
        return _canonical_set_fill(self, color, opacity)
    assert _ORIGINAL_VMOBJECT_SET_FILL is not None
    return _ORIGINAL_VMOBJECT_SET_FILL(self, color=color, opacity=opacity, family=family)


def _canonical_vmobject_set_stroke(
    self: _base.Mobject,
    color: object = None,
    width: float | None = None,
    opacity: float | None = None,
    family: bool = True,
) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        del family
        if opacity is not None:
            raise NotImplementedError(
                "canonical callback stroke opacity is not supported; use set_opacity"
            )
        if color is not None:
            from _manim_phase_b import _as_color

            color = _as_color("stroke color", color)
        return _canonical_set_stroke(self, color, width)
    assert _ORIGINAL_VMOBJECT_SET_STROKE is not None
    return _ORIGINAL_VMOBJECT_SET_STROKE(
        self, color=color, width=width, opacity=opacity, family=family
    )


def _canonical_vmobject_set_opacity(
    self: _base.Mobject,
    opacity: float,
    family: bool = True,
) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        del family
        from _manim_phase_b import _opacity

        return _canonical_set_opacity(self, _opacity("opacity", opacity))
    assert _ORIGINAL_VMOBJECT_SET_OPACITY is not None
    return _ORIGINAL_VMOBJECT_SET_OPACITY(self, opacity, family=family)


def run_canonical_callback_phase(session_id: int, frame: dict[str, Any]) -> str:
    """Invoke one Rust-selected callback phase and return only effective writes.

    The compiler supplies invocation order and semantic targets. Python neither
    chooses active callbacks nor maps semantic targets through export ObjectIds.
    The caller commits this one batch to the exact phase token in the existing
    canonical execution session.
    """

    try:
        session = _CANONICAL_SESSIONS[int(session_id)]
    except KeyError as error:
        raise ValueError(f"unknown canonical Noon updater session {session_id}") from error

    context = _CanonicalCallbackContext(frame, session.context)
    scene_key = id(session.scene)
    if scene_key in _ACTIVE_CONTEXTS:
        raise RuntimeError("nested Noon callback phases are not supported")
    _ACTIVE_CONTEXTS[scene_key] = context
    context_token = _ACTIVE_CANONICAL_CONTEXT.set(context)
    # A canonical phase currently has no typed signal read-set. Enter an empty
    # signal scope so ValueTracker reads fail explicitly instead of falling back
    # to the wrapper's authored scalar value.
    import _manim_reactive as reactive

    reactive._enter_callback_signal_values({"signals": []})
    try:
        for invocation in frame.get("invocations", []):
            if not isinstance(invocation, dict):
                raise TypeError("canonical callback invocation must be an object")
            callback_id = invocation.get("callback_id")
            if isinstance(callback_id, bool) or not isinstance(callback_id, (int, str)):
                raise TypeError("canonical callback ID must be an integer string")
            try:
                callback = session.callbacks[int(callback_id)]
            except (KeyError, ValueError) as error:
                raise RuntimeError(
                    f"canonical callback phase received unknown callback {callback_id}"
                ) from error
            target = _phase_node_key(invocation.get("target"))
            occurrence_index = invocation.get("occurrence_index")
            if (
                isinstance(occurrence_index, bool)
                or not isinstance(occurrence_index, int)
                or occurrence_index < 0
            ):
                raise TypeError("canonical callback occurrence index must be a u32")
            try:
                mobject = session.targets[target]
            except KeyError as error:
                raise RuntimeError(
                    "canonical callback phase received an unbound semantic target "
                    f"{target[0]}:{target[1]}"
                ) from error
            _invoke(callback, mobject, context.delta_time)
    finally:
        reactive._leave_callback_signal_values()
        _ACTIVE_CANONICAL_CONTEXT.reset(context_token)
        _ACTIVE_CONTEXTS.pop(scene_key, None)

    return _json_phase(context.effective_batch())


def _json_phase(value: object) -> str:
    # This is the explicit Pyodide callback boundary. It is never an in-process
    # Rust engine boundary: the semantic store, compiler plan, session and
    # renderer delta encoder remain in the same WASM runtime.
    import json

    return json.dumps(value, separators=(",", ":"), allow_nan=False)


def release_session(session_id: int) -> None:
    _CANONICAL_SESSIONS.pop(int(session_id), None)


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _base.Mobject.add_updater = add_updater
    _base.Mobject.remove_updater = remove_updater
    _base.Mobject.clear_updaters = clear_updaters
    _base.Mobject.get_updaters = get_updaters
    _base.Mobject.has_updaters = has_updaters
    _base.Mobject._current_raw = _canonical_current_raw
    _base.Mobject._apply = _canonical_apply
    _base.Mobject.get_center = _canonical_get_center
    _base.Mobject.shift = _canonical_shift
    _base.Mobject.move_to = _canonical_move_to
    _base.Mobject.set_x = _canonical_set_x
    _base.Mobject.set_y = _canonical_set_y
    _base.Mobject.scale = _canonical_scale
    _base.Mobject.rotate = _canonical_rotate
    _base.Mobject.set_color = _canonical_set_color
    _base.Mobject.set_fill = _canonical_set_fill
    _base.Mobject.set_stroke = _canonical_set_stroke
    _base.Mobject.set_opacity = _canonical_set_opacity
    # Semantic-handle installation gives VMobject its own final public style
    # methods. Reinstall the phase dispatch at that public boundary so it cannot
    # fall through to raw snapshot mutation while a canonical callback is active.
    import _manim_compat as _compat

    global _ORIGINAL_VMOBJECT_SET_COLOR
    global _ORIGINAL_VMOBJECT_SET_FILL
    global _ORIGINAL_VMOBJECT_SET_STROKE
    global _ORIGINAL_VMOBJECT_SET_OPACITY
    # Inherited methods already use the Mobject phase dispatcher above. Wrap
    # only VMobject's own overrides, preserving the inherited base signatures.
    if "set_color" in _compat.VMobject.__dict__:
        _ORIGINAL_VMOBJECT_SET_COLOR = _compat.VMobject.set_color
        _compat.VMobject.set_color = _canonical_vmobject_set_color
    if "set_fill" in _compat.VMobject.__dict__:
        _ORIGINAL_VMOBJECT_SET_FILL = _compat.VMobject.set_fill
        _compat.VMobject.set_fill = _canonical_vmobject_set_fill
    if "set_stroke" in _compat.VMobject.__dict__:
        _ORIGINAL_VMOBJECT_SET_STROKE = _compat.VMobject.set_stroke
        _compat.VMobject.set_stroke = _canonical_vmobject_set_stroke
    if "set_opacity" in _compat.VMobject.__dict__:
        _ORIGINAL_VMOBJECT_SET_OPACITY = _compat.VMobject.set_opacity
        _compat.VMobject.set_opacity = _canonical_vmobject_set_opacity
    _INSTALLED = True
