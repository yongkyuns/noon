"""Thin Python adapters for Noon's shared deterministic rate-function vocabulary.

Playback remains authoritative in Rust (`noon_core::RateFunction`). This module only
provides Manim-compatible public callables, maps known callables to the shared semantic
IDs passed to Rust. Python callables remain available for explicit user evaluation.
"""

from __future__ import annotations

import math
from typing import Callable



INFLECTION = 10.0


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
