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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manim_source", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    source = args.manim_source / "manim/utils/rate_functions.py"
    reference = load_helpers(source, {"unit_interval", "zero"}, {"wraps": wraps})
    expected = observations(reference["unit_interval"], reference["zero"])
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_rate_functions as noon
    actual = observations(noon._unit_interval, noon._zero_outside_interval)
    errors = [] if actual == expected else ["Python call guards differ from pinned source"]
    if args.installed_manim:
        from manim.utils import rate_functions
        if Path(rate_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed rate source differs from pin")
        if observations(rate_functions.unit_interval, rate_functions.zero) != expected:
            errors.append("installed decorator observations differ")
        for name in ["linear", "smooth", "rush_into", "rush_from", "there_and_back"]:
            for t in [-math.inf, -0.01, 1.01, math.inf, math.nan]:
                if getattr(noon, name)(t) != getattr(rate_functions, name)(t):
                    errors.append(f"{name}: boundary {t} differs")
    print(json.dumps({"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "decorator_observations": len(expected), "public_boundary_observations": 25 if args.installed_manim else 0,
                      "installed_module_checked": args.installed_manim, "errors": errors}, indent=2))
    return bool(errors)


if __name__ == "__main__": raise SystemExit(main())
