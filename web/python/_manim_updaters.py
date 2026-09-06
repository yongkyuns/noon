"""Manim-style callback ergonomics over the canonical callback-phase contract.

Python owns callable identity and invocation. Rust owns semantic registration,
activation ordering, the staged effective snapshot, and publication. The legacy
patch codec below remains only for its explicit migration consumer; canonical
callbacks return one property-only effective batch to their existing session.
"""

from __future__ import annotations

import copy
import inspect
import math
import sys
from dataclasses import dataclass, replace
from typing import Any, Callable

import _noon_ir as _ir
import noon as _base

_INSTALLED = False
_NEXT_SESSION_ID = 0
_TRACKED_MOBJECTS: list[_base.Mobject] = []
_SESSIONS: dict[int, "_UpdaterSession"] = {}
_CANONICAL_SESSIONS: dict[int, "_CanonicalCallbackSession"] = {}
_ACTIVE_CONTEXTS: dict[int, Any] = {}

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


def _current_raw(self: _base.Mobject) -> _ir.Mobject:
    scene = self._scene
    if scene is not None and self._object is not None:
        context = _ACTIVE_CONTEXTS.get(id(scene))
        if context is not None:
            return context.current_raw(self)
    return _ORIGINAL_CURRENT_RAW(self)


def _apply(self: _base.Mobject, raw: _ir.Mobject) -> _base.Mobject:
    scene = self._scene
    if scene is not None and self._object is not None:
        context = _ACTIVE_CONTEXTS.get(id(scene))
        if context is not None:
            context.replace_raw(self, raw)
            return self
    return _ORIGINAL_APPLY(self, raw)


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


@dataclass(slots=True)
class _UpdaterSession:
    scene: _base.Scene
    registrations: dict[int, _UpdaterRegistration]


class _CallbackContext:
    def __init__(self, scene: _base.Scene, frame: dict[str, Any]) -> None:
        self.scene = scene
        self.delta_time = float(frame["delta_time"])
        self._frame_items = {
            int(item["object"]): item
            for item in frame["objects"]
        }
        self._baseline: dict[int, _ir.Mobject] = {}
        self._current: dict[int, _ir.Mobject] = {}

    def _materialize(self, object_id: int) -> _ir.Mobject:
        existing = self._current.get(object_id)
        if existing is not None:
            return existing
        try:
            item = self._frame_items[object_id]
        except KeyError as error:
            raise RuntimeError(
                f"host callback snapshot does not contain object {object_id}"
            ) from error
        authored = self.scene._objects[object_id]
        raw = _ir.Mobject(
            geometry=copy.deepcopy(authored["geometry"]),
            transform=copy.deepcopy(item["transform"]),
            style=copy.deepcopy(item["style"]),
        )
        self._baseline[object_id] = raw
        current = copy.deepcopy(raw)
        self._current[object_id] = current
        return current

    def current_raw(self, mobject: _base.Mobject | int) -> _ir.Mobject:
        if isinstance(mobject, int):
            return self._materialize(mobject)
        obj = mobject._object
        if obj is None:
            raise RuntimeError("legacy callback target has no object identity")
        return self._materialize(obj.id)

    def replace_raw(self, mobject: _base.Mobject | int, raw: _ir.Mobject) -> None:
        if isinstance(mobject, int):
            object_id = mobject
        else:
            obj = mobject._object
            if obj is None:
                raise RuntimeError("legacy callback target has no object identity")
            object_id = obj.id
        self._materialize(object_id)
        self._current[object_id] = _ir.Mobject(
            geometry=copy.deepcopy(raw.geometry),
            transform=copy.deepcopy(raw.transform),
            style=copy.deepcopy(raw.style),
        )

    def patch_batch(self, sequence: int) -> _ir.PatchBatch:
        batch = _ir.PatchBatch(sequence)
        for object_id in sorted(self._current):
            before = self._baseline[object_id]
            after = self._current[object_id]
            if before.geometry != after.geometry:
                batch.set_geometry(object_id, after.geometry)
            if before.transform != after.transform:
                translation = after.transform["translation"]
                scale = after.transform["scale"]
                batch.set_transform(
                    object_id,
                    translation=(translation["x"], translation["y"]),
                    rotation=after.transform["rotation"],
                    scale=(scale["x"], scale["y"]),
                )
            if before.style != after.style:
                batch.set_style(
                    object_id,
                    fill=_color(after.style["fill"]),
                    stroke=_color(after.style["stroke"]),
                    stroke_width=after.style["stroke_width"],
                    stroke_width_mode=after.style.get(
                        "stroke_width_mode", _DEFAULT_STROKE_WIDTH_MODE
                    ),
                    stroke_join=after.style["stroke_join"],
                    stroke_cap=after.style["stroke_cap"],
                    opacity=after.style["opacity"],
                )
        return batch


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

    def __init__(self, frame: dict[str, Any], operations: object) -> None:
        self.time = float(frame["time"])
        self.delta_time = float(frame["delta_time"])
        self.token = frame["token"]
        self._operations = operations
        self._frame_items = {
            _phase_node_key(item["node"]): item for item in frame["objects"]
        }
        self._rows: dict[tuple[int, int], _PhasePropertyRow] = {}
        self._writes: list[dict[str, Any]] = []

    def row(self, mobject: _base.Mobject) -> tuple[tuple[int, int], _PhasePropertyRow]:
        key = _semantic_key(mobject)
        existing = self._rows.get(key)
        if existing is not None:
            return key, existing
        try:
            item = self._frame_items[key]
        except KeyError as error:
            raise RuntimeError(
                "canonical callback phase exposes only active callback targets; "
                "reading undeclared semantic node "
                f"{key[0]}:{key[1]} is not supported"
            ) from error
        row = _PhasePropertyRow.from_wire(item)
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
    return _current_raw(self)


