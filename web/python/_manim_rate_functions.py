"""Thin Python adapters for Noon's shared deterministic rate-function vocabulary.

Playback remains authoritative in Rust (`noon_core::RateFunction`). This module only
provides Manim-compatible public callables, maps known callables to the shared semantic
IDs passed to Rust. Python callables remain available for explicit user evaluation.
"""

from __future__ import annotations

import math
import dis
import inspect
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
        # The admitted bytecode necessarily loads the global smooth callable.
        # Reject ordinary easing without building Instruction objects per leaf.
        or "smooth" not in code.co_names
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
