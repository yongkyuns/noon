"""Thin Python adapters for Noon's shared deterministic rate-function vocabulary.

Playback remains authoritative in Rust (`noon_core::RateFunction`). This module only
provides Manim-compatible public callables, maps known callables to the shared semantic
IDs passed to Rust. Python callables remain available for explicit user evaluation.
"""

from __future__ import annotations

import dis
import inspect
import sys
from typing import Callable
from types import FunctionType
from functools import wraps

# Preserve the public module alias and callable identities without a second registry.
rate_functions = sys.modules[__name__]

INFLECTION = 10.0


def _unit_interval(function):
    @wraps(function)
    def wrapper(t, *args, **kwargs):
        if 0 <= t <= 1:
            return function(t, *args, **kwargs)
        return 0 if t < 0 else 1
    return wrapper


def _zero_outside_interval(function):
    @wraps(function)
    def wrapper(t, *args, **kwargs):
        return function(t, *args, **kwargs) if 0 <= t <= 1 else 0
    return wrapper


@_unit_interval
def linear(t: float) -> float:
    """Manim's linear rate function."""

    return t


@_unit_interval
def smoothstep(t: float) -> float:
    return 3 * t**2 - 2 * t**3


@_unit_interval
def smootherstep(t: float) -> float:
    return 6 * t**5 - 15 * t**4 + 10 * t**3


@_unit_interval
def smoothererstep(t: float) -> float:
    return 35 * t**4 - 84 * t**5 + 70 * t**6 - 20 * t**7


@_unit_interval
def smooth(t: float, inflection: float = INFLECTION) -> float:
    """Manim-compatible normalized logistic smooth rate function."""

    from _manim_simple_functions import sigmoid
    return min(max(_smooth_value(t, inflection, sigmoid), 0), 1)


def _smooth_value(t, inflection, sigmoid):
    error = sigmoid(-inflection / 2)
    return (sigmoid(inflection * (t - 0.5)) - error) / (1 - 2 * error)


@_unit_interval
def slow_into(t: float) -> float:
    import numpy as np
    return np.sqrt(1 - (1 - t) * (1 - t))


@_unit_interval
def double_smooth(t: float) -> float:
    from _manim_simple_functions import sigmoid
    if t < 0.5:
        return 0.5 * min(max(_smooth_value(2 * t, INFLECTION, sigmoid), 0), 1)
    return 0.5 * (1 + min(max(_smooth_value(2 * t - 1, INFLECTION, sigmoid), 0), 1))


@_unit_interval
def rush_into(t: float, inflection: float = INFLECTION) -> float:
    return 2 * smooth(t / 2.0, inflection)


@_unit_interval
def rush_from(t: float, inflection: float = INFLECTION) -> float:
    return 2 * smooth(t / 2.0 + 0.5, inflection) - 1


@_zero_outside_interval
def there_and_back(t: float, inflection: float = INFLECTION) -> float:
    mirrored = 2 * t if t < 0.5 else 2 * (1 - t)
    return smooth(mirrored, inflection)


@_zero_outside_interval
def there_and_back_with_pause(t: float, pause_ratio: float = 1.0 / 3) -> float:
    a = 2.0 / (1.0 - pause_ratio)
    if t < 0.5 - pause_ratio / 2:
        return smooth(a * t)
    elif t < 0.5 + pause_ratio / 2:
        return 1
    return smooth(a - a * t)


@_unit_interval
def running_start(t: float, pull_factor: float = -0.5) -> float:
    t2 = t * t
    t3 = t2 * t
    t4 = t3 * t
    t5 = t4 * t
    t6 = t5 * t
    mt = 1 - t
    mt2 = mt * mt
    mt3 = mt2 * mt
    mt4 = mt3 * mt
    return (15 * t2 * mt4 * pull_factor + 20 * t3 * mt3 * pull_factor
            + 15 * t4 * mt2 + 6 * t5 * mt + t6)


@_zero_outside_interval
def wiggle(t: float, wiggles: float = 2) -> float:
    import numpy as np
    new_t = 2 * t if t < 0.5 else 2 * (1 - t)
    return smooth(new_t) * np.sin(wiggles * np.pi * t)


@_unit_interval
def exponential_decay(t: float, half_life: float = 0.1) -> float:
    import numpy as np
    return 1 - np.exp(-t / half_life)


def not_quite_there(func: Callable[..., float] = smooth, proportion: float = 0.7):
    """Wrap explicit user evaluation with Manim's output proportion."""
    def result(t, *args, **kwargs):
        return proportion * func(t, *args, **kwargs)
    return result


def squish_rate_func(func: Callable[..., float], a: float = 0.4, b: float = 0.6):
    """Wrap user evaluation with Manim's interval mapping and callback dispatch."""
    def result(t, *args, **kwargs):
        if a == b:
            return a
        if t < a:
            new_t = 0.0
        elif t > b:
            new_t = 1.0
        else:
            new_t = (t - a) / (b - a)
        return func(new_t, *args, **kwargs)
    return result


@_unit_interval
def ease_in_quad(t: float) -> float:
    return t * t

@_unit_interval
def ease_out_quad(t: float) -> float:
    return 1 - (1 - t) * (1 - t)

@_unit_interval
def ease_in_out_quad(t: float) -> float:
    return 2 * t * t if t < 0.5 else 1 - pow(-2 * t + 2, 2) / 2

@_unit_interval
def ease_in_cubic(t: float) -> float:
    return t * t * t

@_unit_interval
def ease_out_cubic(t: float) -> float:
    return 1 - pow(1 - t, 3)

