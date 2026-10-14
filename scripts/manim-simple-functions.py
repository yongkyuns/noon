"""Qualify user numerical helpers against unchanged pinned definitions."""
import argparse
from decimal import Decimal
from fractions import Fraction
from functools import lru_cache
import hashlib
import inspect
import json
import math
from pathlib import Path
import sys
import warnings

import numpy as np

from pinned_python_helpers import load_helpers

NAMES = {"binary_search", "choose", "clip", "sigmoid"}


def normalize(value):
    if isinstance(value, np.ndarray):
        return ["array", str(value.dtype), list(value.shape), normalize(value.tolist())]
    if isinstance(value, np.generic):
        return [type(value).__name__, normalize(value.item())]
    if isinstance(value, float) and not math.isfinite(value):
        return ["float", "nan" if math.isnan(value) else str(value)]
    if isinstance(value, (Decimal, Fraction, complex)):
        return [type(value).__name__, str(value)]
    if isinstance(value, (list, tuple)):
        return [normalize(item) for item in value]
    return value


def observations(namespace):
    result = [["signature", name, str(inspect.signature(namespace[name]))]
              for name in sorted(NAMES)]
    def record(name, *args, **kwargs):
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                value = namespace[name](*args, **kwargs)
                outcome = ["value", normalize(value)]
                if name == "clip": outcome.append([value is arg for arg in args])
            except Exception as error:
                outcome = ["error", type(error).__name__]
        result.append([name, outcome, [type(item.message).__name__ for item in caught]])
    choose = namespace["choose"]
    choose.cache_clear()
    for n in [0, 1, 6, 20, 100, -1, True, 2.0, np.int64(7)]:
        for k in [0, 1, 2, 6, -1, True, 1.0]: record("choose", n, k)
    record("choose", [], 1)
    choose.cache_clear()
    record("choose", 6, 2)
    record("choose", 6, 2)
    result.append(["choose_cache", list(choose.cache_info())])
    for n in range(11): choose(n, 0)
    result.append(["choose_eviction", list(choose.cache_info())])
    for args in [(5, 0, 10), (-1, 0, 10), (11, 0, 10), (5, 10, 0),
                 ("a", "h", "k"), (None, 0, 1), (math.nan, 0, 1),
                 (Fraction(1, 3), Fraction(0), Fraction(1)),
                 (Decimal("0.2"), Decimal(0), Decimal(1)),
                 (np.array([1, 2]), 0, 1)]: record("clip", *args)
    calls = []
    class Comparable:
        def __lt__(self, other): calls.append("lt"); return False
        def __gt__(self, other): calls.append("gt"); return False
    value = Comparable()
    selected = namespace["clip"](value, object(), object())
    result.append(["clip_order_identity", calls, selected is value])
    for value in [-math.inf, -1000, -10, -1, 0, 1, 10, 1000, math.inf,
                  math.nan, True, 1j, "1", [0, 1], Decimal("0.2"),
                  Fraction(1, 3), np.float32(0.25), np.float64(0.25),
                  np.array([], dtype=np.float32), np.array([-1, 0, 1], dtype=np.float32),
                  np.array([0, 1], dtype=np.int64)]: record("sigmoid", value)
    for direction in [1, -1]:
        for bounds in [(0, 1), (1, 0), (0, 0), (0, 1e-5), (Fraction(0), Fraction(1))]:
            for target in [-2, 0, direction * 0.25, direction, 2]:
                calls = []
                def callback(value):
                    calls.append(normalize(value))
                    return direction * value
                record("binary_search", callback, target, *bounds)
                result.append(["search_calls", calls])
    for tolerance in [2, math.nan, "invalid"]:
        calls = []
        def callback(value): calls.append(normalize(value)); return value
        record("binary_search", callback, 0.25, 0, 1, tolerance=tolerance)
        result.append(["search_tolerance_calls", calls])
    calls = []
    def failing_callback(value):
        calls.append(normalize(value))
        if len(calls) == 2: raise RuntimeError("callback")
        return value
    record("binary_search", failing_callback, 0.25, 0, 1)
    result.append(["search_failure_calls", calls])
    return result



INTERPOLATION_NAMES = {"interpolate", "integer_interpolate", "mid", "inverse_interpolate", "match_interpolate"}