def _canonical_apply(self: _base.Mobject, raw: object) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        raise NotImplementedError(
            "canonical callbacks support property operations only; raw replacement is unsupported"
        )
    return _apply(self, raw)  # type: ignore[arg-type]


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


def _canonical_move_to(self: _base.Mobject, point: object) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _ORIGINAL_MOVE_TO(self, point)
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
    before = row.style
    stroke = None if color is None else _phase_color("set_stroke", color.to_ir())
    style = replace(row.style, stroke=stroke)
    if width is not None:
        style = replace(style, stroke_width=float(width))
    row.style = style
    if (
        (row.style.stroke is None) != (before.stroke is None)
        or row.style.stroke_width != before.stroke_width
    ):
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


def _color(value: dict[str, float] | None) -> _ir.Color | None:
    if value is None:
        return None
    return _ir.Color(
        value["red"],
        value["green"],
        value["blue"],
        value.get("alpha", 1.0),
    )


def register_scene(scene: _base.Scene) -> dict[str, Any] | None:
    global _NEXT_SESSION_ID

    history: list[_UpdaterRegistration] = []
    for mobject in _TRACKED_MOBJECTS:
        if mobject._scene is not scene or mobject._object is None:
            continue
        history.extend(_registration_history(mobject))
    if not history:
        return None

    # Detached mobjects commonly receive updaters before Scene.add at authored time
    # zero. Resolve that pending start once the object is known to belong to this
    # scene; removals recorded after binding retain their exact scene-time endpoint.
    for registration in history:
        if registration.active_after is None:
            registration.active_after = 0.0

    session_id = _NEXT_SESSION_ID
    _NEXT_SESSION_ID += 1
    registrations = {slot_id: registration for slot_id, registration in enumerate(history)}
    _SESSIONS[session_id] = _UpdaterSession(scene=scene, registrations=registrations)

    # Arbitrary Python closures may read any bound mobject. Every scheduled slot
    # observes the same complete semantic table; the Rust runtime deduplicates that
    # table once per phase and owns which callback slots are active at the frame time.
    object_ids = [int(obj["id"]) for obj in scene._objects]
    slots = []
    for slot_id, registration in registrations.items():
        slot = {
            "id": slot_id,
            "objects": object_ids,
            "active_after": registration.active_after,
        }
        if registration.active_through is not None:
            slot["active_through"] = registration.active_through
        slots.append(slot)
    return {
        "session_id": session_id,
        "slots": slots,
    }