@_unit_interval
def ease_in_out_cubic(t: float) -> float:
    return 4 * t * t * t if t < 0.5 else 1 - pow(-2 * t + 2, 3) / 2

@_unit_interval
def ease_in_quart(t: float) -> float:
    return t * t * t * t

@_unit_interval
def ease_out_quart(t: float) -> float:
    return 1 - pow(1 - t, 4)

@_unit_interval
def ease_in_out_quart(t: float) -> float:
    return 8 * t * t * t * t if t < 0.5 else 1 - pow(-2 * t + 2, 4) / 2

@_unit_interval
def ease_in_quint(t: float) -> float:
    return t * t * t * t * t

@_unit_interval
def ease_out_quint(t: float) -> float:
    return 1 - pow(1 - t, 5)

@_unit_interval
def ease_in_out_quint(t: float) -> float:
    return 16 * t * t * t * t * t if t < 0.5 else 1 - pow(-2 * t + 2, 5) / 2


def _step_start(t: float) -> float:
    """Internal retained step: source at t=0, target for every t>0."""

    return 0.0 if float(t) <= 0.0 else 1.0


def _step_end(t: float) -> float:
    """Internal retained step: source for t<1, target exactly at t=1."""

    return 0.0 if float(t) < 1.0 else 1.0


@_unit_interval
def ease_in_sine(t: float) -> float:
    import numpy as np
    val: float = 1 - np.cos((t * np.pi) / 2)
    return val

@_unit_interval
def ease_out_sine(t: float) -> float:
    import numpy as np
    val: float = np.sin((t * np.pi) / 2)
    return val

@_unit_interval
def ease_in_out_sine(t: float) -> float:
    import numpy as np
    val: float = -(np.cos(np.pi * t) - 1) / 2
    return val


_KNOWN_RATE_FUNCTIONS: dict[str, Callable[..., float]] = {
    "ease_in_sine": ease_in_sine,
    "ease_out_sine": ease_out_sine,
    "ease_in_out_sine": ease_in_out_sine,

    "ease_in_quad": ease_in_quad,
    "ease_out_quad": ease_out_quad,
    "ease_in_out_quad": ease_in_out_quad,
    "ease_in_cubic": ease_in_cubic,
    "ease_out_cubic": ease_out_cubic,
    "ease_in_out_cubic": ease_in_out_cubic,
    "ease_in_quart": ease_in_quart,
    "ease_out_quart": ease_out_quart,
    "ease_in_out_quart": ease_in_out_quart,
    "ease_in_quint": ease_in_quint,
    "ease_out_quint": ease_out_quint,
    "ease_in_out_quint": ease_in_out_quint,

    "linear": linear,
    "smooth": smooth,
    "smoothstep": smoothstep,
    "smootherstep": smootherstep,
    "smoothererstep": smoothererstep,
    "slow_into": slow_into,
    "double_smooth": double_smooth,
    "rush_into": rush_into,
    "rush_from": rush_from,
    "there_and_back": there_and_back,
    "step_start": _step_start,
    "step_end": _step_end,
}


def easing_from_rate_func(rate_func: object) -> str:
    """Map a known Manim callable to the language-neutral core semantic ID."""

    for semantic_id, function in _KNOWN_RATE_FUNCTIONS.items():
        if rate_func is function:
            return semantic_id
    raise NotImplementedError(
        "Noon currently supports deterministic rate_func=linear, smooth, rush_into, "
        "rush_from, there_and_back, smoothstep, smootherstep, smoothererstep, slow_into, "
        "double_smooth and polynomial ease_in/out/in_out functions; "
        "arbitrary Python per-frame rate functions "
        "are intentionally unsupported"
    )


def is_reverse_smooth_rate_func(rate_func: object) -> bool:
    """Recognize only the inert expression ``lambda t: smooth(1 - t)``.

    This cold, structural bytecode check never calls or samples user code. It is
    deliberately narrow so arbitrary Python easing callables stay unsupported.
    """

    if type(rate_func) is not FunctionType:
        return False

    code = rate_func.__code__
    global_smooth = rate_func.__globals__.get("smooth")
    if (
        global_smooth is not smooth
        or getattr(rate_func, "__defaults__", None) is not None
        or getattr(rate_func, "__kwdefaults__", None) is not None
        or getattr(rate_func, "__closure__", None) is not None
        or code.co_argcount != 1
        or code.co_posonlyargcount != 0
        or code.co_kwonlyargcount != 0
        or code.co_flags & (inspect.CO_VARARGS | inspect.CO_VARKEYWORDS)
    ):
        return False

    ignored = {"CACHE", "EXTENDED_ARG", "PRECALL", "PUSH_NULL", "RESUME"}
    instructions = [
        instruction
        for instruction in dis.get_instructions(rate_func)
        if instruction.opname not in ignored
    ]
    if len(instructions) != 6:
        return False
    load_global, one, argument, subtract, call, returned = instructions
    return (
        load_global.opname == "LOAD_GLOBAL"
        and load_global.argval == "smooth"
        and one.opname in {"LOAD_CONST", "LOAD_SMALL_INT"}
        and one.argval == 1
        and argument.opname in {"LOAD_FAST", "LOAD_FAST_BORROW"}
        and argument.argval == code.co_varnames[0]
        and subtract.opname in {"BINARY_OP", "BINARY_SUBTRACT"}
        and (subtract.opname != "BINARY_OP" or subtract.argrepr == "-")
        and call.opname in {"CALL", "CALL_FUNCTION"}
        and call.arg == 1
        and returned.opname == "RETURN_VALUE"
    )
