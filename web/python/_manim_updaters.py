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
from dataclasses import dataclass, field, replace
from typing import Any, Callable

import noon as _base
from _noon_errors import engine_await, engine_call, raise_engine_error

_NEXT_SESSION_ID = 0
_TRACKED_MOBJECTS: list[_base.Mobject] = []
_CANONICAL_SESSIONS: dict[int, "_CanonicalCallbackSession"] = {}
_ACTIVE_CONTEXTS: dict[int, Any] = {}
_ACTIVE_CANONICAL_CONTEXT: ContextVar["_CanonicalCallbackContext | None"] = ContextVar(
    "noon_active_canonical_callback", default=None
)



def _coordinate_operations():
    import _manim_shared_geometry
    return _manim_shared_geometry


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
    if scene is None:
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
    pending_callback_context: "_CanonicalCallbackContext | None" = None
    pending_region_contexts: list["_CanonicalCallbackContext"] = field(default_factory=list)

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
    """Phase-local effective rows plus Rust-owned existing-handle membership staging.

    Python can read and write the prepared effective rows, then stage ordered
    membership edits for already-bound handles through the pinned Rust player.
    The player retains semantic identity, transaction ownership, and the one
    callback publication; Python only delays wrapper associations until commit.
    """

    def __init__(
        self,
        frame: dict[str, Any],
        authoring_context: object,
        *,
        callback_player: object | None = None,
        scene: _base.Scene | None = None,
    ) -> None:
        self.time = float(frame["time"])
        self.delta_time = float(frame["delta_time"])
        self.token = frame["token"]
        self.region = int(frame.get("region", 0))
        self._authoring_context = authoring_context
        self._callback_player = callback_player
        self._scene = scene
        self._operations = authoring_context
        self._frame_items = {
            _phase_node_key(item["node"]): item for item in frame["objects"]
        }
        self._rows: dict[tuple[int, int], _PhasePropertyRow] = {}
        self._signals: dict[tuple[int, int], float] = {}
        self._prefetch_errors: dict[tuple[int, int], Exception] = {}
        self._next_read_request_id = 0
        self._writes: list[dict[str, Any]] = []
        # Python retains only delayed wrapper bookkeeping. The player owns the
        # one typed semantic transaction and is the sole publication authority.
        self._membership_finalizers: list[Callable[[], None]] = []
        self._membership_wrappers: dict[str, object] = {}
        self._next_provisional_binding_id: int | None = None

    def stage_membership(self, batch: object, finalize: Callable[[], None]) -> None:
        if self._callback_player is None:
            raise NotImplementedError(
                "callback structural staging requires the pinned semantic execution player"
            )
        engine_call(
            self._callback_player.stageCallbackMembership,
            json.dumps(self.token, separators=(",", ":")),
            batch,
            operation="callback.membership",
        )
        self._membership_finalizers.append(finalize)

    def stage_provisional_geometry(self, options: object) -> object:
        """Create one phase-local geometry object through the pinned player."""
        if self._callback_player is None:
            raise NotImplementedError(
                "callback provisional construction requires the pinned semantic execution player"
            )
        return engine_call(
            self._callback_player.stageCallbackProvisionalGeometry,
            json.dumps(self.token, separators=(",", ":")),
            options,
            operation="callback.provisional_geometry",
        )

    def provisional_shift(self, provisional: object, offset: _base.Vec2) -> None:
        """Stage one authored construction translation, never an effective row."""
        if self._callback_player is None:
            raise NotImplementedError(
                "callback provisional construction requires the pinned semantic execution player"
            )
        engine_call(
            self._callback_player.stageCallbackProvisionalShift,
            json.dumps(self.token, separators=(",", ":")),
            provisional,
            float(offset.x),
            float(offset.y),
            operation="callback.provisional_geometry",
        )

    def provisional_set_fill(
        self,
        provisional: object,
        color: _base.Color,
        opacity: float | None,
    ) -> None:
        """Stage an authored fill beside the provisional declaration."""
        if self._callback_player is None:
            raise NotImplementedError(
                "callback provisional construction requires the pinned semantic execution player"
            )
        engine_call(
            self._callback_player.stageCallbackProvisionalFill,
            json.dumps(self.token, separators=(",", ":")),
            provisional,
            float(color.red),
            float(color.green),
            float(color.blue),
            float(color.alpha),
            None if opacity is None else float(opacity),
            operation="callback.provisional_geometry",
        )

    def provisional_center(self, provisional: object) -> _base.Vec2:
        """Read the prepared local declaration without assigning it a node ID."""
        if self._callback_player is None:
            raise NotImplementedError(
                "callback provisional construction requires the pinned semantic execution player"
            )
        point = engine_call(
            self._callback_player.callbackProvisionalCenter,
            json.dumps(self.token, separators=(",", ":")),
            provisional,
            operation="callback.provisional_geometry",
        )
        return _base.Vec2(float(point.x), float(point.y))

    def reserve_provisional_binding(self, scene: _base.Scene, mobject: _base.Mobject, provisional: object):
        """Reserve a callback-local derived wrapper ID without assigning a node ID.

        Several separate Scene.add calls can be staged before Rust publishes any
        binding. This counter prevents their delayed Python wrappers from
        selecting the same derived object ID.
        """
        if self._next_provisional_binding_id is None:
            self._next_provisional_binding_id = scene._next_object_id
        object_id = self._next_provisional_binding_id
        self._next_provisional_binding_id += 1
        from _manim_scene import _reserve_typed_binding
        return _reserve_typed_binding(mobject, scene, provisional, None, object_id=object_id)

    @staticmethod
    def provisional_membership_key(provisional: object) -> str:
        """Read the opaque phase-local key used by the Rust membership view."""
        key = getattr(provisional, "localKey", None)
        if key is None:
            raise RuntimeError("callback provisional object has no local membership key")
        return str(key)

    def resolve_provisional(self, provisional: object) -> object:
        """Redeem one phase-local name after Rust committed the whole callback."""
        if self._callback_player is None:
            raise RuntimeError("callback provisional construction has no pinned player")
        return engine_call(
            self._callback_player.resolveCallbackProvisionalMobject,
            json.dumps(self.token, separators=(",", ":")),
            provisional,
            operation="callback.provisional_geometry",
        )

    def associate_published(self, batch: object) -> None:
        engine_call(
            self._callback_player.associatePublishedCallbackMobjects,
            self._authoring_context,
            batch,
            operation="callback.membership_binding",
        )

    def membership_root_keys(self) -> list[str] | None:
        if self._callback_player is None:
            return None
        return [str(key) for key in engine_call(
            self._callback_player.callbackMembershipRootKeys,
            json.dumps(self.token, separators=(",", ":")),
            operation="callback.membership_read",
        )]

    def finalize_membership(self) -> None:
        # All finalizers were validated before the Rust commit. They mutate
        # derived Python wrapper associations only after its one publication.
        for finalize in self._membership_finalizers:
            finalize()
        self._membership_finalizers.clear()

    def discard_membership(self) -> None:
        self._membership_finalizers.clear()
        self._membership_wrappers.clear()

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
            raise_engine_error(error, operation="callback.read")
        expected_kind = "scalar" if kind == "scalar_signal" else kind
        if not isinstance(result, dict) or result.get("kind") != expected_kind:
            raise RuntimeError("canonical callback sparse read returned the wrong typed value")
        return result

    async def _read_scalar_async(self, key: tuple[int, int]) -> float:
        """Read the same Rust-pinned phase without suspending a Python stack."""
        from js import noonReadSemanticContinuationCallback

        request_id = self._next_read_request_id
        self._next_read_request_id += 1
        request = {"request_id": request_id, "kind": "scalar_signal", "node": _phase_node_json(key)}
        raw = await engine_await(noonReadSemanticContinuationCallback(
            self._authoring_context,
            json.dumps(self.token, separators=(",", ":")),
            json.dumps(request, separators=(",", ":")),
        ), operation="callback.read")
        result = json.loads(str(raw))
        if not isinstance(result, dict) or result.get("kind") != "scalar":
            raise RuntimeError("canonical callback scalar prefetch returned the wrong typed value")
        return _phase_number("scalar callback read", result.get("value"))

    async def prefetch_captured_scalars(self, callbacks, tracker_type) -> None:
        """Resolve direct Python captures, never invoke callbacks or user getters.

        These are optional phase-local read hints, not authored signal values or
        a callback dependency graph. Rust validates every read against the pinned
        token. Unused/invalid speculative reads must not change callback behavior;
        defer their errors until an actual scalar read. Dynamic misses keep the
        existing suspended-read contract instead of replaying callback effects.
        """
        from types import FunctionType, MethodType

        seen_functions = set()
        for callback in callbacks:
            function = callback.__func__ if isinstance(callback, MethodType) else callback
            if not isinstance(function, FunctionType) or id(function) in seen_functions:
                continue
            seen_functions.add(id(function))
            values = list(function.__defaults__ or ())
            values.extend((function.__kwdefaults__ or {}).values())
            for cell in function.__closure__ or ():
                try:
                    values.append(cell.cell_contents)
                except ValueError:
                    pass
            values.extend(function.__globals__[name] for name in function.__code__.co_names
                          if name in function.__globals__)
            for value in values:
                if type(value) is not tracker_type:
                    continue
                if inspect.getattr_static(value, "_canonical_context", None) is not self._authoring_context:
                    continue
                handle = inspect.getattr_static(value, "_canonical_handle", None)
                if handle is None or isinstance(handle, property):
                    continue
                key = (int(handle.semanticSlot), int(handle.semanticGeneration))
                if key in self._signals or key in self._prefetch_errors:
                    continue
                try:
                    self._signals[key] = await self._read_scalar_async(key)
                except Exception as error:
                    self._prefetch_errors[key] = error

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
        if key in self._prefetch_errors:
            raise_engine_error(self._prefetch_errors[key], operation="callback.read")
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

    def _family_rows(self, family):
        # Rust selects the unique leaves and reads them against this phase token
        # in one request. Python only retains the permitted callback read view.
        from _noon_errors import engine_call
        revision = str(self.token["publication"]["scene_revision"])
        keys = [tuple(int(part) for part in str(key).split(":")) for key in
                engine_call(self._operations.callbackFamilyKeys, family, revision)]
        if any(node not in self._rows and node not in self._frame_items for node in keys):
            family_key = (int(family.semanticSlot), int(family.semanticGeneration))
            result = self._read("family", family_key)
            items = result.get("objects")
            if not isinstance(items, list):
                raise RuntimeError("family callback read did not return object rows")
            received = {_phase_node_key(item["node"]): item for item in items}
            if set(received) != set(keys):
                raise RuntimeError("family callback read returned different membership")
        else:
            received = self._frame_items
        rows = {node: self._rows.get(node) or _PhasePropertyRow.from_wire(received[node])
                for node in keys}
        return rows

    def paint_family(self, family, operation, arguments):
        from _noon_errors import engine_call
        rows = self._family_rows(family)
        # Row selection preserves preceding writes, including scalar leaf edits.
        styles = [[*node, row.style.to_wire()] for node, row in rows.items()]
        if operation == "Color":
            paint = (True, *arguments, None, None)
        elif operation == "Fill":
            paint = (*arguments[:5], None, arguments[5])
        elif operation == "Stroke":
            paint = arguments
        else:
            paint = (False, 0.0, 0.0, 0.0, 1.0, None, arguments[0])
        raw = engine_call(self._operations.callbackFamilyPaint, family,
            str(self.token["publication"]["scene_revision"]), operation,
            json.dumps(styles, separators=(",", ":")), *paint)
        # Decode and validate every returned row before exposing any writes, so
        # user code may catch a failure without a partially changed family.
        changes = []
        for slot, generation, style in json.loads(str(raw)):
            node = (slot, generation)
            if node not in rows:
                raise RuntimeError("family paint returned an unread semantic node")
            changes.append((node, _PhaseStyle.from_wire(style)))
        self._rows.update(rows)
        for node, style in changes:
            row = rows[node]
            before = row.style
            row.style = style
            if ((style.stroke is None) != (before.stroke is None) or
                    (style.stroke is not None and style.stroke_width != before.stroke_width)):
                row.invalidate_bounds()
            self.style_changed(node, before, row)

    def shift_family(self, family, offset):
        from _noon_errors import engine_call
        rows = self._family_rows(family)
        def bounds_wire(row):
            if row.bounds is None or not row.bounds_translation_only:
                return None
            x0, y0, x1, y1 = row.bounds
            return {"min": {"x": x0, "y": y0}, "max": {"x": x1, "y": y1}}
        wire = [[*node, row.transform.to_wire(), bounds_wire(row)] for node, row in rows.items()]
        raw = engine_call(self._operations.callbackFamilyShift, family,
            str(self.token["publication"]["scene_revision"]),
            json.dumps(wire, separators=(",", ":")), offset.x, offset.y,
            operation="Group.shift")
        # Decode the complete Rust result before exposing any property/write.
        changes = []
        for slot, generation, transform, bounds in json.loads(str(raw)):
            node = (slot, generation)
            if node not in rows:
                raise RuntimeError("family translation returned an unread semantic node")
            translated = _PhasePropertyRow.from_wire({"transform": transform,
                "style": rows[node].style.to_wire(), "bounds": bounds})
            changes.append((node, translated))
        self._rows.update(rows)
        for node, translated in changes:
            row = rows[node]
            before = row.transform
            row.transform = translated.transform
            row.bounds = translated.bounds
            row.bounds_translation_only = translated.bounds_translation_only
            self.transform_changed(node, before, row)

    def transform_changed(
        self, key: tuple[int, int], before: _PhaseTransform, row: _PhasePropertyRow
    ) -> None:
        # These are non-authoritative property deltas. Rust owns validation,
        # driver arbitration and application to the prepared effective row.
        old, new = before.to_wire(), row.transform.to_wire()
        for channel in ("translation", "rotation", "scale"):
            if old[channel] != new[channel]:
                self._writes.append({"kind": channel, "object": _phase_node_json(key),
                                     channel: new[channel]})

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

    def paint_set_opacity(self, style: _PhaseStyle, opacity: float):
        from _noon_errors import engine_call
        result = engine_call(self._operations.callbackPaintSetOpacity,
            *_phase_optional_color_args(style.fill),
            *_phase_optional_color_args(style.stroke), opacity,
            operation="VMobject.set_opacity")
        return (_phase_callback_paint_color(result, "fill"),
                _phase_callback_paint_color(result, "stroke"))

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
        old, new = before.to_wire(), row.style.to_wire()
        for channel in ("fill", "stroke", "stroke_width", "opacity"):
            if old[channel] != new[channel]:
                self._writes.append({"kind": channel, "object": _phase_node_json(key),
                                     channel: new[channel]})

    def effective_batch(self) -> dict[str, Any]:
        return {"token": self.token, "region": self.region, "writes": self._writes}


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


