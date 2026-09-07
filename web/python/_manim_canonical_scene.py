"""Bind geometry handles directly into the shared Rust Scene.

Static geometry lowers to one ExecutionSession in the authoring worker. Python
keeps identity metadata; geometry values are projected only for explicit exports
or the legacy animation/retained-text adapter still owned for deletion by #959.
"""

from __future__ import annotations

import copy
import inspect
import json
import math
import sys
from dataclasses import dataclass
from typing import Any

import _manim_retained_state as _retained_state
import _manim_typst as _typst
import _manim_animation_options as _options
import _manim_animate as _animate
import _manim_compat as _compat
import _manim_composition as _composition
import _manim_reactive as _reactive
import _manim_semantic_handles as _semantic_handles
import _noon_ir as _ir
import noon as _base

try:
    from js import noonCreateCanonicalAuthoringSceneContext as _create_context
except ImportError:  # pragma: no cover - native import smoke only
    _create_context = None


_INSTALLED = False
_CHECKPOINT_TAG = object()
_ORIGINAL_APPEND_SNAPSHOT = _ir.Scene._append_snapshot
_ORIGINAL_AUTHORING_CHECKPOINT = _ir.Scene._authoring_checkpoint
_ORIGINAL_RESTORE_AUTHORING_CHECKPOINT = _ir.Scene._restore_authoring_checkpoint
_ORIGINAL_REPLACE_STATIC_SNAPSHOT = _base.Scene._replace_static_snapshot
_ORIGINAL_BIND = _base.Mobject._bind_to_scene
_ORIGINAL_PLAY = _base.Scene.play
_ORIGINAL_BIND_POSITION = _base.Scene.bind_position
_ORIGINAL_BIND_ROTATION = _base.Scene.bind_rotation
_ORIGINAL_BIND_OPACITY = _base.Scene.bind_opacity
_ORIGINAL_BIND_APPEARANCE = _base.Scene.bind_appearance
_ORIGINAL_BIND_REVEAL = _base.Scene.bind_reveal
_ORIGINAL_BIND_MORPH = _base.Scene.bind_morph
_ORIGINAL_BIND_PRESENCE = _base.Scene.bind_presence
_ORIGINAL_WAIT = _base.Scene.wait
_ORIGINAL_TIME = _base.Scene.time
_ORIGINAL_TO_DOCUMENT = _ir.Scene.to_document
_ORIGINAL_IDENTITY_DOCUMENT = _ir.Scene.identity_document
_ASYNC_CONTINUATION_MODE = "_noon_async_continuation_mode"
_ASYNC_CONTINUATION_PENDING = "_noon_async_continuation_pending"
_SYNCHRONOUS_CONTINUATION_MODE = "_noon_synchronous_continuation_mode"
_EXPORT_DOCUMENT_CONSTRUCT = "_noon_export_document_construct"
_DEFAULT_SYNCHRONOUS_CONTINUATION_CANDIDATE = (
    "_noon_default_synchronous_continuation_candidate"
)


def _json(value: object) -> str:
    # This is a deliberate migration seam for #367. The canonical context owns
    # the scene, while the existing typed/semantic handles still expose their
    # stable snapshot/spec accessors here. Replace these bind/update/finalize
    # payloads with typed WASM handle arguments once the context API can consume
    # those handles directly; JSON must remain a boundary/debug/export format,
    # not a per-frame mutation API.
    return json.dumps(value, separators=(",", ":"), allow_nan=False)


def _context(scene: _ir.Scene):
    context = getattr(scene, "_canonical_authoring_context", None)
    if context is not None:
        return context
    if _create_context is None:
        raise RuntimeError(
            "canonical SceneSpec authoring requires the Noon browser Rust/WASM context"
        )
    context = _create_context()
    scene._canonical_authoring_context = context
    return context


@dataclass(frozen=True)
class _TypedBindingReservation:
    object: _ir.Object
    key: str
    legacy_snapshot: dict[str, Any] | None
    reuse_existing_identity: bool = False


def _reserve_typed_binding(
    mobject: _base.Mobject,
    scene: _base.Scene,
    handle: object,
    key: str | None,
    *,
    object_id: int | None = None,
) -> _TypedBindingReservation:
    if mobject._scene is not None:
        if mobject._scene is scene:
            raise ValueError("Mobject is already bound to this Scene")
        raise ValueError("Mobject already belongs to another Scene")

    # A completed canonical FadeOut removes shared root membership but retains
    # the semantic handle and this wrapper's derived ObjectId. Re-adding that
    # exact handle must use liveAdd, not allocate a second export identity.
    prior = getattr(mobject, "_object", None)
    if prior is not None and prior.id in scene._object_positions:
        prior_key = scene._object_keys.get(prior.id)
        geometry_handles = getattr(scene, "_semantic_geometry_handles", {})
        text_handles = getattr(scene, "_semantic_text_handles", {})
        prior_handle = geometry_handles.get(prior.id, text_handles.get(prior.id))
        if prior_key is not None and prior_handle is handle:
            if key is not None and _ir._authoring_key("key", key, prior_key) != prior_key:
                raise ValueError("a re-added canonical Mobject keeps its existing key")
            return _TypedBindingReservation(
                prior, prior_key, None, reuse_existing_identity=True
            )

    object_id = scene._next_object_id if object_id is None else object_id
    authoring_key = _ir._authoring_key("key", key, f"@object:{object_id}")
    if object_id in scene._object_keys:
        raise ValueError(f"canonical wrapper object identity is already bound: {object_id}")
    if authoring_key in scene._object_key_ids:
        raise ValueError(f"duplicate object key: {authoring_key}")

    legacy_snapshot = None
    if getattr(scene, "_legacy_geometry_materialized", False) and not isinstance(
        mobject, _typst._RetainedTextMobject
    ):
        legacy_snapshot = json.loads(str(handle.snapshotJson()))
        legacy_snapshot["id"] = object_id
    return _TypedBindingReservation(
        _ir.Object(object_id, scene._owner), authoring_key, legacy_snapshot
    )


def _commit_typed_binding(
    mobject: _base.Mobject,
    scene: _base.Scene,
    reservation: _TypedBindingReservation,
    handle: object,
) -> _ir.Object:
    """Commit derived wrapper bookkeeping after the shared Rust bind succeeded."""
    obj = reservation.object
    if reservation.reuse_existing_identity:
        mobject._bind(scene, obj)
        return obj
    scene._object_keys[obj.id] = reservation.key
    scene._object_key_ids[reservation.key] = obj.id
    scene._next_object_id = obj.id + 1
    scene._next_painter_order += 1
    _record_mobject_binding(mobject, scene, obj, handle, reservation.legacy_snapshot)
    return obj


def _bind_mobject(self: _base.Mobject, scene: _base.Scene, *, key=None):
    handle = getattr(self, "_semantic_handle", None)
    if handle is None:
        materialize_legacy_geometry(scene)
        return _ORIGINAL_BIND(self, scene, key=key)
    reservation = _reserve_typed_binding(self, scene, handle, key)
    context = _context(scene)
    if reservation.reuse_existing_identity:
        context.liveAdd(str(reservation.object.id), handle)
    elif str(context.liveExecutionOwnership()) in {"active", "returned", "transferred"}:
        # A resumed source continuation may introduce an object after a
        # play/wait barrier. Rust atomically publishes both root membership and execution-slot
        # enrollment before Python records its derived wrapper identity.
        context.liveAdd(str(reservation.object.id), handle)
    else:
        context.bindMobject(str(reservation.object.id), handle)
    return _commit_typed_binding(self, scene, reservation, handle)


def _bind_camera_frame(scene: _base.Scene, mobject: _base.Mobject) -> _ir.Object:
    """Bind one context-created semantic camera without constructing Python geometry state."""
    if getattr(mobject, "_scene", None) is not None:
        raise ValueError("camera frame is already bound")
    if getattr(scene, "_legacy_geometry_materialized", False):
        raise NotImplementedError(
            "moving camera construction cannot follow legacy geometry materialization"
        )
    object_id = scene._next_object_id
    authoring_key = _ir._authoring_key("key", None, f"@object:{object_id}")
    if object_id in scene._object_keys or authoring_key in scene._object_key_ids:
        raise ValueError("camera frame wrapper identity is already bound")
    reservation = _TypedBindingReservation(
        _ir.Object(object_id, scene._owner), authoring_key, None
    )
    handle = _context(scene).createCameraFrame(str(object_id))
    _semantic_handles._attach_shared_handle(mobject, handle)
    return _commit_typed_binding(mobject, scene, reservation, handle)


def _record_mobject_binding(
    mobject: _base.Mobject,
    scene: _base.Scene,
    obj: _ir.Object,
    handle: object,
    legacy_snapshot: dict[str, Any] | None = None,
) -> None:
    scene._object_positions[obj.id] = len(scene._objects)
    # The compatibility table retains identity only on the shared path.
    scene._objects.append({"id": obj.id})
    if isinstance(mobject, _typst._RetainedTextMobject):
        handles = getattr(scene, "_semantic_text_handles", None)
        if handles is None:
            handles = scene._semantic_text_handles = {}
    else:
        handles = getattr(scene, "_semantic_geometry_handles", None)
        if handles is None:
            handles = scene._semantic_geometry_handles = {}
    handles[obj.id] = handle
    if legacy_snapshot is not None:
        scene._objects[-1] = legacy_snapshot
    mobject._bind(scene, obj)


def materialize_legacy_geometry(scene):
    """Enter the explicit legacy animation/export adapter once (#959)."""
    if getattr(scene, "_legacy_geometry_materialized", False):
        return
    for object_id, handle in getattr(scene, "_semantic_geometry_handles", {}).items():
        snapshot = json.loads(str(handle.snapshotJson()))
        snapshot["id"] = object_id
        scene._objects[scene._object_positions[object_id]] = snapshot
    scene._legacy_geometry_materialized = True


def _canonical_tracker_builder(builder: object) -> bool:
    return (
        isinstance(builder, _reactive._ValueAnimationBuilder)
        and (
            builder.tracker._canonical_context_handle() is not None
            or builder.tracker._detached_canonical_handle() is not None
        )
    )


def _associate_tracker(scene: _base.Scene, tracker: _reactive.ValueTracker) -> None:
    if tracker._canonical_context_handle() is not None:
        return
    handle = tracker._detached_canonical_handle()
    if handle is None:
        return
    tracker._associate_canonical(scene, _context(scene))


def _rust_authored_time(scene: _base.Scene) -> float | None:
    """Read only the canonical Rust authoring cursor when a context exists."""
    context = getattr(scene, "_canonical_authoring_context", None)
    if context is None:
        return None
    return float(context.authoredDuration())


def _legacy_authored_time(scene: _base.Scene) -> float:
    return float(_ORIGINAL_TIME.__get__(scene, type(scene)))


def _timing_authority(scene: _base.Scene) -> tuple[str, float]:
    """Choose one authored cursor during the #959 timing migration.

    Rust owns the scalar path once it has advanced. The retained legacy cursor
    remains available only for scenes that have not entered that path. A scene
    must never merge, synchronize, or select a maximum across both cursors.
    Remove the legacy branch with the #959 timeline adapter.
    """
    rust_time = _rust_authored_time(scene)
    legacy_time = _legacy_authored_time(scene)
    if rust_time is None or rust_time == 0.0:
        return "legacy", legacy_time
    if legacy_time == 0.0:
        return "canonical", rust_time
    raise NotImplementedError(
        "mixed legacy and canonical Scene timing is unsupported during #959 migration"
    )


def _canonical_scene_time(scene: _base.Scene) -> float:
    return _timing_authority(scene)[1]


def _begin_async_continuation_construct(scene: _base.Scene) -> None:
    if getattr(scene, _ASYNC_CONTINUATION_MODE, False):
        raise RuntimeError("canonical async construct is already active")
    setattr(scene, _ASYNC_CONTINUATION_MODE, True)
    setattr(scene, _ASYNC_CONTINUATION_PENDING, set())


def _finish_async_continuation_construct(scene: _base.Scene) -> None:
    pending = getattr(scene, _ASYNC_CONTINUATION_PENDING, set())
    try:
        if pending:
            raise RuntimeError(
                "async construct must await every supported Scene.play/Scene.wait continuation"
            )
    finally:
        setattr(scene, _ASYNC_CONTINUATION_MODE, False)
        setattr(scene, _ASYNC_CONTINUATION_PENDING, set())