def interpolation_observations(namespace):
    result = [["signature", name, str(inspect.signature(namespace[name]))]
              for name in sorted(INTERPOLATION_NAMES)]
    def record(name, *args):
        before = normalize(args)
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            try:
                value = namespace[name](*args)
                outcome = ["value", type(value).__name__, normalize(value),
                           [value is arg for arg in args]]
                if isinstance(value, np.ndarray):
                    outcome.append([np.shares_memory(value, arg) if isinstance(arg, np.ndarray)
                                    else False for arg in args])
            except Exception as error:
                outcome = ["error", type(error).__name__]
        result.append([name, outcome, [type(item.message).__name__ for item in caught],
                       normalize(args) == before])
    ranges = [(0, 10), (10, 0), (2, 2), (-2.5, 3.5),
              (Fraction(1, 3), Fraction(5, 3)), (Decimal("0.2"), Decimal("0.8")),
              (np.float32(1), np.float32(3)),
              (np.array([0, 1, 2]), np.array([3, 2, 1])),
              (np.array([[0], [1]], dtype=np.float32), np.array([2, 3, 4], dtype=np.float32)),
              (np.empty((0, 2)), np.empty((0, 2))), ([1, 2], [3, 4])]
    alphas = [-1, 0, 0.25, 1, 2, math.inf, math.nan, True, "0.5",
              Fraction(1, 4), Decimal("0.25"), np.float32(0.25),
              np.array([[0.25], [0.75]])]
    for start, end in ranges:
        record("mid", start, end)
        for alpha in alphas:
            record("interpolate", start, end, alpha)
            record("integer_interpolate", start, end, alpha)
            record("inverse_interpolate", start, end, alpha)
            record("match_interpolate", -5, 5, start, end, alpha)
    events = []
    class Operand:
        def __init__(self, name): self.name = name
        def __rsub__(self, other): events.append(["rsub", self.name, other]); return Operand("complement")
        def __mul__(self, other): events.append(["mul", self.name, other.name]); return Operand("product")
        def __add__(self, other): events.append(["add", self.name, other.name]); return Operand("sum")
    value = namespace["interpolate"](Operand("start"), Operand("end"), Operand("alpha"))
    result.append(["operator_order", events, value.name])
    return result


def qualify_interpolation(manim_source, *, installed_manim=False):
    from typing import overload
    source = manim_source / "manim/utils/bezier.py"
    expected = interpolation_observations(load_helpers(source, INTERPOLATION_NAMES,
                                                      {"np": np, "overload": overload}))
    import _manim_interpolation
    import noon
    actual = interpolation_observations(vars(_manim_interpolation))
    errors = [] if actual == expected else ["user interpolation observations differ"]
    if installed_manim:
        from manim.utils import bezier
        if Path(bezier.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed interpolation source differs from pin")
        if interpolation_observations(vars(bezier)) != expected:
            errors.append("installed interpolation observations differ")
    for name in INTERPOLATION_NAMES:
        if getattr(noon, name) is not getattr(_manim_interpolation, name) or name not in noon.__all__:
            errors.append(f"public interpolation export {name} differs")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "symbols": len(INTERPOLATION_NAMES), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors}


def qualify(manim_source, *, installed_manim=False):
    source = manim_source / "manim/utils/simple_functions.py"
    expected = observations(load_helpers(source, NAMES, {"math": math, "np": np, "lru_cache": lru_cache}))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_simple_functions
    import noon
    actual = observations(vars(_manim_simple_functions))
    errors = [] if actual == expected else ["user numerical observations differ"]
    if installed_manim:
        from manim.utils import simple_functions
        if Path(simple_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed simple-function source differs from pin")
        if observations(vars(simple_functions)) != expected: errors.append("full installed module differs")
    for name in NAMES:
        if getattr(noon, name) is not getattr(_manim_simple_functions, name) or name not in noon.__all__:
            errors.append(f"public export {name} differs")
    interpolation = qualify_interpolation(manim_source, installed_manim=installed_manim)
    errors.extend(interpolation["errors"])
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "symbols": len(NAMES), "observations": len(expected), "numpy_version": np.__version__,
            "interpolation_helpers": interpolation,
            "installed_module_checked": installed_manim, "errors": errors}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.manim_source, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    return bool(result["errors"])


if __name__ == "__main__": raise SystemExit(main())