def canonical_callback_phase_active() -> bool:
    """Whether this invocation is inside the existing ordered callback phase."""
    return _ACTIVE_CANONICAL_CONTEXT.get() is not None


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


def _canonical_provisional_context(
    mobject: _base.Mobject,
) -> tuple[_CanonicalCallbackContext, object] | None:
    """Return one exact callback-local construction capability.

    The marker has no semantic slot or generation. It is accepted only while
    the owning callback phase is active, so ordinary callback rows retain
    their batched effective-write path.
    """
    context = getattr(mobject, "_callback_provisional_context", None)
    provisional = getattr(mobject, "_callback_provisional_handle", None)
    active = _ACTIVE_CANONICAL_CONTEXT.get()
    if (not isinstance(context, _CanonicalCallbackContext)
            or not isinstance(active, _CanonicalCallbackContext)
            or provisional is None
            or active._scene is not context._scene
            or active._authoring_context is not context._authoring_context
            or active.token != context.token):
        return None
    # A later host region reads the same transaction-local object through its
    # current region view. The constructor's Python context is not authority.
    return active, provisional


def active_callback_membership_context(scene: _base.Scene) -> _CanonicalCallbackContext | None:
    """Return the one phase-local structural collector for this exact Scene."""
    context = _ACTIVE_CANONICAL_CONTEXT.get()
    if isinstance(context, _CanonicalCallbackContext) and context._scene is scene:
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
    return _base._semantic_operations()._current_raw(self)


