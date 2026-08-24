"""Thin Manim-style tracker adapters for Noon's native reactive graph.

This module never evaluates dependencies or mutates rendered objects per frame. It records
signal declarations, deterministic signal tracks, and property bindings in the language-neutral
semantic document; Rust validates, lowers, evaluates, and invalidates runtime state.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _noon_ir as _ir
import _manim_animate as _animate

_INSTALLED = False
_ORIGINAL_SCENE_INIT = _ir.Scene.__init__
_ORIGINAL_TO_DOCUMENT = _ir.Scene.to_document
_ORIGINAL_SCENE_PLAY = _base.Scene.play


def _finite_scalar(name: str, value: object) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise TypeError(f"{name} must be a number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{name} must be finite")
    return result


def _vec2_ir(value: object) -> dict[str, float]:
    vector = _base._as_vec2(value)
    return {"x": float(vector.x), "y": float(vector.y)}


class _ValueAnimationBuilder:
    def __init__(self, tracker: ValueTracker) -> None:
        self.tracker = tracker
        self.anim_args: dict[str, Any] = {}
        self.cannot_pass_args = False
        self.target_value: float | None = None

    def __call__(self, **kwargs: Any) -> _ValueAnimationBuilder:
        if self.cannot_pass_args:
            raise ValueError(
                "Animation arguments must be passed before accessing methods and can only be passed once"
            )
        self.anim_args = dict(kwargs)
        self.cannot_pass_args = True
        return self

    def set_value(self, value: float) -> _ValueAnimationBuilder:
        self.cannot_pass_args = True
        self.target_value = _finite_scalar("value", value)
        return self

    def increment_value(self, delta: float) -> _ValueAnimationBuilder:
        self.cannot_pass_args = True
        self.target_value = self.tracker.get_value() + _finite_scalar("delta", delta)
        return self


class ValueTracker:
    """Declarative scalar input compatible with Manim's common tracker vocabulary."""

    def __init__(self, value: float = 0.0) -> None:
        self._value = _finite_scalar("value", value)
        self._scene: _ir.Scene | None = None
        self._signal_id: int | None = None

    def get_value(self) -> float:
        return self._value

    def set_value(self, value: float) -> ValueTracker:
        value = _finite_scalar("value", value)
        if self._scene is not None and self._signal_id is not None:
            if any(
                track["signal"] == self._signal_id
                for track in getattr(self._scene, "_reactive_signal_tracks", [])
            ):
                raise ValueError(
                    "direct ValueTracker.set_value after timeline animation is ambiguous; "
                    "use tracker.animate.set_value(...) for subsequent authored changes"
                )
            self._scene._reactive_signals[self._signal_id]["source"]["input"][
                "scalar"
            ] = value
        self._value = value
        return self

    def increment_value(self, delta: float) -> ValueTracker:
        return self.set_value(self._value + _finite_scalar("delta", delta))

    @property
    def signal_id(self) -> int:
        if self._signal_id is None:
            raise AttributeError("ValueTracker has no signal id until attached to a Scene")
        return self._signal_id

    @property
    def animate(self) -> _ValueAnimationBuilder:
        return _ValueAnimationBuilder(self)


def _scene_init(self: _ir.Scene) -> None:
    _ORIGINAL_SCENE_INIT(self)
    self._reactive_signals: list[dict[str, Any]] = []
    self._reactive_bindings: list[dict[str, Any]] = []
    self._reactive_signal_tracks: list[dict[str, Any]] = []


def _to_document(self: _ir.Scene) -> dict[str, Any]:
    document = _ORIGINAL_TO_DOCUMENT(self)
    signals = getattr(self, "_reactive_signals", [])
    bindings = getattr(self, "_reactive_bindings", [])
    signal_tracks = getattr(self, "_reactive_signal_tracks", [])
    if signals or bindings:
        document["reactive"] = {
            "signals": list(signals),
            "bindings": list(bindings),
        }
    if signal_tracks:
        document["signal_tracks"] = list(signal_tracks)
    return document


