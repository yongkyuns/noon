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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    source = args.manim_source / "manim/utils/simple_functions.py"
    expected = observations(load_helpers(source, NAMES, {"math": math, "np": np, "lru_cache": lru_cache}))
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_simple_functions
    import noon
    actual = observations(vars(_manim_simple_functions))
    errors = [] if actual == expected else ["user numerical observations differ"]
    if args.installed_manim:
        from manim.utils import simple_functions
        if Path(simple_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed simple-function source differs from pin")
        if observations(vars(simple_functions)) != expected: errors.append("full installed module differs")
    for name in NAMES:
        if getattr(noon, name) is not getattr(_manim_simple_functions, name) or name not in noon.__all__:
            errors.append(f"public export {name} differs")
    print(json.dumps({"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "symbols": len(NAMES), "observations": len(expected), "numpy_version": np.__version__,
                      "installed_module_checked": args.installed_manim, "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__": raise SystemExit(main())
