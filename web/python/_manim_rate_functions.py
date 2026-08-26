"""Thin Python adapters for Noon's shared deterministic rate-function vocabulary.

Playback remains authoritative in Rust (`noon_core::RateFunction`). This module only
provides Manim-compatible public callables, maps known callables to the shared semantic
IDs written into scene IR, and mirrors those deterministic functions for authoring-time
snapshot evaluation inside the Python frontend.
"""

from __future__ import annotations

import math
from typing import Any, Callable

import noon as _base
import _manim_compat as _compat
import _noon_ir as _ir


INFLECTION = 10.0
_INSTALLED = False
_ORIGINAL_ADD_TRACK = _ir.Scene._add_track
_ORIGINAL_MOBJECT_SET_COLOR = _base.Mobject.set_color


def linear(t: float) -> float:
    """Manim's linear rate function."""

    return float(t)


def _sigmoid(value: float) -> float:
    return 1.0 / (1.0 + math.exp(-value))


def smooth(t: float, inflection: float = INFLECTION) -> float:
    """Manim-compatible normalized logistic smooth rate function."""

    value = float(t)
    sharpness = float(inflection)
    error = _sigmoid(-sharpness / 2.0)
    result = (
        _sigmoid(sharpness * (value - 0.5)) - error
    ) / (1.0 - 2.0 * error)
    return min(max(result, 0.0), 1.0)


def rush_into(t: float, inflection: float = INFLECTION) -> float:
    return 2.0 * smooth(float(t) / 2.0, inflection)


def rush_from(t: float, inflection: float = INFLECTION) -> float:
    return 2.0 * smooth(float(t) / 2.0 + 0.5, inflection) - 1.0


def there_and_back(t: float, inflection: float = INFLECTION) -> float:
    value = float(t)
    mirrored = 2.0 * value if value < 0.5 else 2.0 * (1.0 - value)
    return smooth(mirrored, inflection)


def _ease_in_out_cubic(t: float) -> float:
    value = min(max(float(t), 0.0), 1.0)
    if value < 0.5:
        return 4.0 * value * value * value
    return 1.0 - ((-2.0 * value + 2.0) ** 3) / 2.0


def _step_start(t: float) -> float:
    """Internal retained step: source at t=0, target for every t>0."""

    return 0.0 if float(t) <= 0.0 else 1.0


def _step_end(t: float) -> float:
    """Internal retained step: source for t<1, target exactly at t=1."""

    return 0.0 if float(t) < 1.0 else 1.0


_KNOWN_RATE_FUNCTIONS: dict[str, Callable[..., float]] = {
    "linear": linear,
    "smooth": smooth,
    "rush_into": rush_into,
    "rush_from": rush_from,
    "there_and_back": there_and_back,
    "step_start": _step_start,
    "step_end": _step_end,
}


def easing_from_rate_func(rate_func: object) -> str:
    """Map a known Manim callable to the language-neutral core semantic ID."""

    name = getattr(rate_func, "__name__", None)
    for semantic_id, function in _KNOWN_RATE_FUNCTIONS.items():
        if rate_func is function or rate_func == function or name == semantic_id:
            return semantic_id
    raise NotImplementedError(
        "Noon currently supports deterministic rate_func=linear, smooth, rush_into, "
        "rush_from, and there_and_back; arbitrary Python per-frame rate functions "
        "are intentionally unsupported"
    )


def evaluate_rate_function(semantic_id: str, progress: float) -> float:
    """Mirror core RateFunction evaluation for authoring-time snapshots only."""

    value = min(max(float(progress), 0.0), 1.0)
    if semantic_id == "ease_in_out_cubic":
        return _ease_in_out_cubic(value)
    function = _KNOWN_RATE_FUNCTIONS.get(semantic_id)
    if function is None:
        raise ValueError(f"unsupported easing: {semantic_id}")
    return function(value)


def _track_progress(timing: dict[str, Any], time: float) -> float:
    raw = max(
        0.0,
        min(1.0, (time - timing["start_time"]) / timing["duration"]),
    )
    return evaluate_rate_function(timing["easing"], raw)


def _add_track(
    self: _ir.Scene,
    obj: _ir.Object,
    property_name: str,
    values: dict[str, Any],
    start_time: float,
    duration: float,
    easing: str,
    key: str | None,
) -> None:
    """Bridge the legacy Python IR whitelist to the shared core vocabulary.

    The old IR builder validates only ``linear`` and ``ease_in_out_cubic``. For a
    known shared semantic ID, reuse all of its existing structural validation with
    ``linear`` as the temporary accepted token, then restore the semantic ID in the
    emitted track. Unknown values still flow through the original validator and fail.
    """

    if easing in _KNOWN_RATE_FUNCTIONS and easing != "linear":
        _ORIGINAL_ADD_TRACK(
            self,
            obj,
            property_name,
            values,
            start_time,
            duration,
            "linear",
            key,
        )
        self._tracks[-1]["timing"]["easing"] = easing
        return
    _ORIGINAL_ADD_TRACK(
        self,
        obj,
        property_name,
        values,
        start_time,
        duration,
        easing,
        key,
    )


def _set_color_preserving_opacity(
    self: _base.Mobject, color: _base.Color
) -> _base.Mobject:
    """Match Manim ``set_color`` without coupling RGB to fill/stroke opacity."""

    before = self._current_raw().style
    result = _ORIGINAL_MOBJECT_SET_COLOR(self, color)
    raw = _base._raw_mobject(self._current_raw())
    for channel in ("fill", "stroke"):
        previous = before[channel]
        current = raw.style[channel]
        if previous is not None and current is not None:
            current["alpha"] = previous["alpha"]
    result._apply(raw)
    return result


def install() -> None:
    """Install thin public adapters without creating a second playback engine."""

    global _INSTALLED
    if _INSTALLED:
        return
    _INSTALLED = True

    public = {
        "linear": linear,
        "smooth": smooth,
        "rush_into": rush_into,
        "rush_from": rush_from,
        "there_and_back": there_and_back,
    }
    for name, value in public.items():
        setattr(_compat, name, value)
        setattr(_base, name, value)

    _compat._easing_from_rate_func = easing_from_rate_func

    # Manim's set_color changes fill/stroke RGB while preserving each channel's
    # independent opacity. This matters for Transform-style effects such as Indicate:
    # a transparent stroke must not become visible while the color interpolates.
    _base.Mobject.set_color = _set_color_preserving_opacity

    # `_noon_ir` needs progress only while materializing authoring-time snapshots.
    # Runtime playback never calls this mirror; Rust RateFunction remains authoritative.
    _ir._track_progress = _track_progress
    _ir.Scene._add_track = _add_track

    exports = list(_base.__all__)
    for name in public:
        if name not in exports:
            exports.append(name)
    _base.__all__ = exports