def _attach_tracker(scene: _ir.Scene, tracker: ValueTracker) -> int:
    if not isinstance(tracker, ValueTracker):
        raise TypeError("expected a ValueTracker")
    if tracker._scene is not None and tracker._scene is not scene:
        raise ValueError("ValueTracker already belongs to another Scene")
    if tracker._signal_id is None:
        signal_id = len(scene._reactive_signals)
        scene._reactive_signals.append(
            {
                "id": signal_id,
                "source": {"input": {"scalar": tracker.get_value()}},
            }
        )
        tracker._scene = scene
        tracker._signal_id = signal_id
    return tracker._signal_id


def _append_derived(scene: _ir.Scene, expression: dict[str, Any]) -> int:
    signal_id = len(scene._reactive_signals)
    scene._reactive_signals.append(
        {"id": signal_id, "source": {"derived": expression}}
    )
    return signal_id


def _raw_object_id(scene: _base.Scene, mobject: object) -> int:
    raw = scene._raw_object(mobject)
    return raw.id


def _bind(scene: _base.Scene, signal_id: int, mobject: object, property_name: str) -> None:
    scene._reactive_bindings.append(
        {
            "signal": signal_id,
            "object": _raw_object_id(scene, mobject),
            "property": property_name,
        }
    )


def _initial_scalar(scene: _ir.Scene, signal_id: int) -> float:
    source = scene._reactive_signals[signal_id]["source"]
    if "input" not in source or "scalar" not in source["input"]:
        raise TypeError("ValueTracker animation requires a scalar input signal")
    return float(source["input"]["scalar"])


def _schedule_value_builder(
    scene: _base.Scene,
    builder: _ValueAnimationBuilder,
    *,
    start_time: float,
    run_time: float,
    easing: str,
) -> None:
    if builder.target_value is None:
        raise ValueError("ValueTracker.animate must call set_value or increment_value")
    signal_id = _attach_tracker(scene, builder.tracker)
    previous = next(
        (
            track
            for track in reversed(scene._reactive_signal_tracks)
            if track["signal"] == signal_id
        ),
        None,
    )
    from_value = _initial_scalar(scene, signal_id) if previous is None else float(previous["to"])
    if previous is not None:
        previous_end = previous["timing"]["start_time"] + previous["timing"]["duration"]
        if start_time < previous_end:
            raise ValueError("ValueTracker animations for one tracker must not overlap")
    scene._reactive_signal_tracks.append(
        {
            "signal": signal_id,
            "from": from_value,
            "to": builder.target_value,
            "timing": {
                "start_time": start_time,
                "duration": run_time,
                "easing": easing,
            },
        }
    )
    builder.tracker._value = builder.target_value


