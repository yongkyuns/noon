"""Original-source public polynomial easing observations for explicit user data."""
import argparse
from decimal import Decimal
from fractions import Fraction
from functools import wraps
import hashlib
import inspect
import json
import math
from pathlib import Path
import runpy
import sys

import numpy as np
from pinned_python_helpers import load_helpers

NAMES = tuple(f"ease_{direction}_{degree}" for degree in ("quad", "cubic", "quart", "quint")
              for direction in ("in", "out", "in_out"))
normalize = runpy.run_path(str(Path(__file__).with_name("manim-simple-functions.py")),
                          run_name="_easing_normalization")["normalize"]


def observations(namespace):
    result = []
    for name in NAMES:
        function = namespace[name]
        result.append([name, str(inspect.signature(function))])
        for value in [-math.inf, -1, *[index/256 for index in range(257)], 2, math.inf,
                      math.nan, Fraction(1, 4), Decimal("0.25"), True, np.float32(0.25),
                      np.float64(0.25), "0.5", np.array([0.25]), np.array([0.25, 0.5])]:
            try:
                observed = function(value)
                result.append([name, "value", normalize(observed), type(observed).__name__])
            except Exception as error:
                result.append([name, "error", type(error).__name__])
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/rate_functions.py"
    original = load_helpers(source, set(NAMES) | {"unit_interval"}, {"wraps": wraps})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_rate_functions as noon
    import noon as public
    errors = [] if observations(vars(noon)) == expected else ["polynomial easing observations differ"]
    for name in NAMES:
        function = getattr(public.rate_functions, name)
        if function is not getattr(noon, name) or noon.easing_from_rate_func(function) != name:
            errors.append(f"public easing identity {name} differs")
        if name in public.__all__ or hasattr(public, name):
            errors.append(f"module-only easing {name} leaked into the public namespace")
    if installed_manim:
        import manim
        from manim.utils import rate_functions
        if Path(rate_functions.__file__).read_bytes() != source.read_bytes():
            raise ValueError("installed complete rate module differs from pin")
        if observations(vars(rate_functions)) != expected:
            errors.append("installed polynomial easing observations differ")
        for name in NAMES:
            if hasattr(manim, name):
                errors.append(f"pinned module-only easing {name} unexpectedly became public")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "functions": len(NAMES), "observations": len(expected),
            "installed_module_checked": installed_manim, "errors": errors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("--installed-manim", action="store_true")
    args = parser.parse_args()
    result = qualify(args.source_root, installed_manim=args.installed_manim)
    print(json.dumps(result, indent=2))
    sys.exit(bool(result["errors"]))