def _canonical_apply(self: _base.Mobject, raw: object) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        raise NotImplementedError(
            "canonical callbacks support property operations only; raw replacement is unsupported"
        )
    return _base._semantic_operations()._apply(self, raw)


def _canonical_get_center(self: _base.Mobject) -> _base.Vec2:
    provisional = _canonical_provisional_context(self)
    if provisional is not None:
        context, local = provisional
        return context.provisional_center(local)
    value = _canonical_row(self)
    if value is None:
        return _base._semantic_operations()._get_center(self)
    _, _, row = value
    return row.center()


def _canonical_shift(self: _base.Mobject, direction: object) -> _base.Mobject:
    provisional = _canonical_provisional_context(self)
    if provisional is not None:
        context, local = provisional
        context.provisional_shift(local, _base._as_vec2(direction))
        return self
    value = _canonical_row(self)
    if value is None:
        return _base._semantic_operations()._shift(self, direction)
    context, key, row = value
    before = row.transform
    row.shift(_base._as_vec2(direction))
    context.transform_changed(key, before, row)
    return self


def _canonical_move_to(self: _base.Mobject, point: object, *args: object, **kwargs: object) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _base._semantic_operations()._move_to(self, point, *args, **kwargs)
    if args or kwargs:
        raise NotImplementedError("callback move_to currently supports center point placement only")
    _, _, row = value
    return _canonical_shift(self, _base._as_vec2(point) - row.center())