def _scene_play(
    self: _base.Scene,
    *animations: Any,
    duration: float | None = None,
    run_time: float | None = None,
    start_time: float | None = None,
    easing: str | None = None,
    rate_func: object | None = None,
    lag_ratio: float | None = None,
    **kwargs: Any,
) -> _base.Scene:
    value_builders = [
        animation for animation in animations if isinstance(animation, _ValueAnimationBuilder)
    ]
    if not value_builders:
        return _ORIGINAL_SCENE_PLAY(
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
    if duration is not None and run_time is not None:
        raise ValueError("use either duration or run_time, not both")
    if easing is not None and rate_func is not None:
        raise ValueError("use either rate_func or the low-level easing alias, not both")
    if kwargs:
        unsupported = ", ".join(sorted(kwargs))
        raise NotImplementedError(f"unsupported Manim Scene.play option(s): {unsupported}")

    base_start = self._cursor if start_time is None else float(start_time)
    if not math.isfinite(base_start) or base_start < 0.0:
        raise ValueError("start_time must be finite and non-negative")
    play_duration = None
    if run_time is not None:
        play_duration = _animate._positive_duration("run_time", run_time)
    elif duration is not None:
        play_duration = _animate._positive_duration("duration", duration)
    if lag_ratio is not None:
        _animate._lag_ratio(lag_ratio)

    ordinary = [
        animation for animation in animations if not isinstance(animation, _ValueAnimationBuilder)
    ]
    checkpoint = self._authoring_checkpoint()
    signal_track_count = len(self._reactive_signal_tracks)
    cursor_before = self._cursor
    tracker_values = [(builder.tracker, builder.tracker._value) for builder in value_builders]
    max_end = base_start
    try:
        if ordinary:
            _ORIGINAL_SCENE_PLAY(
                self,
                *ordinary,
                duration=duration,
                run_time=run_time,
                start_time=base_start,
                easing=easing,
                rate_func=rate_func,
                lag_ratio=lag_ratio,
            )
        for builder in value_builders:
            builder_args = _animate._validate_builder_args(builder)
            item_duration = (
                play_duration
                if play_duration is not None
                else builder_args.get("run_time", 1.0)
            )
            item_duration = _animate._positive_duration("run_time", item_duration)
            item_easing = _animate._resolve_easing(
                builder_args=builder_args,
                play_easing=easing,
                play_rate_func=rate_func,
            )
            _schedule_value_builder(
                self,
                builder,
                start_time=base_start,
                run_time=item_duration,
                easing=item_easing,
            )
            max_end = max(max_end, base_start + item_duration)
        self._cursor = max(self._cursor, cursor_before, max_end)
        return self
    except Exception:
        self._restore_authoring_checkpoint(checkpoint)
        del self._reactive_signal_tracks[signal_track_count:]
        self._cursor = cursor_before
        for tracker, value in tracker_values:
            tracker._value = value
        raise


def value_tracker(scene: _base.Scene, value: float = 0.0) -> ValueTracker:
    tracker = ValueTracker(value)
    _attach_tracker(scene, tracker)
    return tracker


def bind_rotation(
    scene: _base.Scene, mobject: object, tracker: ValueTracker
) -> _base.Scene:
    _bind(scene, _attach_tracker(scene, tracker), mobject, "rotation")
    return scene


def bind_opacity(
    scene: _base.Scene, mobject: object, tracker: ValueTracker
) -> _base.Scene:
    _bind(scene, _attach_tracker(scene, tracker), mobject, "opacity")
    return scene


def bind_appearance(
    scene: _base.Scene, mobject: object, tracker: ValueTracker
) -> _base.Scene:
    _bind(scene, _attach_tracker(scene, tracker), mobject, "appearance")
    return scene


def bind_reveal(
    scene: _base.Scene, mobject: object, tracker: ValueTracker
) -> _base.Scene:
    _bind(scene, _attach_tracker(scene, tracker), mobject, "reveal")
    return scene


def bind_morph(
    scene: _base.Scene, mobject: object, tracker: ValueTracker
) -> _base.Scene:
    _bind(scene, _attach_tracker(scene, tracker), mobject, "morph")
    return scene


def bind_position(
    scene: _base.Scene,
    mobject: object,
    tracker: ValueTracker,
    direction: object = None,
    offset: object = None,
) -> _base.Scene:
    tracker_id = _attach_tracker(scene, tracker)
    direction_ir = _vec2_ir(_base.RIGHT if direction is None else direction)
    offset_ir = _vec2_ir(_base.ORIGIN if offset is None else offset)
    expression = {
        "add": [
            {"constant": {"vec2": offset_ir}},
            {
                "mul": [
                    {"signal": tracker_id},
                    {"constant": {"vec2": direction_ir}},
                ]
            },
        ]
    }
    derived = _append_derived(scene, expression)
    _bind(scene, derived, mobject, "position")
    return scene


def install() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    _ir.Scene.__init__ = _scene_init
    _ir.Scene.to_document = _to_document

    # `_base.Scene` is the compatibility Scene after `_manim_compat.install()`.
    _base.Scene.play = _scene_play
    _base.Scene.value_tracker = value_tracker
    _base.Scene.bind_rotation = bind_rotation
    _base.Scene.bind_opacity = bind_opacity
    _base.Scene.bind_appearance = bind_appearance
    _base.Scene.bind_reveal = bind_reveal
    _base.Scene.bind_morph = bind_morph
    _base.Scene.bind_position = bind_position
    _base.ValueTracker = ValueTracker

    if "ValueTracker" not in _base.__all__:
        _base.__all__.append("ValueTracker")


install()