def _async_continuation_active(scene: _base.Scene) -> bool:
    return bool(getattr(scene, _ASYNC_CONTINUATION_MODE, False))


def _default_synchronous_continuation_candidate(scene: _base.Scene) -> bool:
    """Whether this ordinary construct may enter one supported JSPI barrier."""
    return bool(getattr(scene, _DEFAULT_SYNCHRONOUS_CONTINUATION_CANDIDATE, False))


def _start_default_synchronous_continuation(scene: _base.Scene) -> None:
    """Enter the existing synchronous continuation only before Rust mutation."""
    if (
        not _default_synchronous_continuation_candidate(scene)
        or getattr(scene, _EXPORT_DOCUMENT_CONSTRUCT, False)
    ):
        return
    if _synchronous_continuation_active(scene):
        return
    if _async_continuation_active(scene):
        raise RuntimeError("canonical async and synchronous constructs cannot overlap")
    from pyodide.ffi import can_run_sync

    if not can_run_sync():
        raise RuntimeError(
            "ordinary synchronous canonical play/wait requires Pyodide JS Promise "
            "Integration in this browser; use async construct or a JSPI-capable browser"
        )
    setattr(scene, _SYNCHRONOUS_CONTINUATION_MODE, True)


def _finish_synchronous_continuation_construct(scene: _base.Scene) -> None:
    setattr(scene, _SYNCHRONOUS_CONTINUATION_MODE, False)


def _synchronous_continuation_active(scene: _base.Scene) -> bool:
    return bool(getattr(scene, _SYNCHRONOUS_CONTINUATION_MODE, False))


def _semantic_continuation_active(scene: _base.Scene) -> bool:
    return _async_continuation_active(scene) or _synchronous_continuation_active(scene)


async def execute_construct(
    scene: _base.Scene, *, export_document: bool = False
) -> None:
    """Run one Scene construct lifecycle with its canonical continuation mode."""
    if export_document and inspect.iscoroutinefunction(scene.construct):
        raise RuntimeError(
            "exportDocument cannot run an async Scene construct; "
            "async source requires a semantic continuation renderer"
        )
    canonical = not export_document and (
        _create_context is not None
        or getattr(scene, "_canonical_authoring_context", None) is not None
    )
    token = _reactive._enter_authoring_scene(scene if canonical else None)
    try:
        scene.setup()
        try:
            if export_document:
                # #959 owns this explicit codec/export boundary. Ordinary supported
                # operations retain their existing Rust endpoint helpers here; they
                # must not request a renderer continuation lease from an exporter.
                setattr(scene, _EXPORT_DOCUMENT_CONSTRUCT, True)
                try:
                    scene.construct()
                finally:
                    setattr(scene, _EXPORT_DOCUMENT_CONSTRUCT, False)
            elif inspect.iscoroutinefunction(scene.construct):
                _begin_async_continuation_construct(scene)
                try:
                    await scene.construct()
                finally:
                    _finish_async_continuation_construct(scene)
            else:
                # Do not probe JSPI for a static or legacy-only ordinary construct.
                # The first supported canonical segment preflights it before Rust
                # creates or activates that segment, so unsupported browsers fail
                # without selecting endpoint-only execution for the same operation.
                setattr(scene, _DEFAULT_SYNCHRONOUS_CONTINUATION_CANDIDATE, True)
                try:
                    scene.construct()
                finally:
                    setattr(scene, _DEFAULT_SYNCHRONOUS_CONTINUATION_CANDIDATE, False)
                    _finish_synchronous_continuation_construct(scene)
        finally:
            scene.tear_down()
    finally:
        _reactive._leave_authoring_scene(token)


class _SemanticContinuationAwaitable:
    """One consumed Python await over the worker-owned semantic endpoint lease."""

    def __init__(self, scene: _base.Scene, on_complete=None) -> None:
        self._scene = scene
        self._on_complete = on_complete
        self._consumed = False
        getattr(scene, _ASYNC_CONTINUATION_PENDING).add(self)

    def __await__(self):
        if self._consumed:
            raise RuntimeError("a Scene.play/Scene.wait continuation can be awaited only once")
        self._consumed = True
        return self._wait().__await__()

    async def _wait(self) -> _base.Scene:
        try:
            await _await_semantic_continuation(self._scene)
            if self._on_complete is not None:
                self._on_complete()
            return self._scene
        finally:
            getattr(self._scene, _ASYNC_CONTINUATION_PENDING).discard(self)


def _continuation_awaitable(
    scene: _base.Scene, on_complete=None
) -> _SemanticContinuationAwaitable:
    if not _async_continuation_active(scene):
        raise RuntimeError("semantic continuation awaitable requires async construct")
    return _SemanticContinuationAwaitable(scene, on_complete)


def _require_semantic_continuation_active(scene: _base.Scene) -> None:
    if not _semantic_continuation_active(scene):
        return
    from js import noonRequireSemanticContinuationActive

    noonRequireSemanticContinuationActive(_context(scene))


def _prepare_semantic_continuation_callbacks(
    scene: _base.Scene, context: object
) -> None:
    """Publish Python callable identity before Rust lowers the live session.

    Rust owns callback occurrence selection, phase timing, and the token that
    accepts this one batch. Python supplies only its existing callable table so
    a suspended source stack can service a Rust-issued phase without opening a
    second interpreter turn.
    """

    import _manim_updaters

    session_id = _manim_updaters.prepare_canonical_callbacks(scene, context)
    if session_id is None:
        session_id = _manim_updaters.canonical_callback_session_id(scene)
    if session_id is None:
        return
    from js import noonSetSemanticContinuationCallbackSession

    noonSetSemanticContinuationCallbackSession(context, int(session_id))


def _continuation_event(event_json: object) -> dict[str, object]:
    try:
        event = json.loads(str(event_json))
    except (TypeError, ValueError) as error:
        raise RuntimeError("semantic continuation returned invalid event JSON") from error
    if not isinstance(event, dict) or not isinstance(event.get("kind"), str):
        raise RuntimeError("semantic continuation event is missing its kind")
    return event


def _service_semantic_continuation_event(
    scene: _base.Scene, event_json: object
) -> object | None:
    """Service one Rust-issued callback phase on the suspended source stack.

    The returned JavaScript promise resolves to the next Rust event. ``None``
    means the segment completed and the user construct may continue. This keeps
    no Python cursor, callback schedule, or phase identity.
    """

    event = _continuation_event(event_json)
    kind = event["kind"]
    if kind == "complete":
        return None
    if kind != "callback":
        raise RuntimeError(f"unsupported semantic continuation event: {kind}")
    phase = event.get("phase")
    if not isinstance(phase, dict) or not isinstance(phase.get("token"), dict):
        raise RuntimeError("semantic continuation callback event is missing its phase token")

    import _manim_updaters
    from js import (
        noonCompleteSemanticContinuationCallback,
        noonFailSemanticContinuationCallback,
    )

    session_id = _manim_updaters.canonical_callback_session_id(scene)
    if session_id is None:
        raise RuntimeError("semantic continuation callback event has no callable session")
    context = _context(scene)
    token_json = _json(phase["token"])
    try:
        batch_json = _manim_updaters.run_canonical_callback_phase(session_id, phase)
    except Exception as error:
        # Failing the exact pending phase latches terminal Rust state. Its
        # returned promise rejects this suspended construct; no retry occurs.
        return noonFailSemanticContinuationCallback(context, token_json, str(error))
    return noonCompleteSemanticContinuationCallback(context, token_json, batch_json)


async def _await_semantic_continuation(scene: _base.Scene) -> None:
    from js import noonAwaitSemanticContinuation

    event_json = await noonAwaitSemanticContinuation(_context(scene))
    while True:
        next_event = _service_semantic_continuation_event(scene, event_json)
        if next_event is None:
            return
        event_json = await next_event


def _synchronous_continuation_wait(scene: _base.Scene) -> _base.Scene:
    """Suspend the current JSPI-enabled Python stack on the worker lease."""
    if not _synchronous_continuation_active(scene):
        raise RuntimeError("synchronous semantic continuation is not active")
    from js import noonAwaitSemanticContinuation
    from pyodide.ffi import run_sync

    event_json = run_sync(noonAwaitSemanticContinuation(_context(scene)))
    while True:
        next_event = _service_semantic_continuation_event(scene, event_json)
        if next_event is None:
            break
        event_json = run_sync(next_event)
    return scene


def _canonical_wait(
    scene: _base.Scene, duration: float = 1.0
) -> _base.Scene | _SemanticContinuationAwaitable:
    if (
        _default_synchronous_continuation_candidate(scene)
        and not getattr(scene, "_legacy_geometry_materialized", False)
        and getattr(scene, "_canonical_authoring_context", None) is not None
    ):
        _start_default_synchronous_continuation(scene)
    if _semantic_continuation_active(scene):
        try:
            _require_semantic_continuation_active(scene)
            context = _context(scene)
            _prepare_semantic_continuation_callbacks(scene, context)
            context.beginOrdinaryWait(float(duration))
        except Exception as error:
            raise ValueError(str(error)) from None
        if _async_continuation_active(scene):
            return _continuation_awaitable(scene)
        return _synchronous_continuation_wait(scene)
    authority, _ = _timing_authority(scene)
    if authority == "canonical":
        context = _context(scene)
        try:
            context.ordinaryWait(float(duration))
        except Exception as error:
            raise ValueError(str(error)) from None
        return scene
    return _ORIGINAL_WAIT(scene, duration)


def _declare_wait(scene: _base.Scene, duration: float = 1.0) -> _base.Scene:
    """Declare a pre-execution interval on the shared Rust authoring cursor."""
    if _legacy_authored_time(scene) != 0.0:
        raise NotImplementedError("canonical wait declaration cannot follow legacy timing")
    _context(scene).authoredWait(float(duration))
    return scene


def _play_canonical_tracker(
    self: _base.Scene,
    builder: _reactive._ValueAnimationBuilder,
    *,
    duration: float | None,
    run_time: float | None,
    start_time: float | None,
    easing: str | None,
    rate_func: object | None,
    lag_ratio: float | None,
    kwargs: dict[str, object],
) -> _base.Scene | _SemanticContinuationAwaitable:
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if start_time is not None:
        raise NotImplementedError(
            "canonical ValueTracker.play uses the shared Scene authoring cursor"
        )
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")
    if builder.target_value is None:
        raise ValueError("ValueTracker.animate must call set_value or increment_value")
    resolved = _options.resolve(
        builder_args=_options.builder_args(builder),
        default_lag_ratio=0.0,
        play_run_time=(run_time if run_time is not None else duration),
        play_easing=easing,
        play_rate_func=rate_func,
        play_lag_ratio=lag_ratio,
    )
    if resolved.lag_ratio != 0.0:
        raise NotImplementedError(
            "canonical ValueTracker.play currently supports one scalar track at a time"
        )
    authority, _ = _timing_authority(self)
    if authority != "canonical" and _legacy_authored_time(self) != 0.0:
        raise NotImplementedError(
            "canonical ValueTracker.play cannot follow legacy Scene timing"
        )
    try:
        _start_default_synchronous_continuation(self)
        _require_semantic_continuation_active(self)
        _associate_tracker(self, builder.tracker)
        if builder.tracker._scene is not self:
            raise ValueError("ValueTracker belongs to another Scene")
        context, handle = builder.tracker._canonical_context_handle()
        if context is not _context(self):
            raise ValueError("ValueTracker belongs to another canonical Scene context")
        if _semantic_continuation_active(self):
            _prepare_semantic_continuation_callbacks(self, context)
        method = (
            context.beginOrdinaryValueTrackerPlay
            if _semantic_continuation_active(self)
            else context.declareValueTrackerPlay
        )
        method(
            handle,
            float(builder.target_value),
            float(resolved.run_time),
            str(resolved.rate_func),
        )
    except Exception as error:
        raise ValueError(str(error)) from None
    if _async_continuation_active(self):
        return _continuation_awaitable(self)
    if _synchronous_continuation_active(self):
        return _synchronous_continuation_wait(self)
    return self


