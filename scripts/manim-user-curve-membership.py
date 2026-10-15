"""Qualify root-based membership queries on explicit user control arrays."""
import argparse
from functools import lru_cache, reduce
import hashlib
import inspect
import json
import math
from pathlib import Path
import runpy
import sys
from typing import overload
import warnings

import numpy as np
from pinned_python_helpers import load_helpers

normalize = runpy.run_path(str(Path(__file__).with_name("manim-simple-functions.py")),
                          run_name="_membership_normalization")["normalize"]
NAMES = {"proportions_along_bezier_curve_for_point", "point_lies_on_bezier"}


def observations(namespace):
    result = [[name, str(inspect.signature(namespace[name]))] for name in sorted(NAMES)]

    def record(point, control_points, precision):
        args = (point, control_points, precision)
        before = normalize(args)
        for name in sorted(NAMES):
            with warnings.catch_warnings(record=True) as caught:
                warnings.simplefilter("always")
                try:
                    value = namespace[name](*args)
                    outcome = ["value", type(value).__name__, normalize(value)]
                except Exception as error:
                    outcome = ["error", type(error).__name__]
            result.append([name, outcome, [type(w.message).__name__ for w in caught],
                           normalize(args) == before])

    for dtype in (np.int64, np.float32, np.float64, np.complex128):
        for count in (1, 2, 3, 4, 5):
            points = np.arange(count * 3).reshape(count, 3).astype(dtype)
            for t in (0, .25, .5, .75, 1):
                point = namespace["bezier"](points)(t)
                for precision in (1e-6, 1e-3):
                    record(point, points, precision)
                record(point + [0, .125, 0], points, 1e-6)
            record(points[0].tolist(), points.tolist(), 1e-6)
    for point, controls in (([0, 0, 0], []), ([1, 2], [[0, 0, 0], [1, 2, 3]]),
                            ([1, 2, 3], [[1, 2, 3]]), (None, [[0, 0, 0]]),
                            ([1, 2, 3], None), ([float("nan"), 0, 0], [[0, 0, 0], [1, 1, 1]]),
                            (["x", 0, 0], [[0, 0, 0], [1, 1, 1]])):
        record(point, controls, 1e-6)
    for precision in (0, -1, None, "x", float("nan"), float("inf")):
        record([.5, .5, .5], [[0, 0, 0], [1, 1, 1]], precision)
    return result


def qualify(source_root, *, installed_manim=False):
    source = source_root / "manim/utils/bezier.py"
    simple_source = source_root / "manim/utils/simple_functions.py"
    simple = load_helpers(simple_source, {"choose"}, {"lru_cache": lru_cache, "math": math})
    original = load_helpers(source, NAMES | {"bezier"},
                            {"np": np, "reduce": reduce, "overload": overload,
                             "choose": simple["choose"]})
    expected = observations(original)
    sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "web/python"))
    import _manim_user_vectors as helpers
    import noon
    errors = [] if observations(vars(helpers)) == expected else ["curve membership observations differ"]
    for name in NAMES:
        if getattr(noon, name) is not getattr(helpers, name):
            errors.append(f"public identity {name} differs")
    if installed_manim:
        from manim.utils import bezier, simple_functions
        for module, path in ((bezier, source), (simple_functions, simple_source)):
            if Path(module.__file__).read_bytes() != path.read_bytes():
                raise ValueError("installed complete source differs from pin")
        if observations(vars(bezier)) != expected:
            errors.append("installed curve membership observations differ")
    return {"source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "simple_source_sha256": hashlib.sha256(simple_source.read_bytes()).hexdigest(),
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