def run_callback_phase(
    session_id: int,
    frame: dict[str, Any],
    sequence: int,
) -> str:
    try:
        session = _SESSIONS[int(session_id)]
    except KeyError as error:
        raise ValueError(f"unknown Noon updater session {session_id}") from error

    invocations = frame.get("invocations", [])
    context = _CallbackContext(session.scene, frame)
    scene_key = id(session.scene)
    if scene_key in _ACTIVE_CONTEXTS:
        raise RuntimeError("nested Noon host callback phases are not supported")
    _ACTIVE_CONTEXTS[scene_key] = context

    # Keep the updater adapter usable by native Python tests and non-reactive scenes:
    # importing the reactive facade eagerly would require Pyodide's `js` bridge even
    # when this callback phase has no signals. Only enter the ValueTracker signal
    # context when the runtime actually supplied signal values.
    reactive = None
    if frame.get("signals"):
        import _manim_reactive as reactive

        reactive._enter_callback_signal_values(frame)
    try:
        for invocation in invocations:
            slot_id = int(invocation["callback"])
            try:
                registration = session.registrations[slot_id]
            except KeyError as error:
                raise RuntimeError(
                    f"updater session received unknown callback slot {slot_id}"
                ) from error
            _invoke(registration.callback, registration.mobject, context.delta_time)
    finally:
        if reactive is not None:
            reactive._leave_callback_signal_values()
        _ACTIVE_CONTEXTS.pop(scene_key, None)

    return context.patch_batch(int(sequence)).to_json()


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
        _ACTIVE_CONTEXTS.pop(scene_key, None)

    return _json_phase(context.effective_batch())


def _json_phase(value: object) -> str:
    # This is the explicit Pyodide callback boundary. It is never an in-process
    # Rust engine boundary: the semantic store, compiler plan, session and
    # renderer delta encoder remain in the same WASM runtime.
    import json

    return json.dumps(value, separators=(",", ":"), allow_nan=False)


def release_session(session_id: int) -> None:
    _SESSIONS.pop(int(session_id), None)
    _CANONICAL_SESSIONS.pop(int(session_id), None)