def _canonical_affine_animation(
    scene: _base.Scene, animation: object
) -> tuple[_base.Mobject, _base.Mobject, object] | None:
    """Classify one supported ordinary leaf-affine animation without lowering it.

    The returned detached target is already an opaque same-store handle.  Python
    does not create a track, timeline entry, or target snapshot for this path.
    """
    # The final production compatibility bootstrap replaces the early generic
    # builder with `_AlignedAnimationBuilder`. It deliberately does not inherit
    # Noon’s original `_AnimationBuilder`, so accepting only the latter skips
    # this canonical route and incorrectly lowers an ordinary legacy track.
    # Do not accept subclasses here. Several compatibility operations inherit the
    # builder solely to reuse option handling and materialize a target lazily;
    # reading that property before their own dispatcher runs can be invalid.
    if type(animation) in (_base._AnimationBuilder, _compat._CompatAnimationBuilder):
        source, target = animation.source, animation.target
    elif type(animation) is _base.Transform:
        source, target = animation.source, animation.target
    else:
        return None
    if not isinstance(source, _base.Mobject) or source._scene not in (None, scene):
        return None
    if getattr(source, "_semantic_handle", None) is None:
        return None
    if not isinstance(target, _base.Mobject):
        raise NotImplementedError("canonical ordinary animation target must be a Mobject")
    if target._scene is not None:
        raise NotImplementedError("canonical ordinary animation target must be detached")
    if getattr(target, "_semantic_handle", None) is None:
        raise NotImplementedError(
            "canonical ordinary animation requires typed semantic Mobject handles"
        )
    # A detached target may have been authored before live execution began, as in
    # Manim's ordinary `circle = Circle(); Transform(square, circle)` pattern.
    # Rust validates its generational identity, shared-store provenance, detached
    # status, payload, and current publication revision before activation.
    return source, target, animation


def _canonical_affine_options(
    animation: object,
    kwargs: dict[str, object],
    *,
    builder_args: dict[str, object] | None = None,
    allow_family_lag: bool = False,
) -> object | None:
    """Resolve the existing Python play ergonomics before typed Rust preflight."""
    duration = kwargs.get("duration")
    run_time = kwargs.get("run_time")
    easing = kwargs.get("easing")
    rate_func = kwargs.get("rate_func")
    lag_ratio = kwargs.get("lag_ratio")
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if kwargs.keys() - {"duration", "run_time", "start_time", "easing", "rate_func", "lag_ratio"}:
        return None
    if kwargs.get("start_time") is not None:
        return None
    try:
        resolved = _options.resolve(
            builder_args=(
                _options.builder_args(animation) if builder_args is None else builder_args
            ),
            default_lag_ratio=0.0,
            play_run_time=(run_time if run_time is not None else duration),
            play_easing=easing,
            play_rate_func=rate_func,
            play_lag_ratio=lag_ratio,
        )
    except NotImplementedError:
        return None
    if (
        (resolved.lag_ratio != 0.0 and not allow_family_lag)
        or resolved.path_arc != 0.0
        or resolved.reverse_rate_function
    ):
        return None
    return resolved


def _canonical_affine_payload_is_supported(
    scene: _base.Scene,
    source: _base.Mobject,
    target: _base.Mobject,
    animation: object,
    kwargs: dict[str, object],
) -> bool:
    """Ask the shared compiler whether this inert payload can enter live execution."""
    resolved = _canonical_affine_options(animation, kwargs)
    if resolved is None:
        return False
    return bool(_context(scene).ordinaryCanPlayTransformTo(
        getattr(source, "_semantic_handle"),
        getattr(target, "_semantic_handle"),
        float(resolved.run_time),
        str(resolved.rate_func),
    ))


def _canonical_affine_lifecycle_animation(
    scene: _base.Scene, animation: object
) -> tuple[_base.Mobject, object] | None:
    """Classify one inert Grow/Spin/Shrink leaf for shared Rust lifecycle playback."""
    growing = sys.modules.get("_manim_growing")
    if growing is not None and isinstance(animation, growing.GrowFromPoint):
        target = animation.mobject
        if target._scene is not None or getattr(target, "_semantic_handle", None) is None:
            return None
        return target, animation
    if getattr(animation, "_canonical_affine_lifecycle", None) == "shrink":
        target = animation.mobject
        if target._scene not in (None, scene) or getattr(target, "_semantic_handle", None) is None:
            return None
        return target, animation
    return None


def _play_canonical_affine_lifecycle(
    self: _base.Scene,
    target: _base.Mobject,
    animation: object,
    *,
    duration: float | None,
    run_time: float | None,
    start_time: float | None,
    easing: str | None,
    rate_func: object | None,
    lag_ratio: float | None,
    kwargs: dict[str, object],
) -> _base.Scene | _SemanticContinuationAwaitable:
    if start_time is not None or kwargs:
        raise NotImplementedError("canonical affine lifecycle supports only play timing options")
    resolved = _options.resolve(
        builder_args=dict(getattr(animation, "anim_args", {})),
        default_lag_ratio=0.0,
        play_run_time=(run_time if run_time is not None else duration),
        play_easing=easing,
        play_rate_func=rate_func,
        play_lag_ratio=lag_ratio,
    )
    if resolved.lag_ratio != 0.0 or resolved.path_arc != 0.0 or resolved.reverse_rate_function:
        raise NotImplementedError("canonical affine lifecycle does not support lag or path options")
    if getattr(animation, "_canonical_affine_lifecycle", None) == "shrink":
        direction, endpoint, x, y, rotation_offset, color = "remove-to", "effective-center", 0.0, 0.0, 0.0, None
    else:
        direction, endpoint = "introduce-from", "point"
        point = animation.point
        x, y = float(point.x), float(point.y)
        rotation_offset = -float(animation.angle) if animation.__class__.__name__ == "SpinInFromNothing" else 0.0
        color = getattr(animation, "point_color", None)
    rgba = (None, None, None, None) if color is None else (
        float(color.red), float(color.green), float(color.blue), float(color.alpha)
    )
    _start_default_synchronous_continuation(self)
    context = _context(self)
    is_intro = direction == "introduce-from"
    reservation = None
    if target._scene is None:
        reservation = _reserve_typed_binding(target, self, getattr(target, "_semantic_handle"), None)
    object_id = str(reservation.object.id) if reservation is not None else str(target._object.id)
    try:
        _require_semantic_continuation_active(self)
        if _semantic_continuation_active(self):
            _prepare_semantic_continuation_callbacks(self, context)
        method = (
            context.beginOrdinaryAffineLifecycle
            if _semantic_continuation_active(self)
            else context.ordinaryPlayAffineLifecycle
        )
        method(
            object_id,
            getattr(target, "_semantic_handle"),
            direction,
            endpoint,
            x,
            y,
            rotation_offset,
            *rgba,
            float(resolved.run_time),
            str(resolved.rate_func),
        )
    except Exception as error:
        raise ValueError(str(error)) from None
    if reservation is not None:
        _commit_typed_binding(target, self, reservation, getattr(target, "_semantic_handle"))
        register = getattr(self, "_register_top_level", None)
        if register is not None:
            register(target)
    if not is_intro:
        def completed() -> None:
            _reconcile_fade_membership(self, target, "out")
    else:
        completed = lambda: None
    if _async_continuation_active(self):
        return _continuation_awaitable(self, completed)
    if _synchronous_continuation_active(self):
        _synchronous_continuation_wait(self)
        completed()
        return self
    completed()
    return self


def _play_legacy_compatibility(self: _base.Scene, *args, **kwargs):
    """Use the one existing #959 legacy play/export boundary when it is safe."""
    authority, _ = _timing_authority(self)
    if authority == "canonical":
        raise NotImplementedError(
            "legacy Scene.play cannot follow canonical ValueTracker timing"
        )
    context = getattr(self, "_canonical_authoring_context", None)
    ownership = getattr(context, "liveExecutionOwnership", None)
    if callable(ownership) and str(ownership()) in {"active", "transferred", "returned"}:
        raise NotImplementedError(
            "an active canonical session cannot fall back to the legacy animation scheduler"
        )
    # Native Text timeline export stays in the canonical context. Its #959
    # codec is store-derived at finalization, so geometry materialization must
    # not force it through a geometry-only legacy document.
    materialize_legacy_geometry(self)
    return _ORIGINAL_PLAY(self, *args, **kwargs)


def _play_canonical_affine(
    self: _base.Scene,
    source: _base.Mobject,
    target: _base.Mobject,
    animation: object,
    *,
    duration: float | None,
    run_time: float | None,
    start_time: float | None,
    easing: str | None,
    rate_func: object | None,
    lag_ratio: float | None,
    kwargs: dict[str, object],
) -> _base.Scene | _SemanticContinuationAwaitable:
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if start_time is not None:
        raise NotImplementedError(
            "canonical ordinary Scene.play uses the shared session cursor"
        )
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")
    if getattr(self, "_legacy_geometry_materialized", False):
        raise NotImplementedError(
            "canonical ordinary animation cannot follow legacy geometry materialization"
        )
    if _legacy_authored_time(self) != 0.0:
        raise NotImplementedError(
            "canonical ordinary Scene.play cannot follow legacy Scene timing"
        )

    resolved = _options.resolve(
        builder_args=_options.builder_args(animation),
        default_lag_ratio=0.0,
        play_run_time=(run_time if run_time is not None else duration),
        play_easing=easing,
        play_rate_func=rate_func,
        play_lag_ratio=lag_ratio,
    )
    if resolved.lag_ratio != 0.0 or resolved.path_arc != 0.0 or resolved.reverse_rate_function:
        raise NotImplementedError(
            "canonical ordinary Scene.play currently supports one affine transform without path or lag options"
        )
    _start_default_synchronous_continuation(self)
    context = _context(self)
    try:
        _require_semantic_continuation_active(self)
        if _semantic_continuation_active(self):
            _prepare_semantic_continuation_callbacks(self, context)
        method = (
            context.beginOrdinaryTransformTo
            if _semantic_continuation_active(self)
            else context.ordinaryPlayTransformTo
        )
        method(
            getattr(source, "_semantic_handle"),
            getattr(target, "_semantic_handle"),
            float(resolved.run_time),
            str(resolved.rate_func),
        )
    except Exception as error:
        raise ValueError(str(error)) from None
    if _async_continuation_active(self):
        return _continuation_awaitable(self)
    if _synchronous_continuation_active(self):
        return _synchronous_continuation_wait(self)
    return self


def _canonical_fade_animation(
    scene: _base.Scene, animation: object
) -> tuple[_base.Mobject, str] | None:
    """Classify one exact basic FadeIn/FadeOut without legacy lifecycle setup."""
    if type(animation) is _base.FadeIn:
        direction = "in"
    elif type(animation) is _base.FadeOut:
        direction = "out"
    else:
        return None
    target = getattr(animation, "target", None)
    if not isinstance(target, _base.Mobject):
        raise NotImplementedError("canonical ordinary Fade target must be a Mobject")
    if getattr(target, "_semantic_handle", None) is None:
        # Group/retained targets still belong to the existing #959 migration
        # consumer. The leaf classifier must not claim their lifecycle; the
        # shared play boundary rejects fallback once canonical execution starts.
        return None
    if direction == "in":
        if target._scene is not None and target._scene is not scene:
            raise ValueError("FadeIn target already belongs to another Scene")
        if target._scene is scene:
            raise NotImplementedError(
                "canonical FadeIn requires a detached Mobject at activation"
            )
    elif target._scene is not scene:
        raise NotImplementedError(
            "canonical FadeOut target must be bound to this Scene"
        )
    return target, direction


def _canonical_create_animation(
    scene: _base.Scene, animation: object
) -> _base.Mobject | None:
    """Classify one exact detached single-leaf Create."""
    if type(animation) is not _base.Create:
        return None
    target = getattr(animation, "target", None)
    if not isinstance(target, _base.Mobject):
        raise NotImplementedError("canonical ordinary Create target must be a Mobject")
    if getattr(target, "_semantic_handle", None) is None:
        return None
    if target._scene is not None:
        if target._scene is scene:
            raise NotImplementedError("canonical Create requires a detached Mobject")
        raise ValueError("Create target already belongs to another Scene")
    return target