def _canonical_set_x(self: _base.Mobject, x: float, direction: object = _base.ORIGIN) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _coordinate_operations()._set_x(self, x, direction)
    if _base._as_vec2(direction) != _base.ORIGIN:
        raise NotImplementedError("callback coordinate placement supports center coordinates only")
    _, _, row = value
    return _canonical_shift(self, _base.Vec2(float(x) - row.center().x, 0.0))


def _canonical_set_y(self: _base.Mobject, y: float, direction: object = _base.ORIGIN) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _coordinate_operations()._set_y(self, y, direction)
    if _base._as_vec2(direction) != _base.ORIGIN:
        raise NotImplementedError("callback coordinate placement supports center coordinates only")
    _, _, row = value
    return _canonical_shift(self, _base.Vec2(0.0, float(y) - row.center().y))


def _canonical_scale(self: _base.Mobject, *args: object, **kwargs: object) -> _base.Mobject:
    if _canonical_phase_context(self) is not None:
        raise NotImplementedError(
            "canonical callback scale is not supported; use shared semantic operations"
        )
    return _base._semantic_operations()._scale(self, *args, **kwargs)


def _canonical_rotate(self: _base.Mobject, *args: object, **kwargs: object) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _base._semantic_operations()._rotate(self, *args, **kwargs)
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
    pivot = _base._as_vec2(about_point)
    before = row.transform
    row.transform = context.rotate_transform_about_point(before, angle, pivot)
    row.invalidate_bounds()
    context.transform_changed(key, before, row)
    return self

