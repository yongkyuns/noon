"""Thin Manim-style tracker adapters for Noon's native reactive graph.

This module does not evaluate dependencies or mutate rendered objects. It only records
signal declarations and bindings in the language-neutral semantic scene document; Rust
validates, lowers, evaluates, and invalidates runtime state.
"""

from __future__ import annotations

import math
from typing import Any

import noon as _base
import _noon_ir as _ir

_INSTALLED = False
_ORIGINAL_SCENE_INIT = _ir.Scene.__init__
_ORIGINAL_TO_DOCUMENT = _ir.Scene.to_document


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


class ValueTracker:
    """Declarative scalar input compatible with Manim's common tracker vocabulary.

    `set_value` changes the authored/current input value. Once the scene is running,
    runtime hosts update the same signal ID through Noon's native input API; Python is
    not invoked per frame.
    """

    def __init__(self, value: float = 0.0) -> None:
        self._value = _finite_scalar("value", value)
        self._scene: _ir.Scene | None = None
        self._signal_id: int | None = None

    def get_value(self) -> float:
        return self._value

    def set_value(self, value: float) -> ValueTracker:
        self._value = _finite_scalar("value", value)
        if self._scene is not None and self._signal_id is not None:
            self._scene._reactive_signals[self._signal_id]["source"]["input"][
                "scalar"
            ] = self._value
        return self

    def increment_value(self, delta: float) -> ValueTracker:
        return self.set_value(self._value + _finite_scalar("delta", delta))

    @property
    def signal_id(self) -> int:
        if self._signal_id is None:
            raise AttributeError("ValueTracker has no signal id until attached to a Scene")
        return self._signal_id

    @property
    def animate(self) -> Any:
        raise NotImplementedError(
            "ValueTracker.animate requires timeline-driven signal tracks; "
            "native input/binding support is available now, but this animation form "
            "is not yet implemented"
        )


def _scene_init(self: _ir.Scene) -> None:
    _ORIGINAL_SCENE_INIT(self)
    self._reactive_signals: list[dict[str, Any]] = []
    self._reactive_bindings: list[dict[str, Any]] = []


def _to_document(self: _ir.Scene) -> dict[str, Any]:
    document = _ORIGINAL_TO_DOCUMENT(self)
    signals = getattr(self, "_reactive_signals", [])
    bindings = getattr(self, "_reactive_bindings", [])
    if signals or bindings:
        document["reactive"] = {
            "signals": list(signals),
            "bindings": list(bindings),
        }
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