def _canonical_uncreate_animation(
    scene: _base.Scene, animation: object
) -> _base.Mobject | None:
    """Classify one exact detached or direct-bound single-leaf Uncreate."""
    if type(animation) is not _base.Uncreate:
        return None
    target = getattr(animation, "target", None)
    if not isinstance(target, _base.Mobject):
        raise NotImplementedError("canonical ordinary Uncreate target must be a Mobject")
    if getattr(target, "_semantic_handle", None) is None:
        return None
    if target._scene is not None and target._scene is not scene:
        raise ValueError("Uncreate target already belongs to another Scene")
    return target


def _canonical_create_options(animation: object, kwargs: dict[str, object]) -> object | None:
    args = dict(getattr(animation, "anim_args", {}))
    if "introducer" in args and args.pop("introducer") is not True:
        return None
    if "remover" in args and args.pop("remover") is not False:
        return None
    return _canonical_affine_options(animation, kwargs, builder_args=args)


def _canonical_uncreate_options(animation: object, kwargs: dict[str, object]) -> object | None:
    if getattr(animation, "reverse_rate_function", None) is not True:
        return None
    if getattr(animation, "remover", None) is not True:
        return None
    resolved = _canonical_affine_options(animation, kwargs)
    if resolved is None or resolved.rate_func not in {"linear", "smooth"}:
        return None
    return resolved


def _play_canonical_create(
    self: _base.Scene,
    target: _base.Mobject,
    animation: object,
    *,
    remove: bool = False,
    **kwargs: object,
) -> _base.Scene | _SemanticContinuationAwaitable:
    duration = kwargs.pop("duration", None)
    run_time = kwargs.pop("run_time", None)
    start_time = kwargs.pop("start_time", None)
    easing = kwargs.pop("easing", None)
    rate_func = kwargs.pop("rate_func", None)
    lag_ratio = kwargs.pop("lag_ratio", None)
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if start_time is not None:
        raise NotImplementedError("canonical ordinary Scene.play uses the shared session cursor")
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")
    if _legacy_authored_time(self) != 0.0:
        raise NotImplementedError("canonical ordinary Create cannot follow legacy Scene timing")
    option_kwargs = {
        "duration": duration,
        "run_time": run_time,
        "start_time": start_time,
        "easing": easing,
        "rate_func": rate_func,
        "lag_ratio": lag_ratio,
    }
    resolved = (
        _canonical_uncreate_options(animation, option_kwargs)
        if remove
        else _canonical_create_options(animation, option_kwargs)
    )
    if resolved is None:
        raise NotImplementedError(
            "canonical ordinary Create currently supports one basic detached leaf"
        )
    _start_default_synchronous_continuation(self)
    handle = getattr(target, "_semantic_handle")
    reservation = None
    if target._scene is None:
        reservation = _reserve_typed_binding(target, self, handle, None)
        object_id = str(reservation.object.id)
    elif remove and target._scene is self and target._object is not None:
        object_id = str(target._object.id)
    else:
        raise ValueError("canonical Create target must be detached")
    context = _context(self)
    try:
        _require_semantic_continuation_active(self)
        method = (
            (context.beginOrdinaryUncreate if remove else context.beginOrdinaryCreate)
            if _semantic_continuation_active(self)
            else (context.ordinaryPlayUncreate if remove else context.ordinaryPlayCreate)
        )
        method(
            object_id,
            handle,
            float(resolved.run_time),
            str(resolved.rate_func),
        )
    except Exception as error:
        raise ValueError(str(error)) from None
    if reservation is not None:
        _commit_typed_binding(target, self, reservation, handle)
        register = getattr(self, "_register_top_level", None)
        if register is not None:
            register(target)

    def completed() -> None:
        if remove:
            _reconcile_fade_membership(self, target, "out")

    if _async_continuation_active(self):
        return _continuation_awaitable(self, completed)
    if _synchronous_continuation_active(self):
        _synchronous_continuation_wait(self)
        completed()
        return self
    completed()
    return self


def _canonical_fade_endpoint(animation: object) -> tuple[float, str, float, float] | None:
    """Return the inert endpoint values that Rust resolves at activation."""
    shift = getattr(animation, "_fade_shift_vector", None)
    scale_factor = float(getattr(animation, "_fade_scale_factor", float("nan")))
    if shift is None or not math.isfinite(scale_factor):
        return None
    if bool(getattr(animation, "_fade_point_target", False)):
        point = getattr(animation, "_fade_point", None)
        if point is None:
            return None
        return scale_factor, "point", float(point.x), float(point.y)
    return scale_factor, "shift", float(shift.x), float(shift.y)


def _canonical_fade_options(animation: object, kwargs: dict[str, object]) -> object | None:
    """Resolve timing while Rust retains all fade endpoint meaning."""
    if _canonical_fade_endpoint(animation) is None:
        return None
    args = dict(getattr(animation, "anim_args", {}))
    lifecycle = "introducer" if type(animation) is _base.FadeIn else "remover"
    for name in ("introducer", "remover"):
        if name not in args:
            continue
        if name != lifecycle or args.pop(name) is not True:
            return None
    return _canonical_affine_options(animation, kwargs, builder_args=args)


def _fade_object_id(
    scene: _base.Scene, target: _base.Mobject, direction: str
) -> tuple[str, _TypedBindingReservation | None]:
    """Reserve only derived wrapper identity; Rust owns fade membership."""
    handle = getattr(target, "_semantic_handle")
    if direction == "in":
        reservation = _reserve_typed_binding(target, scene, handle, None)
        return str(reservation.object.id), reservation
    if target._object is None:
        raise ValueError("canonical FadeOut target has no wrapper object identity")
    return str(target._object.id), None


def _reconcile_fade_membership(
    scene: _base.Scene, target: _base.Mobject, direction: str
) -> None:
    """Reflect completed shared membership in Python's derived wrapper attachment."""
    if direction != "out":
        return
    context = _context(scene)
    if bool(context.liveContainsMobject(getattr(target, "_semantic_handle"))):
        raise RuntimeError("completed FadeOut still belongs to the canonical Scene")
    if target._scene is not scene:
        raise RuntimeError("FadeOut wrapper binding changed before completion")
    # Preserve ObjectId/key/opaque handle for a same-handle `Scene.add` re-entry.
    # The context's membership query is authoritative; this is only wrapper state.
    target._scene = None
    top_level = getattr(scene, "_compat_top_level", None)
    if top_level is not None:
        scene._compat_top_level = [value for value in top_level if value is not target]


def _play_canonical_fade(
    self: _base.Scene,
    target: _base.Mobject,
    direction: str,
    animation: object,
    *,
    duration: float | None,
    run_time: float | None,
    start_time: float | None,
    easing: str | None,
    rate_func: object | None,
    lag_ratio: float | None,
    kwargs: dict[str, object],
) -> _base.Scene | _SemanticContinuationAwaitable:
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if start_time is not None:
        raise NotImplementedError("canonical ordinary Scene.play uses the shared session cursor")
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")
    if getattr(self, "_legacy_geometry_materialized", False):
        raise NotImplementedError("canonical ordinary Fade cannot follow legacy geometry materialization")
    if _legacy_authored_time(self) != 0.0:
        raise NotImplementedError("canonical ordinary Fade cannot follow legacy Scene timing")

    resolved = _canonical_fade_options(
        animation,
        {
            "duration": duration,
            "run_time": run_time,
            "start_time": start_time,
            "easing": easing,
            "rate_func": rate_func,
            "lag_ratio": lag_ratio,
        },
    )
    if resolved is None:
        raise NotImplementedError(
            "canonical ordinary Fade requires one finite typed leaf endpoint"
        )
    endpoint = _canonical_fade_endpoint(animation)
    assert endpoint is not None
    scale_factor, translation, x, y = endpoint
    _start_default_synchronous_continuation(self)
    object_id, reservation = _fade_object_id(self, target, direction)
    context = _context(self)
    try:
        _require_semantic_continuation_active(self)
        method = (
            context.beginOrdinaryFade
            if _semantic_continuation_active(self)
            else context.ordinaryPlayFade
        )
        method(
            object_id,
            getattr(target, "_semantic_handle"),
            direction,
            scale_factor,
            translation,
            x,
            y,
            float(resolved.run_time),
            str(resolved.rate_func),
        )
    except Exception as error:
        raise ValueError(str(error)) from None
    if reservation is not None:
        _commit_typed_binding(target, self, reservation, getattr(target, "_semantic_handle"))
        register = getattr(self, "_register_top_level", None)
        if register is not None:
            register(target)

    def completed() -> None:
        _reconcile_fade_membership(self, target, direction)

    if _async_continuation_active(self):
        return _continuation_awaitable(self, completed)
    if _synchronous_continuation_active(self):
        _synchronous_continuation_wait(self)
        completed()
        return self
    completed()
    return self


def _canonical_composition_shape(scene: _base.Scene, args: tuple[object, ...]):
    """Return one recursive Rust composition request shape without scheduling it."""
    if len(args) == 1 and isinstance(args[0], _composition.AnimationGroup):
        group = args[0]
        kind = "sequence" if isinstance(group, _composition.Succession) else "parallel"
        return kind, tuple(group.animations), group
    if len(args) > 1:
        return "parallel", args, None
    if len(args) == 1:
        import _manim_rotate as _rotate

        animation = args[0]
        if isinstance(animation, (_animate._AlignedGroupAnimationBuilder, _animate.Indicate)):
            return "parallel", args, None
        if type(animation) is _rotate.Rotate:
            return "parallel", args, None
        affine = _canonical_affine_animation(scene, animation)
        if affine is not None and type(animation) in (
            _base._AnimationBuilder,
            _compat._CompatAnimationBuilder,
        ):
            source, target, _ = affine
            if not math.isclose(
                float(getattr(source, "_semantic_handle").wireRotation),
                float(getattr(target, "_semantic_handle").wireRotation),
                abs_tol=1e-12,
            ):
                return "parallel", args, None
    return None


def _canonical_play_options(kwargs: dict[str, object]) -> float | None:
    duration = kwargs.pop("duration", None)
    run_time = kwargs.pop("run_time", None)
    start_time = kwargs.pop("start_time", None)
    easing = kwargs.pop("easing", None)
    rate_func = kwargs.pop("rate_func", None)
    lag_ratio = kwargs.pop("lag_ratio", None)
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if start_time is not None:
        raise NotImplementedError(
            "canonical ordinary Scene.play uses the shared session cursor"
        )
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")
    if lag_ratio is not None:
        raise NotImplementedError(
            "canonical ordinary composition does not yet support a Scene.play lag_ratio override"
        )
    value = run_time if run_time is not None else duration
    return None if value is None else float(value)


def _canonical_composition_rate_id(kwargs: dict[str, object]) -> str | None:
    easing = kwargs.get("easing")
    rate_func = kwargs.get("rate_func")
    if easing is None and rate_func is None:
        return None
    if easing is not None and rate_func is not None:
        raise ValueError("use either easing or rate_func, not both")
    return str(easing) if easing is not None else (
        _compat._easing_from_rate_func(rate_func) if rate_func is not None else "linear"
    )


def _canonical_composition_child_options(animation: object, kwargs: dict[str, object]):
    resolved = _canonical_affine_options(animation, kwargs)
    if resolved is None or resolved.rate_func not in ("linear", "smooth"):
        raise NotImplementedError(
            "canonical ordinary composition currently requires linear or smooth affine leaves"
        )
    return resolved


def _canonical_family_transform_animation(
    scene: _base.Scene, animation: object
) -> tuple[_compat.Group, _compat.Group, object] | None:
    if isinstance(animation, _animate._AlignedGroupAnimationBuilder):
        source, target = animation.source, animation.target
    elif type(animation) is _base.Transform and isinstance(animation.source, _compat.Group):
        source, target = animation.source, animation.target
    else:
        return None
    if not isinstance(source, _compat.Group):
        return None
    source_leaves = _compat._leaf_mobjects(source)
    if not source_leaves or any(member._scene is not scene for member in source_leaves):
        return None
    if not isinstance(target, _compat.Group) or any(
        member._scene is not None for member in _compat._leaf_mobjects(target)
    ):
        raise NotImplementedError("canonical family Transform target must be detached")
    if getattr(source, "_semantic_family_handle", None) is None or getattr(
        target, "_semantic_family_handle", None
    ) is None:
        raise NotImplementedError("canonical family Transform requires shared family handles")
    return source, target, animation