def _canonical_set_color(self: _base.Mobject, color: _base.Color) -> _base.Mobject:
    value = _canonical_row(self)
    if value is None:
        return _base._semantic_operations()._set_color(self, color)
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
    provisional = _canonical_provisional_context(self)
    if provisional is not None:
        if color is None:
            raise NotImplementedError(
                "callback provisional fill requires an explicit Color in this slice"
            )
        context, local = provisional
        context.provisional_set_fill(local, color, opacity)
        return self
    value = _canonical_row(self)
    if value is None:
        return _base._semantic_operations()._set_fill(self, color, opacity)
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
        return _base._semantic_operations()._set_stroke(self, color, width)
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
        return _base._semantic_operations()._set_object_opacity(self, opacity)
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
        from _manim_compat import _as_color

        return _canonical_set_color(self, _as_color("color", color))
    return _base._semantic_operations()._set_vmobject_color(self, color, family=family)


def _canonical_vmobject_set_fill(
    self: _base.Mobject,
    color: object = None,
    opacity: float | None = None,
    family: bool = True,
) -> _base.Mobject:
    if (_canonical_phase_context(self) is not None
            or _canonical_provisional_context(self) is not None):
        del family
        if color is not None:
            from _manim_compat import _as_color

            color = _as_color("fill color", color)
        return _canonical_set_fill(self, color, opacity)
    return _base._semantic_operations()._set_fill(self, color=color, opacity=opacity, family=family)


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
            from _manim_compat import _as_color

            color = _as_color("stroke color", color)
        return _canonical_set_stroke(self, color, width)
    return _base._semantic_operations()._set_stroke(
        self, color=color, width=width, opacity=opacity, family=family
    )


