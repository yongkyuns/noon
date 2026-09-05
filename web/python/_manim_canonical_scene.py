"""Bind geometry handles directly into the shared Rust Scene.

Static geometry lowers to one ExecutionSession in the authoring worker. Python
keeps identity metadata; geometry values are projected only for explicit exports
or the legacy animation/retained-text adapter still owned for deletion by #959.
"""

from __future__ import annotations

import copy
import json
from typing import Any

import _manim_retained_state as _retained_state
import _manim_typst as _typst
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
_ORIGINAL_TO_SCENE_SPEC = _base.Scene.to_scene_spec
_ORIGINAL_TO_DOCUMENT = _ir.Scene.to_document


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


def _bind_mobject(self: _base.Mobject, scene: _base.Scene, *, key=None):
    handle = getattr(self, "_semantic_handle", None)
    if handle is None:
        materialize_legacy_geometry(scene)
        return _ORIGINAL_BIND(self, scene, key=key)
    checkpoint = scene._authoring_checkpoint()
    obj, _ = scene._allocate_object(key)
    try:
        _context(scene).bindMobject(str(obj.id), handle)
    except Exception:
        scene._restore_authoring_checkpoint(checkpoint)
        raise
    _record_mobject_binding(self, scene, obj, handle)
    return obj


def _record_mobject_binding(
    mobject: _base.Mobject,
    scene: _base.Scene,
    obj: _ir.Object,
    handle: object,
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
    if getattr(scene, "_legacy_geometry_materialized", False):
        snapshot = json.loads(str(handle.snapshotJson()))
        snapshot["id"] = obj.id
        scene._objects[-1] = snapshot
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


def _play(self, *args, **kwargs):
    # Native Text timeline export stays in the canonical context. Its #959
    # codec is store-derived at finalization, so geometry materialization must
    # not force it through a geometry-only legacy document.
    if not getattr(self, "_semantic_text_handles", {}):
        materialize_legacy_geometry(self)
    return _ORIGINAL_PLAY(self, *args, **kwargs)


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
    return document


def execution_context(scene, callbacks=None):
    """Select typed geometry/native-Text execution; unsupported contracts stay explicit."""
    if callbacks or getattr(scene, "_legacy_geometry_materialized", False):
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
    return _context(scene)


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
                "live execution currently supports typed static geometry/native Text without "
                "callbacks, retained text, or timeline tracks"
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
        checkpoint = self._scene._authoring_checkpoint()
        obj, _ = self._scene._allocate_object(None)
        try:
            self._context.liveAdd(str(obj.id), handle)
        except Exception:
            self._scene._restore_authoring_checkpoint(checkpoint)
            raise
        _record_mobject_binding(mobject, self._scene, obj, handle)

    def remove(self, mobject: _base.Mobject) -> None:
        self._context.liveRemove(self._handle(mobject))

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
    # Native Text now owns an ordinary shared semantic Mobject handle. Bind it
    # through the same scene operation as geometry; the retained source adapter
    # remains only for Typst until that backend reaches this resource path.
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

    if getattr(self, "_legacy_geometry_materialized", False):
        return _ORIGINAL_TO_SCENE_SPEC(self)

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
    _base.Scene.play = _play
    _base.Scene.live_execution = _live_execution
    _ir.Scene.to_document = _to_document