def _canonical_indicate_animation(
    scene: _base.Scene, animation: object
) -> tuple[object, bool] | None:
    if not isinstance(animation, _animate.Indicate):
        return None
    target = animation.mobject
    family = isinstance(target, _compat.Group)
    handle_name = "_semantic_family_handle" if family else "_semantic_handle"
    bound = (
        bool(_compat._leaf_mobjects(target))
        and all(member._scene is scene for member in _compat._leaf_mobjects(target))
        if family
        else getattr(target, "_scene", None) is scene
    )
    if not isinstance(target, (_base.Mobject, _compat.Group)) or not bound:
        raise ValueError("Indicate target must belong to this Scene")
    if getattr(target, handle_name, None) is None:
        raise NotImplementedError("canonical Indicate requires a shared semantic handle")
    return target, family


def _build_canonical_composition_candidate(
    self: _base.Scene,
    kind: str,
    animations: tuple[object, ...],
    group: object | None,
    kwargs: dict[str, object],
):
    """Build one inert recursive composition tree owned by the WASM context."""
    import _manim_rotate as _rotate

    play_run_time = _canonical_play_options(dict(kwargs))
    composition_run_time = None if group is None else group.run_time
    composition_lag_ratio = 0.0 if group is None else float(group.lag_ratio)
    context = _context(self)
    candidate = context.beginOrdinaryCompositionBuilder(
        kind, composition_run_time, composition_lag_ratio, play_run_time,
    )
    candidate.setCompositionRateFunction(
        _compat._easing_from_rate_func(group.rate_func) if group is not None else "linear"
    )
    # For flat Scene.play arguments, shared child option resolution already
    # applies the play rate. Only an explicit group gets a root rate override.
    play_rate = _canonical_composition_rate_id(kwargs) if group is not None else None
    if play_rate is not None:
        candidate.setPlayRateFunction(play_rate)
    reservations: list[tuple[_base.Mobject, object]] = []
    removals: list[_base.Mobject] = []
    tracker_associations: list[_reactive.ValueTracker] = []
    next_object_id = self._next_object_id

    def reserve(target: _base.Mobject):
        nonlocal next_object_id
        reservation = _reserve_typed_binding(
            target, self, getattr(target, "_semantic_handle"), None, object_id=next_object_id,
        )
        reservations.append((target, reservation))
        next_object_id += 1
        return reservation

    def append_leaf(builder: object, animation: object, child_kwargs: dict[str, object]) -> None:
        nonlocal next_object_id
        if isinstance(animation, _composition.AnimationGroup):
            nested_kind = "sequence" if isinstance(animation, _composition.Succession) else "parallel"
            nested = build(nested_kind, tuple(animation.animations), animation, {})
            builder.appendComposition(nested)
            return
        if _canonical_tracker_builder(animation):
            tracker = animation.tracker
            if animation.target_value is None:
                raise ValueError("ValueTracker.animate must call set_value or increment_value")
            canonical = tracker._canonical_context_handle()
            if canonical is not None:
                tracker_context, handle = canonical
                if tracker._scene is not self or tracker_context is not context:
                    raise ValueError("ValueTracker belongs to another Scene")
            else:
                if tracker._scene is not None:
                    raise ValueError("ValueTracker belongs to another Scene")
                handle = tracker._detached_canonical_handle()
                tracker_associations.append(tracker)
            child = _canonical_composition_child_options(animation, child_kwargs)
            builder.appendValueTracker(
                handle, float(animation.target_value), float(child.run_time), str(child.rate_func),
            )
            return
        if isinstance(animation, _composition.Wait):
            if child_kwargs:
                raise NotImplementedError("Wait inside a composition does not accept play timing overrides")
            builder.appendWait(float(animation.run_time))
            return
        indicate = _canonical_indicate_animation(self, animation)
        if indicate is not None:
            target, family = indicate
            resolved = _options.resolve(
                builder_args=_options.builder_args(animation),
                default_lag_ratio=0.0,
                play_run_time=child_kwargs.get("run_time", child_kwargs.get("duration")),
                play_easing=child_kwargs.get("easing"),
                play_rate_func=child_kwargs.get("rate_func"),
                play_lag_ratio=child_kwargs.get("lag_ratio"),
            )
            if resolved.path_arc != 0.0 or resolved.reverse_rate_function:
                raise NotImplementedError("canonical Indicate does not support path options")
            if resolved.rate_func != "there_and_back":
                raise NotImplementedError(
                    "canonical restoring Indicate requires there_and_back easing"
                )
            color = animation.color
            rgba = tuple(
                float(getattr(color, name))
                for name in ("red", "green", "blue", "alpha")
            )
            method = builder.appendIndicateFamily if family else builder.appendIndicateMobject
            handle = getattr(
                target,
                "_semantic_family_handle" if family else "_semantic_handle",
            )
            method(
                handle,
                float(animation.scale_factor),
                *rgba,
                float(resolved.run_time),
                str(resolved.rate_func),
                float(resolved.lag_ratio),
            )
            return
        family_transform = _canonical_family_transform_animation(self, animation)
        if family_transform is not None:
            source, target, leaf = family_transform
            child = _canonical_affine_options(leaf, child_kwargs, allow_family_lag=True)
            if child is None or child.path_arc != 0.0 or child.reverse_rate_function:
                raise NotImplementedError("unsupported canonical family Transform options")
            if child.rate_func not in ("linear", "smooth"):
                raise NotImplementedError(
                    "canonical family Transform requires linear or smooth easing"
                )
            builder.appendFamilyTransformTo(
                source._semantic_family_handle,
                target._semantic_family_handle,
                float(child.run_time),
                str(child.rate_func),
                float(child.lag_ratio),
            )
            return
        if isinstance(animation, _composition.Add):
            if child_kwargs:
                raise NotImplementedError("Add inside a composition does not accept play timing overrides")
            if not isinstance(animation.mobject, _base.Mobject) or isinstance(animation.mobject, _compat.Group):
                raise NotImplementedError("canonical Add requires one detached typed leaf")
            member = animation.mobject
            if getattr(member, "_semantic_handle", None) is None or member._scene is not None:
                raise NotImplementedError("canonical Add requires one detached typed leaf")
            reservation = reserve(member)
            builder.appendAdd(str(reservation.object.id), getattr(member, "_semantic_handle"), float(animation.run_time), "linear")
            return
        lifecycle = _canonical_affine_lifecycle_animation(self, animation)
        if lifecycle is not None:
            target, lifecycle_animation = lifecycle
            child = _canonical_composition_child_options(lifecycle_animation, child_kwargs)
            if child.lag_ratio != 0.0 or child.path_arc != 0.0 or child.reverse_rate_function:
                raise NotImplementedError("canonical lifecycle leaves do not support lag or path options")
            if getattr(lifecycle_animation, "_canonical_affine_lifecycle", None) == "shrink":
                direction, endpoint, x, y, rotation_offset, color = "remove-to", "effective-center", 0.0, 0.0, 0.0, None
            else:
                direction, endpoint = "introduce-from", "point"
                point = lifecycle_animation.point
                x, y = float(point.x), float(point.y)
                rotation_offset = -float(lifecycle_animation.angle) if type(lifecycle_animation).__name__ == "SpinInFromNothing" else 0.0
                color = getattr(lifecycle_animation, "point_color", None)
            rgba = (None, None, None, None) if color is None else tuple(float(getattr(color, name)) for name in ("red", "green", "blue", "alpha"))
            reservation = reserve(target) if target._scene is None else None
            if direction == "remove-to":
                removals.append(target)
            object_id = str(reservation.object.id) if reservation is not None else ""
            builder.appendAffineLifecycle(object_id, getattr(target, "_semantic_handle"), direction, endpoint, x, y, rotation_offset, *rgba, float(child.run_time), str(child.rate_func))
            return
        fade = _canonical_fade_animation(self, animation)
        if fade is not None:
            target, direction = fade
            child = _canonical_fade_options(animation, child_kwargs)
            if child is None:
                raise NotImplementedError("unsupported canonical fade options")
            endpoint = _canonical_fade_endpoint(animation)
            assert endpoint is not None
            scale_factor, translation, x, y = endpoint
            if direction == "in":
                reservation = reserve(target)
                object_id = str(reservation.object.id)
            else:
                object_id = ""
                removals.append(target)
            builder.appendFade(
                object_id,
                getattr(target, "_semantic_handle"),
                direction,
                scale_factor,
                translation,
                x,
                y,
                float(child.run_time),
                str(child.rate_func),
            )
            return
        created = _canonical_create_animation(self, animation)
        if created is not None:
            child = _canonical_create_options(animation, child_kwargs)
            if child is None:
                raise NotImplementedError("unsupported canonical Create options")
            reservation = reserve(created)
            builder.appendCreate(str(reservation.object.id), getattr(created, "_semantic_handle"), float(child.run_time), str(child.rate_func))
            return
        affine = _canonical_affine_animation(self, animation)
        if affine is not None:
            source, target, leaf = affine
            child = _canonical_composition_child_options(leaf, child_kwargs)
            source_handle = getattr(source, "_semantic_handle")
            target_handle = getattr(target, "_semantic_handle")
            # Resource-backed text has no vector point correspondence. Its
            # transform uses the same Rust affine channels as native Text.
            point_correspondence = (
                not isinstance(source, _typst._RetainedTextMobject)
                and type(leaf) in (_base._AnimationBuilder, _compat._CompatAnimationBuilder)
                and not math.isclose(
                    float(source_handle.wireRotation),
                    float(target_handle.wireRotation),
                    abs_tol=1e-12,
                )
            )
            if source._scene is None:
                reservation = reserve(source)
                method = builder.appendEnteringPointTransformTo if point_correspondence else builder.appendEnteringTransformTo
                method(str(reservation.object.id), source_handle, target_handle, float(child.run_time), str(child.rate_func))
            else:
                method = builder.appendPointTransformTo if point_correspondence else builder.appendTransformTo
                method(source_handle, target_handle, float(child.run_time), str(child.rate_func))
            return
        if type(animation) is _rotate.Rotate:
            target = animation.mobject
            if not isinstance(target, _base.Mobject) or getattr(target, "_semantic_handle", None) is None:
                raise NotImplementedError("canonical Rotate requires a typed Mobject")
            child = _canonical_composition_child_options(animation, child_kwargs)
            angle = float(animation.angle) * _rotate._axis_sign(animation.axis)
            if target._scene is None:
                reservation = reserve(target)
                builder.appendRotate(str(reservation.object.id), target._semantic_handle, angle, float(child.run_time), str(child.rate_func))
            else:
                builder.appendBoundRotate(target._semantic_handle, angle, float(child.run_time), str(child.rate_func))
            return
        raise NotImplementedError(f"canonical composition does not support {type(animation).__name__}")

    def build(root_kind: str, root_animations: tuple[object, ...], root_group: object | None, root_kwargs: dict[str, object]):
        # Recursion uses a fresh inert builder while one reservation accumulator is shared.
        nested = context.beginOrdinaryCompositionBuilder(
            root_kind,
            None if root_group is None else root_group.run_time,
            0.0 if root_group is None else float(root_group.lag_ratio),
            None if root_group is not None else _canonical_play_options(dict(root_kwargs)),
        )
        nested.setCompositionRateFunction(
            _compat._easing_from_rate_func(root_group.rate_func) if root_group is not None else "linear"
        )
        play_rate = _canonical_composition_rate_id(root_kwargs)
        if play_rate is not None:
            nested.setPlayRateFunction(play_rate)
        for child_animation in root_animations:
            append_leaf(nested, child_animation, {} if root_group is not None else root_kwargs)
        return nested

    for animation in animations:
        append_leaf(candidate, animation, {} if group is not None else kwargs)
    try:
        supported = bool(context.ordinaryCanPlayComposition(candidate))
    except Exception as error:
        raise ValueError(str(error)) from None
    return (candidate, reservations, removals, tracker_associations) if supported else False


