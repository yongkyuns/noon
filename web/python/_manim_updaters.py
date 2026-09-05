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
from dataclasses import dataclass
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
_ORIGINAL_BOUNDS = _base._bounds
_CALLBACK_BOUNDS = "__noon_callback_bounds__"
_CALLBACK_BOUNDS_UNAVAILABLE = "__noon_callback_bounds_unavailable__"


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

    def current_raw(self, mobject: _base.Mobject) -> _ir.Mobject:
        obj = mobject._object
        if obj is None:
            raise RuntimeError("legacy callback target has no object identity")
        return self._materialize(obj.id)

    def replace_raw(self, mobject: _base.Mobject, raw: _ir.Mobject) -> None:
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
                    stroke_width_mode=after.style.get("stroke_width_mode", "scale_with_object"),
                    stroke_join=after.style["stroke_join"],
                    stroke_cap=after.style["stroke_cap"],
                    opacity=after.style["opacity"],
                )
        return batch


class _CanonicalCallbackContext:
    """Property-only Python overlay over a Rust-prepared phase view.

    It is keyed exclusively by the generational semantic identity supplied by
    Rust. Geometry/content/membership mutation cannot be encoded here, so an
    updater either produces ordered effective transform/style writes or fails
    before the pinned session publication changes.
    """

    def __init__(self, frame: dict[str, Any]) -> None:
        self.delta_time = float(frame["delta_time"])
        self.token = frame["token"]
        self._frame_items = {
            _phase_node_key(item["node"]): item for item in frame["objects"]
        }
        self._baseline: dict[tuple[int, int], _ir.Mobject] = {}
        self._current: dict[tuple[int, int], _ir.Mobject] = {}
        self._writes: list[dict[str, Any]] = []

    def _materialize(self, mobject: _base.Mobject) -> tuple[tuple[int, int], _ir.Mobject]:
        key = _semantic_key(mobject)
        existing = self._current.get(key)
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
        # The phase never transfers immutable geometry/text payloads. Cached
        # effective bounds are sufficient for center/size scalar access; missing
        # bounds remain an explicit unsupported geometry query rather than a
        # fabricated zero-size shape.
        bounds = item.get("bounds")
        geometry = (
            {_CALLBACK_BOUNDS: copy.deepcopy(bounds), "transform": copy.deepcopy(item["transform"])}
            if bounds is not None
            else {_CALLBACK_BOUNDS_UNAVAILABLE: True}
        )
        raw = _ir.Mobject(
            geometry=geometry,
            transform=copy.deepcopy(item["transform"]),
            style=copy.deepcopy(item["style"]),
        )
        self._baseline[key] = raw
        current = copy.deepcopy(raw)
        self._current[key] = current
        return key, current

    def current_raw(self, mobject: _base.Mobject) -> _ir.Mobject:
        return self._materialize(mobject)[1]

    def replace_raw(self, mobject: _base.Mobject, raw: _ir.Mobject) -> None:
        key, before = self._materialize(mobject)
        if before.geometry != raw.geometry:
            raise NotImplementedError(
                "canonical callbacks support transform/style properties only; "
                "geometry/content/membership mutation is not supported"
            )
        current = _ir.Mobject(
            geometry=copy.deepcopy(before.geometry),
            transform=copy.deepcopy(raw.transform),
            style=copy.deepcopy(raw.style),
        )
        previous = self._current[key]
        self._current[key] = current
        if previous.transform != current.transform:
            self._writes.append(
                {
                    "kind": "transform",
                    "object": _phase_node_json(key),
                    "transform": copy.deepcopy(current.transform),
                }
            )
        if previous.style != current.style:
            self._writes.append(
                {
                    "kind": "style",
                    "object": _phase_node_json(key),
                    "style": copy.deepcopy(current.style),
                }
            )

    def effective_batch(self) -> dict[str, Any]:
        return {"token": self.token, "writes": self._writes}


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


def _callback_bounds(raw: _ir.Mobject):
    """Resolve callback-only cached bounds without reading copied geometry.

    The Rust phase view supplies a current effective axis-aligned bound. This
    adapter keeps that bound coherent through subsequent Python affine writes by
    mapping its four corners through the relative transform. Degenerate source
    scales cannot be inverted, so those geometry-derived queries fail instead of
    producing invented values.
    """

    geometry = raw.geometry
    if _CALLBACK_BOUNDS_UNAVAILABLE in geometry:
        raise NotImplementedError(
            "canonical callback geometry/bounds reads require Rust-published derived bounds"
        )
    payload = geometry.get(_CALLBACK_BOUNDS)
    if payload is None:
        return _ORIGINAL_BOUNDS(raw)
    if not isinstance(payload, dict) or not isinstance(payload.get("min"), dict) or not isinstance(payload.get("max"), dict):
        raise RuntimeError("canonical callback bounds payload is malformed")
    original = geometry.get("transform")
    if not isinstance(original, dict):
        raise RuntimeError("canonical callback bounds have no source transform")
    original_scale = original["scale"]
    if float(original_scale["x"]) == 0.0 or float(original_scale["y"]) == 0.0:
        raise NotImplementedError("canonical callback bounds are unavailable for zero-scale objects")

    def inverse(point: dict[str, Any]) -> _base.Vec2:
        x = float(point["x"]) - float(original["translation"]["x"])
        y = float(point["y"]) - float(original["translation"]["y"])
        angle = float(original["rotation"])
        cosine, sine = math.cos(angle), math.sin(angle)
        return _base.Vec2(
            (x * cosine + y * sine) / float(original_scale["x"]),
            (-x * sine + y * cosine) / float(original_scale["y"]),
        )

    local_min, local_max = inverse(payload["min"]), inverse(payload["max"])
    corners = (
        _base.Vec2(local_min.x, local_min.y),
        _base.Vec2(local_min.x, local_max.y),
        _base.Vec2(local_max.x, local_min.y),
        _base.Vec2(local_max.x, local_max.y),
    )
    transform = raw.transform
    scale = transform["scale"]
    angle = float(transform["rotation"])
    cosine, sine = math.cos(angle), math.sin(angle)
    translation = transform["translation"]
    world = [
        _base.Vec2(
            (corner.x * float(scale["x"])) * cosine -
            (corner.y * float(scale["y"])) * sine + float(translation["x"]),
            (corner.x * float(scale["x"])) * sine +
            (corner.y * float(scale["y"])) * cosine + float(translation["y"]),
        )
        for corner in corners
    ]
    return (
        _base.Vec2(min(point.x for point in world), min(point.y for point in world)),
        _base.Vec2(max(point.x for point in world), max(point.y for point in world)),
    )


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

    context = _CanonicalCallbackContext(frame)
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
    _base.Mobject._current_raw = _current_raw
    _base.Mobject._apply = _apply
    _base._bounds = _callback_bounds
    _install_rotating_breadth()
    _INSTALLED = True