def _canonical_vmobject_set_opacity(
    self: _base.Mobject,
    opacity: float,
    family: bool = True,
) -> _base.Mobject:
    context = _canonical_phase_context(self)
    if context is not None:
        del family
        from _manim_compat import _opacity

        opacity = _opacity("opacity", opacity)
        key, row = context.row(self)
        before = row.style
        fill, stroke = context.paint_set_opacity(before, opacity)
        row.style = replace(before, fill=fill, stroke=stroke)
        context.style_changed(key, before, row)
        return self
    return _base._semantic_operations()._set_opacity(self, opacity, family=family)


def _carry_callback_region_state(
    session: _CanonicalCallbackSession, context: _CanonicalCallbackContext
) -> None:
    if session.pending_region_contexts:
        previous = session.pending_region_contexts[-1]
        if previous.token == context.token:
            context._next_provisional_binding_id = previous._next_provisional_binding_id
            context._membership_wrappers = previous._membership_wrappers


async def prepare_canonical_callback_phase(session_id: int, frame: dict[str, Any]):
    """Prepare bounded capture reads before executing this callback phase once."""
    session = _CANONICAL_SESSIONS[int(session_id)]
    context = _CanonicalCallbackContext(frame, session.context, scene=session.scene)
    _carry_callback_region_state(session, context)
    from _manim_reactive import ValueTracker

    callbacks = [session.callbacks[int(item["callback_id"])] for item in frame.get("invocations", [])]
    await context.prefetch_captured_scalars(callbacks, ValueTracker)
    return context


def run_canonical_callback_phase(
    session_id: int, frame: dict[str, Any], *, prepared_context=None, callback_player=None
) -> str:
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

    context = prepared_context or _CanonicalCallbackContext(
        frame,
        session.context,
        callback_player=callback_player,
        scene=session.scene,
    )
    if prepared_context is None:
        _carry_callback_region_state(session, context)
    if prepared_context is not None:
        context._callback_player = callback_player
        context._scene = session.scene
    if (context.token != frame["token"] or context.region != int(frame.get("region", 0))
            or context._authoring_context is not session.context):
        raise RuntimeError("prepared canonical callback reads belong to a different phase")
    scene_key = id(session.scene)
    if scene_key in _ACTIVE_CONTEXTS:
        raise RuntimeError("nested Noon callback phases are not supported")
    _ACTIVE_CONTEXTS[scene_key] = context
    context_token = _ACTIVE_CANONICAL_CONTEXT.set(context)
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
    except Exception:
        context.discard_membership()
        raise
    finally:
        _ACTIVE_CANONICAL_CONTEXT.reset(context_token)
        _ACTIVE_CONTEXTS.pop(scene_key, None)

    session.pending_callback_context = context
    session.pending_region_contexts.append(context)
    return _json_phase(context.effective_batch())


def complete_canonical_callback_phase(session_id: int, frame: dict[str, Any]) -> None:
    """Commit Python-only wrapper bindings after the player published the phase."""
    session = _CANONICAL_SESSIONS[int(session_id)]
    context = session.pending_callback_context
    if (context is None or context.token != frame.get("token")
            or context.region != int(frame.get("region", 0))):
        raise RuntimeError("canonical callback completion does not match the pending phase")
    try:
        for region in session.pending_region_contexts:
            region.finalize_membership()
    finally:
        session.pending_region_contexts.clear()
        session.pending_callback_context = None


def discard_canonical_callback_phase(session_id: int, frame: dict[str, Any]) -> None:
    """Drop delayed wrapper work when the exact Rust callback phase aborts."""
    session = _CANONICAL_SESSIONS.get(int(session_id))
    if session is not None and any(
        region.token == frame.get("token") for region in session.pending_region_contexts
    ):
        for region in session.pending_region_contexts:
            region.discard_membership()
        session.pending_region_contexts.clear()
        session.pending_callback_context = None


def _json_phase(value: object) -> str:
    # This is the explicit Pyodide callback boundary. It is never an in-process
    # Rust engine boundary: the semantic store, compiler plan, session and
    # renderer delta encoder remain in the same WASM runtime.
    import json

    return json.dumps(value, separators=(",", ":"), allow_nan=False)


def release_session(session_id: int) -> None:
    _CANONICAL_SESSIONS.pop(int(session_id), None)
