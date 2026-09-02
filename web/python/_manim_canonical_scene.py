"""Bind Python scene authoring directly into one Rust-owned canonical SceneSpec stream.

Legacy geometry documents and retained Text sidecars remain available as compatibility
views during #367 migration, but canonical production no longer reads either view.
Geometry/Text bind events populate a per-scene Rust/WASM context directly; finalization
adds timeline, family-animation, and camera state from their authoritative authoring
stores.
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