def _play_canonical_composition(
    self: _base.Scene,
    candidate: object,
    reservations: list[tuple[_base.Mobject, object]],
    removals: list[_base.Mobject],
    tracker_associations: list[_reactive.ValueTracker],
) -> _base.Scene | _SemanticContinuationAwaitable:
    if getattr(self, "_legacy_geometry_materialized", False):
        raise NotImplementedError(
            "canonical ordinary composition cannot follow legacy geometry materialization"
        )
    if _legacy_authored_time(self) != 0.0:
        raise NotImplementedError(
            "canonical ordinary composition cannot follow legacy Scene timing"
        )
    _start_default_synchronous_continuation(self)
    context = _context(self)
    try:
        if _semantic_continuation_active(self):
            _require_semantic_continuation_active(self)
            _prepare_semantic_continuation_callbacks(self, context)
            context.beginOrdinaryComposition(candidate)
        else:
            context.ordinaryPlayComposition(candidate)
    except Exception as error:
        raise ValueError(str(error)) from None
    for tracker in tracker_associations:
        tracker._commit_canonical_association(self, context)
    for source, reservation in reservations:
        handle = getattr(source, "_semantic_handle")
        _commit_typed_binding(source, self, reservation, handle)
        register = getattr(self, "_register_top_level", None)
        if register is not None:
            register(source)
    def completed() -> None:
        for target in removals:
            _reconcile_fade_membership(self, target, "out")

    if _async_continuation_active(self):
        return _continuation_awaitable(self, completed)
    if _synchronous_continuation_active(self):
        _synchronous_continuation_wait(self)
        completed()
        return self
    completed()
    return self


