"""Thin Python adapters for Noon's shared deterministic rate-function vocabulary.

Playback remains authoritative in Rust (`noon_core::RateFunction`). This module only
provides Manim-compatible public callables, maps known callables to the shared semantic
IDs passed to Rust. Python callables remain available for explicit user evaluation.
"""

from __future__ import annotations

import math
import dis
import inspect
import sys
from typing import Callable
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


def _sigmoid(value: float) -> float:
    return 1.0 / (1.0 + math.exp(-value))


@_unit_interval
def smooth(t: float, inflection: float = INFLECTION) -> float:
    """Manim-compatible normalized logistic smooth rate function."""

    return min(max(_smooth_value(float(t), float(inflection), _sigmoid), 0.0), 1.0)


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
    return 2.0 * smooth(float(t) / 2.0, inflection)


@_unit_interval
def rush_from(t: float, inflection: float = INFLECTION) -> float:
    return 2.0 * smooth(float(t) / 2.0 + 0.5, inflection) - 1.0


@_zero_outside_interval
def there_and_back(t: float, inflection: float = INFLECTION) -> float:
    value = float(t)
    mirrored = 2.0 * value if value < 0.5 else 2.0 * (1.0 - value)
    return smooth(mirrored, inflection)




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


@_unit_interval
def ease_in_expo(t: float) -> float:
    return 0 if t == 0 else pow(2, 10 * t - 10)

@_unit_interval
def ease_out_expo(t: float) -> float:
    return 1 if t == 1 else 1 - pow(2, -10 * t)

@_unit_interval
def ease_in_out_expo(t: float) -> float:
    if t == 0:
        return 0
    elif t == 1:
        return 1
    elif t < 0.5:
        return pow(2, 20 * t - 10) / 2
    else:
        return (2 - pow(2, -20 * t + 10)) / 2

@_unit_interval
def ease_in_circ(t: float) -> float:
    from math import sqrt
    return 1 - sqrt(1 - pow(t, 2))

@_unit_interval
def ease_out_circ(t: float) -> float:
    from math import sqrt
    return sqrt(1 - pow(t - 1, 2))

@_unit_interval
def ease_in_out_circ(t: float) -> float:
    from math import sqrt
    return (
        (1 - sqrt(1 - pow(2 * t, 2))) / 2
        if t < 0.5
        else (sqrt(1 - pow(-2 * t + 2, 2)) + 1) / 2
    )


_KNOWN_RATE_FUNCTIONS: dict[str, Callable[..., float]] = {
    "ease_in_expo": ease_in_expo,
    "ease_out_expo": ease_out_expo,
    "ease_in_out_expo": ease_in_out_expo,
    "ease_in_circ": ease_in_circ,
    "ease_out_circ": ease_out_circ,
    "ease_in_out_circ": ease_in_out_circ,

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

    name = getattr(rate_func, "__name__", None)
    for semantic_id, function in _KNOWN_RATE_FUNCTIONS.items():
        if rate_func is function or rate_func == function or name == semantic_id:
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

    code = getattr(rate_func, "__code__", None)
    global_smooth = getattr(rate_func, "__globals__", {}).get("smooth")
    if (
        not inspect.isfunction(rate_func)
        or code is None
        or global_smooth is not smooth
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
