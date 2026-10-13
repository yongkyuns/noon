"""Compare Python rate-call guards with the pinned Manim decorators."""
import argparse
from functools import wraps
import hashlib
import json
import math
from pathlib import Path
import sys

from pinned_python_helpers import load_helpers


def observations(unit_interval, zero):
    result = []
    for decorator in [unit_interval, zero]:
        calls = []
        def probe(t, value=2):
            calls.append([t, value])
            return value
        function = decorator(probe)
        for t in [-math.inf, -2, -0.01, 0, 0.25, 1, 1.01, 2, math.inf, math.nan, "0.5"]:
            try: result.append(function(t, value=3))
            except TypeError: result.append("TypeError")
        result.append(calls)
        result.append(function.__name__ == "probe" and function.__wrapped__ is probe)
    return result


def wrapper_observations(namespace):
    def normalized(value):
        if isinstance(value, float) and math.isnan(value): return "NaN"
        return value
    result = []
    sentinel = object()
    for name in ["not_quite_there", "squish_rate_func"]:
        for parameters in ([{}, {"proportion": 0.25}] if name == "not_quite_there" else
                           [{}, {"a": 0.2, "b": 0.8}, {"a": 0.8, "b": 0.2}, {"a": 0.5, "b": 0.5}]):
            calls = []
            def callback(t, positional, *, token):
                calls.append([normalized(t), positional is sentinel, token is sentinel])
                return t + 2
            wrapped = namespace[name](callback, **parameters)
            result.append([wrapped.__name__, len(calls)])
            for t in [-math.inf, -1, 0, 0.2, 0.4, 0.5, 0.6, 0.8, 1, math.inf, math.nan]:
                result.append(normalized(wrapped(t, sentinel, token=sentinel)))
            result.append(calls)
        failure = ValueError("callback failed")
        def failing(t): raise failure
        try: namespace[name](failing)(0.5)
        except ValueError as error: result.append(error is failure)
        else: result.append(False)
    from fractions import Fraction
    endpoint = Fraction(1, 3)
    result.append(namespace["squish_rate_func"](failing, endpoint, Fraction(1, 3))(object()) is endpoint)
    return result


def polynomial_observations(namespace):
    from fractions import Fraction
    from decimal import Decimal
    result = []
    for name in ["smoothstep", "smootherstep", "smoothererstep"]:
        function = namespace[name]
        for value in [-math.inf, -1, *[index / 256 for index in range(257)], 2, math.inf, math.nan,
                      Fraction(1, 4), Decimal("0.25"), True]:
            observed = function(value)
            result.append([str(observed), type(observed).__name__])
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    source = args.manim_source / "manim/utils/rate_functions.py"
    reference = load_helpers(source, {"unit_interval", "zero", "not_quite_there", "squish_rate_func", "smoothstep", "smootherstep", "smoothererstep"}, {"wraps": wraps, "smooth": lambda t: t})
    expected = observations(reference["unit_interval"], reference["zero"])
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_rate_functions as noon
    actual = observations(noon._unit_interval, noon._zero_outside_interval)
    errors = [] if actual == expected else ["Python call guards differ from pinned source"]
    expected_polynomials = polynomial_observations(reference)
    if polynomial_observations(vars(noon)) != expected_polynomials:
        errors.append("polynomial observations differ from pinned source")
    expected_wrappers = wrapper_observations(reference)
    if wrapper_observations(vars(noon)) != expected_wrappers:
        errors.append("callable wrapper observations differ from pinned source")
    if args.installed_manim:
        from manim.utils import rate_functions
        if Path(rate_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed rate source differs from pin")
        if polynomial_observations(vars(rate_functions)) != expected_polynomials:
            errors.append("installed polynomial observations differ")
        if wrapper_observations(vars(rate_functions)) != expected_wrappers:
            errors.append("installed callable wrapper observations differ")
        if observations(rate_functions.unit_interval, rate_functions.zero) != expected:
            errors.append("installed decorator observations differ")
        for name in ["linear", "smooth", "rush_into", "rush_from", "there_and_back"]:
            for t in [-math.inf, -0.01, 1.01, math.inf, math.nan]:
                if getattr(noon, name)(t) != getattr(rate_functions, name)(t):
                    errors.append(f"{name}: boundary {t} differs")
    print(json.dumps({"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "decorator_observations": len(expected), "public_boundary_observations": 25 if args.installed_manim else 0,
                      "callable_wrapper_observations": len(expected_wrappers),
                      "polynomial_observations": len(expected_polynomials),
                      "installed_module_checked": args.installed_manim, "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__": raise SystemExit(main())