def _play(self, *args, **kwargs):
    # Grow/Spin/Shrink are shared Rust lifecycle operations even while an
    # explicit export document is being authored. Only the other compatibility
    # animations use the document codec below.
    # Single-leaf Grow/Spin/Shrink retain the export-friendly shared lifecycle
    # route. Mixed and grouped lifecycle requests are classified by the shared
    # composition builder below so sibling order and admission stay atomic.
    if len(args) == 1:
        classified = _canonical_affine_lifecycle_animation(self, args[0])
        if classified is not None:
            target, animation = classified
            return _play_canonical_affine_lifecycle(
                self,
                target,
                animation,
                duration=kwargs.pop("duration", None),
                run_time=kwargs.pop("run_time", None),
                start_time=kwargs.pop("start_time", None),
                easing=kwargs.pop("easing", None),
                rate_func=kwargs.pop("rate_func", None),
                lag_ratio=kwargs.pop("lag_ratio", None),
                kwargs=kwargs,
            )
    # An explicit document request authors tracks for its external artifact.
    # Completing a live segment here would discard the exported animation and
    # mix runtime completion with the codec's authored-time cursor.
    if getattr(self, _EXPORT_DOCUMENT_CONSTRUCT, False):
        return _play_legacy_compatibility(self, *args, **kwargs)
    # Once an unsupported compatibility animation has selected the explicit
    # #959 export/materialization boundary, its timing and lowering remain on
    # that path.  Typed handles deliberately survive materialization for
    # identity/export purposes, so they must not reclassify a later ordinary
    # compatibility play as a canonical live animation.
    if getattr(self, "_legacy_geometry_materialized", False):
        if _semantic_continuation_active(self):
            raise NotImplementedError(
                "realtime construct supports only canonical affine Scene.play and Scene.wait"
            )
        return _play_legacy_compatibility(self, *args, **kwargs)

    # Classify every recursive composition before the single-leaf classifiers:
    # multiple arguments and grouped children must share one Rust admission
    # transaction, including mixed Create/Fade/lifecycle/Wait siblings.
    if (shape := _canonical_composition_shape(self, args)) is not None:
        kind, animations, group = shape
        try:
            candidate = _build_canonical_composition_candidate(
                self, kind, animations, group, kwargs
            )
        except NotImplementedError:
            context = getattr(self, "_canonical_authoring_context", None)
            ownership = getattr(context, "liveExecutionOwnership", None)
            if callable(ownership) and str(ownership()) != "none":
                raise
            return _play_legacy_compatibility(self, *args, **kwargs)
        if candidate is False:
            context = _context(self)
            if str(context.liveExecutionOwnership()) != "none":
                raise NotImplementedError(
                    "active canonical execution cannot fall back to the legacy composition scheduler"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        if candidate is not None:
            return _play_canonical_composition(self, *candidate)

    canonical_uncreates = [
        target
        for argument in args
        if (target := _canonical_uncreate_animation(self, argument)) is not None
    ]
    if canonical_uncreates:
        if len(canonical_uncreates) != 1 or len(args) != 1:
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "realtime construct supports one canonical Uncreate per play"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        if _canonical_uncreate_options(args[0], kwargs) is None:
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "realtime construct Uncreate is outside the canonical leaf subset"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        return _play_canonical_create(
            self, canonical_uncreates[0], args[0], remove=True, **kwargs
        )

    canonical_creates = [
        target
        for argument in args
        if (target := _canonical_create_animation(self, argument)) is not None
    ]
    if canonical_creates:
        if len(canonical_creates) != 1 or len(args) != 1:
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "realtime construct supports only flat parallel canonical Create leaves"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        if _canonical_create_options(args[0], kwargs) is None:
            if _semantic_continuation_active(self):
                raise NotImplementedError("realtime construct Create is outside the canonical leaf subset")
            context = getattr(self, "_canonical_authoring_context", None)
            ownership = getattr(context, "liveExecutionOwnership", None)
            if callable(ownership) and str(ownership()) in {"active", "transferred", "returned"}:
                raise NotImplementedError(
                    "active canonical execution cannot fall back to the legacy Create scheduler"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        return _play_canonical_create(self, canonical_creates[0], args[0], **kwargs)

    canonical_fades = [
        classified
        for argument in args
        if (classified := _canonical_fade_animation(self, argument)) is not None
    ]
    if canonical_fades:
        if len(canonical_fades) != 1 or len(args) != 1:
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "realtime construct supports one canonical FadeIn/FadeOut per play"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        target, direction = canonical_fades[0]
        if _canonical_fade_options(args[0], kwargs) is None:
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "realtime construct fade is outside the canonical lifecycle subset"
                )
            context = getattr(self, "_canonical_authoring_context", None)
            ownership = getattr(context, "liveExecutionOwnership", None)
            if callable(ownership) and str(ownership()) in {"active", "transferred", "returned"}:
                raise NotImplementedError(
                    "active canonical execution cannot fall back to the legacy fade scheduler"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        return _play_canonical_fade(
            self,
            target,
            direction,
            args[0],
            duration=kwargs.pop("duration", None),
            run_time=kwargs.pop("run_time", None),
            start_time=kwargs.pop("start_time", None),
            easing=kwargs.pop("easing", None),
            rate_func=kwargs.pop("rate_func", None),
            lag_ratio=kwargs.pop("lag_ratio", None),
            kwargs=kwargs,
        )

    canonical_lifecycles = [
        classified
        for argument in args
        if (classified := _canonical_affine_lifecycle_animation(self, argument)) is not None
    ]
    if canonical_lifecycles:
        if len(canonical_lifecycles) != 1 or len(args) != 1:
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "canonical ordinary affine lifecycle currently supports one leaf per play"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        target, animation = canonical_lifecycles[0]
        return _play_canonical_affine_lifecycle(
            self,
            target,
            animation,
            duration=kwargs.pop("duration", None),
            run_time=kwargs.pop("run_time", None),
            start_time=kwargs.pop("start_time", None),
            easing=kwargs.pop("easing", None),
            rate_func=kwargs.pop("rate_func", None),
            lag_ratio=kwargs.pop("lag_ratio", None),
            kwargs=kwargs,
        )

    canonical_affine = [
        classified
        for argument in args
        if (classified := _canonical_affine_animation(self, argument)) is not None
    ]
    if canonical_affine:
        if len(canonical_affine) != 1 or len(args) != 1:
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "realtime construct supports one canonical affine animation per play"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        source, target, animation = canonical_affine[0]
        if not _canonical_affine_payload_is_supported(
            self, source, target, animation, kwargs
        ):
            if _semantic_continuation_active(self):
                raise NotImplementedError(
                    "realtime construct animation is outside the canonical affine subset"
                )
            return _play_legacy_compatibility(self, *args, **kwargs)
        return _play_canonical_affine(
            self,
            source,
            target,
            animation,
            duration=kwargs.pop("duration", None),
            run_time=kwargs.pop("run_time", None),
            start_time=kwargs.pop("start_time", None),
            easing=kwargs.pop("easing", None),
            rate_func=kwargs.pop("rate_func", None),
            lag_ratio=kwargs.pop("lag_ratio", None),
            kwargs=kwargs,
        )
    if len(args) == 1 and _canonical_tracker_builder(args[0]):
        return _play_canonical_tracker(
            self,
            args[0],
            duration=kwargs.pop("duration", None),
            run_time=kwargs.pop("run_time", None),
            start_time=kwargs.pop("start_time", None),
            easing=kwargs.pop("easing", None),
            rate_func=kwargs.pop("rate_func", None),
            lag_ratio=kwargs.pop("lag_ratio", None),
            kwargs=kwargs,
        )
    if _semantic_continuation_active(self):
        raise NotImplementedError(
            "realtime construct supports only canonical affine Scene.play and Scene.wait"
        )
    return _play_legacy_compatibility(self, *args, **kwargs)


def _canonical_value_tracker(self: _base.Scene, value: float = 0.0) -> _reactive.ValueTracker:
    if getattr(self, "_legacy_geometry_materialized", False):
        raise RuntimeError(
            "canonical ValueTracker cannot be authored after legacy geometry materialization"
        )
    context = _context(self)
    return _reactive.ValueTracker._from_canonical(
        self, context, context.createValueTracker(float(value))
    )


def _canonical_native_context(scene: _base.Scene) -> object:
    if getattr(scene, "_legacy_geometry_materialized", False):
        raise RuntimeError(
            "canonical native input cannot be authored after legacy geometry materialization"
        )
    return _context(scene)


def _canonical_vector_signal(scene: _base.Scene, method: str) -> _reactive.NativeVectorSignal:
    context = _canonical_native_context(scene)
    try:
        handle = getattr(context, method)()
    except Exception as error:
        raise ValueError(str(error)) from None
    return _reactive.NativeVectorSignal._from_canonical(scene, context, handle)


def _canonical_tracker_signal(
    scene: _base.Scene, method: str, *args: object
) -> _reactive.ValueTracker:
    context = _canonical_native_context(scene)
    try:
        handle = getattr(context, method)(*args)
    except Exception as error:
        raise ValueError(str(error)) from None
    return _reactive.ValueTracker._from_canonical(scene, context, handle)


def _canonical_pointer_position_signal(self: _base.Scene) -> _reactive.NativeVectorSignal:
    return _canonical_vector_signal(self, "pointerPositionSignal")


def _canonical_viewport_size_signal(self: _base.Scene) -> _reactive.NativeVectorSignal:
    return _canonical_vector_signal(self, "viewportSizeSignal")


def _canonical_wheel_delta_signal(self: _base.Scene) -> _reactive.NativeVectorSignal:
    return _canonical_vector_signal(self, "wheelDeltaSignal")


def _canonical_key_state_signal(
    self: _base.Scene, code: str, initial: bool = False
) -> _reactive.NativeBoolSignal:
    code = _reactive._nonempty_string("code", code)
    if not isinstance(initial, bool):
        raise TypeError("initial must be a bool")
    context = _canonical_native_context(self)
    try:
        handle = context.keyStateSignal(code, initial)
    except Exception as error:
        raise ValueError(str(error)) from None
    return _reactive.NativeBoolSignal._from_canonical(self, context, handle)


def _canonical_control_signal(
    self: _base.Scene, name: str, value: float = 0.0
) -> _reactive.ValueTracker:
    name = _reactive._nonempty_string("name", name)
    value = _reactive._finite_scalar("value", value)
    return _canonical_tracker_signal(self, "controlSignal", name, value)


def _canonical_pointer_down_events(
    self: _base.Scene, button: int = 0
) -> _reactive.ValueTracker:
    button = _reactive._button(button)
    return _canonical_tracker_signal(self, "pointerDownEvents", button)


def _canonical_wheel_events(self: _base.Scene) -> _reactive.ValueTracker:
    return _canonical_tracker_signal(self, "wheelEvents")


def _canonical_control_commit_events(
    self: _base.Scene, name: str
) -> _reactive.ValueTracker:
    name = _reactive._nonempty_string("name", name)
    return _canonical_tracker_signal(self, "controlCommitEvents", name)


def _is_canonical_scene(scene: _base.Scene) -> bool:
    return getattr(scene, "_canonical_authoring_context", None) is not None


def _canonical_bound_mobject(
    scene: _base.Scene, mobject: object, operation: str
) -> object:
    if not isinstance(mobject, _base.Mobject) or mobject._scene is not scene:
        raise ValueError(f"{operation} target must belong to this Scene")
    handle = getattr(mobject, "_semantic_handle", None)
    if handle is None:
        raise ValueError(f"{operation} requires a typed semantic Mobject")
    return handle


def _canonical_signal_handle(
    scene: _base.Scene, signal: object, expected: type, operation: str
) -> tuple[object, object]:
    if not isinstance(signal, expected):
        raise TypeError(f"{operation} expects a {expected.__name__}")
    if isinstance(signal, _reactive.ValueTracker):
        _associate_tracker(scene, signal)
    canonical = signal._canonical_context_handle()
    if canonical is None:
        if _is_canonical_scene(scene):
            raise ValueError(f"{operation} cannot mix legacy and canonical signals")
        raise TypeError(f"{operation} expects a canonical {expected.__name__}")
    context, handle = canonical
    if context is not getattr(scene, "_canonical_authoring_context", None):
        raise ValueError(f"{expected.__name__} belongs to another canonical Scene context")
    return context, handle


def _canonical_bind_signal(
    self: _base.Scene,
    mobject: object,
    signal: object,
    expected: type,
    operation: str,
    method: str,
) -> _base.Scene:
    handle = _canonical_bound_mobject(self, mobject, operation)
    context, signal_handle = _canonical_signal_handle(self, signal, expected, operation)
    try:
        getattr(context, method)(handle, signal_handle)
    except Exception as error:
        raise ValueError(str(error)) from None
    return self


def _unsupported_canonical_native(scene: _base.Scene, operation: str) -> None:
    # #959 owns the remaining legacy source adapter. Once a canonical context
    # exists, no raw declaration may be appended alongside the shared store.
    if _is_canonical_scene(scene):
        raise NotImplementedError(
            f"{operation} is not supported by canonical native input authoring"
        )


def _unsupported_native_source(operation: str) -> None:
    raise NotImplementedError(
        f"{operation} is not supported by canonical native input authoring"
    )


def _canonical_pointer_button_signal(
    self: _base.Scene, button: int = 0, initial: bool = False
) -> _reactive.NativeBoolSignal:
    _unsupported_native_source("pointer_button_signal")


def _canonical_gesture_delta_signal(
    self: _base.Scene, name: str
) -> _reactive.NativeVectorSignal:
    _unsupported_native_source("gesture_delta_signal")


def _canonical_pointer_up_events(self: _base.Scene, button: int = 0) -> _reactive.ValueTracker:
    _unsupported_native_source("pointer_up_events")


def _canonical_key_press_events(self: _base.Scene, code: str) -> _reactive.ValueTracker:
    _unsupported_native_source("key_press_events")


def _canonical_key_release_events(self: _base.Scene, code: str) -> _reactive.ValueTracker:
    _unsupported_native_source("key_release_events")


def _canonical_gesture_events(self: _base.Scene, name: str) -> _reactive.ValueTracker:
    _unsupported_native_source("gesture_events")


def _canonical_bind_rotation_dispatch(
    self: _base.Scene, mobject: object, tracker: object
) -> _base.Scene:
    if isinstance(tracker, _reactive.ValueTracker) and (
        tracker._canonical_context_handle() is not None
        or tracker._detached_canonical_handle() is not None
    ):
        return _canonical_bind_signal(
            self, mobject, tracker, _reactive.ValueTracker, "bind_rotation", "bindRotation"
        )
    _unsupported_canonical_native(self, "bind_rotation")
    return _ORIGINAL_BIND_ROTATION(self, mobject, tracker)


def _canonical_bind_opacity_dispatch(
    self: _base.Scene, mobject: object, tracker: object
) -> _base.Scene:
    if isinstance(tracker, _reactive.ValueTracker) and (
        tracker._canonical_context_handle() is not None
        or tracker._detached_canonical_handle() is not None
    ):
        return _canonical_bind_signal(
            self, mobject, tracker, _reactive.ValueTracker, "bind_opacity", "bindOpacity"
        )
    _unsupported_canonical_native(self, "bind_opacity")
    return _ORIGINAL_BIND_OPACITY(self, mobject, tracker)


def _canonical_bind_presence_dispatch(
    self: _base.Scene, mobject: object, signal: object
) -> _base.Scene:
    if isinstance(signal, _reactive.NativeBoolSignal) and signal._canonical_context_handle() is not None:
        return _canonical_bind_signal(
            self, mobject, signal, _reactive.NativeBoolSignal, "bind_presence", "bindPresence"
        )
    _unsupported_canonical_native(self, "bind_presence")
    return _ORIGINAL_BIND_PRESENCE(self, mobject, signal)


def _canonical_unsupported_binding(
    self: _base.Scene, operation: str, original: object, *args: object
) -> _base.Scene:
    _unsupported_canonical_native(self, operation)
    return original(self, *args)


def _canonical_bind_appearance_dispatch(
    self: _base.Scene, mobject: object, tracker: object
) -> _base.Scene:
    return _canonical_unsupported_binding(
        self, "bind_appearance", _ORIGINAL_BIND_APPEARANCE, mobject, tracker
    )


def _canonical_bind_reveal_dispatch(
    self: _base.Scene, mobject: object, tracker: object
) -> _base.Scene:
    return _canonical_unsupported_binding(
        self, "bind_reveal", _ORIGINAL_BIND_REVEAL, mobject, tracker
    )


def _canonical_bind_morph_dispatch(
    self: _base.Scene, mobject: object, tracker: object
) -> _base.Scene:
    return _canonical_unsupported_binding(
        self, "bind_morph", _ORIGINAL_BIND_MORPH, mobject, tracker
    )


def _canonical_bind_position(
    self: _base.Scene,
    mobject: object,
    tracker: object,
    direction: object = None,
    offset: object = None,
) -> _base.Scene:
    if isinstance(tracker, _reactive.NativeVectorSignal):
        if tracker._canonical_context_handle() is not None:
            if direction is not None or offset is not None:
                raise ValueError("direction/offset are not valid for a native vector signal")
            return _canonical_bind_signal(
                self,
                mobject,
                tracker,
                _reactive.NativeVectorSignal,
                "bind_position",
                "bindNativeTranslation",
            )
        _unsupported_canonical_native(self, "bind_position")
        return _ORIGINAL_BIND_POSITION(self, mobject, tracker, direction, offset)
    if not isinstance(tracker, _reactive.ValueTracker):
        _unsupported_canonical_native(self, "bind_position")
        return _ORIGINAL_BIND_POSITION(self, mobject, tracker, direction, offset)
    if not isinstance(mobject, _base.Mobject) or mobject._scene is not self:
        raise ValueError("bind_position target must belong to this Scene")
    handle = getattr(mobject, "_semantic_handle", None)
    if handle is None:
        raise ValueError("canonical ValueTracker binding requires a typed semantic Mobject")
    direction_ir = _reactive._vec2_ir(_base.RIGHT if direction is None else direction)
    offset_ir = _reactive._vec2_ir(_base.ORIGIN if offset is None else offset)
    _associate_tracker(self, tracker)
    canonical = tracker._canonical_context_handle()
    if canonical is None:
        _unsupported_canonical_native(self, "bind_position")
        return _ORIGINAL_BIND_POSITION(self, mobject, tracker, direction, offset)
    context, tracker_handle = canonical
    if context is not _context(self):
        raise ValueError("ValueTracker belongs to another canonical Scene context")
    position = context.trackerPosition(
        tracker_handle,
        float(direction_ir["x"]),
        float(direction_ir["y"]),
        float(offset_ir["x"]),
        float(offset_ir["y"]),
    )
    context.bindTrackerPosition(handle, position)
    return self


def _to_document(self):
    # Explicit export may project values; it does not provide execution input on
    # the shared path. Legacy animation retains this adapter until #959.
    document = _ORIGINAL_TO_DOCUMENT(self)
    objects = document["objects"]
    if not getattr(self, "_legacy_geometry_materialized", False):
        for object_id, handle in getattr(self, "_semantic_geometry_handles", {}).items():
            snapshot = json.loads(str(handle.snapshotJson()))
            snapshot["id"] = object_id
            objects[self._object_positions[object_id]] = snapshot
    # Native/Typst Text has no geometry projection. The legacy document remains
    # an explicit geometry-only export while the canonical export carries mixed
    # content, so omit identity-only text rows and their legacy tracks here.
    text_ids = set(getattr(self, "_semantic_text_handles", {}))
    if text_ids:
        document["objects"] = [
            object for object in objects if object.get("id") not in text_ids
        ]
        document["tracks"] = [
            track for track in document["tracks"] if track.get("object") not in text_ids
        ]
    return document


def _identity_document(self: _ir.Scene) -> dict[str, list[dict[str, Any]]]:
    """Project identities for the geometry-only legacy document."""
    document = _ORIGINAL_IDENTITY_DOCUMENT(self)
    text_ids = set(getattr(self, "_semantic_text_handles", {}))
    if text_ids:
        document["objects"] = [
            identity
            for identity in document["objects"]
            if identity["id"] not in text_ids
        ]
        document["tracks"] = [
            identity
            for identity in document["tracks"]
            if self._tracks[identity["id"]].get("object") not in text_ids
        ]
    return document


def execution_context(scene, callbacks=None):
    """Select typed geometry/native-Text execution; unsupported contracts stay explicit."""
    del callbacks  # Callback declarations now lower through the canonical context.
    if getattr(scene, "_legacy_geometry_materialized", False):
        return None
    # The canonical static context does not yet lower the legacy reactive/native
    # declarations.  Reject them here rather than silently constructing a live
    # session that omits their drivers; #61 owns their shared-semantic migration.
    if any(
        getattr(scene, attribute, [])
        for attribute in (
            "_reactive_signals",
            "_reactive_bindings",
            "_reactive_signal_tracks",
            "_native_inputs",
        )
    ):
        return None
    if getattr(scene, "_retained_text_objects", []):
        return None
    handles = getattr(scene, "_semantic_geometry_handles", {})
    text_handles = getattr(scene, "_semantic_text_handles", {})
    if len(handles) + len(text_handles) != len(scene._object_positions):
        return None
    for track in scene._tracks:
        if (track.get("property") != "presence" or
                float(track["timing"]["start_time"]) != 0.0 or
                track.get("values", {}).get("bool", {}).get("to") is not True):
            return None
    context = _context(scene)
    # Python keeps callable identity only. This bootstrap writes the authored
    # occurrence intervals into the one shared Rust semantic store before the
    # execution session is lowered; it does not construct slots or a scheduler.
    import _manim_updaters

    _manim_updaters.prepare_canonical_callbacks(scene, context)
    return context


class LiveExecution:
    """Explicit live property/query facade over one Rust/WASM execution session.

    The wrapper retains only Python object ergonomics. The canonical context
    owns the semantic store and its one runtime session until normal execution
    leases that same session to the renderer; no Python snapshot is consulted
    for a live read or write.
    """

    def __init__(self, scene: _base.Scene, duration: float = 1.0) -> None:
        context = execution_context(scene)
        if context is None:
            raise RuntimeError(
                "live execution currently supports typed static geometry/native Text, "
                "canonical scalar ValueTracker tracks, and predeclared property callbacks"
            )
        self._scene = scene
        context.beginLiveExecution(float(duration))
        self._context = context

    def _handle(self, mobject: _base.Mobject, *, allow_detached: bool = False) -> object:
        if not isinstance(mobject, _base.Mobject):
            raise ValueError("live Mobject must belong to this Scene")
        if mobject._scene is not self._scene and not (
            allow_detached and mobject._scene is None
        ):
            raise ValueError("live Mobject must belong to this Scene")
        handle = getattr(mobject, "_semantic_handle", None)
        if handle is None:
            raise ValueError("live execution requires a typed semantic Mobject handle")
        return handle

    def add(self, mobject: _base.Mobject) -> None:
        handle = self._handle(mobject, allow_detached=True)
        if mobject._scene is self._scene:
            self._context.liveAdd(str(mobject.id), handle)
            return
        reservation = _reserve_typed_binding(mobject, self._scene, handle, None)
        self._context.liveAdd(str(reservation.object.id), handle)
        _commit_typed_binding(mobject, self._scene, reservation, handle)

    def remove(self, mobject: _base.Mobject) -> None:
        self._context.liveRemove(self._handle(mobject))

    def replace_content(self, target: _base.Mobject, source: _base.Mobject) -> None:
        """Use preauthored source content while preserving target identity and state."""
        self._context.liveReplaceContent(
            self._handle(target),
            self._handle(source, allow_detached=True),
        )

    def set_translation(self, mobject: _base.Mobject, x: float, y: float) -> None:
        self._context.liveSetTranslation(self._handle(mobject), float(x), float(y))

    def shift(self, mobject: _base.Mobject, x: float, y: float) -> None:
        self._context.liveShift(self._handle(mobject), float(x), float(y))

    def set_scale(self, mobject: _base.Mobject, x: float, y: float) -> None:
        self._context.liveSetScale(self._handle(mobject), float(x), float(y))

    def set_rotation(self, mobject: _base.Mobject, angle: float) -> None:
        self._context.liveSetRotation(self._handle(mobject), float(angle))

    def effective_center(self, mobject: _base.Mobject) -> _base.Vec2:
        observed = self._context.liveEffectiveMobject(self._handle(mobject))
        return _base.Vec2(
            float(observed.translationX),
            float(observed.translationY),
        )

    def play(self, animation: "LiveAnimation") -> float:
        """Activate one declaration that was authored before this session began."""
        if not isinstance(animation, LiveAnimation) or animation._scene is not self._scene:
            raise ValueError("live animation must belong to this Scene")
        return float(self._context.livePlayAnimation(animation._handle))

    def wait(self, duration: float) -> float:
        """Start a session-owned continuation wait after the active segment completes."""
        return float(self._context.liveWait(float(duration)))

    def advance_to(self, time: float) -> bool:
        """Drive the current segment; affine endpoints require ``complete()``."""
        return bool(self._context.liveAdvanceSegmentTo(float(time)))

    def evaluate(self, time: float) -> None:
        """Evaluate canonical deterministic tracks at one session-owned time."""
        self._context.liveEvaluate(float(time))

    def complete(self) -> None:
        """Publish the active endpoint before sequential authoring continues."""
        self._context.liveCompleteSegment()


class LiveAnimation:
    """Opaque Python identity for a predeclared shared semantic animation."""

    def __init__(self, scene: _base.Scene, handle: object) -> None:
        self._scene = scene
        self._handle = handle


def _live_rate_function_id(rate_func: object) -> str:
    """Resolve only the established deterministic Python rate-function vocabulary."""
    if isinstance(rate_func, str):
        return rate_func
    import _manim_rate_functions as _rate_functions

    return _rate_functions.easing_from_rate_func(rate_func)


def _declare_live_transform_to(
    self: _base.Scene,
    source: _base.Mobject,
    target: _base.Mobject,
    *,
    run_time: float = 1.0,
    rate_func: object = "smooth",
) -> LiveAnimation:
    """Declare a replayable affine TransformTo before lowering a live session.

    ``target`` is an ordinary detached Mobject handle in the same Rust store,
    usually built with ``source.copy()`` and transformed before this call. This
    wrapper declares no scheduler and does not create animation meaning during
    ``LiveExecution.play``.
    """
    context = execution_context(self)
    if context is None:
        raise RuntimeError(
            "live animation currently supports typed static geometry/native Text, "
            "canonical scalar ValueTracker tracks, and predeclared property callbacks"
        )
    if not isinstance(source, _base.Mobject) or source._scene is not self:
        raise ValueError("live animation source must belong to this Scene")
    if not isinstance(target, _base.Mobject) or target._scene is not None:
        raise ValueError("live animation target must be a detached Mobject")
    source_handle = getattr(source, "_semantic_handle", None)
    target_handle = getattr(target, "_semantic_handle", None)
    if source_handle is None or target_handle is None:
        raise ValueError("live animation requires typed semantic Mobject handles")
    return LiveAnimation(
        self,
        context.declareLiveTransformTo(
            source_handle,
            target_handle,
            float(run_time),
            _live_rate_function_id(rate_func),
        ),
    )


def _live_execution(self: _base.Scene, duration: float = 1.0) -> LiveExecution:
    """Create an explicit typed live session for the currently supported subset."""
    return LiveExecution(self, duration)


def _append_snapshot(
    self: _ir.Scene,
    snapshot: dict[str, Any],
    key: str | None,
) -> _ir.Object:
    checkpoint = _ORIGINAL_AUTHORING_CHECKPOINT(self)
    obj = _ORIGINAL_APPEND_SNAPSHOT(self, snapshot, key)
    try:
        _context(self).bindGeometry(str(obj.id), _json(snapshot))
    except Exception:
        _ORIGINAL_RESTORE_AUTHORING_CHECKPOINT(self, checkpoint)
        raise
    return obj


def _authoring_checkpoint(self: _ir.Scene) -> tuple[object, tuple[Any, ...], int]:
    legacy = _ORIGINAL_AUTHORING_CHECKPOINT(self)
    canonical = int(_context(self).checkpoint())
    return (_CHECKPOINT_TAG, legacy, canonical)


def _restore_authoring_checkpoint(
    self: _ir.Scene,
    checkpoint: tuple[Any, ...],
) -> None:
    if (
        len(checkpoint) == 3
        and checkpoint[0] is _CHECKPOINT_TAG
        and isinstance(checkpoint[1], tuple)
    ):
        canonical = int(checkpoint[2])
        _context(self).restore(canonical)
        _ORIGINAL_RESTORE_AUTHORING_CHECKPOINT(self, checkpoint[1])
        handles = getattr(self, "_semantic_geometry_handles", {})
        for object_id in list(handles):
            if object_id not in self._object_positions:
                del handles[object_id]
        return
    # Compatibility with an opaque checkpoint captured before this adapter was
    # installed. Browser authoring installs adapters before user scenes are built,
    # but accepting the old shape keeps direct module-level tests unsurprising.
    _ORIGINAL_RESTORE_AUTHORING_CHECKPOINT(self, checkpoint)


def _replace_static_snapshot(
    self: _base.Scene,
    obj: _ir.Object,
    raw: _ir.Mobject,
) -> None:
    position = self._object_positions.get(obj.id)
    previous = None if position is None else copy.deepcopy(self._objects[position])
    _ORIGINAL_REPLACE_STATIC_SNAPSHOT(self, obj, raw)
    try:
        _context(self).updateGeometry(str(obj.id), _json(raw.to_ir()))
    except Exception:
        if position is not None and previous is not None:
            self._objects[position] = previous
        raise


def _bind_retained_text(
    self: _typst._RetainedTextMobject,
    scene: _base.Scene,
    *,
    key: str | None = None,
) -> object:
    if self._scene is scene and self._object is not None:
        return self._object
    # Native Text, Typst, and MathTypst own ordinary shared semantic Mobject
    # handles. Bind them through the same scene operation as geometry. The
    # retained source branch below remains only for explicit export consumers.
    if getattr(self, "_semantic_handle", None) is not None:
        obj = _bind_mobject(self, scene, key=key)
        self._retained_object_id = int(obj.id)
        self._retained_order = int(scene._object_positions[obj.id])
        return obj
    if self._scene is not None:
        raise ValueError("retained text Mobject already belongs to another Scene")

    _typst._ensure_scene_state(scene)
    checkpoint = scene._authoring_checkpoint()
    obj, order = scene._allocate_object(key)
    try:
        _context(scene).bindText(str(obj.id), str(self._retained_handle.specJson()))
    except Exception:
        scene._restore_authoring_checkpoint(checkpoint)
        raise

    self._bind_retained(scene, obj, order)
    scene._retained_text_objects.append(self)
    return obj


def _camera_object_id(scene: _base.Scene) -> str:
    camera = getattr(scene, "camera", None)
    frame = getattr(camera, "frame", None)
    if frame is None:
        return ""
    try:
        return str(int(frame.id))
    except (AttributeError, TypeError, ValueError):
        return ""


def _to_scene_spec(self: _base.Scene) -> dict[str, Any]:
    """Finalize directly from the per-scene canonical Rust authoring context."""

    # Reconcile direct retained mutations before freezing the source-level time-zero
    # state. Post-timeline edits become retained tracks; pre-timeline edits rewrite
    # only the canonical base TextSpec.
    _retained_state._sync_all(self)
    _retained_state._freeze_bound_sources(self)

    context = _context(self)
    for source in _retained_state._bound_sources(self):
        context.updateText(
            str(int(source.id)),
            _json(copy.deepcopy(_retained_state._freeze_base(source))),
        )

    scene_spec_json = context.sceneSpecJson(
        _json(list(self._tracks)),
        _json(list(getattr(self, "_retained_animation_tracks", []))),
        _json(list(getattr(self, "_retained_family_animations", []))),
        _camera_object_id(self),
    )
    return json.loads(str(scene_spec_json))


def install() -> None:
    """Install the canonical per-scene Rust authoring context as the production path."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    _ir.Scene._append_snapshot = _append_snapshot
    _ir.Scene._authoring_checkpoint = _authoring_checkpoint
    _ir.Scene._restore_authoring_checkpoint = _restore_authoring_checkpoint
    _base.Scene._replace_static_snapshot = _replace_static_snapshot
    _typst._RetainedTextMobject._bind_to_scene = _bind_retained_text
    _base.Scene.to_scene_spec = _to_scene_spec
    _base.Mobject._bind_to_scene = _bind_mobject
    _base.Scene._bind_camera_frame = _bind_camera_frame
    _base.Scene.play = _play
    _base.Scene.wait = _canonical_wait
    _base.Scene.declare_wait = _declare_wait
    _base.Scene.time = property(_canonical_scene_time)
    _base.Scene.value_tracker = _canonical_value_tracker
    _base.Scene.bind_position = _canonical_bind_position
    _base.Scene.pointer_position_signal = _canonical_pointer_position_signal
    _base.Scene.pointer_button_signal = _canonical_pointer_button_signal
    _base.Scene.key_state_signal = _canonical_key_state_signal
    _base.Scene.viewport_size_signal = _canonical_viewport_size_signal
    _base.Scene.wheel_delta_signal = _canonical_wheel_delta_signal
    _base.Scene.gesture_delta_signal = _canonical_gesture_delta_signal
    _base.Scene.control_signal = _canonical_control_signal
    _base.Scene.pointer_down_events = _canonical_pointer_down_events
    _base.Scene.pointer_up_events = _canonical_pointer_up_events
    _base.Scene.key_press_events = _canonical_key_press_events
    _base.Scene.key_release_events = _canonical_key_release_events
    _base.Scene.wheel_events = _canonical_wheel_events
    _base.Scene.gesture_events = _canonical_gesture_events
    _base.Scene.control_commit_events = _canonical_control_commit_events
    _base.Scene.bind_rotation = _canonical_bind_rotation_dispatch
    _base.Scene.bind_opacity = _canonical_bind_opacity_dispatch
    _base.Scene.bind_presence = _canonical_bind_presence_dispatch
    _base.Scene.bind_appearance = _canonical_bind_appearance_dispatch
    _base.Scene.bind_reveal = _canonical_bind_reveal_dispatch
    _base.Scene.bind_morph = _canonical_bind_morph_dispatch
    _base.Scene.live_execution = _live_execution
    _base.Scene.declare_live_transform_to = _declare_live_transform_to
    _ir.Scene.to_document = _to_document
    _ir.Scene.identity_document = _identity_document
