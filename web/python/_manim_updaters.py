"""Manim-style arbitrary updater adapter over Noon's host callback protocol.

The Python callable stays in Pyodide. During playback the main thread sends one
coherent runtime snapshot for the callback phase; all getter/setter traffic stays
inside this module and only one PatchBatch crosses back to WASM.
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
                raise NotImplementedError(
                    "host updaters cannot mutate geometry yet; use transform/style "
                    "mutations or native reactive expressions"
                )
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
    try:
        for mobject in session.mobjects:
            for callback in list(_updaters(mobject)):
                _invoke(callback, mobject, context.delta_time)
    finally:
        _ACTIVE_CONTEXTS.pop(scene_key, None)

    return context.patch_batch(int(sequence)).to_json()


def release_session(session_id: int) -> None:
    _SESSIONS.pop(int(session_id), None)


def _install_rotating_breadth() -> None:
    """Install source-execution breadth for Manim's literal ``RotatingDemo``.

    The qualified centered-leaf implementation stays in ``_manim_rotate``. This final
    bootstrap wrapper handles retained groups, external pivots, and the 180-degree x/y
    projections used by the upstream documentation scene. It intentionally remains a
    parity candidate until curved z-pivot and 3D intermediate-frame tracks are native.
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

    def reflected_target(
        current: _base.Mobject, *, axis: str, pivot_point: _base.Vec2
    ) -> _base.Mobject:
        target_snapshot = current.to_ir()
        transform = target_snapshot["transform"]
        translation = transform["translation"]
        rotation = float(transform["rotation"])
        scale = transform["scale"]
        if axis == "y":
            translation["x"] = 2.0 * pivot_point.x - float(translation["x"])
            transform["rotation"] = math.pi - rotation
            scale["y"] = -float(scale["y"])
        elif axis == "x":
            translation["y"] = 2.0 * pivot_point.y - float(translation["y"])
            transform["rotation"] = -rotation
            scale["y"] = -float(scale["y"])
        else:
            raise ValueError(f"unsupported reflection axis {axis!r}")
        return animate._snapshot_mobject(target_snapshot)

    def schedule_family(
        scene: object,
        animation: Rotating,
        *,
        start_time: float,
        duration: float,
        easing: str,
    ) -> None:
        sources, detached = current_detached_members(scene, animation.mobject, start_time)
        pivot_point = pivot(detached, animation)
        axis, sign = axis_kind(animation.axis)
        if axis in {"x", "y"} and not math.isclose(
            abs(animation.angle), math.pi, rel_tol=0.0, abs_tol=1e-12
        ):
            raise NotImplementedError(
                "non-z Rotating currently supports the 180-degree projection used by Manim RotatingDemo"
            )

        for index, (source, current) in enumerate(zip(sources, detached, strict=True)):
            assert source._object is not None
            obj = source._object
            if axis == "z":
                target = animate._snapshot_mobject(current.to_ir())
                target.rotate(sign * animation.angle, compat.OUT, about_point=pivot_point)
            else:
                target = reflected_target(current, axis=axis, pivot_point=pivot_point)

            object_key = scene._object_keys[obj.id]
            compat._BaseScene.play(
                scene,
                _base.Transform(
                    source,
                    target,
                    key=f"@rotating-family:{object_key}:{start_time:g}:{index}",
                ),
                run_time=duration,
                start_time=start_time,
                easing=easing,
            )

    def schedule(
        scene: object,
        animation: Rotating,
        *,
        start_time: float,
        duration: float,
        easing: str,
    ) -> None:
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
            return
        schedule_family(
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
                schedule(
                    self,
                    animation,
                    start_time=base_start,
                    duration=resolved.run_time,
                    easing=resolved.rate_func,
                )
                max_end = max(max_end, base_start + resolved.run_time)
            self._cursor = max(cursor_before, max_end)
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
    _install_rotating_breadth()
    _INSTALLED = True