def _install_rotating_breadth() -> None:
    """Install source-execution breadth for Manim's literal ``RotatingDemo``.

    The qualified centered-leaf implementation stays in ``_manim_rotate``. This final
    bootstrap wrapper handles retained groups, external pivots, curved z-axis family
    motion, and the planar principal-axis projection used by the documentation scene.
    Arbitrary 3D retained state remains a separate representation problem.
    """

    compat = sys.modules.get("_manim_compat")
    options = sys.modules.get("_manim_animation_options")
    animate = sys.modules.get("_manim_animate")
    rates = sys.modules.get("_manim_rate_functions")
    rotate = sys.modules.get("_manim_rotate")
    if any(module is None for module in (compat, options, animate, rates, rotate)):
        return
    if getattr(_base, "_noon_rotating_breadth_installed", False):
        return

    original_scene_play = compat.Scene.play

    class Rotating:
        def __init__(
            self,
            mobject: object,
            angle: float = math.tau,
            axis: object = compat.OUT,
            about_point: object | None = None,
            about_edge: object | None = None,
            run_time: float = 5.0,
            rate_func: object = rates.linear,
            **kwargs: Any,
        ) -> None:
            if not isinstance(mobject, (_base.Mobject, compat.Group)):
                raise TypeError("Rotating target must be a Mobject or Group")
            value = float(angle)
            if not math.isfinite(value):
                raise ValueError("Rotating angle must be finite")
            self.mobject = mobject
            self.angle = value
            self.axis = axis
            self.about_point = about_point
            self.about_edge = about_edge
            self.anim_args = dict(kwargs)
            self.anim_args["run_time"] = run_time
            self.anim_args["rate_func"] = rate_func

    def axis_kind(axis: object) -> tuple[str, float]:
        try:
            length = len(axis)  # type: ignore[arg-type]
        except (TypeError, AttributeError) as error:
            raise TypeError("rotation axis must be a vector") from error
        values = [float(axis[index]) for index in range(length)]  # type: ignore[index]
        if length == 2:
            x, y = values
            if math.isclose(y, 0.0, abs_tol=1e-12) and not math.isclose(x, 0.0, abs_tol=1e-12):
                return "x", 1.0 if x > 0.0 else -1.0
            if math.isclose(x, 0.0, abs_tol=1e-12) and not math.isclose(y, 0.0, abs_tol=1e-12):
                return "y", 1.0 if y > 0.0 else -1.0
            raise NotImplementedError("2D axis constants must be parallel to RIGHT or UP")
        if length == 3:
            x, y, z = values
            nonzero = [not math.isclose(value, 0.0, abs_tol=1e-12) for value in (x, y, z)]
            if sum(nonzero) != 1:
                raise NotImplementedError("Rotating breadth currently supports principal axes only")
            if nonzero[2]:
                return "z", 1.0 if z > 0.0 else -1.0
            if nonzero[0]:
                return "x", 1.0 if x > 0.0 else -1.0
            return "y", 1.0 if y > 0.0 else -1.0
        raise TypeError("rotation axis must have two or three components")

    def current_detached_members(
        scene: object, value: object, start_time: float
    ) -> tuple[list[_base.Mobject], list[_base.Mobject]]:
        sources = compat._leaf_mobjects(value)
        detached: list[_base.Mobject] = []
        for source in sources:
            if source._scene is not scene or source._object is None:
                raise ValueError("Rotating target must belong to this Scene")
            snapshot = scene._snapshot_for_object_at(source._object, start_time)
            detached.append(animate._snapshot_mobject(snapshot))
        return sources, detached

    def pivot(detached: list[_base.Mobject], animation: Rotating) -> _base.Vec2:
        group = compat.Group(*detached)
        if animation.about_point is not None:
            return compat._as_vec2(animation.about_point)
        if animation.about_edge is not None:
            return compat._critical_for(group, compat._as_vec2(animation.about_edge))
        return group.get_center()

    def projection_scale_key(current: _base.Mobject, *, axis: str) -> str:
        """Return the local scale channel matching the projected world dimension.

        The scene wire stores legacy rotation as f32, so exact quarter turns authored
        through repeated retained Transforms can return with O(1e-7) rad quantization
        residue. Classify against the nearest quarter turn using a bound derived from
        f32 machine epsilon; larger residuals are genuine non-principal bases whose
        projected affine map would require shear in Noon's rotation+scale transform.
        """

        rotation = float(current.to_ir()["transform"]["rotation"])
        quarter_turn = math.pi / 2.0
        quarter_index = round(rotation / quarter_turn)
        nearest = quarter_index * quarter_turn
        f32_epsilon = 2.0**-23
        tolerance = 8.0 * f32_epsilon * max(1.0, abs(rotation), abs(nearest))
        if abs(rotation - nearest) > tolerance:
            raise NotImplementedError(
                "non-z Rotating projection would require shear for a non-axis-aligned retained leaf"
            )
        local_x_is_world_x = quarter_index % 2 == 0
        if axis == "y":
            return "x" if local_x_is_world_x else "y"
        if axis == "x":
            return "y" if local_x_is_world_x else "x"
        raise ValueError(f"unsupported projection axis {axis!r}")

    def projected_target(
        current: _base.Mobject,
        *,
        axis: str,
        pivot_point: _base.Vec2,
        angle: float,
        scale_key: str,
    ) -> _base.Mobject:
        """Project a planar principal-axis rotation back onto the xy authoring plane."""

        target_snapshot = current.to_ir()
        transform = target_snapshot["transform"]
        translation = transform["translation"]
        scale = transform["scale"]
        compression = math.cos(angle)
        if axis == "y":
            translation["x"] = pivot_point.x + compression * (
                float(translation["x"]) - pivot_point.x
            )
        elif axis == "x":
            translation["y"] = pivot_point.y + compression * (
                float(translation["y"]) - pivot_point.y
            )
        else:
            raise ValueError(f"unsupported projection axis {axis!r}")
        scale[scale_key] = float(scale[scale_key]) * compression
        return animate._snapshot_mobject(target_snapshot)

    def schedule_family(
        scene: object,
        animation: Rotating,
        *,
        start_time: float,
        duration: float,
        easing: str,
    ) -> list[tuple[_base.Mobject, _base.Mobject]]:
        sources, detached = current_detached_members(scene, animation.mobject, start_time)
        pivot_point = pivot(detached, animation)
        axis, sign = axis_kind(animation.axis)
        if axis in {"x", "y"} and not math.isclose(
            abs(animation.angle), math.pi, rel_tol=0.0, abs_tol=1e-12
        ):
            raise NotImplementedError(
                "non-z Rotating currently supports the 180-degree planar turn used by Manim RotatingDemo"
            )

        projection_keys: list[str] | None = None
        if axis in {"x", "y"}:
            projection_keys = [
                projection_scale_key(current, axis=axis) for current in detached
            ]

        # Manim procedural Rotating restores the play-start geometry at every alpha
        # and applies the current rotation. One generic source/target Transform cannot
        # model that path: a 180-degree z turn collapses its matrix at the midpoint,
        # while a principal x/y turn should follow an orthographic cos(theta)
        # compression. Sample the one global rate function into short deterministic
        # retained intervals, always deriving each endpoint from the original play-start
        # snapshots. This keeps family members coherent without a Python frame callback
        # or a renderer-specific primitive. The final interval endpoint is authored
        # exactly at ``start_time + duration`` to avoid floating handoff drift.
        max_step = math.pi / 36.0
        segment_count = max(1, math.ceil(abs(animation.angle) / max_step))
        end_time = start_time + duration
        final_targets: list[tuple[_base.Mobject, _base.Mobject]] = []
        for segment in range(segment_count):
            alpha = (segment + 1) / segment_count
            eased_alpha = rates.evaluate_rate_function(easing, alpha)
            cumulative_angle = sign * animation.angle * eased_alpha
            segment_start = start_time + duration * segment / segment_count
            segment_end = (
                end_time
                if segment + 1 == segment_count
                else start_time + duration * (segment + 1) / segment_count
            )
            segment_duration = segment_end - segment_start
            for index, (source, current) in enumerate(
                zip(sources, detached, strict=True)
            ):
                assert source._object is not None
                obj = source._object
                if axis == "z":
                    target = animate._snapshot_mobject(current.to_ir())
                    target.rotate(cumulative_angle, compat.OUT, about_point=pivot_point)
                else:
                    assert projection_keys is not None
                    target = projected_target(
                        current,
                        axis=axis,
                        pivot_point=pivot_point,
                        angle=cumulative_angle,
                        scale_key=projection_keys[index],
                    )
                object_key = scene._object_keys[obj.id]
                compat._BaseScene.play(
                    scene,
                    _base.Transform(
                        source,
                        target,
                        key=(
                            f"@rotating-family:{object_key}:{start_time:g}:{index}"
                            f":segment:{segment}"
                        ),
                    ),
                    run_time=segment_duration,
                    start_time=segment_start,
                    easing="linear",
                )
                if segment + 1 == segment_count:
                    final_targets.append((source, target))
        return final_targets

    def schedule(
        scene: object,
        animation: Rotating,
        *,
        start_time: float,
        duration: float,
        easing: str,
    ) -> list[tuple[_base.Mobject, _base.Mobject]]:
        if not isinstance(animation.mobject, compat.Group):
            exact = rotate.Rotating(
                animation.mobject,
                angle=animation.angle,
                axis=animation.axis,
                about_point=animation.about_point,
                about_edge=animation.about_edge,
                run_time=duration,
                rate_func=rates.linear,
            )
            rotate._schedule_rotate(
                scene,
                exact,
                start_time=start_time,
                duration=duration,
                easing=easing,
            )
            return []
        return schedule_family(
            scene,
            animation,
            start_time=start_time,
            duration=duration,
            easing=easing,
        )

    def scene_play(
        self: object,
        *animations: Any,
        duration: float | None = None,
        run_time: float | None = None,
        start_time: float | None = None,
        easing: str | None = None,
        rate_func: object | None = None,
        lag_ratio: float | None = None,
        **kwargs: Any,
    ):
        broad = [animation for animation in animations if isinstance(animation, Rotating)]
        if not broad:
            return original_scene_play(
                self,
                *animations,
                duration=duration,
                run_time=run_time,
                start_time=start_time,
                easing=easing,
                rate_func=rate_func,
                lag_ratio=lag_ratio,
                **kwargs,
            )
        if len(broad) != len(animations):
            raise NotImplementedError(
                "mixing broad Rotating with unrelated top-level animations remains a parity candidate"
            )
        if duration is not None and run_time is not None:
            raise ValueError("use either duration or run_time, not both")
        if easing is not None and rate_func is not None:
            raise ValueError("use either rate_func or the low-level easing alias, not both")
        if kwargs:
            unsupported = ", ".join(sorted(kwargs))
            raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")

        play_run_time = run_time if run_time is not None else duration
        if play_run_time is not None:
            play_run_time = float(play_run_time)
        if lag_ratio is not None:
            lag_ratio = float(lag_ratio)
        base_start = self._cursor if start_time is None else float(start_time)
        if not math.isfinite(base_start) or base_start < 0.0:
            raise ValueError("start_time must be finite and non-negative")

        checkpoint = self._authoring_checkpoint()
        cursor_before = self._cursor
        top_level_before = list(self._compat_top_level)
        wrapper_states: dict[int, tuple[_base.Mobject, object, object]] = {}
        for animation in broad:
            animate._record_wrapper_state(animation.mobject, wrapper_states)

        max_end = base_start
        semantic_targets: dict[int, tuple[_base.Mobject, _base.Mobject]] = {}
        try:
            for animation in broad:
                animate._bind_for_animation(self, animation.mobject, start_time=base_start)
                resolved = options.resolve(
                    builder_args=options.builder_args(animation),
                    default_lag_ratio=0.0,
                    play_run_time=play_run_time,
                    play_easing=easing,
                    play_rate_func=rate_func,
                    play_lag_ratio=lag_ratio,
                )
                for source, target in schedule(
                    self,
                    animation,
                    start_time=base_start,
                    duration=resolved.run_time,
                    easing=resolved.rate_func,
                ):
                    semantic_targets[id(source)] = (source, target)
                max_end = max(max_end, base_start + resolved.run_time)
            self._cursor = max(cursor_before, max_end)
            # Internal family segments intentionally bypass the aligned scheduler to
            # keep one deterministic global rate function. Mirror its successful-play
            # ownership handoff once, after every segment has scheduled, so browser
            # semantic handles and the legacy scene timeline agree for the next play.
            for source, target in semantic_targets.values():
                animate._semantic_handles.commit_transform_target(source, target)
            return self
        except Exception:
            self._restore_authoring_checkpoint(checkpoint)
            self._cursor = cursor_before
            self._compat_top_level = top_level_before
            for member, old_scene, old_object in wrapper_states.values():
                member._scene = old_scene
                member._object = old_object
            raise

    for module in (_base, compat, animate):
        setattr(module, "Rotating", Rotating)
    if "Rotating" not in _base.__all__:
        _base.__all__.append("Rotating")
    compat.Scene.play = scene_play
    setattr(_base, "_noon_rotating_breadth_installed", True)


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
    _install_rotating_breadth()
    _INSTALLED = True
