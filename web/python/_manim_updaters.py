"""Manim-style arbitrary updater adapter over Noon's host callback protocol.

The Python callable stays in Pyodide. During playback the main thread sends one
coherent runtime snapshot for the callback phase; all getter/setter traffic stays
inside this module and only one PatchBatch crosses back to WASM.
"""

from __future__ import annotations

import copy
import inspect
from dataclasses import dataclass
from typing import Any, Callable

import _noon_ir as _ir
import noon as _base

_INSTALLED = False
_NEXT_SESSION_ID = 0
_TRACKED_MOBJECTS: list[_base.Mobject] = []
_SESSIONS: dict[int, "_UpdaterSession"] = {}
_ACTIVE_CONTEXTS: dict[int, "_CallbackContext"] = {}

_ORIGINAL_CURRENT_RAW = _base.Mobject._current_raw
_ORIGINAL_APPLY = _base.Mobject._apply


def _track(mobject: _base.Mobject) -> None:
    if not any(existing is mobject for existing in _TRACKED_MOBJECTS):
        _TRACKED_MOBJECTS.append(mobject)


def _updaters(mobject: _base.Mobject) -> list[Callable[..., Any]]:
    value = getattr(mobject, "_noon_updaters", None)
    if value is None:
        value = []
        setattr(mobject, "_noon_updaters", value)
    return value


def add_updater(
    self: _base.Mobject,
    update_function: Callable[..., Any],
    index: int | None = None,
    call_updater: bool = False,
) -> _base.Mobject:
    if not callable(update_function):
        raise TypeError("updater must be callable")
    callbacks = _updaters(self)
    if index is None:
        callbacks.append(update_function)
    else:
        if isinstance(index, bool) or not isinstance(index, int):
            raise TypeError("updater index must be an integer")
        callbacks.insert(index, update_function)
    _track(self)
    if call_updater:
        _invoke(update_function, self, 0.0)
    return self


def remove_updater(
    self: _base.Mobject, update_function: Callable[..., Any]
) -> _base.Mobject:
    callbacks = _updaters(self)
    for index, callback in enumerate(callbacks):
        if callback is update_function:
            del callbacks[index]
            break
    return self


def clear_updaters(self: _base.Mobject, recursive: bool = True) -> _base.Mobject:
    # Noon has no persisted runtime hierarchy, but Group/VGroup recurse in their own
    # Python wrappers. The flag is accepted for Manim source compatibility.
    del recursive
    _updaters(self).clear()
    return self


def get_updaters(self: _base.Mobject) -> list[Callable[..., Any]]:
    return list(_updaters(self))


def has_updaters(self: _base.Mobject) -> bool:
    return bool(_updaters(self))


def _current_raw(self: _base.Mobject) -> _ir.Mobject:
    scene = self._scene
    obj = self._object
    if scene is not None and obj is not None:
        context = _ACTIVE_CONTEXTS.get(id(scene))
        if context is not None:
            return context.current_raw(obj.id)
    return _ORIGINAL_CURRENT_RAW(self)


def _apply(self: _base.Mobject, raw: _ir.Mobject) -> _base.Mobject:
    scene = self._scene
    obj = self._object
    if scene is not None and obj is not None:
        context = _ACTIVE_CONTEXTS.get(id(scene))
        if context is not None:
            context.replace_raw(obj.id, raw)
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
    mobjects: list[_base.Mobject]


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

    def current_raw(self, object_id: int) -> _ir.Mobject:
        return self._materialize(object_id)

    def replace_raw(self, object_id: int, raw: _ir.Mobject) -> None:
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

    mobjects = [
        mobject
        for mobject in _TRACKED_MOBJECTS
        if mobject._scene is scene
        and mobject._object is not None
        and bool(_updaters(mobject))
    ]
    mobjects.sort(key=lambda value: value.id)
    if not mobjects:
        return None

    session_id = _NEXT_SESSION_ID
    _NEXT_SESSION_ID += 1
    _SESSIONS[session_id] = _UpdaterSession(scene=scene, mobjects=mobjects)

    # Arbitrary Python closures may read any bound mobject. Observe the complete
    # semantic object table once per callback phase so all such reads are coherent
    # and local inside Pyodide. The Python callback context materializes the
    # corresponding Mobject snapshots lazily, so cost scales with the closure's
    # touched set rather than eagerly deep-copying every scene object.
    object_ids = [int(obj["id"]) for obj in scene._objects]
    return {
        "session_id": session_id,
        "slots": [{"id": 0, "objects": object_ids}],
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
    if len(invocations) != 1 or int(invocations[0]["callback"]) != 0:
        raise RuntimeError("updater session received an unexpected callback invocation set")

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
        for mobject in session.mobjects:
            for callback in list(_updaters(mobject)):
                _invoke(callback, mobject, context.delta_time)
    finally:
        if reactive is not None:
            reactive._leave_callback_signal_values()
        _ACTIVE_CONTEXTS.pop(scene_key, None)

    return context.patch_batch(int(sequence)).to_json()


def release_session(session_id: int) -> None:
    _SESSIONS.pop(int(session_id), None)


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
    _INSTALLED = True