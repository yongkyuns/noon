"""Thin Manim-style tracker and native-input adapters for Noon's reactive graph.

This module never evaluates dependencies or mutates rendered objects per frame. It records
signal declarations, deterministic signal tracks, native input bindings, and property bindings
in the language-neutral semantic document; Rust validates, lowers, evaluates, and invalidates
runtime state.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _noon_ir as _ir
import _manim_animation_options as _options

_INSTALLED = False
_ORIGINAL_SCENE_INIT = _ir.Scene.__init__
_ORIGINAL_TO_DOCUMENT = _ir.Scene.to_document
_ORIGINAL_SCENE_PLAY = _base.Scene.play
_ACTIVE_CALLBACK_SIGNAL_VALUES: dict[int, dict[str, Any]] | None = None


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


def _nonempty_string(name: str, value: object) -> str:
    if not isinstance(value, str):
        raise TypeError(f"{name} must be a string")
    if not value.strip():
        raise ValueError(f"{name} must not be empty")
    return value


def _button(value: object) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError("button must be an integer")
    if value < 0 or value > 255:
        raise ValueError("button must be in the range 0..255")
    return value


def _enter_callback_signal_values(frame: dict[str, Any]) -> None:
    global _ACTIVE_CALLBACK_SIGNAL_VALUES
    if _ACTIVE_CALLBACK_SIGNAL_VALUES is not None:
        raise RuntimeError("nested Noon callback signal contexts are not supported")
    _ACTIVE_CALLBACK_SIGNAL_VALUES = {
        int(item["signal"]): item["value"] for item in frame.get("signals", [])
    }


def _leave_callback_signal_values() -> None:
    global _ACTIVE_CALLBACK_SIGNAL_VALUES
    _ACTIVE_CALLBACK_SIGNAL_VALUES = None


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

    @classmethod
    def _from_canonical(
        cls, scene: _base.Scene, context: object, handle: object
    ) -> ValueTracker:
        """Create the typed canonical wrapper without a Python scalar value.

        The context owns the signal's semantic identity and authored/runtime
        values. This object keeps only Python ownership ergonomics plus the
        opaque typed WASM handle.
        """
        tracker = object.__new__(cls)
        tracker._scene = scene
        tracker._canonical_context = context
        tracker._canonical_handle = handle
        return tracker

    def _canonical_context_handle(self) -> tuple[object, object] | None:
        context = getattr(self, "_canonical_context", None)
        handle = getattr(self, "_canonical_handle", None)
        if context is None or handle is None:
            return None
        return context, handle

    def get_value(self) -> float:
        canonical = self._canonical_context_handle()
        if _ACTIVE_CALLBACK_SIGNAL_VALUES is not None and canonical is not None:
            raise NotImplementedError(
                "canonical ValueTracker callback reads require a Rust-published signal read set"
            )
        if canonical is not None:
            context, handle = canonical
            return float(context.valueTrackerValue(handle))
        if self._signal_id is not None and _ACTIVE_CALLBACK_SIGNAL_VALUES is not None:
            payload = _ACTIVE_CALLBACK_SIGNAL_VALUES.get(self._signal_id)
            if payload is None:
                raise NotImplementedError(
                    "canonical callback signal reads require a Rust-published signal read set"
                )
            if "scalar" not in payload:
                raise TypeError("ValueTracker runtime signal is not scalar")
            return float(payload["scalar"])
        return self._value

    def set_value(self, value: float) -> ValueTracker:
        value = _finite_scalar("value", value)
        canonical = self._canonical_context_handle()
        if canonical is not None:
            if _ACTIVE_CALLBACK_SIGNAL_VALUES is not None:
                raise NotImplementedError(
                    "canonical ValueTracker callback writes are not supported"
                )
            context, handle = canonical
            try:
                context.setValueTracker(handle, value)
            except Exception as error:
                raise ValueError(str(error)) from None
            return self
        if self._scene is not None and self._signal_id is not None:
            if _native_drives_signal(self._scene, self._signal_id):
                raise ValueError(
                    "native input-owned ValueTracker cannot be set directly after authoring"
                )
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
        return self.set_value(self.get_value() + _finite_scalar("delta", delta))

    @property
    def signal_id(self) -> int:
        if self._canonical_context_handle() is not None:
            raise AttributeError(
                "canonical ValueTracker identity belongs to the shared Rust semantic store"
            )
        if self._signal_id is None:
            raise AttributeError("ValueTracker has no signal id until attached to a Scene")
        return self._signal_id

    @property
    def animate(self) -> _ValueAnimationBuilder:
        return _ValueAnimationBuilder(self)


class _NativeSignal:
    def __init__(self, scene: _ir.Scene, signal_id: int) -> None:
        self._scene = scene
        self._signal_id = signal_id

    @classmethod
    def _from_canonical(
        cls, scene: _base.Scene, context: object, handle: object
    ) -> _NativeSignal:
        """Create a wrapper over one store-owned native signal handle.

        Native signal identity and values remain in Rust. Python retains only
        the opaque handle required to pass the source back to the same context.
        """
        signal = object.__new__(cls)
        signal._scene = scene
        signal._canonical_context = context
        signal._canonical_handle = handle
        return signal

    def _canonical_context_handle(self) -> tuple[object, object] | None:
        context = getattr(self, "_canonical_context", None)
        handle = getattr(self, "_canonical_handle", None)
        if context is None or handle is None:
            return None
        return context, handle

    @property
    def signal_id(self) -> int:
        if self._canonical_context_handle() is not None:
            raise AttributeError(
                "canonical native signal identity belongs to the shared Rust semantic store"
            )
        return self._signal_id


class NativeVectorSignal(_NativeSignal):
    """Thin handle for a native Vec2-valued input signal."""


class NativeBoolSignal(_NativeSignal):
    """Thin handle for a native bool-valued input signal."""


def _scene_init(self: _ir.Scene) -> None:
    _ORIGINAL_SCENE_INIT(self)
    self._reactive_signals: list[dict[str, Any]] = []
    self._reactive_bindings: list[dict[str, Any]] = []
    self._reactive_signal_tracks: list[dict[str, Any]] = []
    self._native_inputs: list[dict[str, Any]] = []


def _to_document(self: _ir.Scene) -> dict[str, Any]:
    document = _ORIGINAL_TO_DOCUMENT(self)
    signals = getattr(self, "_reactive_signals", [])
    bindings = getattr(self, "_reactive_bindings", [])
    signal_tracks = getattr(self, "_reactive_signal_tracks", [])
    native_inputs = getattr(self, "_native_inputs", [])
    if signals or bindings:
        document["reactive"] = {
            "signals": list(signals),
            "bindings": list(bindings),
        }
    if signal_tracks:
        document["signal_tracks"] = list(signal_tracks)
    if native_inputs:
        document["native_inputs"] = list(native_inputs)
    return document


def _append_input(scene: _ir.Scene, value: dict[str, Any]) -> int:
    signal_id = len(scene._reactive_signals)
    scene._reactive_signals.append(
        {"id": signal_id, "source": {"input": value}}
    )
    return signal_id


def _attach_tracker(scene: _ir.Scene, tracker: ValueTracker) -> int:
    if not isinstance(tracker, ValueTracker):
        raise TypeError("expected a ValueTracker")
    if tracker._scene is not None and tracker._scene is not scene:
        raise ValueError("ValueTracker already belongs to another Scene")
    if tracker._signal_id is None:
        tracker._signal_id = _append_input(scene, {"scalar": tracker.get_value()})
        tracker._scene = scene
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


def _native_drives_signal(scene: _ir.Scene, signal_id: int) -> bool:
    for binding in getattr(scene, "_native_inputs", []):
        payload = binding.get("state") or binding.get("event")
        if payload is not None and payload.get("signal") == signal_id:
            return True
    return False


def _register_native_state(
    scene: _ir.Scene, source: dict[str, Any], signal_id: int
) -> None:
    if _native_drives_signal(scene, signal_id):
        raise ValueError(f"signal {signal_id} already has a native input driver")
    scene._native_inputs.append(
        {"state": {"source": source, "signal": signal_id}}
    )


def _register_native_event(
    scene: _ir.Scene, source: dict[str, Any], signal_id: int
) -> None:
    if _native_drives_signal(scene, signal_id):
        raise ValueError(f"signal {signal_id} already has a native input driver")
    scene._native_inputs.append(
        {"event": {"source": source, "signal": signal_id}}
    )


def _native_vector_signal(
    scene: _ir.Scene, source: dict[str, Any]
) -> NativeVectorSignal:
    signal_id = _append_input(scene, {"vec2": {"x": 0.0, "y": 0.0}})
    _register_native_state(scene, source, signal_id)
    return NativeVectorSignal(scene, signal_id)


def _native_bool_signal(
    scene: _ir.Scene, source: dict[str, Any], initial: bool
) -> NativeBoolSignal:
    if not isinstance(initial, bool):
        raise TypeError("initial must be a bool")
    signal_id = _append_input(scene, {"bool": initial})
    _register_native_state(scene, source, signal_id)
    return NativeBoolSignal(scene, signal_id)


def _native_event_tracker(scene: _ir.Scene, source: dict[str, Any]) -> ValueTracker:
    tracker = value_tracker(scene, 0.0)
    _register_native_event(scene, source, tracker.signal_id)
    return tracker


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
    if _native_drives_signal(scene, signal_id):
        raise ValueError(
            "native input-owned ValueTracker cannot also be timeline-driven"
        )
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
    play_run_time = run_time if run_time is not None else duration
    if play_run_time is not None:
        play_run_time = float(play_run_time)
    if lag_ratio is not None:
        lag_ratio = float(lag_ratio)

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
            builder_args = _options.builder_args(builder)
            resolved = _options.resolve(
                builder_args=builder_args,
                default_lag_ratio=0.0,
                play_run_time=play_run_time,
                play_easing=easing,
                play_rate_func=rate_func,
                play_lag_ratio=lag_ratio,
            )
            _schedule_value_builder(
                self,
                builder,
                start_time=base_start,
                run_time=resolved.run_time,
                easing=resolved.rate_func,
            )
            max_end = max(max_end, base_start + resolved.run_time)
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


def pointer_position_signal(scene: _base.Scene) -> NativeVectorSignal:
    return _native_vector_signal(scene, {"kind": "pointer_position"})


def pointer_button_signal(
    scene: _base.Scene, button: int = 0, initial: bool = False
) -> NativeBoolSignal:
    return _native_bool_signal(
        scene,
        {"kind": "pointer_button", "button": _button(button)},
        initial,
    )


def key_state_signal(
    scene: _base.Scene, code: str, initial: bool = False
) -> NativeBoolSignal:
    return _native_bool_signal(
        scene,
        {"kind": "key", "code": _nonempty_string("code", code)},
        initial,
    )


def viewport_size_signal(scene: _base.Scene) -> NativeVectorSignal:
    return _native_vector_signal(scene, {"kind": "viewport_size"})


def wheel_delta_signal(scene: _base.Scene) -> NativeVectorSignal:
    return _native_vector_signal(scene, {"kind": "wheel_delta"})


def gesture_delta_signal(scene: _base.Scene, name: str) -> NativeVectorSignal:
    return _native_vector_signal(
        scene,
        {"kind": "gesture_delta", "name": _nonempty_string("name", name)},
    )


def control_signal(
    scene: _base.Scene, name: str, value: float = 0.0
) -> ValueTracker:
    tracker = value_tracker(scene, value)
    _register_native_state(
        scene,
        {"kind": "control", "name": _nonempty_string("name", name)},
        tracker.signal_id,
    )
    return tracker


def pointer_down_events(scene: _base.Scene, button: int = 0) -> ValueTracker:
    return _native_event_tracker(
        scene, {"kind": "pointer_down", "button": _button(button)}
    )


def pointer_up_events(scene: _base.Scene, button: int = 0) -> ValueTracker:
    return _native_event_tracker(
        scene, {"kind": "pointer_up", "button": _button(button)}
    )


def key_press_events(scene: _base.Scene, code: str) -> ValueTracker:
    return _native_event_tracker(
        scene,
        {"kind": "key_press", "code": _nonempty_string("code", code)},
    )


def key_release_events(scene: _base.Scene, code: str) -> ValueTracker:
    return _native_event_tracker(
        scene,
        {"kind": "key_release", "code": _nonempty_string("code", code)},
    )


def wheel_events(scene: _base.Scene) -> ValueTracker:
    return _native_event_tracker(scene, {"kind": "wheel"})


def gesture_events(scene: _base.Scene, name: str) -> ValueTracker:
    return _native_event_tracker(
        scene,
        {"kind": "gesture", "name": _nonempty_string("name", name)},
    )


def control_commit_events(scene: _base.Scene, name: str) -> ValueTracker:
    return _native_event_tracker(
        scene,
        {"kind": "control_commit", "name": _nonempty_string("name", name)},
    )


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


def bind_presence(
    scene: _base.Scene, mobject: object, signal: NativeBoolSignal
) -> _base.Scene:
    if not isinstance(signal, NativeBoolSignal) or signal._scene is not scene:
        raise TypeError("bind_presence expects a NativeBoolSignal from this Scene")
    _bind(scene, signal.signal_id, mobject, "presence")
    return scene


def bind_position(
    scene: _base.Scene,
    mobject: object,
    tracker: object,
    direction: object = None,
    offset: object = None,
) -> _base.Scene:
    if isinstance(tracker, NativeVectorSignal):
        if tracker._scene is not scene:
            raise ValueError("NativeVectorSignal belongs to another Scene")
        if direction is not None or offset is not None:
            raise ValueError(
                "direction/offset are only valid for ValueTracker-derived positions"
            )
        _bind(scene, tracker.signal_id, mobject, "position")
        return scene
    if not isinstance(tracker, ValueTracker):
        raise TypeError("bind_position expects a ValueTracker or NativeVectorSignal")
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


public = {
    "ValueTracker": ValueTracker,
    "NativeVectorSignal": NativeVectorSignal,
    "NativeBoolSignal": NativeBoolSignal,
}


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
    _base.Scene.pointer_position_signal = pointer_position_signal
    _base.Scene.pointer_button_signal = pointer_button_signal
    _base.Scene.key_state_signal = key_state_signal
    _base.Scene.viewport_size_signal = viewport_size_signal
    _base.Scene.wheel_delta_signal = wheel_delta_signal
    _base.Scene.gesture_delta_signal = gesture_delta_signal
    _base.Scene.control_signal = control_signal
    _base.Scene.pointer_down_events = pointer_down_events
    _base.Scene.pointer_up_events = pointer_up_events
    _base.Scene.key_press_events = key_press_events
    _base.Scene.key_release_events = key_release_events
    _base.Scene.wheel_events = wheel_events
    _base.Scene.gesture_events = gesture_events
    _base.Scene.control_commit_events = control_commit_events
    _base.Scene.bind_rotation = bind_rotation
    _base.Scene.bind_opacity = bind_opacity
    _base.Scene.bind_appearance = bind_appearance
    _base.Scene.bind_reveal = bind_reveal
    _base.Scene.bind_morph = bind_morph
    _base.Scene.bind_presence = bind_presence
    _base.Scene.bind_position = bind_position
    _base.ValueTracker = ValueTracker
    _base.NativeVectorSignal = NativeVectorSignal
    _base.NativeBoolSignal = NativeBoolSignal

    for name in ("ValueTracker", "NativeVectorSignal", "NativeBoolSignal"):
        if name not in _base.__all__:
            _base.__all__.append(name)


install()
